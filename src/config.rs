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
    pub profile: bool,
    pub offline: bool,
    pub flavor: Option<String>,
    pub size_str: String,
}

impl CliConfig {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<CliAction, String> {
        let mut cfg = Self {
            port: DEFAULT_PORT,
            no_hot: false,
            profile: false,
            offline: false,
            flavor: None,
            size_str: DEFAULT_PRESET.to_string(),
        };

        let mut iter = args.into_iter().skip(1);
        while let Some(arg) = iter.next() {
            let (flag, inline_val) = match arg.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (arg.as_str(), None),
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
                "--no-hot" => cfg.no_hot = true,
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
    println!("  {} --mcp\n", PKG_NAME);
    println!("{}OPTIONS:{}", BOLD, RESET);
    let opts = [
        (
            "    --mcp, mcp",
            "Run as a native Model Context Protocol (MCP) JSON-RPC server over stdio",
        ),
        (
            "-s, --size <PRESET|WxH>",
            "Viewport size preset or custom WxH (default: mobile)",
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
        ("    --no-hot", "Disable hot reload support"),
        ("    --list-sizes", "Display all built-in device presets"),
        ("-V, --version", "Print version information"),
        ("-h, --help", "Print this help message"),
    ];
    for (flag, desc) in opts {
        println!("  {}{:<26}{} {}{}{}", CYAN, flag, RESET, TEXT, desc, RESET);
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
