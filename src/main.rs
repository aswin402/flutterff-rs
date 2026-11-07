use gtk::prelude::*;
use gtk::{Box as GtkBox, HeaderBar, Menu, MenuButton, MenuItem};
use webkit2gtk::{
    CacheModel, UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript,
    UserScriptInjectionTime, WebContext, WebContextExt, WebView, WebViewExt, WebViewExtManual,
};

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_PORT: u16 = 8080;
const PORT_SCAN_RANGE: u16 = 120;
const DEFAULT_PRESET: &str = "mobile";
const SEPARATOR_WIDTH: usize = 40;
const BAR_WIDTH: usize = 28;
const MIN_NAME_WIDTH: usize = 32;
const BUILDING_ROWS: usize = 4;
const DONE_ROWS: usize = 5;

// ─────────────────────────────────────────────────────────────────────────────
//  Unified 24-bit Truecolor Theme Palette (Single Source of Truth)
// ─────────────────────────────────────────────────────────────────────────────
type Rgb = (u8, u8, u8);

macro_rules! define_palette {
    ($($rgb_name:ident, $ansi_name:ident = ($r:literal, $g:literal, $b:literal);)+) => {
        $(
            const $rgb_name: Rgb = ($r, $g, $b);
            const $ansi_name: &str = concat!("\x1b[38;2;", stringify!($r), ";", stringify!($g), ";", stringify!($b), "m");
        )+
    };
}

define_palette! {
    BASE_RGB,     _BASE_ANSI     = (30, 30, 46);
    TEXT_RGB,     TEXT           = (205, 214, 244);
    SUBTEXT_RGB,  SUB            = (166, 173, 200);
    SURFACE1_RGB, _SURFACE1_ANSI = (69, 71, 90);
    DIM_RGB,      DIM            = (108, 112, 134);
    CYAN_RGB,     CYAN           = (0, 229, 255);
    SAPPHIRE_RGB, BLUE           = (56, 189, 248);
    MINT_RGB,     GREEN          = (80, 250, 123);
    YELLOW_RGB,   YELLOW         = (249, 226, 175);
    RED_RGB,      RED            = (243, 139, 168);
}

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Gradient stops for the sweeping progress bar in the flutterff-rs theme
const FLUTTERFF_RAMP: &[Rgb] = &[
    (2, 125, 253),   // #027DFD Flutter Blue
    (0, 180, 216),   // #00B4D8 Deep Cyan
    (0, 229, 255),   // #00E5FF Electric Cyan
    (114, 239, 221), // #72EFDD Aqua Mint
    (255, 255, 255), // #FFFFFF Ice Highlight
    (114, 239, 221), // #72EFDD Aqua Mint
    (0, 229, 255),   // #00E5FF Electric Cyan
    (2, 125, 253),   // #027DFD Flutter Blue
];

/// Deterministic ordered device presets (zero heap allocation)
const DEVICE_PRESETS: &[(&str, (i32, i32))] = &[
    ("mobile", (412, 915)),
    ("mobile-small", (360, 800)),
    ("iphone", (390, 844)),
    ("tablet", (768, 1024)),
    ("desktop", (1280, 800)),
];

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

const CONSOLE_BRIDGE_JS: &str = r#"
(function() {
    ['log','warn','error','info','debug'].forEach(function(level) {
        var orig = console[level];
        console[level] = function() {
            var msg = Array.prototype.slice.call(arguments).join(' ');
            try { window.webkit.messageHandlers.flutterLog.postMessage(msg); } catch(e) {}
            orig.apply(console, arguments);
        };
    });
})();
"#;

fn fg_rgb((r, g, b): Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Blends `fg` toward `bg` by `alpha` (`0.0` = bg, `1.0` = fg), matching cargo-pretty's `fade()`
fn blend(fg: Rgb, bg: Rgb, alpha: f32) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let lerp = |f: u8, b: u8| (b as f32 + (f as f32 - b as f32) * a).round() as u8;
    (lerp(fg.0, bg.0), lerp(fg.1, bg.1), lerp(fg.2, bg.2))
}

