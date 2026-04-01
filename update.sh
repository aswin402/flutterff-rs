#!/usr/bin/env bash
# ─────────────────────────────────────────────
#  flutterff-rs — update.sh
#  Run after editing src/main.rs to rebuild
#  and push the new binary to ~/.local/bin
# ─────────────────────────────────────────────
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="$HOME/.local/bin"
INSTALL_PATH="$INSTALL_DIR/flutterff-rs"

GREEN="\033[92m"; YELLOW="\033[93m"; CYAN="\033[96m"
RED="\033[91m";   RESET="\033[0m";   BOLD="\033[1m"

echo ""
echo -e "${BOLD}${CYAN}🦊 flutterff-rs — update${RESET}"
echo ""

# ── check installed ───────────────────────────
if [ ! -f "$INSTALL_PATH" ]; then
    echo -e "${YELLOW}Not installed yet. Run install.sh first.${RESET}"
    exit 1
fi

# ── show version diff ─────────────────────────
OLD_VER=$("$INSTALL_PATH" --version 2>/dev/null || echo "unknown")
echo -e "${YELLOW}Current:${RESET} $OLD_VER"

# ── backup old binary ─────────────────────────
BACKUP="$INSTALL_DIR/flutterff-rs.bak"
cp "$INSTALL_PATH" "$BACKUP"
echo -e "${GREEN}✔ Backup saved:${RESET} $BACKUP"

# ── rebuild ───────────────────────────────────
echo -e "${YELLOW}Rebuilding...${RESET}"
cd "$SCRIPT_DIR"
cargo build --release
echo -e "${GREEN}✔ Build complete${RESET}"

# ── install new binary ────────────────────────
cp "$SCRIPT_DIR/target/release/flutterff-rs" "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"

NEW_VER=$("$INSTALL_PATH" --version 2>/dev/null || echo "unknown")
echo -e "${GREEN}✔ Updated:${RESET} $NEW_VER → $INSTALL_PATH"

echo ""
echo -e "${BOLD}${GREEN}✔ Update complete!${RESET}"
echo ""
echo "To rollback:  ${CYAN}cp ~/.local/bin/flutterff-rs.bak ~/.local/bin/flutterff-rs${RESET}"
echo ""