# Modifications and Package Usage

## Core Dependencies

The `flutterff-rs` package targets **Rust Edition 2024** (`rustc 1.99+`) and is built on the latest compatible `gtk-rs` bindings and utility crates:

- **[gtk v0.18.2](https://crates.io/crates/gtk)**: Supplies the core window rendering API and HeaderBars (`v3_24`).
- **[webkit2gtk v2.0.2](https://crates.io/crates/webkit2gtk)**: Creates the JS execution frame loading standard Flutter Web artifacts (`v2_38`).
- **[glib v0.18.5](https://crates.io/crates/glib)**: Event loops, local timestamps (`glib::DateTime::now_local()`), and bindings to coordinate thread safety between Rust OS channels and GTK UI rendering.
- **[cairo-rs v0.18.5 (png)](https://crates.io/crates/cairo-rs)**: Cairo implementations for reliable snapshot mechanisms saving `.png` bounds effectively on any system (X11/Wayland).

*(Note: ANSI escape stripping, localhost URL extraction, JSON parsing, base64 encoding, and local timestamps are implemented with zero external utility crates — avoiding `regex`, `serde`, `chrono`, and `base64` dependencies.)*

## Future Modifications

When making improvements to UI:
Ensure all elements attached into the `HeaderBar` maintain signal `move` closure cloning using standard `Arc<Mutex>` locks. Attempting to pass internal references blindly will violate GTK standard memory models due to multithreaded limitations attached to the `flutter` execution listener.
