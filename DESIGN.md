---
version: "2.5.2"
name: flutterff-rs-design-system
description: "Semantic design system for flutterff-rs — a terminal-first, ultra-lightweight Linux GTK3 + WebKit2GTK mobile simulator and CLI orchestrator for Flutter Web. Combines Catppuccin Mocha dark-mode surfaces with Electric Cyan (#00E5FF), Flutter Blue (#027DFD), and Neon Mint (#50FA7B) 24-bit Truecolor terminal telemetry inspired by cargo-pretty."

colors:
  primary: "#00E5FF"
  secondary: "#027DFD"
  accent-mint: "#72EFDD"
  success: "#50FA7B"
  warning: "#F9E2AF"
  danger: "#F38BA8"
  sapphire: "#38BDF8"
  canvas: "#1E1E2E"
  surface: "#45475A"
  ink: "#CDD6F4"
  subtext: "#A6ADC8"
  dim: "#6C7086"

typography:
  terminal-header:
    fontFamily: "JetBrains Mono, Fira Code, DM Mono, ui-monospace, monospace"
    fontWeight: 700
    tracking: "0px"
  terminal-body:
    fontFamily: "JetBrains Mono, Fira Code, DM Mono, ui-monospace, monospace"
    fontWeight: 400
    lineHeight: "1.4"
  gtk-headerbar:
    fontFamily: "Inter, Cantarell, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600

components:
  terminal-live-row:
    indicator: "● (Electric Cyan #00E5FF)"
    labelColor: "Sapphire (#38BDF8) / Lavender Ink (#CDD6F4)"
    timerColor: "Muted Slate (#A6ADC8)"
    fadeBehavior: "Linear RGB interpolation toward Obsidian Base (#1E1E2E)"
  terminal-done-row:
    indicator: "✓ (Neon Mint #50FA7B)"
    labelColor: "Lavender Ink (#CDD6F4)"
    timerColor: "Muted Slate (#A6ADC8)"
    fadeBehavior: "Linear RGB interpolation toward Obsidian Base (#1E1E2E)"
  terminal-progress-bar:
    width: "28 cells"
    filledGlyph: "█ (Animated 8-stop Flutter Blue -> Electric Cyan -> Aqua Mint -> Ice White ramp)"
    emptyGlyph: "░ (Charcoal Surface #45475A)"
  gtk-shell-window:
    chrome: "Minimal GTK3 HeaderBar (menu:close decoration layout)"
    defaultViewport: "412×915 (Mobile preset)"
---

