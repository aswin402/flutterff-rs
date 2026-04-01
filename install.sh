#!/usr/bin/env bash
# ─────────────────────────────────────────────
#  flutterff-rs — install.sh
#  First time install. Run once.
# ─────────────────────────────────────────────
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="$HOME/.local/bin"
INSTALL_PATH="$INSTALL_DIR/flutterff-rs"

GREEN="\033[92m"; YELLOW="\033[93m"; CYAN="\033[96m"
RED="\033[91m";   RESET="\033[0m";   BOLD="\033[1m"

echo ""
echo -e "${BOLD}${CYAN}🦊 flutterff-rs — install${RESET}"
echo ""

# ── 1. check rust ─────────────────────────────
echo -e "${YELLOW}Checking Rust...${RESET}"
if ! command -v cargo &>/dev/null; then
    echo -e "${RED}Rust not found!${RESET}"
    echo "Install with:  curl -LsSf https://rustup.rs | sh"
    exit 1
fi
echo -e "${GREEN}✔ Rust:${RESET} $(cargo --version)"

# ── 2. check flutter ──────────────────────────
echo -e "${YELLOW}Checking Flutter...${RESET}"
if ! command -v flutter &>/dev/null; then
    echo -e "${RED}Flutter not found in PATH!${RESET}"
    exit 1
fi
echo -e "${GREEN}✔ Flutter:${RESET} $(flutter --version --machine 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('frameworkVersion','unknown'))" 2>/dev/null || echo "found")"

# ── 3. check WebKitGTK dev libs ───────────────
echo -e "${YELLOW}Checking WebKitGTK dev libraries...${RESET}"
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    echo -e "${GREEN}✔ webkit2gtk-4.1 found${RESET}"
elif pkg-config --exists webkit2gtk-4.0 2>/dev/null; then
    echo -e "${GREEN}✔ webkit2gtk-4.0 found${RESET}"
    # patch Cargo.toml to use 4.0 feature
    sed -i 's/webkit2gtk = { version = "2.0", features = \["v2_38"\] }/webkit2gtk = { version = "2.0", features = ["v2_34"] }/' \
        "$SCRIPT_DIR/Cargo.toml"
    echo -e "${YELLOW}ℹ Patched Cargo.toml for webkit2gtk-4.0${RESET}"
else
    echo -e "${RED}WebKitGTK dev libs not found!${RESET}"
    echo ""
    echo "Install with:"
    echo "  Ubuntu 22.04+:  sudo apt install libwebkit2gtk-4.1-dev"
    echo "  Ubuntu 20.04:   sudo apt install libwebkit2gtk-4.0-dev"
    echo "  Arch:           sudo pacman -S webkit2gtk"
    echo "  Fedora:         sudo dnf install webkit2gtk4.1-devel"
    exit 1
fi

# ── 4. check GTK dev libs ─────────────────────
echo -e "${YELLOW}Checking GTK3 dev libraries...${RESET}"
if ! pkg-config --exists gtk+-3.0 2>/dev/null; then
    echo -e "${YELLOW}GTK3 dev libs not found. Installing...${RESET}"
    if command -v apt &>/dev/null; then
        sudo apt install -y libgtk-3-dev
    elif command -v pacman &>/dev/null; then
        sudo pacman -S --noconfirm gtk3
    elif command -v dnf &>/dev/null; then
        sudo dnf install -y gtk3-devel
    else
        echo -e "${RED}Please install libgtk-3-dev manually.${RESET}"
        exit 1
    fi
fi
echo -e "${GREEN}✔ GTK3 dev libs found${RESET}"

# ── 5. build ──────────────────────────────────
echo ""
echo -e "${YELLOW}Building flutterff-rs (release)...${RESET}"
echo -e "${YELLOW}This takes 1–3 minutes on first build.${RESET}"
echo ""
cd "$SCRIPT_DIR"
cargo build --release
echo ""
echo -e "${GREEN}✔ Build complete${RESET}"

# ── 6. install binary ─────────────────────────
echo -e "${YELLOW}Installing to ~/.local/bin...${RESET}"
mkdir -p "$INSTALL_DIR"
cp "$SCRIPT_DIR/target/release/flutterff-rs" "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"
echo -e "${GREEN}✔ Installed:${RESET} $INSTALL_PATH"

# ── 7. PATH check ─────────────────────────────
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo ""
    echo -e "${YELLOW}~/.local/bin is not in your PATH.${RESET}"
    echo "Add to your ~/.bashrc or ~/.zshrc:"
    echo ""
    echo -e "  ${CYAN}export PATH=\"\$HOME/.local/bin:\$PATH\"${RESET}"
    echo ""
    echo "Then run:  source ~/.bashrc"
else
    echo -e "${GREEN}✔ ~/.local/bin already in PATH${RESET}"
fi

echo ""
echo -e "${BOLD}${GREEN}✔ Done!${RESET}"
echo ""
echo -e "Run inside any Flutter project:  ${CYAN}flutterff-rs${RESET}"
echo -e "See all options:                 ${CYAN}flutterff-rs --help 2>&1 | head -5${RESET}"
echo ""