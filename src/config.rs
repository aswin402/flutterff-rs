use std::thread;

use crate::theme::{BOLD, CYAN, DIM, RESET, SUB, TEXT};

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const DEFAULT_PORT: u16 = 8080;
pub const PORT_SCAN_RANGE: u16 = 120;
pub const DEFAULT_PRESET: &str = "mobile";
pub const SEPARATOR_WIDTH: usize = 40;
pub const BAR_WIDTH: usize = 28;
pub const MIN_NAME_WIDTH: usize = 32;
pub const BUILDING_ROWS: usize = 4;
pub const DONE_ROWS: usize = 5;
pub const MAX_LOG_HISTORY: usize = 200;
pub const DESKTOP_MIN_WIDTH: i32 = 1024;
pub const COMPACT_STACK_SIZE: usize = 256 * 1024;

/// Spawns a background worker thread with a compact 256 KiB stack instead of the 8 MiB OS default
pub fn spawn_compact_thread<F>(name: &str, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let _ = thread::Builder::new()
        .name(name.to_string())
        .stack_size(COMPACT_STACK_SIZE)
        .spawn(f);
}

/// Deterministic ordered device presets (zero heap allocation)
pub const DEVICE_PRESETS: &[(&str, (i32, i32))] = &[
    ("mobile", (412, 915)),
    ("mobile-small", (360, 800)),
    ("iphone", (390, 844)),
    ("tablet", (768, 1024)),
    ("desktop", (1280, 800)),
];

pub fn pointer_mode_for_width(w: i32) -> &'static str {
    if w >= DESKTOP_MIN_WIDTH {
        "cursor"
    } else {
        "touch"
    }
}

pub fn orientation_for_dims(w: i32, h: i32) -> &'static str {
    if w > h { "landscape" } else { "portrait" }
}

pub fn find_preset(name: &str) -> Option<(i32, i32)> {
    DEVICE_PRESETS
        .iter()
        .find(|&&(preset, _)| preset.eq_ignore_ascii_case(name))
        .map(|&(_, dims)| dims)
}

pub fn parse_size(s: &str) -> Result<(i32, i32), String> {
    let (w_str, h_str) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("Invalid size '{}'. Use WxH e.g. 390x844", s))?;
    let w = w_str
        .parse::<i32>()
        .map_err(|_| format!("Invalid width: {}", w_str))?;
    let h = h_str
        .parse::<i32>()
        .map_err(|_| format!("Invalid height: {}", h_str))?;
    if w <= 0 || h <= 0 {
        return Err(format!("Dimensions must be positive: {}x{}", w, h));
    }
    Ok((w, h))
}

pub fn resolve_dimensions(size_str: &str) -> Result<(i32, i32, String), String> {
    if let Some((w, h)) = find_preset(size_str) {
        Ok((w, h, size_str.to_lowercase()))
    } else {
        let (w, h) = parse_size(size_str)?;
        Ok((w, h, "custom".to_string()))
    }
}

pub fn normalize_url(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return "about:blank".to_string();
    }
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("file://")
        || s.starts_with("about:")
        || s.starts_with("data:")
    {
        return s.to_string();
    }
    if let Some(port_part) = s.strip_prefix(':') {
        return format!("http://localhost:{}", port_part);
    }
    if s.starts_with("localhost") || s.starts_with("127.0.0.1") || s.starts_with("0.0.0.0") {
        return format!("http://{}", s);
    }
    if !s.contains(' ') && s.contains('.') {
        return format!("https://{}", s);
    }
    let encoded: String = s
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect();
    format!("https://duckduckgo.com/?q={}", encoded)
}

pub enum CliAction {
    Run(CliConfig),
    McpServer,
    ShowVersion,
    ShowHelp,
    ListSizes,
}

pub struct CliConfig {
    pub port: u16,
    pub no_hot: bool,
    pub watch: bool,
    pub landscape: bool,
    pub profile: bool,
    pub offline: bool,
    pub flavor: Option<String>,
    pub size_str: String,
    pub url: Option<String>,
}

impl CliConfig {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<CliAction, String> {
        let mut cfg = Self {
            port: DEFAULT_PORT,
            no_hot: false,
            watch: true,
            landscape: false,
            profile: false,
            offline: false,
            flavor: None,
            size_str: DEFAULT_PRESET.to_string(),
            url: None,
        };

