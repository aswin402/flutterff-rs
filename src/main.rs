use gtk::prelude::*;
use gtk::{Box as GtkBox, HeaderBar, Menu, MenuButton, MenuItem};
use webkit2gtk::{
    CacheModel, UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript,
    UserScriptInjectionTime, WebContext, WebContextExt, WebView, WebViewExt, WebViewExtManual,
};

use chrono;
use glib;
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const VERSION: &str = "2.5.1";

// ─────────────────────────────────────────────────────────────────────────────
//  flutterff-rs × cargo-pretty 24-bit Truecolor Theme Palette
// ─────────────────────────────────────────────────────────────────────────────
type Rgb = (u8, u8, u8);

const BASE: Rgb = (30, 30, 46);
const TEXT_RGB: Rgb = (205, 214, 244);
const SUBTEXT0: Rgb = (166, 173, 200);
const SURFACE1: Rgb = (69, 71, 90);
const DIM_RGB: Rgb = (108, 112, 134);
const CYAN_RGB: Rgb = (0, 229, 255);
const SAPPHIRE_RGB: Rgb = (56, 189, 248);
const MINT_RGB: Rgb = (80, 250, 123);
const YELLOW_RGB: Rgb = (249, 226, 175);
const RED_RGB: Rgb = (243, 139, 168);

const GREEN: &str = "\x1b[38;2;80;250;123m";
const YELLOW: &str = "\x1b[38;2;249;226;175m";
const CYAN: &str = "\x1b[38;2;0;229;255m";
const RED: &str = "\x1b[38;2;243;139;168m";
const BLUE: &str = "\x1b[38;2;56;189;248m";
const TEXT: &str = "\x1b[38;2;205;214;244m";
const SUB: &str = "\x1b[38;2;166;173;200m";
const DIM: &str = "\x1b[38;2;108;112;134m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

const BAR_WIDTH: usize = 28;
const MIN_NAME_WIDTH: usize = 32;
const BUILDING_ROWS: usize = 4;
const DONE_ROWS: usize = 5;

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

fn fg_rgb((r, g, b): Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Blends `fg` toward `bg` by `alpha` (`0.0` = bg, `1.0` = fg), matching cargo-pretty's `fade()`
fn blend(fg: Rgb, bg: Rgb, alpha: f32) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let r = (bg.0 as f32 + (fg.0 as f32 - bg.0 as f32) * a).round() as u8;
    let g = (bg.1 as f32 + (fg.1 as f32 - bg.1 as f32) * a).round() as u8;
    let b = (bg.2 as f32 + (fg.2 as f32 - bg.2 as f32) * a).round() as u8;
    (r, g, b)
}

fn sample_flutterff_ramp(t: f32) -> Rgb {
    let u = t.rem_euclid(1.0);
    let pos = u * (FLUTTERFF_RAMP.len() - 1) as f32;
    let idx = pos.floor() as usize;
    let frac = pos - idx as f32;
    let c1 = FLUTTERFF_RAMP[idx];
    let c2 = FLUTTERFF_RAMP[(idx + 1).min(FLUTTERFF_RAMP.len() - 1)];
    blend(c2, c1, frac)
}

macro_rules! any {
    ($text:expr, $patterns:expr) => {
        $patterns.iter().any(|p| $text.contains(p))
    };
}

// ─────────────────────────────────────────────────────────────────────────────
//  cargo-pretty Style Live Terminal State
// ─────────────────────────────────────────────────────────────────────────────
struct PrettyProgress {
    flag_line: String,
    window_label: String,
    mode_label: String,
    total: usize,
    seen: usize,
    building: Vec<(String, Instant)>,
    done: Vec<(String, f32)>,
    settled: Option<bool>,
    tui_finished: bool,
    server_url: Option<String>,
    error_msg: Option<String>,
    queued_logs: Vec<String>,
}

