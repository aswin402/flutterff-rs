use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::PORT_SCAN_RANGE;
use crate::mcp;
use crate::theme::{BLUE, DIM, RED, RESET, TEXT, YELLOW, now_timestamp};
use crate::tui::{PrettyProgress, Step};

/// Network probe endpoints (Google DNS & Cloudflare DNS)
const PROBE_ENDPOINTS: &[SocketAddr] = &[
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(8, 8, 8, 8), 53)),
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 1), 53)),
];

/// Substrings that mark noisy web/compiler output to suppress
const NOISE_PATTERNS: &[&str] = &[
    "surface instance already deleted",
    "http://",
    "https://",
    ".js:",
    "console",
    "flutter_bootstrap",
    "ddc_module_loader",
    "dart_sdk",
    "web_entrypoint",
];

/// Fatal/error substrings emitted directly by Flutter tool stdout/stderr
const ERROR_PATTERNS: &[&str] = &["error:", "exception:", "fatal:", "unhandled", "══╡", "══╞"];

/// Table-driven log level rules evaluated in priority order
const LEVEL_RULES: &[(&[&str], (&str, &str))] = &[
    (
        &["error:", "exception:", "fatal:", "unhandled"],
        ("ERR", RED),
    ),
    (&["warning:", "warn:", "deprecated"], ("WRN", YELLOW)),
    (&["debug:"], ("DBG", DIM)),
];

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

pub fn is_port_free(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

pub fn find_free_port(start: u16) -> u16 {
    let end = start.saturating_add(PORT_SCAN_RANGE);
    if let Some(port) = (start..=end).find(|&p| is_port_free(p)) {
        return port;
    }
    if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        && let Ok(addr) = listener.local_addr()
    {
        return addr.port();
    }
    eprintln!("{}No free port found starting from {}{}", RED, start, RESET);
    std::process::exit(1);
}

pub fn check_online() -> bool {
    PROBE_ENDPOINTS
        .iter()
        .any(|addr| TcpStream::connect_timeout(addr, Duration::from_secs(2)).is_ok())
}

/// O(n) single-pass ANSI escape sequence stripper (no regex needed)
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        match c {
            '\x1b' => in_escape = true,
            c if in_escape => in_escape = !c.is_ascii_alphabetic(),
            c => out.push(c),
        }
    }
    out.trim().to_string()
}

/// Extracts `http://localhost:<port>...` or `http://127.0.0.1:<port>...` without regex
pub fn extract_local_url(line: &str) -> Option<String> {
    const PREFIXES: &[&str] = &["http://localhost:", "http://127.0.0.1:"];
    let start = PREFIXES.iter().find_map(|prefix| line.find(prefix))?;
    let candidate = line[start..].split_whitespace().next()?;
    let port_part = candidate
        .split(':')
        .nth(2)?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .count();
    (port_part > 0).then(|| candidate.to_string())
}

/// Extracts the TCP port from a URL string, falling back to `fallback_port`
pub fn extract_url_port(url: &str, fallback_port: u16) -> u16 {
    url.split(':')
        .nth(2)
        .and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u16>().ok()
        })
        .unwrap_or(fallback_port)
}

fn detect_level(lo: &str) -> (&'static str, &'static str) {
    LEVEL_RULES
        .iter()
        .find(|&&(patterns, _)| contains_any(lo, patterns))
        .map(|&(_, level)| level)
        .unwrap_or(("INF", BLUE))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Flutter,
    WebView,
}

pub fn format_flutter_log(line: &str, source: LogSource) -> Option<String> {
    let text = strip_ansi(line);
    if text.is_empty() {
        return None;
    }

    let lo = text.to_lowercase();
    if contains_any(&lo, NOISE_PATTERNS) {
        return None;
    }

    let (tag, color, body): (&str, &str, &str) = match source {
        _ if lo.contains("flutter:") => {
            let idx = lo.find("flutter:")? + "flutter:".len();
            let msg = text.get(idx..)?.trim();
            if msg.is_empty() {
                return None;
            }
            let (t, c) = detect_level(&lo[idx..]);
            (t, c, msg)
        }
        LogSource::WebView => {
            let (t, c) = detect_level(&lo);
            (t, c, text.as_str())
        }
        LogSource::Flutter if contains_any(&lo, ERROR_PATTERNS) => ("ERR", RED, text.as_str()),
        LogSource::Flutter => return None,
    };

    Some(format!(
        "{}{}{} {}{}{} {}{}{}",
        DIM,
        now_timestamp(),
        RESET,
        color,
        tag,
        RESET,
        TEXT,
        body,
        RESET
    ))
}

pub fn run_flutter(
    cmd: Vec<String>,
    port: u16,
    url_tx: mpsc::Sender<String>,
    child_slot: Arc<Mutex<Option<Child>>>,
    stdin_slot: Arc<Mutex<Option<ChildStdin>>>,
    progress: Arc<Mutex<PrettyProgress>>,
) {
    let mut url_sent = false;

    progress
        .lock()
        .unwrap()
        .start_step(Step::FlutterDaemon, None);

    let mut child = match Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            {
                let mut p = progress.lock().unwrap();
                p.error_msg = Some(format!("Flutter not found: {}", e));
                p.settled = Some(false);
            }
            glib::idle_add(|| {
                gtk::main_quit();
                glib::ControlFlow::Break
            });
            return;
        }
    };

    {
        let mut p = progress.lock().unwrap();
        p.finish_step(Step::FlutterDaemon, None);
        p.start_step(Step::CompileWeb, None);
    }

    *stdin_slot.lock().unwrap() = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    *child_slot.lock().unwrap() = Some(child);

    let prog_err = progress.clone();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(out) = format_flutter_log(&line, LogSource::Flutter) {
                prog_err.lock().unwrap().log_or_print(out);
            }
        }
    });

    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if !url_sent {
            let clean = strip_ansi(&line);
            let detected_url = extract_local_url(&clean).or_else(|| {
                let lo = clean.to_lowercase();
                (lo.contains("serving") || lo.contains("listening"))
                    .then(|| format!("http://localhost:{}", port))
            });

            if let Some(url_str) = detected_url {
                url_sent = true;
                {
                    let mut p = progress.lock().unwrap();
                    p.finish_step(Step::CompileWeb, None);
                    p.start_step(
                        Step::BindServer,
                        Some(format!("bind http://localhost:{}", port)),
                    );
                }
                let _ = url_tx.send(url_str);
            }
        }

        if let Some(out) = format_flutter_log(&line, LogSource::Flutter) {
            progress.lock().unwrap().log_or_print(out);
        }
    }

    if !url_sent {
        let mut p = progress.lock().unwrap();
        if p.settled.is_none() {
            p.error_msg = Some("Flutter process exited before starting web-server".to_string());
            p.settled = Some(false);
        }
    }

    if let Some(mut c) = child_slot.lock().unwrap().take() {
        let _ = c.wait();
    }
    mcp::cleanup_socket();
    print!("\x1b[?25h");
    let _ = std::io::stdout().flush();
    glib::idle_add(|| {
        gtk::main_quit();
        glib::ControlFlow::Break
    });
}
