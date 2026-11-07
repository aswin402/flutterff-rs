# Changelog

All notable changes to **`flutterff-rs`** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [2.5.4] - 2026-11-08

### ✨ Added
- **In-Page GUI Agent & Flutter Semantics Inspector (`src/agent_bridge.js` — Inspired by [alibaba/page-agent](https://github.com/alibaba/page-agent))**:
  - Auto-activates Flutter Web's `<flt-semantics-placeholder>` and traverses `document` plus open Shadow Roots (`<flutter-view>`, `<flt-glass-pane>`) to index interactive widgets (`[0]`, `[1]`, `[2]...`) with roles, labels, values, and `(x, y)` coordinates.
  - Added 6 new table-driven MCP UI testing & debugging tools (`WEB_AGENT_TOOL_MAP` in `src/mcp.rs`):
    - `flutter_ui_tree` *(+ `flutterff://ui_tree` resource)* — Returns the indexed widget tree (`[0] [textfield] ...`) and active pointer mode.
    - `flutter_tap` — Taps/clicks a widget by `index`, `text`/label, or `(x, y)` coordinates.
    - `flutter_type` — Focuses a target Flutter `TextField` or HTML input, sets its value via the native prototype setter + `InputEvent`, and optionally submits with `Enter`.
    - `flutter_scroll` — Scrolls/swipes any scrollable container or coordinate `(dx, dy)`.
    - `flutter_press_key` — Dispatches keyboard events (`Enter`, `Escape`, `Backspace`, `Tab`, arrow keys).
    - `flutter_inspect_overlay` — Toggles visual Set-of-Mark (`[0]`, `[1]`) cyan/mint bounding boxes and configures `pointer_mode` (`"auto"`, `"cursor"`, `"touch"`).
- **Adaptive Pointer Visualization & Events (Desktop Cursor vs. Mobile Touch Dot)**:
  - **Desktop Mode (`"cursor"`, width `>= 1024px`)**: Renders a Red-Orange (`#FF4500`) `Bibata-Modern-Ice`-shaped `24×24` SVG pointer with white border and dispatches desktop mouse event sequences (`pointermove` → `mouseover` → `mousedown` → `mouseup` → `click`).
  - **Mobile/Tablet Mode (`"touch"`, width `< 1024px`)**: Renders a compact `12×12px` solid Red-Orange (`#FF4500`) touch dot (without outer ring clutter) and dispatches mobile touch event sequences (`pointerdown` → `touchstart` → `touchend` → `click`).
  - Automatically switches between `"touch"` and `"cursor"` when resizing via the GTK HeaderBar preset switcher or `flutter_resize`.
- **GTK HeaderBar Inspector Toggle & Synchronous JS RPC**:
  - Added a HeaderBar button (`find-location-symbolic`) to toggle the `[0]`, `[1]` UI Semantics overlay with one click.
  - Added bidirectional JS return-value routing (`MCP_BRIDGE_PREFIX`) so `flutter_eval_js` and all `WebAgent` commands return evaluated results synchronously to MCP clients.

---

## [2.5.3] - 2026-11-07

### ✨ Added
- **Native Rust MCP Server (`src/mcp.rs` — `flutterff-rs --mcp` / `flutterff-rs mcp`)**:
  - Built a zero-dependency, ultra-lightweight Model Context Protocol (MCP `2024-11-05`) JSON-RPC 2.0 server over `stdio` directly into the `flutterff-rs` binary.
  - Added a local Unix domain socket IPC bridge (`$XDG_RUNTIME_DIR/flutterff-rs.sock` or `std::env::temp_dir()/flutterff-rs.sock`) connecting the running GTK3 + WebKit2 simulator window to AI agents (Claude Code, Cursor, Antigravity, Windsurf, Zed).
  - Exposed 8 MCP tools driven by static descriptor tables (`TOOL_SPECS`):
    - `flutter_status` — Live simulator status, URL, port, viewport dimensions, mode, and log count.
    - `flutter_launch` — Spawns a detached `flutterff-rs` GUI window with `project_dir`, `size`, `port`, `offline`, `profile`, and `flavor` options and polls IPC until ready.
    - `flutter_hot_reload` — Sends `'r\n'` to the running Flutter daemon for instant Hot Reload.
    - `flutter_hot_restart` — Triggers a crash-safe WebView Hot Restart.
    - `flutter_resize` — Dynamically resizes the GTK window to any preset (`mobile`, `mobile-small`, `iphone`, `tablet`, `desktop`) or custom `WxH`.
    - `flutter_screenshot` — Captures a Wayland-safe Cairo `ARgb32` PNG snapshot (`screenshots/screenshot_*.png`) and returns inline base64 `image/png` content via a pure-Rust encoder (`base64_encode`).
    - `flutter_logs` — Queries the bounded ring buffer (`log_history`) of formatted `INF`/`WRN`/`ERR`/`DBG`/`HOT`/`WIN`/`SCR` logs with optional `limit`, `level`, and `clear`.
    - `flutter_eval_js` — Evaluates arbitrary JavaScript inside the live WebKit2 WebView.
    - `flutter_stop` — Gracefully shuts down the Flutter daemon and closes the simulator window.
  - Exposed 3 MCP resources (`RESOURCE_SPECS`): `flutterff://status`, `flutterff://logs`, and `flutterff://presets`.

### ⚡ Changed & Refactored
- **Zero-Allocation Byte-Slice JSON Parser (`JsonVal::parse`)**:
  - Parses JSON-RPC messages directly over `&[u8]` (`input.as_bytes()`) without allocating a `Vec<char>` or pulling in `serde`/`serde_json`/`tokio`.
- **Table-Driven MCP Schema Generation & Validation**:
  - `build_tools_schema` and `build_resources_schema` generate JSON-RPC schemas algorithmically from `TOOL_SPECS` and `RESOURCE_SPECS`, and `handle_tool_call` automatically validates `required: true` parameters.
- **Unified Screenshot & CLI Flag Helpers**:
  - Extracted synchronous `capture_screenshot_surface` in `src/main.rs` so both the GTK HeaderBar button (`take_screenshot`) and MCP `flutter_screenshot` share the same Cairo capture pipeline.
  - Replaced chained `if` flag builders in `src/main.rs` and `src/mcp.rs` with table-driven loops.

---

## [2.5.2] - 2026-11-07

### ⚡ Changed & Refactored
- **Removed `regex` Dependency (Pure $O(n)$ Algorithms)**:
  - Dropped `regex` (and transitive `regex-automata`, `regex-syntax`, `aho-corasick`) from `Cargo.toml` and `Cargo.lock`, as well as `import re` from `localinstall.sh` and `localupdate.sh`.
  - Replaced per-line regex compilation in `strip_ansi` with an $O(n)$ single-pass character state machine that strips all ANSI CSI sequences.
  - Replaced localhost URL regex matching with zero-allocation slice scanning (`extract_local_url` & `extract_url_port`).
- **Eliminated Hardcoded Values & Duplication**:
  - Replaced hardcoded version strings in `src/main.rs` with compile-time `env!("CARGO_PKG_NAME")` and `env!("CARGO_PKG_VERSION")`.
  - Added `define_palette!` macro to generate both `Rgb` tuples and `const &str` ANSI escape sequences from a single source of truth.
  - Replaced heap-allocated `HashMap` device presets with a deterministic static slice (`DEVICE_PRESETS`).
  - Replaced scattered pipeline step strings and magic counts with a strongly-typed `Step` enum (`NetCheck`, `GtkShell`, `FlutterDaemon`, `CompileWeb`, `BindServer`).
- **Table-Driven Log Classification & Iterator CLI Parser**:
  - Replaced `if/else` log-level chains and triple `.to_lowercase()` calls with a single-pass table lookup (`LEVEL_RULES`, `NOISE_PATTERNS`, `ERROR_PATTERNS`, `LogSource`).
  - Replaced index-based CLI loop with `CliConfig::parse`, adding support for `--help` / `-h`, `-V`, and `--key=value` syntax.

### 🐛 Fixed
- **Port Discovery Above `8200` (`find_free_port`)**: Fixed bug where passing `--port > 8200` on a busy port produced an empty range (`start..8200`); now scans `start..=start.saturating_add(120)` with an OS ephemeral port fallback.
- **Infinite 100ms GLib Timer Leak**: Stopped the outer `url_rx` polling timer (`glib::ControlFlow::Break`) once the server URL is received.
- **Graceful Flutter Daemon Shutdown**: Sends `"q\n"` to Flutter's `stdin` on window destroy before `child.kill()` so spawned `dart` web-server processes exit cleanly and release the port.
- **Direct WebView Capture**: Initialized `WebView` before `HeaderBar` controls, removing two redundant `Arc<Mutex<Option<WebView>>>` wrappers.

### 🗑️ Removed
- Removed unused `src/macros.rs`, `test_cairo.rs`, and `flutter_help.txt`.

---

## [2.5.1] - 2026-11-07

### ✨ Added
- **`cargo-pretty`-Style Live Terminal TUI (`src/main.rs`)**:
  - Implemented `PrettyProgress` and `spawn_pretty_tui` to render a 20 FPS inline terminal status view while `flutter run -d web-server` initializes and compiles.
  - Added 24-bit Truecolor row alpha-blending (`blend`) so older `Building` and `Completed` pipeline steps smoothly recede toward the dark base (`#1E1E2E`) while active steps stand out with live `00.00s` timers.
  - Added a sweeping `flutterff-rs` theme gradient progress bar (`FLUTTERFF_RAMP`: `#027DFD` Flutter Blue → `#00B4D8` Deep Cyan → `#00E5FF` Electric Cyan → `#72EFDD` Aqua Mint → `#FFFFFF` Ice White) that settles into solid Mint Green (`#50FA7B`) or Coral Red (`#F38BA8`) before transitioning into a compact summary block and live log stream.
- **Single-Job (`-j 1`) Cargo Configuration (`.cargo/config.toml`)**:
  - Added `.cargo/config.toml` with `[build] jobs = 1` so `cargo build`, `cargo clippy`, `cargo check`, and `cargo test` automatically execute with `-j 1`.
- **Themed Local Installer & Updater (`localinstall.sh`, `localupdate.sh`)**:
  - Added `cargo-pretty`-inspired live build progress monitors powered by `cargo metadata` and `cargo build --release -j 1 --message-format=json-diagnostic-rendered-ansi`.
- **Semantic Design System (`DESIGN.md` & `README.md`)**:
  - Documented the complete visual language, 24-bit color tokens, terminal typography, and GTK3 HeaderBar component architecture following the [VoltAgent/awesome-design-md](https://github.com/VoltAgent/awesome-design-md) and `design-md` specification.

### 🔄 Changed
- **Rust Edition & Toolchain Upgrade**:
  - Upgraded `Cargo.toml` to **Rust Edition 2024** (`edition = "2024"`, `rust-version = "1.99"`).
- **Latest Dependency Versions**:
  - Updated direct dependencies in `Cargo.toml` to their latest compatible releases (`gtk 0.18.2`, `webkit2gtk 2.0.2`, `glib 0.18.5`, `cairo-rs 0.18.5`, `chrono 0.4.45`).
- **Local Script Hygiene & `.gitignore`**:
  - Renamed `install.sh` → `localinstall.sh` and `update.sh` → `localupdate.sh`.
  - Removed `install.sh` and `update.sh` from Git tracking and added `localinstall.sh`, `localupdate.sh`, and `*.bak` to `.gitignore`.

---

## [2.5.0] - Initial Release

### ✨ Added
- **Lightweight GTK3 + WebKit2GTK Shell**: Native Linux window wrapper for `flutter run -d web-server` eliminating full Chrome browser overhead.
- **Responsive Device Presets**: Instant switching between `mobile` (`412×915`), `mobile-small` (`360×800`), `iphone` (`390×844`), `tablet` (`768×1024`), `desktop` (`1280×800`), and custom `--size WxH` dimensions.
- **Native Cairo Screenshot Engine**: Direct GTK surface-to-PNG capture (`screenshots/screenshot_YYYYMMDD_HHMMSS.png`) compatible with both X11 and Wayland.
- **HeaderBar Developer Controls**: One-click Hot Reload (`r` via `stdin`) and crash-safe WebView Hot Restart.
- **JS Console Bridge & Offline Mode**: Automatic `window.webkit.messageHandlers.flutterLog` interception and auto-fallback to `--no-pub --no-web-resources-cdn` when offline.