impl PrettyProgress {
    fn new(flag_line: String, window_label: String, mode_label: String, total: usize) -> Self {
        Self {
            flag_line,
            window_label,
            mode_label,
            total,
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

    fn start_step(&mut self, name: &str) {
        if !self.building.iter().any(|(n, _)| n == name) {
            self.building.push((name.to_string(), Instant::now()));
        }
    }

    fn finish_step(&mut self, name: &str, display_done: Option<&str>) {
        let secs = match self.building.iter().position(|(n, _)| n == name) {
            Some(pos) => self.building.remove(pos).1.elapsed().as_secs_f32(),
            None => 0.01,
        };
        self.seen = (self.seen + 1).min(self.total);
        let label = display_done.unwrap_or(name).to_string();
        self.done.push((label, secs));
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
        print!("\x1b[?25l");
        let _ = std::io::stdout().flush();

        let render_frame = |s: &PrettyProgress, settled_mode: bool, drawn: &mut usize| {
            let elapsed = started.elapsed().as_secs_f32();
            let ratio = if settled_mode && s.settled == Some(true) {
                1.0
            } else {
                (s.seen as f32 / s.total.max(1) as f32).min(1.0)
            };
            let filled = (BAR_WIDTH as f32 * ratio).round() as usize;

            let mut lines: Vec<String> = Vec::with_capacity(20);
            lines.push(format!(
                "🦊 {}{}flutterff-rs{} {}v{}{}",
                BOLD, CYAN, RESET, DIM, VERSION, RESET
            ));
            lines.push(format!("{}{}{}", DIM, s.flag_line, RESET));
            lines.push(format!("{}{}{}", DIM, "─".repeat(40), RESET));
            lines.push(format!("{}Building{}", DIM, RESET));

            let b_count = s.building.len().min(BUILDING_ROWS);
            let b_name_w = s
                .building
                .iter()
                .take(BUILDING_ROWS)
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(0)
                .max(MIN_NAME_WIDTH - 1)
                + 1;

            for i in 0..BUILDING_ROWS {
                if let Some((name, st)) = s.building.get(i) {
                    let alpha = (i + 1) as f32 / b_count.max(1) as f32;
                    let dot_c = fg_rgb(blend(CYAN_RGB, BASE, alpha));
                    let name_c = fg_rgb(blend(SAPPHIRE_RGB, BASE, alpha));
                    let time_c = fg_rgb(blend(SUBTEXT0, BASE, alpha));
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
                } else if i == 0 && s.building.is_empty() {
                    lines.push(format!("{}  …{}", DIM, RESET));
                } else {
                    lines.push(String::new());
                }
            }

            lines.push(format!("{}{}{}", DIM, "─".repeat(40), RESET));
            lines.push(format!(
                "{}Completed ({}/{}){}",
                DIM, s.seen, s.total, RESET
            ));

            let recent: Vec<&(String, f32)> = s.done.iter().rev().take(DONE_ROWS).collect();
            let r_count = recent.len().max(1);
            for _ in 0..DONE_ROWS.saturating_sub(recent.len()) {
                lines.push(String::new());
            }
            let d_name_w = recent
                .iter()
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(0)
                .max(MIN_NAME_WIDTH - 1)
                + 1;

            for (i, (name, secs)) in recent.into_iter().rev().enumerate() {
                let alpha = (i + 1) as f32 / r_count as f32;
                let chk_c = fg_rgb(blend(MINT_RGB, BASE, alpha));
                let name_c = fg_rgb(blend(TEXT_RGB, BASE, alpha));
                let time_c = fg_rgb(blend(SUBTEXT0, BASE, alpha));
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
                let col = if s.settled == Some(true) {
                    fg_rgb(MINT_RGB)
                } else {
                    fg_rgb(RED_RGB)
                };
                format!("{}{}{}", col, "█".repeat(filled), RESET)
            } else {
                let phase = elapsed * 0.9;
                let mut buf = String::new();
                for bi in 0..filled {
                    let u = (bi as f32 / BAR_WIDTH as f32) - phase;
                    let c = sample_flutterff_ramp(u);
                    buf.push_str(&fg_rgb(c));
                    buf.push('█');
                }
                buf.push_str(RESET);
                buf
            };
            let bar_empty = format!(
                "{}{}{}",
                fg_rgb(SURFACE1),
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

        // Post-build settle animation frames (like cargo-pretty SETTLE_STEP)
        for _ in 0..4 {
            {
                let guard = progress.lock().unwrap();
                render_frame(&guard, true, &mut drawn_lines);
            }
            thread::sleep(Duration::from_millis(40));
        }

        // Clear inline TUI and print cargo-pretty summary_block
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
                "{}{}✓ flutterff-rs v{} serving in {:.2}s{}",
                GREEN, BOLD, VERSION, total_secs, RESET
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
            println!("{}{}{}", DIM, "─".repeat(40), RESET);
        } else {
            println!(
                "{}{}✗ flutterff-rs failed after {:.2}s{}",
                RED, BOLD, total_secs, RESET
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

fn device_presets() -> HashMap<&'static str, (i32, i32)> {
    HashMap::from([
        ("mobile", (412, 915)),
        ("mobile-small", (360, 800)),
        ("iphone", (390, 844)),
        ("tablet", (768, 1024)),
        ("desktop", (1280, 800)),
    ])
}

fn parse_size(s: &str) -> Result<(i32, i32), String> {
    let lower = s.to_lowercase();
    let parts: Vec<&str> = lower.splitn(2, 'x').collect();
    if parts.len() == 2 {
        let w = parts[0]
            .parse::<i32>()
            .map_err(|_| format!("Invalid width: {}", parts[0]))?;
        let h = parts[1]
            .parse::<i32>()
            .map_err(|_| format!("Invalid height: {}", parts[1]))?;
        Ok((w, h))
    } else {
        Err(format!("Invalid size '{}'. Use WxH e.g. 390x844", s))
    }
}

fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

fn find_free_port(start: u16) -> u16 {
    for port in start..8200 {
        if is_port_free(port) {
            return port;
        }
    }
    eprintln!("{}No free port found{}", RED, RESET);
    std::process::exit(1);
}

fn check_online() -> bool {
    TcpStream::connect_timeout(&"8.8.8.8:53".parse().unwrap(), Duration::from_secs(2)).is_ok()
}

fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").trim().to_string()
}

fn detect_level(text: &str) -> (&'static str, &'static str) {
    let lo = text.to_lowercase();
    if any!(lo, ["error:", "exception:", "fatal:", "unhandled"]) {
        ("ERR", RED)
    } else if any!(lo, ["warning:", "warn:", "deprecated"]) {
        ("WRN", YELLOW)
    } else if lo.contains("debug:") {
        ("DBG", DIM)
    } else {
        ("INF", BLUE)
    }
}

fn format_flutter_log(line: &str, source: &str) -> Option<String> {
    let lo_raw = line.to_lowercase();

    // Filter out the hot restart error completely
    if lo_raw.contains("surface instance already deleted") {
        return None;
    }

    if any!(
        lo_raw,
        [
            "http://",
            "https://",
            ".js:",
            "console",
            "flutter_bootstrap",
            "ddc_module_loader",
            "dart_sdk",
            "web_entrypoint"
        ]
    ) {
        return None;
    }

    let text = strip_ansi(line);
    if text.trim().is_empty() {
        return None;
    }

    let ts = chrono::Local::now().format("%H:%M:%S").to_string();
    let lo = text.to_lowercase();

    if lo.contains("flutter:") {
        let msg = text
            .splitn(2, "flutter:")
            .nth(1)
            .unwrap_or(&text)
            .trim()
            .to_string();
        if msg.is_empty() {
            return None;
        }
        let (tag, color) = detect_level(&msg);
        Some(format!(
            "{}{}{} {}{}{} {}{}{}",
            DIM, ts, RESET, color, tag, RESET, TEXT, msg, RESET
        ))
    } else if source == "webview" {
        let (tag, color) = detect_level(&text);
        Some(format!(
            "{}{}{} {}{}{} {}{}{}",
            DIM, ts, RESET, color, tag, RESET, TEXT, text, RESET
        ))
    } else if any!(lo, ["error:", "exception:", "fatal:", "══╡", "══╞"])
        && !lo_raw.contains("surface instance")
    {
        Some(format!(
            "{}{}{} {}ERR{} {}{}{}",
            DIM, ts, RESET, RED, RESET, TEXT, text, RESET
        ))
    } else {
        None
    }
}

fn run_flutter(
    cmd: Vec<String>,
    port: u16,
    url_tx: mpsc::Sender<String>,
    child_slot: Arc<Mutex<Option<Child>>>,
    stdin_slot: Arc<Mutex<Option<ChildStdin>>>,
    progress: Arc<Mutex<PrettyProgress>>,
) {
    let pattern = Regex::new(r#"(http://(?:localhost|127\.0\.0\.1):\d+\S*)"#).unwrap();
    let mut url_sent = false;

    {
        let mut p = progress.lock().unwrap();
        p.start_step("flutter web-server daemon");
    }

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
        p.finish_step("flutter web-server daemon", None);
        p.start_step("compile lib/main.dart (web)");
    }

    *stdin_slot.lock().unwrap() = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    *child_slot.lock().unwrap() = Some(child);

    let prog_err = progress.clone();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            if let Ok(l) = line {
                if let Some(out) = format_flutter_log(&l, "flutter") {
                    prog_err.lock().unwrap().log_or_print(out);
                }
            }
        }
    });

    for line in BufReader::new(stdout).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if !url_sent {
            if let Some(cap) = pattern.captures(&line) {
                if let Some(m) = cap.get(1) {
                    url_sent = true;
                    let url_str = m.as_str().to_string();
                    {
                        let mut p = progress.lock().unwrap();
                        p.finish_step("compile lib/main.dart (web)", None);
                        p.start_step(&format!("bind http://localhost:{}", port));
                    }
                    let _ = url_tx.send(url_str);
                }
            } else if line.to_lowercase().contains("serving")
                || line.to_lowercase().contains("listening")
            {
                url_sent = true;
                let url_str = format!("http://localhost:{}", port);
                {
                    let mut p = progress.lock().unwrap();
                    p.finish_step("compile lib/main.dart (web)", None);
                    p.start_step(&format!("bind http://localhost:{}", port));
                }
                let _ = url_tx.send(url_str);
            }
        }

        if let Some(out) = format_flutter_log(&line, "flutter") {
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
        let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let shots_dir = root.join("screenshots");
        let _ = std::fs::create_dir_all(&shots_dir);

        let fname = chrono::Local::now()
            .format("screenshot_%Y%m%d_%H%M%S.png")
            .to_string();
        let fpath = shots_dir.join(&fname);

        let alloc = wv.allocation();
        let w = alloc.width();
        let h = alloc.height();

        if w > 0 && h > 0 {
            if let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h) {
                if let Ok(cr) = cairo::Context::new(&surface) {
                    wv.draw(&cr);
                    if let Ok(mut file) = std::fs::File::create(&fpath) {
                        if surface.write_to_png(&mut file).is_ok() {
                            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
                            println!(
                                "{}{}{} {}SCR{} {}saved → screenshots/{}{} ({}×{})",
                                DIM, ts, RESET, GREEN, RESET, TEXT, fname, SUB, w, h
                            );
                        }
                    }
                }
            }
        } else {
            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
            println!(
                "{}{}{} {}ERR{} invalid webview size",
                DIM, ts, RESET, RED, RESET
            );
        }
        glib::ControlFlow::Break
    });
}