fn sample_flutterff_ramp(t: f32) -> Rgb {
    let u = t.rem_euclid(1.0);
    let max_idx = FLUTTERFF_RAMP.len() - 1;
    let pos = u * max_idx as f32;
    let idx = pos.floor() as usize;
    let frac = pos - idx as f32;
    blend(
        FLUTTERFF_RAMP[(idx + 1).min(max_idx)],
        FLUTTERFF_RAMP[idx],
        frac,
    )
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn now_timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn print_tagged_event(tag: &str, tag_color: &str, message: &str) {
    println!(
        "{}{}{} {}{}{} {}{}{}",
        DIM,
        now_timestamp(),
        RESET,
        tag_color,
        tag,
        RESET,
        TEXT,
        message,
        RESET
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  Typed Startup Pipeline & cargo-pretty Style Live Terminal State
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    NetCheck,
    GtkShell,
    FlutterDaemon,
    CompileWeb,
    BindServer,
}

impl Step {
    const TOTAL: usize = 5;

    fn label(self) -> &'static str {
        match self {
            Self::NetCheck => "network check",
            Self::GtkShell => "gtk3 + webkit2 shell",
            Self::FlutterDaemon => "flutter web-server daemon",
            Self::CompileWeb => "compile lib/main.dart (web)",
            Self::BindServer => "bind local web-server",
        }
    }
}

struct PrettyProgress {
    flag_line: String,
    window_label: String,
    mode_label: String,
    seen: usize,
    building: Vec<(Step, String, Instant)>,
    done: Vec<(String, f32)>,
    settled: Option<bool>,
    tui_finished: bool,
    server_url: Option<String>,
    error_msg: Option<String>,
    queued_logs: Vec<String>,
}

impl PrettyProgress {
    fn new(flag_line: String, window_label: String, mode_label: String) -> Self {
        Self {
            flag_line,
            window_label,
            mode_label,
            seen: 0,
            building: Vec::new(),
            done: Vec::new(),
            settled: None,
            tui_finished: false,
            server_url: None,
            error_msg: None,
            queued_logs: Vec::new(),
        }
    }

    fn start_step(&mut self, step: Step, custom_label: Option<String>) {
        if !self.building.iter().any(|(s, _, _)| *s == step) {
            let label = custom_label.unwrap_or_else(|| step.label().to_string());
            self.building.push((step, label, Instant::now()));
        }
    }

    fn finish_step(&mut self, step: Step, done_label: Option<String>) {
        let (default_label, secs) = match self.building.iter().position(|(s, _, _)| *s == step) {
            Some(pos) => {
                let (_, lbl, start) = self.building.remove(pos);
                (lbl, start.elapsed().as_secs_f32())
            }
            None => (step.label().to_string(), 0.01),
        };
        self.seen = (self.seen + 1).min(Step::TOTAL);
        self.done
            .push((done_label.unwrap_or(default_label), secs.max(0.01)));
    }

    fn log_or_print(&mut self, formatted: String) {
        if self.tui_finished {
            println!("{}", formatted);
        } else {
            self.queued_logs.push(formatted);
        }
    }
}

fn spawn_pretty_tui(progress: Arc<Mutex<PrettyProgress>>, started: Instant) {
    thread::spawn(move || {
        let mut drawn_lines: usize = 0;
        let sep = "─".repeat(SEPARATOR_WIDTH);
        print!("\x1b[?25l");
        let _ = std::io::stdout().flush();

        let render_frame = |s: &PrettyProgress, settled_mode: bool, drawn: &mut usize| {
            let elapsed = started.elapsed().as_secs_f32();
            let ratio = if settled_mode && s.settled == Some(true) {
                1.0
            } else {
                (s.seen as f32 / Step::TOTAL as f32).min(1.0)
            };
            let filled = (BAR_WIDTH as f32 * ratio).round() as usize;

            let mut lines: Vec<String> = Vec::with_capacity(20);
            lines.push(format!(
                "🦊 {}{}{}{} {}v{}{}",
                BOLD, CYAN, PKG_NAME, RESET, DIM, VERSION, RESET
            ));
            lines.push(format!("{}{}{}", DIM, s.flag_line, RESET));
            lines.push(format!("{}{}{}", DIM, sep, RESET));
            lines.push(format!("{}Building{}", DIM, RESET));

            let b_count = s.building.len().min(BUILDING_ROWS);
            let b_name_w = s
                .building
                .iter()
                .take(BUILDING_ROWS)
                .map(|(_, n, _)| n.len())
                .max()
                .unwrap_or(0)
                .max(MIN_NAME_WIDTH - 1)
                + 1;

            for i in 0..BUILDING_ROWS {
                match s.building.get(i) {
                    Some((_, name, st)) => {
                        let alpha = (i + 1) as f32 / b_count.max(1) as f32;
                        let dot_c = fg_rgb(blend(CYAN_RGB, BASE_RGB, alpha));
                        let name_c = fg_rgb(blend(SAPPHIRE_RGB, BASE_RGB, alpha));
                        let time_c = fg_rgb(blend(SUBTEXT_RGB, BASE_RGB, alpha));
                        let secs = st.elapsed().as_secs_f32();
                        lines.push(format!(
                            "{}● {}{}{:<width$}{}{}{:05.2}s{}",
                            dot_c,
                            RESET,
                            name_c,
                            name,
                            RESET,
                            time_c,
                            secs,
                            RESET,
                            width = b_name_w
                        ));
                    }
                    None if i == 0 && s.building.is_empty() => {
                        lines.push(format!("{}  …{}", DIM, RESET));
                    }
                    None => lines.push(String::new()),
                }
            }

            lines.push(format!("{}{}{}", DIM, sep, RESET));
            lines.push(format!(
                "{}Completed ({}/{}){}",
                DIM,
                s.seen,
                Step::TOTAL,
                RESET
            ));

            let recent: Vec<&(String, f32)> = s.done.iter().rev().take(DONE_ROWS).collect();
            let r_count = recent.len().max(1);
            lines.extend(std::iter::repeat_n(
                String::new(),
                DONE_ROWS.saturating_sub(recent.len()),
            ));
            let d_name_w = recent
                .iter()
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(0)
                .max(MIN_NAME_WIDTH - 1)
                + 1;

            for (i, (name, secs)) in recent.into_iter().rev().enumerate() {
                let alpha = (i + 1) as f32 / r_count as f32;
                let chk_c = fg_rgb(blend(MINT_RGB, BASE_RGB, alpha));
                let name_c = fg_rgb(blend(TEXT_RGB, BASE_RGB, alpha));
                let time_c = fg_rgb(blend(SUBTEXT_RGB, BASE_RGB, alpha));
                lines.push(format!(
                    "{}✓ {}{}{:<width$}{}{}{:05.2}s{}",
                    chk_c,
                    RESET,
                    name_c,
                    name,
                    RESET,
                    time_c,
                    secs,
                    RESET,
                    width = d_name_w
                ));
            }

            lines.push(String::new());

            let bar_filled = if settled_mode {
                let col = if s.settled == Some(true) { GREEN } else { RED };
                format!("{}{}{}", col, "█".repeat(filled), RESET)
            } else {
                let phase = elapsed * 0.9;
                let mut buf: String = (0..filled)
                    .map(|bi| {
                        let u = (bi as f32 / BAR_WIDTH as f32) - phase;
                        format!("{}█", fg_rgb(sample_flutterff_ramp(u)))
                    })
                    .collect();
                buf.push_str(RESET);
                buf
            };
            let bar_empty = format!(
                "{}{}{}",
                fg_rgb(SURFACE1_RGB),
                "░".repeat(BAR_WIDTH.saturating_sub(filled)),
                RESET
            );

            lines.push(format!(
                "{}{:<14}{}{}{}{}{:>5.0}%{}",
                DIM,
                "Launching",
                RESET,
                bar_filled,
                bar_empty,
                TEXT,
                ratio * 100.0,
                RESET
            ));
            lines.push(format!(
                "{}{:<14}{}{}{:.2}s{}",
                DIM, "Elapsed", RESET, TEXT, elapsed, RESET
            ));

            let mut out = String::new();
            if *drawn > 0 {
                out.push_str(&format!("\x1b[{}A", *drawn));
            }
            for l in &lines {
                out.push_str(&format!("\r\x1b[2K{}\n", l));
            }
            print!("{}", out);
            let _ = std::io::stdout().flush();
            *drawn = lines.len();
        };

        loop {
            let is_settled = {
                let guard = progress.lock().unwrap();
                render_frame(&guard, false, &mut drawn_lines);
                guard.settled.is_some()
            };
            if is_settled {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        for _ in 0..4 {
            {
                let guard = progress.lock().unwrap();
                render_frame(&guard, true, &mut drawn_lines);
            }
            thread::sleep(Duration::from_millis(40));
        }

        if drawn_lines > 0 {
            print!("\x1b[{}A", drawn_lines);
            for _ in 0..drawn_lines {
                print!("\r\x1b[2K\n");
            }
            print!("\x1b[{}A", drawn_lines);
        }
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();

        let total_secs = started.elapsed().as_secs_f32();
        let mut guard = progress.lock().unwrap();
        let ok = guard.settled.unwrap_or(false);
        let srv = guard
            .server_url
            .clone()
            .unwrap_or_else(|| "stopped".to_string());

        if ok {
            println!(
                "{}{}✓ {} v{} serving in {:.2}s{}",
                GREEN, BOLD, PKG_NAME, VERSION, total_secs, RESET
            );
            println!(
                "{}  {:<15}{}{}{}",
                DIM, "window", RESET, TEXT, guard.window_label
            );
            println!("{}  {:<15}{}{}{}", DIM, "server", RESET, CYAN, srv);
            println!(
                "{}  {:<15}{}{}{}",
                DIM, "mode", RESET, TEXT, guard.mode_label
            );
            println!("{}{}{}", DIM, sep, RESET);
        } else {
            println!(
                "{}{}✗ {} failed after {:.2}s{}",
                RED, BOLD, PKG_NAME, total_secs, RESET
            );
            if let Some(err) = &guard.error_msg {
                eprintln!("  {}{}{}", RED, err, RESET);
            }
        }

        for log in guard.queued_logs.drain(..) {
            println!("{}", log);
        }
        guard.tui_finished = true;
    });
}

// ─────────────────────────────────────────────────────────────────────────────
//  Pure Algorithmic Helpers (Zero Regex, Zero Redundant Allocations)
// ─────────────────────────────────────────────────────────────────────────────
fn find_preset(name: &str) -> Option<(i32, i32)> {
    DEVICE_PRESETS
        .iter()
        .find(|&&(preset, _)| preset.eq_ignore_ascii_case(name))
        .map(|&(_, dims)| dims)
}

fn parse_size(s: &str) -> Result<(i32, i32), String> {
    let (w_str, h_str) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("Invalid size '{}'. Use WxH e.g. 390x844", s))?;
    let w = w_str
        .parse::<i32>()
        .map_err(|_| format!("Invalid width: {}", w_str))?;
    let h = h_str
        .parse::<i32>()
        .map_err(|_| format!("Invalid height: {}", h_str))?;
    if w <= 0 || h <= 0 {
        return Err(format!("Dimensions must be positive: {}x{}", w, h));
    }
    Ok((w, h))
}

fn is_port_free(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

fn find_free_port(start: u16) -> u16 {
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

fn check_online() -> bool {
    PROBE_ENDPOINTS
        .iter()
        .any(|addr| TcpStream::connect_timeout(addr, Duration::from_secs(2)).is_ok())
}

/// O(n) single-pass ANSI escape sequence stripper (no regex needed)
fn strip_ansi(s: &str) -> String {
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
fn extract_local_url(line: &str) -> Option<String> {
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
fn extract_url_port(url: &str, fallback_port: u16) -> u16 {
    url.split(':')
        .nth(2)
        .and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u16>().ok()
        })
        .unwrap_or(fallback_port)
}

/// Table-driven log level detector operating on an already-lowercased slice
fn detect_level(lo: &str) -> (&'static str, &'static str) {
    LEVEL_RULES
        .iter()
        .find(|&&(patterns, _)| contains_any(lo, patterns))
        .map(|&(_, level)| level)
        .unwrap_or(("INF", BLUE))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogSource {
    Flutter,
    WebView,
}

fn format_flutter_log(line: &str, source: LogSource) -> Option<String> {
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

fn run_flutter(
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
    print!("\x1b[?25h");
    let _ = std::io::stdout().flush();
    glib::idle_add(|| {
        gtk::main_quit();
        glib::ControlFlow::Break
    });
}

fn take_screenshot(webview: &WebView) {
    let wv = webview.clone();
    wv.queue_draw();
    glib::timeout_add_local(Duration::from_millis(300), move || {
        let root = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let shots_dir = root.join("screenshots");
        let _ = std::fs::create_dir_all(&shots_dir);

        let fname = chrono::Local::now()
            .format("screenshot_%Y%m%d_%H%M%S.png")
            .to_string();
        let fpath = shots_dir.join(&fname);

        let alloc = wv.allocation();
        let (w, h) = (alloc.width(), alloc.height());

        if w > 0
            && h > 0
            && let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)
            && let Ok(cr) = cairo::Context::new(&surface)
        {
            wv.draw(&cr);
            if let Ok(mut file) = std::fs::File::create(&fpath)
                && surface.write_to_png(&mut file).is_ok()
            {
                print_tagged_event(
                    "SCR",
                    GREEN,
                    &format!(
                        "saved → screenshots/{} {}({}×{}){}",
                        fname, SUB, w, h, RESET
                    ),
                );
            }
        } else {
            print_tagged_event("ERR", RED, "invalid webview size");
        }
        glib::ControlFlow::Break
    });
}

// ─────────────────────────────────────────────────────────────────────────────
//  Iterator-Based CLI Argument Parser
// ─────────────────────────────────────────────────────────────────────────────
enum CliAction {
    Run(CliConfig),
    ShowVersion,
    ShowHelp,
    ListSizes,
}

struct CliConfig {
    port: u16,
    no_hot: bool,
    profile: bool,
    offline: bool,
    flavor: Option<String>,
    size_str: String,
}

impl CliConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<CliAction, String> {
        let mut cfg = Self {
            port: DEFAULT_PORT,
            no_hot: false,
            profile: false,
            offline: false,
            flavor: None,
            size_str: DEFAULT_PRESET.to_string(),
        };

        let mut iter = args.into_iter().skip(1);
        while let Some(arg) = iter.next() {
            let (flag, inline_val) = match arg.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (arg.as_str(), None),
            };

            let mut next_val = |flag_name: &str| -> Result<String, String> {
                inline_val
                    .clone()
                    .or_else(|| iter.next())
                    .ok_or_else(|| format!("Missing value for '{}'", flag_name))
            };

            match flag {
                "--version" | "-V" => return Ok(CliAction::ShowVersion),
                "--help" | "-h" => return Ok(CliAction::ShowHelp),
                "--list-sizes" => return Ok(CliAction::ListSizes),
                "--no-hot" => cfg.no_hot = true,
                "--profile" => cfg.profile = true,
                "--offline" => cfg.offline = true,
                "--port" | "-p" => {
                    let val = next_val(flag)?;
                    cfg.port = val
                        .parse::<u16>()
                        .map_err(|_| format!("Invalid port '{}': expected 1..65535", val))?;
                }
                "--size" | "-s" => cfg.size_str = next_val(flag)?,
                "--flavor" => cfg.flavor = Some(next_val(flag)?),
                other => return Err(format!("Unknown option '{}'. Run with --help.", other)),
            }
        }

        Ok(CliAction::Run(cfg))
    }
}

fn print_help() {
    println!(
        "\n🦊 {}{}{}{} {}v{}{}\n",
        BOLD, CYAN, PKG_NAME, RESET, DIM, VERSION, RESET
    );
    println!("{}USAGE:{}", BOLD, RESET);
    println!("  {} [OPTIONS]\n", PKG_NAME);
    println!("{}OPTIONS:{}", BOLD, RESET);
    let opts = [
        (
            "-s, --size <PRESET|WxH>",
            "Viewport size preset or custom WxH (default: mobile)",
        ),
        (
            "-p, --port <PORT>",
            "Web-server HTTP port (default: 8080, auto-increments)",
        ),
        (
            "    --offline",
            "Force offline mode (--no-pub --no-web-resources-cdn)",
        ),
        ("    --profile", "Run Flutter in profile mode"),
        ("    --flavor <NAME>", "Build a custom Flutter app flavor"),
        ("    --no-hot", "Disable hot reload support"),
        ("    --list-sizes", "Display all built-in device presets"),
        ("-V, --version", "Print version information"),
        ("-h, --help", "Print this help message"),
    ];
    for (flag, desc) in opts {
        println!("  {}{:<26}{} {}{}{}", CYAN, flag, RESET, TEXT, desc, RESET);
    }
    println!();
}

fn print_size_presets() {
    println!(
        "\n🦊 {}{}{}{} {}— size presets{}",
        BOLD, CYAN, PKG_NAME, RESET, DIM, RESET
    );
    println!("{}{}{}", DIM, "─".repeat(SEPARATOR_WIDTH), RESET);
    for &(name, (w, h)) in DEVICE_PRESETS {
        let tag = if name == DEFAULT_PRESET {
            format!(" {}← default{}", SUB, RESET)
        } else {
            String::new()
        };
        println!(
            " {}{:<15}{} {}{}×{}{}{}",
            CYAN, name, RESET, TEXT, w, h, RESET, tag
        );
    }
    println!(
        " {}{:<15}{} {}--size 430x932{}",
        CYAN, "custom", RESET, SUB, RESET
    );
}

fn main() {
    let started = Instant::now();

    let mut cfg = match CliConfig::parse(env::args()) {
        Ok(CliAction::ShowVersion) => {
            println!("{} v{}", PKG_NAME, VERSION);
            return;
        }
        Ok(CliAction::ShowHelp) => {
            print_help();
            return;
        }
        Ok(CliAction::ListSizes) => {
            print_size_presets();
            return;
        }
        Ok(CliAction::Run(c)) => c,
        Err(e) => {
            eprintln!("{}{}{}", RED, e, RESET);
            std::process::exit(1);
        }
    };

    let (width, height, size_label) = match find_preset(&cfg.size_str) {
        Some((w, h)) => (w, h, cfg.size_str.as_str()),
        None => match parse_size(&cfg.size_str) {
            Ok((w, h)) => (w, h, "custom"),
            Err(e) => {
                eprintln!("{}{}{}", RED, e, RESET);
                std::process::exit(1);
            }
        },
    };

    if !is_port_free(cfg.port) {
        cfg.port = find_free_port(cfg.port.saturating_add(1));
    }

    let net_check_start = Instant::now();
    if !cfg.offline && !check_online() {
        cfg.offline = true;
    }
    let net_check_secs = net_check_start.elapsed().as_secs_f32();

    let mut flutter_cmd = vec![
        "flutter".to_string(),
        "run".to_string(),
        "-d".to_string(),
        "web-server".to_string(),
        format!("--web-port={}", cfg.port),
    ];

    if cfg.profile {
        flutter_cmd.push("--profile".to_string());
    }
    if cfg.no_hot {
        flutter_cmd.push("--no-hot".to_string());
    }
    if let Some(f) = &cfg.flavor {
        flutter_cmd.extend(["--flavor".to_string(), f.clone()]);
    }
    if cfg.offline {
        flutter_cmd.extend(["--no-pub".to_string(), "--no-web-resources-cdn".to_string()]);
    }

    let flag_line = flutter_cmd.join(" ");
    let window_label = format!("{}×{} ({})", width, height, size_label);
    let mode_label = format!(
        "{} · {}",
        if cfg.profile { "profile" } else { "debug" },
        if cfg.offline { "offline" } else { "online" }
    );

    let progress = Arc::new(Mutex::new(PrettyProgress::new(
        flag_line,
        window_label,
        mode_label,
    )));

    {
        let mut p = progress.lock().unwrap();
        p.seen = 1;
        p.done.push((
            format!(
                "{} ({})",
                Step::NetCheck.label(),
                if cfg.offline { "offline" } else { "online" }
            ),
            net_check_secs.max(0.01),
        ));
        p.start_step(Step::GtkShell, None);
    }

    spawn_pretty_tui(progress.clone(), started);

    gtk::init().expect("Failed to initialize GTK");

    // Initialize WebKit2 context, JS console bridge, and WebView first so UI controls capture it directly
    let context = WebContext::default().unwrap();
    context.set_cache_model(CacheModel::DocumentViewer);

    let manager = UserContentManager::new();
    manager.register_script_message_handler("flutterLog");

    let prog_js = progress.clone();
    manager.connect_script_message_received(None, move |_, value| {
        if let Some(js_value) = value.js_value() {
            let text = js_value.to_string();
            if let Some(out) = format_flutter_log(&text, LogSource::WebView) {
                prog_js.lock().unwrap().log_or_print(out);
            }
        }
    });

    let console_script = UserScript::new(
        CONSOLE_BRIDGE_JS,
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    );
    manager.add_script(&console_script);

    let webview = WebView::new_with_context_and_user_content_manager(&context, &manager);
    webview.load_uri("about:blank");
    webview.connect_context_menu(|_, _, _, _| true);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("flutterff");
    window.set_default_size(width, height);
    window.set_resizable(true);

    let hb = HeaderBar::new();
    hb.set_show_close_button(true);
    hb.set_title(Some("flutterff"));
    hb.set_decoration_layout(Some("menu:close"));
    window.set_titlebar(Some(&hb));

    let size_btn = MenuButton::new();
    size_btn.set_image(Some(&gtk::Image::from_icon_name(
        Some("view-fullscreen-symbolic"),
        gtk::IconSize::Menu,
    )));
    size_btn.set_tooltip_text(Some("Change Device Size"));

    let menu = Menu::new();
    for &(name, (w, h)) in DEVICE_PRESETS {
        let label = format!("{} ({}×{})", name.replace('-', " ").to_uppercase(), w, h);
        let item = MenuItem::with_label(&label);
        let win = window.clone();
        let wv = webview.clone();
        item.connect_activate(move |_| {
            print_tagged_event(
                "WIN",
                CYAN,
                &format!("{} {}({}×{}){}", name, SUB, w, h, RESET),
            );
            win.resize(w, h);
            wv.queue_resize();
            let wv_clone = wv.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                wv_clone.queue_draw();
                glib::ControlFlow::Break
            });
        });
        menu.append(&item);
    }
    menu.show_all();
    size_btn.set_popup(Some(&menu));
    hb.pack_start(&size_btn);

    let shot_btn = gtk::Button::new();
    shot_btn.set_image(Some(&gtk::Image::from_icon_name(
        Some("camera-photo-symbolic"),
        gtk::IconSize::Menu,
    )));
    shot_btn.set_tooltip_text(Some("Screenshot (screenshots/)"));
    let wv_shot = webview.clone();
    shot_btn.connect_clicked(move |_| {
        take_screenshot(&wv_shot);
    });
    hb.pack_start(&shot_btn);

    let reload_btn = gtk::Button::with_label("🗲");
    reload_btn.set_tooltip_text(Some("Hot Reload (r)"));
    hb.pack_end(&reload_btn);

    let restart_btn = gtk::Button::new();
    restart_btn.set_image(Some(&gtk::Image::from_icon_name(
        Some("view-refresh-symbolic"),
        gtk::IconSize::Menu,
    )));
    restart_btn.set_tooltip_text(Some("Hot Restart (R)"));
    hb.pack_end(&restart_btn);

    let vbox = GtkBox::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&webview, true, true, 0);
    window.add(&vbox);

    let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let stdin_slot: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));
    let current_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Hot reload
    {
        let stdin_r = stdin_slot.clone();
        reload_btn.connect_clicked(move |_| {
            if let Some(stdin) = &mut *stdin_r.lock().unwrap() {
                let _ = stdin.write_all(b"r\n");
                let _ = stdin.flush();
            }
            print_tagged_event("HOT", CYAN, "reload");
        });
    }

    // Hot restart
    {
        let url_r = current_url.clone();
        let wv = webview.clone();
        restart_btn.connect_clicked(move |_| {
            print_tagged_event("HOT", CYAN, "restart · reloading webview...");
            let wv2 = wv.clone();
            let url2 = url_r.clone();
            glib::timeout_add_local(Duration::from_millis(1500), move || {
                match &*url2.lock().unwrap() {
                    Some(url) => wv2.load_uri(url),
                    None => wv2.reload(),
                }
                glib::ControlFlow::Break
            });
        });
    }

    // Graceful daemon quit ('q\n') + process cleanup on window close
    let child_destroy = child_slot.clone();
    let stdin_destroy = stdin_slot.clone();
    window.connect_destroy(move |_| {
        if let Some(mut stdin) = stdin_destroy.lock().unwrap().take() {
            let _ = stdin.write_all(b"q\n");
            let _ = stdin.flush();
        }
        if let Some(mut child) = child_destroy.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
        gtk::main_quit();
    });

    window.show_all();

    progress.lock().unwrap().finish_step(
        Step::GtkShell,
        Some(format!("{} ({}×{})", Step::GtkShell.label(), width, height)),
    );

    let (url_tx, url_rx) = mpsc::channel();
    let c_slot = child_slot.clone();
    let s_slot = stdin_slot.clone();
    let prog_fl = progress.clone();
    let port = cfg.port;
    thread::spawn(move || {
        run_flutter(flutter_cmd, port, url_tx, c_slot, s_slot, prog_fl);
    });

    let current_url_clone = current_url.clone();
    let wv_load = webview.clone();
    let prog_srv = progress.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        match url_rx.try_recv() {
            Ok(url) => {
                *current_url_clone.lock().unwrap() = Some(url.clone());
                let target_port = extract_url_port(&url, port);
                let target_addr =
                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, target_port));
                let wv = wv_load.clone();
                let prog_ready = prog_srv.clone();

                glib::timeout_add_local(Duration::from_millis(250), move || {
                    if TcpStream::connect_timeout(&target_addr, Duration::from_millis(150)).is_ok()
                    {
                        {
                            let mut pr = prog_ready.lock().unwrap();
                            pr.finish_step(
                                Step::BindServer,
                                Some(format!("bind http://localhost:{}", target_port)),
                            );
                            pr.server_url = Some(url.clone());
                            pr.settled = Some(true);
                        }
                        wv.load_uri(&url);
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                });
                // Stop the outer channel polling timer once the URL has arrived
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });

    gtk::main();
}
