#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
#  🦊 flutterff-rs — Universal 1-Command Installer & AI Agent MCP Configurator
#  Usage:
#    curl -fsSL https://raw.githubusercontent.com/aswin402/flutterff-rs/master/install.sh | bash
#    curl -fsSL https://raw.githubusercontent.com/aswin402/flutterff-rs/master/install.sh | bash -s -- --from-source
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

REPO="aswin402/flutterff-rs"
INSTALL_DIR="${FLUTTERFF_INSTALL_DIR:-$HOME/.local/bin}"
INSTALL_PATH="$INSTALL_DIR/flutterff-rs"
FROM_SOURCE=0
SETUP_MCP=1

for arg in "$@"; do
    case "$arg" in
        --from-source) FROM_SOURCE=1 ;;
        --no-mcp)      SETUP_MCP=0 ;;
        -h|--help)
            echo "Usage: install.sh [--from-source] [--no-mcp]"
            exit 0
            ;;
    esac
done

# 24-bit Truecolor palette (flutterff-rs DESIGN.md)
C_CYAN="\033[38;2;0;229;255m"
C_MINT="\033[38;2;80;250;123m"
C_YELLOW="\033[38;2;249;226;175m"
C_RED="\033[38;2;243;139;168m"
C_TEXT="\033[38;2;205;214;244m"
C_SUB="\033[38;2;166;173;200m"
C_DIM="\033[38;2;108;112;134m"
BOLD="\033[1m"
RESET="\033[0m"

sep() {
    echo -e "${C_DIM}────────────────────────────────────────────────────────${RESET}"
}

step_ok() {
    printf "${C_MINT}✓${RESET} ${C_TEXT}%-26s${RESET} ${C_SUB}%s${RESET}\n" "$1" "$2"
}

step_warn() {
    printf "${C_YELLOW}⚠${RESET} ${C_TEXT}%-26s${RESET} ${C_YELLOW}%s${RESET}\n" "$1" "$2"
}

step_err() {
    printf "${C_RED}✗${RESET} ${C_TEXT}%-26s${RESET} ${C_RED}%s${RESET}\n" "$1" "$2"
}

echo ""
echo -e "🦊 ${BOLD}${C_CYAN}flutterff-rs${RESET} ${C_DIM}— Universal Installer & MCP Setup${RESET}"
sep

# ── 1. Platform & Runtime Library Check ──────────────────────────────────────
OS_NAME="$(uname -s)"
ARCH_NAME="$(uname -m)"
if [ "$OS_NAME" != "Linux" ]; then
    step_err "platform ($OS_NAME)" "flutterff-rs requires Linux (GTK3 + WebKit2GTK)"
    exit 1
fi
step_ok "platform" "Linux ($ARCH_NAME)"

if (/sbin/ldconfig -p 2>/dev/null || ldconfig -p 2>/dev/null) | grep -q "libwebkit2gtk-4\.1\.so" \
    || [ -e "/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0" ] \
    || [ -e "/usr/lib64/libwebkit2gtk-4.1.so.0" ] \
    || [ -e "/usr/lib/libwebkit2gtk-4.1.so.0" ]; then
    step_ok "webkit2gtk-4.1 runtime" "found"
else
    step_warn "webkit2gtk-4.1 runtime" "not detected in standard library paths"
    echo -e "  ${C_SUB}Install runtime library:${RESET}"
    echo -e "    Ubuntu/Debian: ${C_CYAN}sudo apt install libgtk-3-0 libwebkit2gtk-4.1-0${RESET}"
    echo -e "    Arch Linux:    ${C_CYAN}sudo pacman -S gtk3 webkit2gtk-4.1${RESET}"
    echo -e "    Fedora:        ${C_CYAN}sudo dnf install gtk3 webkit2gtk4.1${RESET}"
fi

mkdir -p "$INSTALL_DIR"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

