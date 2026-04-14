# Setup and Installation

## Prerequisites

Before starting, ensure you have the following installed on your Linux system:

- **Rust toolchain:** Install via [rustup.rs](https://rustup.rs/).
- **Flutter:** The flutter CLI must be available in your `PATH`.
- **GTK and WebKit2 Development libraries:**
  - Debian/Ubuntu: `sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev python3-gi gir1.2-webkit2-4.1 gir1.2-gtk-3.0`
  - Arch Linux: `sudo pacman -S gtk3 webkit2gtk`
  - Fedora: `sudo dnf install gtk3-devel webkit2gtk4.1-devel`

## Compilation

To manually build `flutterff-rs` locally:

```bash
git clone https://github.com/aswin402/flutterff-rs.git
cd flutterff-rs
cargo build --release
```

## Quick Install (Script)

We provide an installation script to auto-compile and place the binary into `~/.local/bin/`.

```bash
chmod +x install.sh
./install.sh
```

## Update Mechanism 

If you make modifications or pull the latest changes, use the provided update script that maintains a backup:

```bash
./update.sh
```
This script rebuilds the executable, backs up the previous version as `~/.local/bin/flutterff-rs.bak`, and installs the new one.
