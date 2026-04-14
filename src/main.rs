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
use std::time::Duration;

const VERSION: &str = "2.4.0";

const GREEN: &str = "\x1b[92m";
const YELLOW: &str = "\x1b[93m";
const CYAN: &str = "\x1b[96m";
const RED: &str = "\x1b[91m";
const BLUE: &str = "\x1b[94m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

macro_rules! any {
    ($text:expr, $patterns:expr) => {
        $patterns.iter().any(|p| $text.contains(p))
    };
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
    
    if any!(lo_raw, [
        "http://", "https://", ".js:", "console",
        "flutter_bootstrap", "ddc_module_loader",
        "dart_sdk", "web_entrypoint"
    ]) {
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
        Some(format!("{} {} {}{} {}", ts, color, tag, RESET, msg))
    } else if source == "webview" {
        let (tag, color) = detect_level(&text);
        Some(format!("{} {} {}{} {}", ts, color, tag, RESET, text))
    } else if any!(lo, ["error:", "exception:", "fatal:", "══╡", "══╞"]) && !lo_raw.contains("surface instance") {
        Some(format!("{} {}ERR{} {}", ts, RED, RESET, text))
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
) {
    let pattern = Regex::new(r#"(http://(?:localhost|127\.0\.0\.1):\d+\S*)"#).unwrap();
    let mut url_sent = false;

    let mut child = match Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}Flutter not found: {}{}", RED, e, RESET);
            glib::idle_add(|| {
                gtk::main_quit();
                glib::ControlFlow::Break
            });
            return;
        }
    };

    *stdin_slot.lock().unwrap() = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    *child_slot.lock().unwrap() = Some(child);

    thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            if let Ok(l) = line {
                if let Some(out) = format_flutter_log(&l, "flutter") {
                    println!("{}", out);
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
                    let _ = url_tx.send(m.as_str().to_string());
                }
            } else if line.to_lowercase().contains("serving")
                || line.to_lowercase().contains("listening")
            {
                url_sent = true;
                let _ = url_tx.send(format!("http://localhost:{}", port));
            }
        }

        if let Some(out) = format_flutter_log(&line, "flutter") {
            println!("{}", out);
        }
    }

    if let Some(mut c) = child_slot.lock().unwrap().take() {
        let _ = c.wait();
    }
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

        let fname = chrono::Local::now().format("screenshot_%Y%m%d_%H%M%S.png").to_string();
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
                            println!("{} {}SCR{} saved → screenshots/{} ({}x{})", ts, GREEN, RESET, fname, w, h);
                        }
                    }
                }
            }
        } else {
             let ts = chrono::Local::now().format("%H:%M:%S").to_string();
             println!("{} {}ERR{} invalid webview size", ts, RED, RESET);
        }
        glib::ControlFlow::Break
    });
}

fn main() {
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
        println!("\n{}size presets{}", BOLD, RESET);
        println!("{}──────────────────────────────{}", DIM, RESET);
        for (name, (w, h)) in &presets {
            let tag = if *name == "mobile" {
                " ← default"
            } else {
                ""
            };
            println!(" {}{:<15}{} {}×{}{}", CYAN, name, RESET, w, h, tag);
        }
        println!(" {}custom{}      --size 430x932", CYAN, RESET);
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

    if !offline {
        print!("{}Checking connectivity...{} ", YELLOW, RESET);
        if check_online() {
            println!("{}online{}", GREEN, RESET);
        } else {
            println!("{}offline{}", YELLOW, RESET);
            offline = true;
        }
    }

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

    let size_label = if presets.contains_key(size_str.as_str()) {
        size_str.as_str()
    } else {
        "custom"
    };
    println!(
        "\n 🦊{}flutterff-rs{} {}{}v{}{}",
        BOLD, RESET, DIM, CYAN, VERSION, RESET
    );
    println!("{}──────────────────────────────{}", DIM, RESET);
    println!(
        " {}size{}     {}×{} {}",
        DIM, RESET, width, height, size_label
    );
    println!(" {}port{}     {}", DIM, RESET, port);
    println!(
        " {}mode{}     {}",
        DIM,
        RESET,
        if profile { "profile" } else { "debug" }
    );
    println!(
        " {}net{}      {}",
        DIM,
        RESET,
        if offline { "offline" } else { "online" }
    );
    println!("{}──────────────────────────────{}", DIM, RESET);

    gtk::init().expect("Failed to initialize GTK");

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("🦊flutterff");
    window.set_default_size(width, height);
    window.set_resizable(true);

    let hb = HeaderBar::new();
    hb.set_show_close_button(true);
    hb.set_title(Some("🦊flutterff"));
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
            println!("{} {}WIN{} {} ({}×{})", ts, CYAN, RESET, n, w, h);
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
            println!("{} {}ERR{} no webview available", ts, RED, RESET);
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

    manager.connect_script_message_received(None, move |_, value| {
        if let Some(js_value) = value.js_value() {
            let text = js_value.to_string();
            if let Some(out) = format_flutter_log(&text, "webview") {
                println!("{}", out);
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
            println!("{}HOT{} reload", CYAN, RESET);
        });
    }

    // Hot restart
    {
        let stdin_r = stdin_slot.clone();
        let url_r = current_url.clone();
        let wv = webview.clone();
        
        restart_btn.connect_clicked(move |_| {
            if let Some(stdin) = &mut *stdin_r.lock().unwrap() {
                let _ = stdin.write_all(b"R\n");
                let _ = stdin.flush();
            }
            println!("{}HOT{} hot restart", CYAN, RESET);
            
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
        gtk::main_quit();
    });

    window.show_all();

    let (url_tx, url_rx) = mpsc::channel();
    let c_slot = child_slot.clone();
    let s_slot = stdin_slot.clone();
    thread::spawn(move || {
        run_flutter(flutter_cmd, port, url_tx, c_slot, s_slot);
    });

    let current_url_clone = current_url.clone();
    let wv_load = webview.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        if let Ok(url) = url_rx.try_recv() {
            *current_url_clone.lock().unwrap() = Some(url.clone());
            
            if let Some(port_str) = url.split(':').nth(2).and_then(|s| s.split('/').next()) {
                if let Ok(p) = port_str.parse::<u16>() {
                    let wv = wv_load.clone();
                    let url_clone = url.clone();
                    glib::timeout_add_local(Duration::from_millis(500), move || {
                        if TcpStream::connect_timeout(
                            &format!("127.0.0.1:{}", p).parse().unwrap(),
                            Duration::from_millis(150),
                        ).is_ok() {
                            println!("{}SRV{} serving on :{}", GREEN, RESET, p);
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