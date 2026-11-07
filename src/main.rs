mod config;
mod flutter;
mod mcp;
mod screenshot;
mod theme;
mod tui;

use std::cell::RefCell;
use std::env;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{Box as GtkBox, HeaderBar, Menu, MenuButton, MenuItem};
use webkit2gtk::{
    CacheModel, SettingsExt, UserContentInjectedFrames, UserContentManager, UserContentManagerExt,
    UserScript, UserScriptInjectionTime, WebContext, WebContextExt, WebView, WebViewExt,
    WebViewExtManual,
};

use config::{
    CliAction, CliConfig, DEVICE_PRESETS, PKG_NAME, VERSION, pointer_mode_for_width, print_help,
    print_size_presets, resolve_dimensions, spawn_compact_thread,
};
use flutter::{
    LogSource, check_online, extract_url_port, find_free_port, format_flutter_log, is_port_free,
    run_flutter,
};
use mcp::{IpcCommand, IpcRequest, j_bool, j_num, j_obj, j_str};
use screenshot::{capture_screenshot_surface, take_screenshot};
use theme::{CYAN, RED, RESET, SUB, print_tagged_event};
use tui::{PrettyProgress, Step, spawn_pretty_tui};

const AGENT_BRIDGE_JS: &str = include_str!("agent_bridge.js");
const MCP_BRIDGE_PREFIX: &str = "__FF_MCP__:";

thread_local! {
    static IPC_DISPATCHER: RefCell<Option<Box<dyn FnMut()>>> = const { RefCell::new(None) };
}

fn configure_low_memory_webview(webview: &WebView) {
    if let Some(settings) = WebViewExt::settings(webview) {
        settings.set_enable_page_cache(false);
        settings.set_enable_offline_web_application_cache(false);
        settings.set_enable_html5_database(false);
        settings.set_enable_dns_prefetching(false);
        settings.set_enable_hyperlink_auditing(false);
        settings.set_enable_developer_extras(false);
        settings.set_enable_smooth_scrolling(false);
    }
}

fn sync_webview_pointer_mode(webview: &WebView, w: i32) {
    let js = format!(
        "if (window.__flutterff) {{ window.__flutterff.setPointerMode('{}'); }}",
        pointer_mode_for_width(w)
    );
    webview.run_javascript(&js, gtk::gio::Cancellable::NONE, |_| {});
}