fn main() {
    let started = Instant::now();
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 8080;
    let mut no_hot = false;
    let mut profile = false;
    let mut offline = false;
    let mut flavor: Option<String> = None;
    let mut size_str = "mobile".to_string();
    let mut list_sizes = false;
    let mut show_ver = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().unwrap_or(8080);
                }
            }
            "--no-hot" => no_hot = true,
            "--profile" => profile = true,
            "--offline" => offline = true,
            "--flavor" => {
                i += 1;
                if i < args.len() {
                    flavor = Some(args[i].clone());
                }
            }
            "--size" | "-s" => {
                i += 1;
                if i < args.len() {
                    size_str = args[i].clone();
                }
            }
            "--list-sizes" => list_sizes = true,
            "--version" => show_ver = true,
            _ => {}
        }
        i += 1;
    }

    if show_ver {
        println!("flutterff-rs v{}", VERSION);
        return;
    }

    let presets = device_presets();

    if list_sizes {
        println!(
            "\n🦊 {}{}flutterff-rs{} {}— size presets{}",
            BOLD, CYAN, RESET, DIM, RESET
        );
        println!("{}{}{}", DIM, "─".repeat(40), RESET);
        for (name, (w, h)) in &presets {
            let tag = if *name == "mobile" {
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
        return;
    }

    let (width, height) = match presets.get(size_str.as_str()) {
        Some(&(w, h)) => (w, h),
        None => match parse_size(&size_str) {
            Ok(wh) => wh,
            Err(e) => {
                eprintln!("{}{}{}", RED, e, RESET);
                return;
            }
        },
    };

    if !is_port_free(port) {
        port = find_free_port(port + 1);
    }

    let size_label = if presets.contains_key(size_str.as_str()) {
        size_str.as_str()
    } else {
        "custom"
    };

    let net_check_start = Instant::now();
    if !offline && !check_online() {
        offline = true;
    }
    let net_check_secs = net_check_start.elapsed().as_secs_f32();

    let mut flutter_cmd = vec![
        "flutter".to_string(),
        "run".to_string(),
        "-d".to_string(),
        "web-server".to_string(),
        format!("--web-port={}", port),
    ];

    if profile {
        flutter_cmd.push("--profile".to_string());
    }
    if no_hot {
        flutter_cmd.push("--no-hot".to_string());
    }
    if let Some(f) = &flavor {
        flutter_cmd.extend(vec!["--flavor".to_string(), f.clone()]);
    }
    if offline {
        flutter_cmd.extend(vec![
            "--no-pub".to_string(),
            "--no-web-resources-cdn".to_string(),
        ]);
    }

    let flag_line = flutter_cmd.join(" ");
    let window_label = format!("{}×{} ({})", width, height, size_label);
    let mode_label = format!(
        "{} · {}",
        if profile { "profile" } else { "debug" },
        if offline { "offline" } else { "online" }
    );

    let progress = Arc::new(Mutex::new(PrettyProgress::new(
        flag_line,
        window_label,
        mode_label,
        5,
    )));

    {
        let mut p = progress.lock().unwrap();
        p.seen = 1;
        p.done.push((
            format!(
                "network check ({})",
                if offline { "offline" } else { "online" }
            ),
            net_check_secs.max(0.01),
        ));
        p.start_step("gtk3 + webkit2 shell");
    }

    spawn_pretty_tui(progress.clone(), started);

    gtk::init().expect("Failed to initialize GTK");

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
    let menu_wv_slot: Arc<Mutex<Option<WebView>>> = Arc::new(Mutex::new(None));
    for (name, &(w, h)) in &presets {
        let label = format!("{} ({}×{})", name.replace('-', " ").to_uppercase(), w, h);
        let item = MenuItem::with_label(&label);
        let win = window.clone();
        let n = name.to_string();
        let menu_wv = menu_wv_slot.clone();
        item.connect_activate(move |_| {
            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
            println!(
                "{}{}{} {}WIN{} {}{}{} ({}×{})",
                DIM, ts, RESET, CYAN, RESET, TEXT, n, SUB, w, h
            );
            win.resize(w, h);
            if let Some(wv) = &*menu_wv.lock().unwrap() {
                wv.queue_resize();
                let wv_clone = wv.clone();
                glib::timeout_add_local(Duration::from_millis(100), move || {
                    wv_clone.queue_draw();
                    glib::ControlFlow::Break
                });
            }
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
    let shot_wv_slot: Arc<Mutex<Option<WebView>>> = Arc::new(Mutex::new(None));
    let shot_wv_btn = shot_wv_slot.clone();
    shot_btn.connect_clicked(move |_| {
        if let Some(wv) = &*shot_wv_btn.lock().unwrap() {
            take_screenshot(wv);
        } else {
            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
            println!(
                "{}{}{} {}ERR{} no webview available",
                DIM, ts, RESET, RED, RESET
            );
        }
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

    let context = WebContext::default().unwrap();
    context.set_cache_model(CacheModel::DocumentViewer);

    let manager = UserContentManager::new();
    manager.register_script_message_handler("flutterLog");

    let prog_js = progress.clone();
    manager.connect_script_message_received(None, move |_, value| {
        if let Some(js_value) = value.js_value() {
            let text = js_value.to_string();
            if let Some(out) = format_flutter_log(&text, "webview") {
                prog_js.lock().unwrap().log_or_print(out);
            }
        }
    });

    let console_script = UserScript::new(
        r#"
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
        "#,
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    );
    manager.add_script(&console_script);

    let webview = WebView::new_with_context_and_user_content_manager(&context, &manager);
    webview.load_uri("about:blank");
    webview.connect_context_menu(|_, _, _, _| true);

    *menu_wv_slot.lock().unwrap() = Some(webview.clone());
    *shot_wv_slot.lock().unwrap() = Some(webview.clone());

    let vbox = GtkBox::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&webview, true, true, 0);
    window.add(&vbox);

    let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let stdin_slot: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));
    let current_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Hot reload - works fine
    {
        let stdin_r = stdin_slot.clone();
        reload_btn.connect_clicked(move |_| {
            if let Some(stdin) = &mut *stdin_r.lock().unwrap() {
                let _ = stdin.write_all(b"r\n");
                let _ = stdin.flush();
            }
            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
            println!(
                "{}{}{} {}HOT{} {}reload{}",
                DIM, ts, RESET, CYAN, RESET, TEXT, RESET
            );
        });
    }

    // Hot restart
    {
        let url_r = current_url.clone();
        let wv = webview.clone();

        restart_btn.connect_clicked(move |_| {
            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
            println!(
                "{}{}{} {}HOT{} {}restart · reloading webview...{}",
                DIM, ts, RESET, CYAN, RESET, TEXT, RESET
            );

            // Bypass sending 'R' directly as it causes Flutter exit/surface deleted error.
            // Simulate Hot Restart by reloading the webview (Flutter rebuilds automatically).
            let wv2 = wv.clone();
            let url2 = url_r.clone();

            glib::timeout_add_local(Duration::from_millis(1500), move || {
                if let Some(url) = &*url2.lock().unwrap() {
                    wv2.load_uri(url);
                } else {
                    wv2.reload();
                }
                glib::ControlFlow::Break
            });
        });
    }

    let child_destroy = child_slot.clone();
    window.connect_destroy(move |_| {
        if let Some(mut child) = child_destroy.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
        gtk::main_quit();
    });

    window.show_all();

    {
        let mut p = progress.lock().unwrap();
        p.finish_step(
            "gtk3 + webkit2 shell",
            Some(&format!("gtk3 + webkit2 shell ({}×{})", width, height)),
        );
    }

    let (url_tx, url_rx) = mpsc::channel();
    let c_slot = child_slot.clone();
    let s_slot = stdin_slot.clone();
    let prog_fl = progress.clone();
    thread::spawn(move || {
        run_flutter(flutter_cmd, port, url_tx, c_slot, s_slot, prog_fl);
    });

    let current_url_clone = current_url.clone();
    let wv_load = webview.clone();
    let prog_srv = progress.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        if let Ok(url) = url_rx.try_recv() {
            *current_url_clone.lock().unwrap() = Some(url.clone());

            if let Some(port_str) = url.split(':').nth(2).and_then(|s| s.split('/').next()) {
                if let Ok(p) = port_str.parse::<u16>() {
                    let wv = wv_load.clone();
                    let url_clone = url.clone();
                    let prog_ready = prog_srv.clone();
                    glib::timeout_add_local(Duration::from_millis(500), move || {
                        if TcpStream::connect_timeout(
                            &format!("127.0.0.1:{}", p).parse().unwrap(),
                            Duration::from_millis(150),
                        )
                        .is_ok()
                        {
                            {
                                let mut pr = prog_ready.lock().unwrap();
                                pr.finish_step(&format!("bind http://localhost:{}", p), None);
                                pr.server_url = Some(url_clone.clone());
                                pr.settled = Some(true);
                            }
                            wv.load_uri(&url_clone);
                            glib::ControlFlow::Break
                        } else {
                            glib::ControlFlow::Continue
                        }
                    });
                }
            }
        }
        glib::ControlFlow::Continue
    });

    gtk::main();
}