# Design System: `flutterff-rs`
**Project ID:** `flutterff-rs` · **Version:** `2.5.2` · **Spec Standard:** [VoltAgent/awesome-design-md](https://github.com/VoltAgent/awesome-design-md)

## 1. Visual Theme & Atmosphere

`flutterff-rs` embodies a **Utilitarian, Terminal-Native, Zero-Chrome Developer Aesthetic**. The experience is split across two tightly coupled surfaces:

1. **The Native GTK3 + WebKit2 Simulator Shell**: Stripped of browser tab bars, URL omniboxes, and developer-tool drawers. A single compact GTK3 `HeaderBar` frames the mobile viewport (`412×915` default), keeping 96% of pixel area dedicated to the running Flutter Web canvas.
2. **The `cargo-pretty` 24-Bit Truecolor Terminal TUI**: A reactive 20 FPS inline terminal viewport that visualizes build and startup telemetry without scrolling clutter. Older tasks recede into the dark background via real-time RGB alpha-blending, while active tasks pulse with an Electric Cyan (`#00E5FF`) and Flutter Blue (`#027DFD`) sweeping gradient bar.

## 2. Color Palette & Roles

| Token Name | Hex / RGB | ANSI Truecolor | Functional Role |
| :--- | :--- | :--- | :--- |
| **Electric Cyan** (`primary`) | `#00E5FF` · `(0, 229, 255)` | `\x1b[38;2;0;229;255m` | Brand identity, active `●` build dot, `HOT` & `WIN` badges, URL links. |
| **Flutter Blue** (`secondary`) | `#027DFD` · `(2, 125, 253)` | `\x1b[38;2;2;125;253m` | Outer anchor stop of the sweeping progress bar ramp. |
| **Sky Sapphire** (`sapphire`) | `#38BDF8` · `(56, 189, 248)` | `\x1b[38;2;56;189;248m` | Highlighted root package/step name and `INF` runtime log badge. |
| **Aqua Mint** (`accent-mint`) | `#72EFDD` · `(114, 239, 221)` | `\x1b[38;2;114;239;221m` | Mid-ramp transition stop leading into the white crest of the progress bar. |
| **Neon Mint** (`success`) | `#50FA7B` · `(80, 250, 123)` | `\x1b[38;2;80;250;123m` | Completed `✓` step indicators, settled progress bar, and `SCR` screenshot badge. |
| **Warm Amber** (`warning`) | `#F9E2AF` · `(249, 226, 175)` | `\x1b[38;2;249;226;175m` | Compiler warning counts and `WRN` runtime log badge. |
| **Coral Rose** (`danger`) | `#F38BA8` · `(243, 139, 168)` | `\x1b[38;2;243;139;168m` | Fatal build failures (`✗`), port errors, and `ERR` runtime log badge. |
| **Lavender Ink** (`ink`) | `#CDD6F4` · `(205, 214, 244)` | `\x1b[38;2;205;214;244m` | Primary terminal text, completed step labels, and runtime log messages. |
| **Muted Slate** (`subtext`) | `#A6ADC8` · `(166, 173, 200)` | `\x1b[38;2;166;173;200m` | Per-step `00.00s` timers, secondary metadata, and dimensions. |
| **Dim Graphite** (`dim`) | `#6C7086` · `(108, 112, 134)` | `\x1b[38;2;108;112;134m` | Horizontal `─` rules (40-char width), section headers, and `HH:MM:SS` timestamps. |
| **Charcoal Surface** (`surface`) | `#45475A` · `(69, 71, 90)` | `\x1b[38;2;69;71;90m` | Unfilled `░` cells of the 28-cell progress bar. |
| **Obsidian Base** (`canvas`) | `#1E1E2E` · `(30, 30, 46)` | `\x1b[38;2;30;30;46m` | Target background color used for `blend(fg, BASE, alpha)` row fading. |

## 3. Typography & Terminal Hierarchy

* **Brand Header**: Bold Electric Cyan (`🦊 flutterff-rs`) paired with Dim Graphite version tag (`v2.5.2`).
* **Command Echo Line**: Dim Graphite single-line command signature (`flutter run -d web-server --web-port=8080`) immediately above a 40-character `─` hairline divider.
* **Tabular Column Alignment**:
  * **Name Column**: Left-aligned, padded to a minimum of `32` characters (`MIN_NAME_WIDTH = 32`) so timers never jitter horizontally.
  * **Timer Column**: Right-aligned 5-character zero-padded fixed-point seconds (`{:05.2}s`, e.g., `01.42s`).
  * **Runtime Log Stream**: `HH:MM:SS` (Dim Graphite) + 3-letter semantic tag (`INF` / `WRN` / `ERR` / `HOT` / `WIN` / `SCR`) + message in Lavender Ink.

## 4. Component Stylings

* **Live `Building` / `Compiled` Accordion Tables**:
  * Fixed vertical row budgets (`BUILDING_ROWS = 4` in launcher, `6` in build scripts; `DONE_ROWS = 5`/`6`) prevent terminal reflow while tasks start and finish.
  * Each row's foreground RGB is blended toward `#1E1E2E` by `alpha = (row_index + 1) / count`, creating a natural depth gradient toward the newest entry.
* **Sweeping Progress Bar (`28` cells)**:
  * Uses solid full-block `█` glyphs for completed ratio and light-shade `░` (`#45475A`) for remaining capacity, followed by a 3-digit right-aligned percentage (` 60%`).
  * While active, an 8-stop linear color ramp sweeps continuously across the `█` blocks at `0.9 Hz`. Upon completion, it settles for 4 frames into solid Neon Mint (`#50FA7B`).
* **GTK3 Native Simulator Chrome**:
  * **Top-Left Controls**: Symbolic icon `MenuButton` (`view-fullscreen-symbolic`) for instant viewport preset switching + symbolic `Button` (`camera-photo-symbolic`) for Cairo PNG snapshots.
  * **Top-Right Controls**: High-contrast `🗲` Hot Reload trigger + symbolic `view-refresh-symbolic` Hot Restart button + native window close button.

## 5. Layout Principles

* **Zero Reflow Guarantee**: The inline TUI reserves a deterministic line count and redraws in place using ANSI cursor-up (`\x1b[{n}A`) and line-erase (`\r\x1b[2K`), hiding the cursor (`\x1b[?25l`) during animation and restoring it (`\x1b[?25h`) on settle or exit.
* **Settle-to-Summary Hand-off**: Once the local server socket accepts connections, the live TUI erases itself completely and leaves behind a compact 4-line summary card (`window`, `server`, `mode`), giving way to an unobstructed runtime log stream.
