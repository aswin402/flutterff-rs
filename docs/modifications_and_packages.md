# Modifications and Package Usage

## Core Dependencies

The `flutterff-rs` package is built predominantly on `gtk-rs` bindings:

- **[gtk v0.18](https://crates.io/crates/gtk)**: Supplies the core window rendering API and HeaderBars.
- **[webkit2gtk v2.0](https://crates.io/crates/webkit2gtk)**: Creates the JS execution frame loading standard flutter artifacts. 
- **[glib v0.18](https://crates.io/crates/glib)**: Event loops, timeouts, and bindings to coordinate Thread safety between the Rust OS channels and GTK UI rendering.
- **[cairo-rs v0.19/0.18 (png)](https://crates.io/crates/cairo-rs)**: Cairo implementations for reliable snapshot mechanisms saving `.png` bounds effectively on any system (X11/Wayland).
- **[chrono v0.4](https://crates.io/crates/chrono)**: Parsing local times effectively for logging output to terminal standard streams.
- **[regex v1.10](https://crates.io/crates/regex)**: For isolating the server address returned by flutter standard streams safely regardless of internal debugging texts.

## Future Modifications

When making improvements to UI:
Ensure all elements attached into the `HeaderBar` maintain signal `move` closure cloning using standard `Arc<Mutex>` locks. Attempting to pass internal references blindly will violate GTK standard memory models due to multithreaded limitations attached to the `flutter` execution listener.
