use std::env;
use std::path::PathBuf;
use std::time::Duration;

use gtk::prelude::*;
use webkit2gtk::WebView;

use crate::theme::{GREEN, RED, RESET, SUB, print_tagged_event};

pub fn capture_screenshot_surface(webview: &WebView) -> Result<(PathBuf, i32, i32), String> {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let shots_dir = root.join("screenshots");
    let _ = std::fs::create_dir_all(&shots_dir);

    let fname = chrono::Local::now()
        .format("screenshot_%Y%m%d_%H%M%S.png")
        .to_string();
    let fpath = shots_dir.join(&fname);

    let alloc = webview.allocation();
    let (w, h) = (alloc.width(), alloc.height());
    if w <= 0 || h <= 0 {
        return Err("invalid webview size".to_string());
    }

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)
        .map_err(|e| format!("cairo surface error: {}", e))?;
    let cr = cairo::Context::new(&surface).map_err(|e| format!("cairo context error: {}", e))?;
    webview.draw(&cr);

    let mut file =
        std::fs::File::create(&fpath).map_err(|e| format!("file create error: {}", e))?;
    surface
        .write_to_png(&mut file)
        .map_err(|e| format!("png write error: {}", e))?;

    print_tagged_event(
        "SCR",
        GREEN,
        &format!(
            "saved → screenshots/{} {}({}×{}){}",
            fname, SUB, w, h, RESET
        ),
    );
    Ok((fpath, w, h))
}

pub fn take_screenshot(webview: &WebView) {
    let wv = webview.clone();
    wv.queue_draw();
    glib::timeout_add_local(Duration::from_millis(300), move || {
        if let Err(err) = capture_screenshot_surface(&wv) {
            print_tagged_event("ERR", RED, &err);
        }
        glib::ControlFlow::Break
    });
}
