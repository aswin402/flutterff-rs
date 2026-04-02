//! flutterff-rs - Flutter web dev launcher
//! Opens a native mobile window using GTK + WebKit2 directly.
//! No Chrome, no topbar, less RAM.

use gtk::prelude::*;
use gtk::{HeaderBar, Menu, MenuItem, MenuButton};
use gtk::Box as GtkBox;
use webkit2gtk::{WebContext, WebContextExt, WebView, WebViewExt, CacheModel};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio, ChildStdin};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;
use std::env;
use std::time::Duration;
use regex::Regex;

const VERSION: &str = "1.5.0";
const GREEN:  &str = "\x1b[92m";
const YELLOW: &str = "\x1b[93m";
const CYAN:   &str = "\x1b[96m";
const RED:    &str = "\x1b[91m";
const RESET:  &str = "\x1b[0m";
const BOLD:   &str = "\x1b[1m";

fn device_presets() -> HashMap<&'static str, (i32, i32)> {
    HashMap::from([
        ("mobile",        (412, 915)),
        ("mobile-small",  (360, 800)),
        ("iphone",        (390, 844)),
        ("tablet",        (768, 1024)),
        ("desktop",       (1280, 800)),
    ])
}

fn parse_size(s: &str) -> Result<(i32, i32), String> {
    let lower = s.to_lowercase();
    let parts: Vec<&str> = lower.splitn(2, 'x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse::<i32>().map_err(|_| format!("Invalid width: {}", parts[0]))?;
        let h = parts[1].parse::<i32>().map_err(|_| format!("Invalid height: {}", parts[1]))?;
        Ok((w, h))
    } else {
        Err(format!("Invalid size '{}'. Use WxH e.g. 390x844", s))
    }
}

// ── port helpers ──────────────────────────────────────────────────────────────

fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

fn find_free_port(start: u16) -> u16 {
    for port in start..8200 {
        if is_port_free(port) {
            return port;
        }
    }
    eprintln!("{}No free port found between {}–8200{}", RED, start, RESET);
    std::process::exit(1);
}

// ── connectivity check ────────────────────────────────────────────────────────

fn check_online() -> bool {
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = "pub.dev:443".to_socket_addrs() else { return false };
    let Some(addr) = addrs.next() else { return false };
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok()
}

// ── flutter watcher ───────────────────────────────────────────────────────────