fn main() {
    let started = Instant::now();

    let mut cfg = match CliConfig::parse(env::args()) {
        Ok(CliAction::McpServer) => {
            mcp::run_mcp_server();
            return;
        }
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

    let (width, height, size_label) = match resolve_dimensions(&cfg.size_str) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("{}{}{}", RED, e, RESET);
            std::process::exit(1);
        }
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

    for (enabled, flags) in [
        (cfg.profile, &["--profile"][..]),
        (cfg.no_hot, &["--no-hot"][..]),
        (cfg.offline, &["--no-pub", "--no-web-resources-cdn"][..]),
    ] {
        if enabled {
            flutter_cmd.extend(flags.iter().map(|s| (*s).to_string()));
        }
    }
    if let Some(f) = &cfg.flavor {
        flutter_cmd.extend(["--flavor".to_string(), f.clone()]);
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
        width,
        height,
        cfg.port,
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

    let context = WebContext::default().unwrap();
    context.set_cache_model(CacheModel::DocumentViewer);

    let manager = UserContentManager::new();
    manager.register_script_message_handler("flutterLog");

    let pending_js_replies: Arc<Mutex<Vec<(u64, mpsc::SyncSender<mcp::JsonVal>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let next_js_req_id: Arc<Mutex<u64>> = Arc::new(Mutex::new(1));

    let prog_js = progress.clone();
    let pending_js_cb = pending_js_replies.clone();
    manager.connect_script_message_received(None, move |_, value| {
        if let Some(js_value) = value.js_value() {
            let text = js_value.to_string();
            if let Some(json_str) = text.strip_prefix(MCP_BRIDGE_PREFIX) {
                if let Ok(parsed) = mcp::JsonVal::parse(json_str)
                    && let Some(req_id) = parsed.get("req_id").and_then(mcp::JsonVal::as_u64)
                    && let Some(result) = parsed.get("result").cloned()
                {
                    let mut pending = pending_js_cb.lock().unwrap();
                    if let Some(pos) = pending.iter().position(|(id, _)| *id == req_id) {
                        let (_, tx) = pending.swap_remove(pos);
                        let _ = tx.send(result);
                    }
                }
                return;
            }
            if let Some(out) = format_flutter_log(&text, LogSource::WebView) {
                prog_js.lock().unwrap().log_or_print(out);
            }
        }
    });

    let console_script = UserScript::new(
        AGENT_BRIDGE_JS,
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    );
    manager.add_script(&console_script);

    let webview = WebView::new_with_context_and_user_content_manager(&context, &manager);
    configure_low_memory_webview(&webview);
    webview.load_uri("about:blank");
    webview.connect_context_menu(|_, _, _, _| true);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(PKG_NAME);
    window.set_default_size(width, height);
    window.set_resizable(true);

    let hb = HeaderBar::new();
    hb.set_show_close_button(true);
    hb.set_title(Some(PKG_NAME));
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
        let prog_win = progress.clone();
        item.connect_activate(move |_| {
            {
                let mut pr = prog_win.lock().unwrap();
                pr.width = w;
                pr.height = h;
                pr.window_label = format!("{}×{} ({})", w, h, name);
            }
            print_tagged_event(
                "WIN",
                CYAN,
                &format!("{} {}({}×{}){}", name, SUB, w, h, RESET),
            );
            win.resize(w, h);
            wv.queue_resize();
            sync_webview_pointer_mode(&wv, w);
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

    let inspect_btn = gtk::Button::new();
    inspect_btn.set_image(Some(&gtk::Image::from_icon_name(
        Some("find-location-symbolic"),
        gtk::IconSize::Menu,
    )));
    inspect_btn.set_tooltip_text(Some("Toggle UI Inspector Overlay ([0], [1]...)"));
    let wv_inspect = webview.clone();
    inspect_btn.connect_clicked(move |_| {
        wv_inspect.run_javascript(
            "if (window.__flutterff) { window.__flutterff.dispatch(0, { action: 'inspect_overlay' }); }",
            gtk::gio::Cancellable::NONE,
            |_| {},
        );
        print_tagged_event("INS", CYAN, "toggled UI semantics overlay");
    });
    hb.pack_start(&inspect_btn);

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
        let ctx_r = context.clone();
        restart_btn.connect_clicked(move |_| {
            print_tagged_event("HOT", CYAN, "restart · reloading webview...");
            ctx_r.clear_cache();
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

    // Graceful daemon quit ('q\n') + socket cleanup on window close
    let child_destroy = child_slot.clone();
    let stdin_destroy = stdin_slot.clone();
    window.connect_destroy(move |_| {
        mcp::cleanup_socket();
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

    // Spawn event-driven local IPC Unix socket listener (zero idle polling wakeups)
    let (ipc_tx, ipc_rx) = mpsc::channel::<IpcRequest>();
    {
        let win_ipc = window.clone();
        let wv_ipc = webview.clone();
        let ctx_ipc = context.clone();
        let stdin_ipc = stdin_slot.clone();
        let url_ipc = current_url.clone();
        let prog_ipc = progress.clone();
        let pending_ipc = pending_js_replies.clone();
        let next_id_ipc = next_js_req_id.clone();

        IPC_DISPATCHER.with(|cell| {
            *cell.borrow_mut() = Some(Box::new(move || {
                while let Ok(req) = ipc_rx.try_recv() {
                    let resp = match req.cmd {
                        IpcCommand::Status => {
                            let pr = prog_ipc.lock().unwrap();
                            j_obj([
                                ("running", j_bool(true)),
                                ("version", j_str(VERSION)),
                                ("ready", j_bool(pr.settled == Some(true))),
                                (
                                    "url",
                                    j_str(
                                        pr.server_url
                                            .clone()
                                            .unwrap_or_else(|| format!("http://localhost:{}", pr.port)),
                                    ),
                                ),
                                ("port", j_num(pr.port as f64)),
                                ("window", j_str(pr.window_label.clone())),
                                ("width", j_num(pr.width as f64)),
                                ("height", j_num(pr.height as f64)),
                                ("pointer_mode", j_str(pointer_mode_for_width(pr.width))),
                                ("mode", j_str(pr.mode_label.clone())),
                                ("logs_count", j_num(pr.log_history.len() as f64)),
                            ])
                        }
                        IpcCommand::HotReload => {
                            if let Some(stdin) = &mut *stdin_ipc.lock().unwrap() {
                                let _ = stdin.write_all(b"r\n");
                                let _ = stdin.flush();
                            }
                            print_tagged_event("HOT", CYAN, "reload (mcp)");
                            j_obj([("ok", j_bool(true)), ("action", j_str("hot_reload"))])
                        }
                        IpcCommand::HotRestart => {
                            print_tagged_event("HOT", CYAN, "restart · reloading webview (mcp)...");
                            ctx_ipc.clear_cache();
                            match &*url_ipc.lock().unwrap() {
                                Some(url) => wv_ipc.load_uri(url),
                                None => wv_ipc.reload(),
                            }
                            j_obj([("ok", j_bool(true)), ("action", j_str("hot_restart"))])
                        }
                        IpcCommand::Resize { size } => match resolve_dimensions(&size) {
                            Ok((w, h, label)) => {
                                {
                                    let mut pr = prog_ipc.lock().unwrap();
                                    pr.width = w;
                                    pr.height = h;
                                    pr.window_label = format!("{}×{} ({})", w, h, label);
                                }
                                win_ipc.resize(w, h);
                                wv_ipc.queue_resize();
                                wv_ipc.queue_draw();
                                sync_webview_pointer_mode(&wv_ipc, w);
                                print_tagged_event(
                                    "WIN",
                                    CYAN,
                                    &format!("{} {}({}×{} mcp){}", label, SUB, w, h, RESET),
                                );
                                j_obj([
                                    ("ok", j_bool(true)),
                                    ("width", j_num(w as f64)),
                                    ("height", j_num(h as f64)),
                                    ("preset", j_str(label)),
                                    ("pointer_mode", j_str(pointer_mode_for_width(w))),
                                ])
                            }
                            Err(e) => j_obj([("ok", j_bool(false)), ("error", j_str(e))]),
                        },
                        IpcCommand::Screenshot => {
                            wv_ipc.queue_draw();
                            match capture_screenshot_surface(&wv_ipc) {
                                Ok((path, w, h)) => j_obj([
                                    ("ok", j_bool(true)),
                                    ("path", j_str(path.to_string_lossy().to_string())),
                                    ("width", j_num(w as f64)),
                                    ("height", j_num(h as f64)),
                                ]),
                                Err(e) => j_obj([("ok", j_bool(false)), ("error", j_str(e))]),
                            }
                        }
                        IpcCommand::Logs {
                            limit,
                            level,
                            clear,
                        } => {
                            let mut pr = prog_ipc.lock().unwrap();
                            let lvl_upper = level.as_ref().map(|s| s.to_uppercase());
                            let matched: Vec<String> = pr
                                .log_history
                                .iter()
                                .rev()
                                .filter(|line| match &lvl_upper {
                                    Some(l) => line.contains(l.as_str()),
                                    None => true,
                                })
                                .take(limit)
                                .cloned()
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect();
                            if clear {
                                pr.log_history.clear();
                            }
                            j_obj([
                                ("ok", j_bool(true)),
                                ("count", j_num(matched.len() as f64)),
                                ("logs", j_str(matched.join("\n"))),
                            ])
                        }
                        IpcCommand::WebAgent { payload } => {
                            let req_id = {
                                let mut id_guard = next_id_ipc.lock().unwrap();
                                let id = *id_guard;
                                *id_guard = id.wrapping_add(1);
                                id
                            };
                            let action_name = payload
                                .get("action")
                                .and_then(mcp::JsonVal::as_str)
                                .unwrap_or("agent")
                                .to_string();
                            print_tagged_event(
                                "AGT",
                                CYAN,
                                &format!("{} {}#{}{}", action_name, SUB, req_id, RESET),
                            );
                            pending_ipc.lock().unwrap().push((req_id, req.reply_tx));
                            let js = format!(
                                "if (window.__flutterff) {{ window.__flutterff.dispatch({}, {}); }} else {{ window.webkit.messageHandlers.flutterLog.postMessage('{}' + JSON.stringify({{ req_id: {}, result: {{ ok: false, error: 'WebView agent bridge not ready' }} }})); }}",
                                req_id,
                                payload.to_json(),
                                MCP_BRIDGE_PREFIX,
                                req_id
                            );
                            let pending_err = pending_ipc.clone();
                            wv_ipc.run_javascript(&js, gtk::gio::Cancellable::NONE, move |res| {
                                if let Err(e) = res {
                                    let mut pending = pending_err.lock().unwrap();
                                    if let Some(pos) = pending.iter().position(|(id, _)| *id == req_id)
                                    {
                                        let (_, tx) = pending.swap_remove(pos);
                                        let _ = tx.send(j_obj([
                                            ("ok", j_bool(false)),
                                            ("error", j_str(format!("JS evaluation error: {}", e))),
                                        ]));
                                    }
                                }
                            });
                            continue;
                        }
                        IpcCommand::Stop => {
                            let _ = req
                                .reply_tx
                                .send(j_obj([("ok", j_bool(true)), ("action", j_str("stopping"))]));
                            win_ipc.close();
                            return;
                        }
                    };
                    let _ = req.reply_tx.send(resp);
                }
            }));
        });
    }
    mcp::spawn_ipc_listener(ipc_tx, || {
        glib::idle_add(|| {
            IPC_DISPATCHER.with(|cell| {
                if let Some(dispatch) = cell.borrow_mut().as_mut() {
                    dispatch();
                }
            });
            glib::ControlFlow::Break
        });
    });

    let (url_tx, url_rx) = mpsc::channel();
    let c_slot = child_slot.clone();
    let s_slot = stdin_slot.clone();
    let prog_fl = progress.clone();
    let port = cfg.port;
    spawn_compact_thread("ff-flutter", move || {
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
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });

    gtk::main();
}