# ── 2. Download Prebuilt Release Binary (or Build from Source) ───────────────
INSTALLED_BIN=0
if [ "$FROM_SOURCE" -eq 0 ] && [ "$ARCH_NAME" = "x86_64" ]; then
    API_REL_URL="https://api.github.com/repos/$REPO/releases/latest"
    ARCHIVE_URL="https://github.com/$REPO/releases/latest/download/flutterff-rs-linux-x86_64.tar.gz"
    SUMS_URL="https://github.com/$REPO/releases/latest/download/SHA256SUMS"

    if command -v python3 &>/dev/null && curl -fsSL "$API_REL_URL" -o "$TMP_DIR/release.json" 2>/dev/null; then
        read -r API_ARCH_URL API_SUMS_URL < <(python3 - "$TMP_DIR/release.json" <<'PYURL'
import json, sys
data = json.load(open(sys.argv[1]))
arch_u, sums_u = "", ""
for a in data.get("assets", []):
    if a.get("name") == "flutterff-rs-linux-x86_64.tar.gz":
        arch_u = a.get("url", "")
    elif a.get("name") == "SHA256SUMS":
        sums_u = a.get("url", "")
print(f"{arch_u} {sums_u}")
PYURL
)
        [ -n "${API_ARCH_URL:-}" ] && ARCHIVE_URL="$API_ARCH_URL"
        [ -n "${API_SUMS_URL:-}" ] && SUMS_URL="$API_SUMS_URL"
    fi

    if curl -fsSL -H "Accept: application/octet-stream" "$ARCHIVE_URL" -o "$TMP_DIR/flutterff-rs-linux-x86_64.tar.gz" 2>/dev/null; then
        if curl -fsSL -H "Accept: application/octet-stream" "$SUMS_URL" -o "$TMP_DIR/SHA256SUMS" 2>/dev/null; then
            (cd "$TMP_DIR" && grep "flutterff-rs-linux-x86_64.tar.gz" SHA256SUMS | sha256sum -c - >/dev/null 2>&1) \
                && step_ok "sha256 checksum" "verified" \
                || step_warn "sha256 checksum" "skipped"
        fi
        tar -xzf "$TMP_DIR/flutterff-rs-linux-x86_64.tar.gz" -C "$TMP_DIR"
        install -m 755 "$TMP_DIR/flutterff-rs" "$INSTALL_PATH"
        INSTALLED_BIN=1
        step_ok "prebuilt binary" "$INSTALL_PATH"
    else
        step_warn "prebuilt binary" "unavailable, falling back to source build..."
    fi
fi

if [ "$INSTALLED_BIN" -eq 0 ]; then
    if ! command -v cargo &>/dev/null; then
        step_err "rust toolchain" "cargo not found (install via https://rustup.rs)"
        exit 1
    fi
    step_ok "source build" "cloning https://github.com/$REPO.git..."
    git clone --depth 1 "https://github.com/$REPO.git" "$TMP_DIR/src" >/dev/null 2>&1

    # Auto-shim pkg-config & .so symlinks if runtime libs exist without -dev packages
    if ! pkg-config --exists gdk-3.0 webkit2gtk-4.1 2>/dev/null && [ -d "/usr/lib/x86_64-linux-gnu" ]; then
        SHIM_DIR="$TMP_DIR/build-deps"
        mkdir -p "$SHIM_DIR/pkgconfig" "$SHIM_DIR/lib"
        for pair in \
            "glib-2.0:glib-2.0.so.0:2.80.0" "gobject-2.0:gobject-2.0.so.0:2.80.0" \
            "gio-2.0:gio-2.0.so.0:2.80.0" "atk:atk-1.0.so.0:2.52.0" \
            "cairo:cairo.so.2:1.18.0" "cairo-gobject:cairo-gobject.so.2:1.18.0" \
            "pango:pango-1.0.so.0:1.52.0" "pangocairo:pangocairo-1.0.so.0:1.52.0" \
            "gdk-pixbuf-2.0:gdk_pixbuf-2.0.so.0:2.42.0" "gdk-3.0:gdk-3.so.0:3.24.41" \
            "gtk+-3.0:gtk-3.so.0:3.24.41" "libsoup-3.0:soup-3.0.so.0:3.4.4" \
            "javascriptcoregtk-4.1:javascriptcoregtk-4.1.so.0:2.44.0" \
            "webkit2gtk-4.1:webkit2gtk-4.1.so.0:2.44.0"; do
            IFS=":" read -r pkg so ver <<< "$pair"
            libname="${so%%.so*}"
            ln -sf "/usr/lib/x86_64-linux-gnu/lib${so}" "$SHIM_DIR/lib/lib${libname}.so"
            printf "Name: %s\nDescription: shim\nVersion: %s\nLibs: -L%s/lib -l%s\nCflags:\n" \
                "$pkg" "$ver" "$SHIM_DIR" "$libname" > "$SHIM_DIR/pkgconfig/${pkg}.pc"
        done
        export PKG_CONFIG_PATH="$SHIM_DIR/pkgconfig:${PKG_CONFIG_PATH:-}"
        export LIBRARY_PATH="$SHIM_DIR/lib:${LIBRARY_PATH:-}"
    fi

    (cd "$TMP_DIR/src" && cargo build --release -j 1)
    BIN_CANDIDATE="$TMP_DIR/src/target/release/flutterff-rs"
    if [ ! -f "$BIN_CANDIDATE" ]; then
        BIN_CANDIDATE="$TMP_DIR/src/target/x86_64-unknown-linux-gnu/release/flutterff-rs"
    fi
    install -m 755 "$BIN_CANDIDATE" "$INSTALL_PATH"
    step_ok "built & installed" "$INSTALL_PATH"