fn run_flutter(cmd: Vec<String>, port: u16, url_tx: mpsc::Sender<String>, child_slot: Arc<Mutex<Option<std::process::Child>>>, stdin_slot: Arc<Mutex<Option<ChildStdin>>>) {
    let pattern = Regex::new(r"(http://(?:localhost|127\.0\.0\.1):\d+\S*)").unwrap();
    let mut sent = false;

    let child = match Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}{}Flutter not found: {}{}", RED, BOLD, e, RESET);
            eprintln!("Make sure 'flutter' is in your PATH");
            glib::idle_add(|| { gtk::main_quit(); glib::ControlFlow::Break });
            return;
        }
    };

    // store child so window close handler can kill it
    *child_slot.lock().unwrap() = Some(child);
    let slot_ref = child_slot.clone();

    // extract and store stdin for button use
    {
        let mut guard = slot_ref.lock().unwrap();
        if let Some(ref mut c) = *guard {
            *stdin_slot.lock().unwrap() = c.stdin.take();
        }
    }

    let stdout = {
        let mut guard = slot_ref.lock().unwrap();
        guard.as_mut().unwrap().stdout.take().unwrap()
    };
    for line in BufReader::new(stdout).lines() {
        let text = match line { Ok(t) => t, Err(_) => break };
        println!("{}", text);

        if !sent {
            let found = if let Some(cap) = pattern.captures(&text) {
                cap.get(1).map(|m| m.as_str().to_string())
            } else if text.to_lowercase().contains("serving")
                   || text.to_lowercase().contains("listening") {
                Some(format!("http://localhost:{}", port))
            } else {
                None
            };

            if let Some(url) = found {
                sent = true;
                println!("\n{}{}✔ Flutter ready — loading:{} {}{}{}\n",
                    GREEN, BOLD, RESET, CYAN, url, RESET);
                let _ = url_tx.send(url);
            }
        }
    }

    // wait for child
    if let Some(mut c) = slot_ref.lock().unwrap().take() { let _ = c.wait(); }
    glib::idle_add(|| { gtk::main_quit(); glib::ControlFlow::Break });
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut port: u16              = 8080;
    let mut no_hot                 = false;
    let mut profile                = false;
    let mut offline                = false;
    let mut flavor: Option<String> = None;
    let mut size_str               = "mobile".to_string();
    let mut list_sizes             = false;
    let mut show_ver               = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => { i += 1; if i < args.len() { port = args[i].parse().unwrap_or(8080); } }
            "--no-hot"      => no_hot = true,
            "--profile"     => profile = true,
            "--offline"     => offline = true,
            "--flavor"      => { i += 1; if i < args.len() { flavor = Some(args[i].clone()); } }
            "--size" | "-s" => { i += 1; if i < args.len() { size_str = args[i].clone(); } }
            "--list-sizes"  => list_sizes = true,
            "--version"     => show_ver = true,
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
        println!("\n{}Available size presets:{}", BOLD, RESET);
        let mut sorted: Vec<_> = presets.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (name, (w, h)) in &sorted {
            let tag = if **name == "mobile" { "  ← default" } else { "" };
            println!("  {}{:<15}{} {}x{}{}", CYAN, name, RESET, w, h, tag);
        }
        println!("\n  {}custom{}          e.g. --size 430x932\n", CYAN, RESET);
        return;
    }

    let (width, height) = match presets.get(size_str.as_str()) {
        Some(&(w, h)) => (w, h),
        None => match parse_size(&size_str) {
            Ok(wh) => wh,
            Err(e) => { eprintln!("{}{}{}", RED, e, RESET); return; }
        }
    };

    // ── port resolution ───────────────────────────────────────────────────────
    if !is_port_free(port) {
        let free = find_free_port(port + 1);
        println!("{}Port {} is in use — using {} instead{}", YELLOW, port, free, RESET);
        port = free;
    }

    // ── offline detection ─────────────────────────────────────────────────────
    if !offline {
        print!("{}Checking connectivity...{} ", YELLOW, RESET);
        if check_online() {
            println!("{}online{}", GREEN, RESET);
        } else {
            println!("{}offline{}", YELLOW, RESET);
            offline = true;
        }
    }

    // ── build flutter command ─────────────────────────────────────────────────
    let mut flutter_cmd = vec![
        "flutter".to_string(), "run".to_string(),
        "-d".to_string(), "web-server".to_string(),
        format!("--web-port={}", port),
    ];
    if profile  { flutter_cmd.push("--profile".to_string()); }
    if no_hot   { flutter_cmd.push("--no-hot".to_string()); }
    if offline  {
        flutter_cmd.push("--no-pub".to_string());
        // Flutter 3.29+ removed HTML renderer — only CanvasKit remains
        // --no-web-resources-cdn stops Flutter fetching CanvasKit from CDN
        flutter_cmd.push("--no-web-resources-cdn".to_string());
    }
    if let Some(f) = flavor { flutter_cmd.push("--flavor".to_string()); flutter_cmd.push(f); }

    // ── startup info ──────────────────────────────────────────────────────────
    println!("\n{}{}🦊 flutterff-rs v{}{}", BOLD, CYAN, VERSION, RESET);
    let size_label = if presets.contains_key(size_str.as_str()) { size_str.as_str() } else { "custom" };
    println!("{}Size:{}       {}x{}  ({})", YELLOW, RESET, width, height, size_label);
    println!("{}Port:{}       {}", YELLOW, RESET, port);
    println!("{}Mode:{}       {}", YELLOW, RESET, if profile { "profile" } else { "debug (web-server)" });
    println!("{}Hot reload:{} {}", YELLOW, RESET, if no_hot { "disabled" } else { "enabled — press r in terminal" });
    println!("{}Network:{}    {}", YELLOW, RESET,
        if offline { "⚠ offline — using cached packages" } else { "✔ online" });
    println!("\n{}Starting Flutter...{}\n", YELLOW, RESET);

    // ── GTK init ──────────────────────────────────────────────────────────────
    gtk::init().expect("Failed to initialize GTK");

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("flutterff");
    window.set_default_size(width, height);
    window.set_resizable(true);

    // ── Header bar ────────────────────────────────────────────────────────────
    let hb = HeaderBar::new();
    hb.set_show_close_button(true);
    hb.set_title(Some("flutterff"));
    hb.set_decoration_layout(Some("menu:close"));
    window.set_titlebar(Some(&hb));

    // ── Size menu ─────────────────────────────────────────────────────────────
    let size_btn = MenuButton::new();
    let img = gtk::Image::from_icon_name(Some("view-fullscreen-symbolic"), gtk::IconSize::Menu);
    size_btn.set_image(Some(&img));
    size_btn.set_tooltip_text(Some("Change Device Size"));

    let menu = Menu::new();
    let mut sorted_presets: Vec<_> = presets.iter().collect();
    sorted_presets.sort_by_key(|(k, _)| *k);
    for (&name, &(w, h)) in &sorted_presets {
        let label = format!("{} ({}x{})", name.replace('-', " "), w, h);
        let item = MenuItem::with_label(&label);
        let win_ref = window.clone();
        let n = name.to_string();
        item.connect_activate(move |_| {
            println!("{}Resizing to {} ({}x{}){}", YELLOW, n, w, h, RESET);
            win_ref.resize(w, h);
        });
        menu.append(&item);
    }
    menu.show_all();
    size_btn.set_popup(Some(&menu));
    hb.pack_start(&size_btn);

    // ── Hot Reload button (lightning icon = r) ────────────────────────────────
    let stdin_slot: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));

    let reload_btn = gtk::Button::new();
    let reload_img = gtk::Image::from_icon_name(Some("weather-lightning-symbolic"), gtk::IconSize::Menu);
    reload_btn.set_image(Some(&reload_img));
    reload_btn.set_tooltip_text(Some("Hot Reload (r)"));
    hb.pack_end(&reload_btn);

    // ── Hot Restart button (refresh icon = R) ─────────────────────────────────
    let restart_btn = gtk::Button::new();
    let restart_img = gtk::Image::from_icon_name(Some("view-refresh-symbolic"), gtk::IconSize::Menu);
    restart_btn.set_image(Some(&restart_img));
    restart_btn.set_tooltip_text(Some("Hot Restart (R)"));
    hb.pack_end(&restart_btn);

    // ── WebView ───────────────────────────────────────────────────────────────
    let ctx = WebContext::default().unwrap();
    ctx.set_cache_model(CacheModel::DocumentViewer);

    let webview = WebView::with_context(&ctx);
    webview.load_uri("about:blank");
    webview.connect_context_menu(|_, _, _, _| true);

    let vbox = GtkBox::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&webview, true, true, 0);
    window.add(&vbox);

    // wire reload button — sends 'r' to flutter stdin, reloads webview after delay
    let reload_wv     = webview.clone();
    let reload_stdin  = stdin_slot.clone();
    let reload_pending = Arc::new(Mutex::new(None::<String>));
    let reload_pending2 = reload_pending.clone();
    reload_btn.connect_clicked(move |_| {
        println!("{}Hot Reloading...{}", YELLOW, RESET);
        if let Some(ref mut s) = *reload_stdin.lock().unwrap() {
            let _ = s.write_all(b"r
");
            let _ = s.flush();
        }
        // reload webview after 800ms
        let wv = reload_wv.clone();
        let p  = reload_pending2.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(800), move || {
            if let Some(url) = p.lock().unwrap().clone() {
                wv.load_uri(&url);
            }
            glib::ControlFlow::Break
        });
    });

    // wire restart button — sends 'R' to flutter stdin, reloads webview after delay
    let restart_wv    = webview.clone();
    let restart_stdin = stdin_slot.clone();
    let restart_pending = reload_pending.clone();
    restart_btn.connect_clicked(move |_| {
        println!("{}Hot Restarting...{}", YELLOW, RESET);
        if let Some(ref mut s) = *restart_stdin.lock().unwrap() {
            let _ = s.write_all(b"R
");
            let _ = s.flush();
        }
        let wv = restart_wv.clone();
        let p  = restart_pending.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
            if let Some(url) = p.lock().unwrap().clone() {
                wv.load_uri(&url);
            }
            glib::ControlFlow::Break
        });
    });

    window.show_all();

    // ── URL channel + child slot (must be declared before connect_destroy) ──────
    let (url_tx, url_rx) = mpsc::channel::<String>();

    let child_slot: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
    let child_slot_thread  = child_slot.clone();
    let child_slot_destroy = child_slot.clone();

    window.connect_destroy(move |_| {
        if let Some(mut child) = child_slot_destroy.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
            println!("\n{}Stopping Flutter...{}", YELLOW, RESET);
            println!("{}Done.{}", GREEN, RESET);
        }
        gtk::main_quit();
    });

    let stdin_slot_thread = stdin_slot.clone();
    thread::spawn(move || run_flutter(flutter_cmd, port, url_tx, child_slot_thread, stdin_slot_thread));

    let wv = webview.clone();
    // shared state for retry logic
    let pending_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let pending_clone = pending_url.clone();

    // also share URL with button reload logic
    let reload_pending_rx = reload_pending.clone();

    // receive URL from flutter thread
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        if let Ok(url) = url_rx.try_recv() {
            *pending_clone.lock().unwrap() = Some(url.clone());
            *reload_pending_rx.lock().unwrap() = Some(url);
        }
        glib::ControlFlow::Continue
    });

    // retry probe: wait until Flutter's server actually responds before loading
    let wv2 = wv.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let mut guard = pending_url.lock().unwrap();
        if let Some(url) = guard.clone() {
            // probe the port
            let port_num: u16 = url
                .split(':')
                .nth(2)
                .and_then(|s| s.split('/').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080);
            if TcpStream::connect_timeout(
                &format!("127.0.0.1:{}", port_num).parse().unwrap(),
                Duration::from_millis(200),
            ).is_ok() {
                println!("{}✔ Flutter server ready — loading{}", GREEN, RESET);
                wv2.load_uri(&url);
                *guard = None; // clear so we don't reload again
            } else {
                println!("{}Waiting for Flutter server...{}", YELLOW, RESET);
            }
        }
        glib::ControlFlow::Continue
    });

    // ── Run ───────────────────────────────────────────────────────────────────
    gtk::main();

}