# Changelog

All notable changes to **`flutterff-rs`** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
  - Updated direct dependencies in `Cargo.toml` to their latest compatible releases:
    - `gtk` → `0.18.2` (`features = ["v3_24"]`)
    - `webkit2gtk` → `2.0.2` (`features = ["v2_38"]`)
    - `glib` → `0.18.5`
    - `cairo-rs` → `0.18.5` (`features = ["png", "v1_16"]`)
    - `regex` → `1.13.1`
    - `chrono` → `0.4.45`
  - Refreshed `Cargo.lock` across 44 transitive crates (`libc 0.2.191`, `futures-* 0.3.34`, `serde 1.0.229`, `syn 2.0.119`, `pkg-config 0.3.34`, etc.).
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
