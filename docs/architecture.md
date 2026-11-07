# Architecture

`flutterff-rs` handles Native and Web interaction concurrently by splitting workload across threads using GTK callbacks.

## Execution Flow

1. **Initialization:** The parameters and flags are parsed into local instances. Port availability checking automatically prevents conflicts.
2. **Channel & Subprocess Integration:** 
    A `mpsc` channel pairs up with a spawned `Command`. Flutter begins running with standard outputs piped into the background thread.
3. **Web Server Interception:** 
    The background watcher uses $O(n)$ slice scanning (`extract_local_url`) to catch the URL broadcasted when Flutter's `web-server` starts.
4. **GTK/WebKit Native Wrapper:**
    The GTK3 frontend initializes rendering WebKit2. Messages from `console.log` generated inside Javascript bridge over through `WebKitUserContentManager` to native Rust standard output, allowing `flutterff-rs` to format runtime errors cleanly into native Linux colors matching the Flutter SDK.
5. **Native MCP Server & Local Unix Socket IPC (`src/mcp.rs`):**
    When running a simulator window, `flutterff-rs` binds a lightweight Unix domain socket (`$XDG_RUNTIME_DIR/flutterff-rs.sock` or `/tmp/flutterff-rs.sock`). Running `flutterff-rs --mcp` starts a zero-dependency JSON-RPC 2.0 Model Context Protocol server over `stdio` that communicates with the live GTK window over the Unix socket to trigger hot reloads, hot restarts, dynamic window resizing, Cairo PNG screenshot capture (with inline base64 `image/png` encoding), JavaScript evaluation, and log retrieval.

## Fallback Design Choices

- **Webview Snapshot Workaround:** Historically, Wayland displays block proper image snapshots using Webkit's snapshot API. `flutterff-rs` implements an overriding screenshot implementation parsing the raw dimensions via `gtk::allocation` mapped concurrently to a raw `cairo::ImageSurface` rendering.
- **Hot Restart Handling:** Direct `R` (Full Restart) hooks can cause race conditions or crash crashes internally in `web-server`. `flutterff-rs` uses a stable technique where it delays and purely reloads the existing `load_uri` URL locally, forcing standard file-system hot-refresh from the daemon instead without bringing the tree down.
