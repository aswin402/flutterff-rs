# Architecture

`flutterff-rs` handles Native and Web interaction concurrently by splitting workload across threads using GTK callbacks.

## Execution Flow

1. **Initialization:** The parameters and flags are parsed into local instances. Port availability checking automatically prevents conflicts.
2. **Channel & Subprocess Integration:** 
    A `mpsc` channel pairs up with a spawned `Command`. Flutter begins running with standard outputs piped into the background thread.
3. **Web Server Interception:** 
    The background watcher uses regex to catch the URL broadcasted when Flutter's `web-server` starts.
4. **GTK/WebKit Native Wrapper:**
    The GTK3 frontend initializes rendering WebKit2. Messages from `console.log` generated inside Javascript bridge over through `WebKitUserContentManager` to native Rust standard output, allowing `flutterff-rs` to format runtime errors cleanly into native Linux colors matching the Flutter SDK.

## Fallback Design Choices

- **Webview Snapshot Workaround:** Historically, Wayland displays block proper image snapshots using Webkit's snapshot API. `flutterff-rs` implements an overriding screenshot implementation parsing the raw dimensions via `gtk::allocation` mapped concurrently to a raw `cairo::ImageSurface` rendering.
- **Hot Restart Handling:** Direct `R` (Full Restart) hooks can cause race conditions or crash crashes internally in `web-server`. `flutterff-rs` uses a stable technique where it delays and purely reloads the existing `load_uri` URL locally, forcing standard file-system hot-refresh from the daemon instead without bringing the tree down.