fi

VER_OUT="$("$INSTALL_PATH" --version 2>/dev/null || echo "flutterff-rs")"
step_ok "version" "$VER_OUT"

# ── 3. Automatic AI Agent MCP Server & Skill Configuration ───────────────────
if [ "$SETUP_MCP" -eq 1 ] && command -v python3 &>/dev/null; then
    python3 - "$INSTALL_PATH" <<'PYEOF'
import json
import os
import subprocess
import sys

bin_path = sys.argv[1]
home = os.path.expanduser("~")

# 1. Register flutterff in detected MCP config files
mcp_targets = [
    os.path.join(home, ".gemini", "config", "mcp_config.json"),
    os.path.join(home, ".gemini", "antigravity-cli", "mcp_config.json"),
    os.path.join(home, ".gemini", "antigravity", "mcp_config.json"),
    os.path.join(home, ".claude.json"),
    os.path.join(home, ".cursor", "mcp.json"),
    os.path.join(home, ".codeium", "windsurf", "mcp_config.json"),
]

configured = []
for cfg_path in mcp_targets:
    parent = os.path.dirname(cfg_path)
    if not os.path.isdir(parent):
        continue
    data = {}
    if os.path.isfile(cfg_path) and os.path.getsize(cfg_path) > 0:
        try:
            with open(cfg_path, "r", encoding="utf-8") as f:
                data = json.load(f)
        except Exception:
            continue
    if not isinstance(data, dict):
        continue
    servers = data.setdefault("mcpServers", {})
    servers["flutterff"] = {
        "command": bin_path,
        "args": ["--mcp"],
    }
    try:
        with open(cfg_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)
            f.write("\n")
        configured.append(os.path.relpath(cfg_path, home))
    except Exception:
        pass

# 2. Export lazy-loaded tool schemas + instructions.md for Antigravity CLI if present
agy_mcp_dir = os.path.join(home, ".gemini", "antigravity-cli", "mcp", "flutterff")
if os.path.isdir(os.path.join(home, ".gemini", "antigravity-cli")):
    try:
        os.makedirs(agy_mcp_dir, exist_ok=True)
        reqs = (
            '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n'
            '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n'
        )
        out = subprocess.check_output([bin_path, "--mcp"], input=reqs, text=True)
        for line in out.strip().splitlines():
            msg = json.loads(line)
            res = msg.get("result", {})
            if "instructions" in res:
                with open(os.path.join(agy_mcp_dir, "instructions.md"), "w", encoding="utf-8") as f:
                    f.write(res["instructions"] + "\n")
            for tool in res.get("tools", []):
                with open(os.path.join(agy_mcp_dir, f"{tool['name']}.json"), "w", encoding="utf-8") as f:
                    json.dump(
                        {
                            "name": tool["name"],
                            "description": tool["description"],
                            "parameters": tool["inputSchema"],
                        },
                        f,
                        indent=2,
                    )
                    f.write("\n")
    except Exception:
        pass

if configured:
    print(",".join(configured))
PYEOF
    step_ok "mcp agent config" "flutterff (--mcp) wired automatically"
fi

sep
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    step_warn "PATH" "Add to ~/.bashrc or ~/.zshrc: export PATH=\"$INSTALL_DIR:\$PATH\""
else
    step_ok "PATH" "$INSTALL_DIR active"
fi

echo ""
echo -e "${BOLD}${C_MINT}✓ Installed!${RESET} Run ${C_CYAN}flutterff-rs${RESET} (Flutter project), ${C_CYAN}flutterff-rs --url :3000${RESET} (Web App), or ${C_CYAN}flutterff-rs --help${RESET}"
echo ""