        let mut iter = args.into_iter().skip(1);
        while let Some(arg) = iter.next() {
            let (flag, inline_val) = match arg.split_once('=') {
                Some((k, v)) if k.starts_with('-') => (k, Some(v.to_string())),
                _ => (arg.as_str(), None),
            };

            let mut next_val = |flag_name: &str| -> Result<String, String> {
                inline_val
                    .clone()
                    .or_else(|| iter.next())
                    .ok_or_else(|| format!("Missing value for '{}'", flag_name))
            };

            match flag {
                "mcp" | "--mcp" => return Ok(CliAction::McpServer),
                "--version" | "-V" => return Ok(CliAction::ShowVersion),
                "--help" | "-h" => return Ok(CliAction::ShowHelp),
                "--list-sizes" => return Ok(CliAction::ListSizes),
                "--no-hot" => {
                    cfg.no_hot = true;
                    cfg.watch = false;
                }
                "--no-watch" => cfg.watch = false,
                "--landscape" | "-l" => cfg.landscape = true,
                "--profile" => cfg.profile = true,
                "--offline" => cfg.offline = true,
                "--port" | "-p" => {
                    let val = next_val(flag)?;
                    cfg.port = val
                        .parse::<u16>()
                        .map_err(|_| format!("Invalid port '{}': expected 1..65535", val))?;
                }
                "--size" | "-s" => cfg.size_str = next_val(flag)?,
                "--flavor" => cfg.flavor = Some(next_val(flag)?),
                "--url" | "-u" | "--browser" | "-b" => {
                    let val = next_val(flag)?;
                    cfg.url = Some(normalize_url(&val));
                    cfg.watch = false;
                }
                other if !other.starts_with('-') && cfg.url.is_none() => {
                    cfg.url = Some(normalize_url(other));
                    cfg.watch = false;
                }
                other => return Err(format!("Unknown option '{}'. Run with --help.", other)),
            }
        }

        Ok(CliAction::Run(cfg))
    }
}

pub fn print_help() {
    println!(
        "\n🦊 {}{}{}{} {}v{}{}\n",
        BOLD, CYAN, PKG_NAME, RESET, DIM, VERSION, RESET
    );
    println!("{}USAGE:{}", BOLD, RESET);
    println!("  {} [OPTIONS]", PKG_NAME);
    println!("  {} --url <URL|PORT|QUERY> [OPTIONS]", PKG_NAME);
    println!("  {} --mcp\n", PKG_NAME);
    println!("{}OPTIONS:{}", BOLD, RESET);
    let opts = [
        (
            "    --mcp, mcp",
            "Run as a native Model Context Protocol (MCP) JSON-RPC server over stdio",
        ),
        (
            "-u, --url <URL|QUERY>",
            "Instant Browser / Web App mode (opens any URL, :3000, or search query in ~80ms)",
        ),
        (
            "-s, --size <PRESET|WxH>",
            "Viewport size preset or custom WxH (default: mobile)",
        ),
        (
            "-l, --landscape",
            "Start the viewport in landscape orientation (swaps WxH)",
        ),
        (
            "-p, --port <PORT>",
            "Web-server HTTP port (default: 8080, auto-increments)",
        ),
        (
            "    --offline",
            "Force offline mode (--no-pub --no-web-resources-cdn)",
        ),
        ("    --profile", "Run Flutter in profile mode"),
        ("    --flavor <NAME>", "Build a custom Flutter app flavor"),
        (
            "    --no-watch",
            "Disable inotify auto-reload on lib/*.dart save",
        ),
        ("    --no-hot", "Disable hot reload support"),
        ("    --list-sizes", "Display all built-in device presets"),
        ("-V, --version", "Print version information"),
        ("-h, --help", "Print this help message"),
    ];
    for (flag, desc) in opts {
        println!("  {}{:<26}{} {}{}{}", CYAN, flag, RESET, TEXT, desc, RESET);
    }
    println!("\n{}WINDOW SHORTCUTS:{}", BOLD, RESET);
    let keys = [
        ("Ctrl+R / F5", "Instant Hot Reload (or Page Reload)"),
        ("Ctrl+Shift+R", "Crash-safe WebView Hot Restart"),
        ("Ctrl+G", "Toggle HeaderBar URL / Search Bar"),
        ("Alt+Left / Alt+Right", "Navigate Back / Forward in history"),
        ("Ctrl+S", "Capture PNG Screenshot (screenshots/)"),
        (
            "Ctrl+I / F12",
            "Toggle UI Semantics Inspector ([0], [1]...)",
        ),
        ("Ctrl+L", "Rotate Portrait <-> Landscape"),
        ("Ctrl+W", "Toggle Auto-Reload Watcher (lib/*.dart)"),
        ("Ctrl+Q", "Quit simulator and Flutter daemon"),
    ];
    for (k, desc) in keys {
        println!("  {}{:<26}{} {}{}{}", CYAN, k, RESET, TEXT, desc, RESET);
    }
    println!();
}

pub fn print_size_presets() {
    println!(
        "\n🦊 {}{}{}{} {}— size presets{}",
        BOLD, CYAN, PKG_NAME, RESET, DIM, RESET
    );
    println!("{}{}{}", DIM, "─".repeat(SEPARATOR_WIDTH), RESET);
    for &(name, (w, h)) in DEVICE_PRESETS {
        let tag = if name == DEFAULT_PRESET {
            format!(" {}← default{}", SUB, RESET)
        } else {
            String::new()
        };
        println!(
            " {}{:<15}{} {}{}×{}{}{}",
            CYAN, name, RESET, TEXT, w, h, RESET, tag
        );
    }
    println!(
        " {}{:<15}{} {}--size 430x932{}",
        CYAN, "custom", RESET, SUB, RESET
    );
}
