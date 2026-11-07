use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::{DEFAULT_PRESET, DEVICE_PRESETS, PKG_NAME, VERSION};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const DEFAULT_LOG_LIMIT: u64 = 50;
const IPC_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCH_POLL_ATTEMPTS: usize = 20;
const LAUNCH_POLL_INTERVAL: Duration = Duration::from_millis(150);

pub fn socket_path() -> PathBuf {
    let base = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    base.join(format!("{}.sock", PKG_NAME))
}

pub fn cleanup_socket() {
    let _ = fs::remove_file(socket_path());
}

// ─────────────────────────────────────────────────────────────────────────────
//  Zero-Dependency Byte-Slice JSON Parser & Serializer
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
pub enum JsonVal {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JsonVal>),
    Obj(Vec<(String, JsonVal)>),
}

impl JsonVal {
    pub fn get(&self, key: &str) -> Option<&JsonVal> {
        match self {
            Self::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Num(n) if *n >= 0.0 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{:.0}", n));
                } else {
                    out.push_str(&n.to_string());
                }
            }
            Self::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Self::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Self::Obj(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    Self::Str(k.clone()).write_json(out);
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }

    /// Parses JSON directly over `&[u8]` without allocating a `Vec<char>`
    pub fn parse(input: &str) -> Result<Self, String> {
        let bytes = input.as_bytes();
        let mut pos = 0;
        let val = parse_value(bytes, &mut pos)?;
        skip_ws(bytes, &mut pos);
        Ok(val)
    }
}

fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn parse_value(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    skip_ws(bytes, pos);
    let b = *bytes
        .get(*pos)
        .ok_or_else(|| "Unexpected EOF".to_string())?;
    match b {
        b'"' => parse_string(bytes, pos).map(JsonVal::Str),
        b'{' => parse_object(bytes, pos),
        b'[' => parse_array(bytes, pos),
        b't' if bytes[*pos..].starts_with(b"true") => {
            *pos += 4;
            Ok(JsonVal::Bool(true))
        }
        b'f' if bytes[*pos..].starts_with(b"false") => {
            *pos += 5;
            Ok(JsonVal::Bool(false))
        }
        b'n' if bytes[*pos..].starts_with(b"null") => {
            *pos += 4;
            Ok(JsonVal::Null)
        }
        c if c == b'-' || c.is_ascii_digit() => parse_number(bytes, pos),
        other => Err(format!("Unexpected byte '{}'", other as char)),
    }
}

fn parse_string(bytes: &[u8], pos: &mut usize) -> Result<String, String> {
    *pos += 1; // skip opening '"'
    let mut out = String::new();
    while *pos < bytes.len() {
        let b = bytes[*pos];
        *pos += 1;
        match b {
            b'"' => return Ok(out),
            b'\\' => {
                let esc = *bytes
                    .get(*pos)
                    .ok_or_else(|| "Unterminated escape".to_string())?;
                *pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let slice = bytes
                            .get(*pos..*pos + 4)
                            .ok_or_else(|| "Invalid \\u escape".to_string())?;
                        let hex = std::str::from_utf8(slice)
                            .map_err(|_| "Invalid UTF-8 in \\u escape".to_string())?;
                        *pos += 4;
                        let code = u32::from_str_radix(hex, 16)
                            .map_err(|_| "Invalid hex in \\u escape".to_string())?;
                        if let Some(ch) = char::from_u32(code) {
                            out.push(ch);
                        }
                    }
                    other => out.push(other as char),
                }
            }
            _ => {
                // Decode multi-byte UTF-8 sequence starting at `pos - 1`
                let start = *pos - 1;
                let rest = std::str::from_utf8(&bytes[start..])
                    .map_err(|_| "Invalid UTF-8 in JSON string".to_string())?;
                let ch = rest.chars().next().unwrap();
                *pos = start + ch.len_utf8();
                out.push(ch);
            }
        }
    }
    Err("Unterminated string".to_string())
}

fn parse_number(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    let start = *pos;
    if bytes[*pos] == b'-' {
        *pos += 1;
    }
    while *pos < bytes.len()
        && (bytes[*pos].is_ascii_digit() || matches!(bytes[*pos], b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        *pos += 1;
    }
    let num_str = std::str::from_utf8(&bytes[start..*pos])
        .map_err(|_| "Invalid UTF-8 in number".to_string())?;
    let n = num_str
        .parse::<f64>()
        .map_err(|_| format!("Invalid number '{}'", num_str))?;
    Ok(JsonVal::Num(n))
}

fn parse_array(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    *pos += 1; // skip '['
    let mut items = Vec::new();
    loop {
        skip_ws(bytes, pos);
        let b = *bytes
            .get(*pos)
            .ok_or_else(|| "Unterminated array".to_string())?;
        if b == b']' {
            *pos += 1;
            return Ok(JsonVal::Arr(items));
        }
        items.push(parse_value(bytes, pos)?);
        skip_ws(bytes, pos);
        if bytes.get(*pos) == Some(&b',') {
            *pos += 1;
        }
    }
}

fn parse_object(bytes: &[u8], pos: &mut usize) -> Result<JsonVal, String> {
    *pos += 1; // skip '{'
    let mut entries = Vec::new();
    loop {
        skip_ws(bytes, pos);
        let b = *bytes
            .get(*pos)
            .ok_or_else(|| "Unterminated object".to_string())?;
        if b == b'}' {
            *pos += 1;
            return Ok(JsonVal::Obj(entries));
        }
        if b != b'"' {
            return Err("Expected string key in object".to_string());
        }
        let key = parse_string(bytes, pos)?;
        skip_ws(bytes, pos);
        if bytes.get(*pos) != Some(&b':') {
            return Err("Expected ':' after object key".to_string());
        }
        *pos += 1;
        let val = parse_value(bytes, pos)?;
        entries.push((key, val));
        skip_ws(bytes, pos);
        if bytes.get(*pos) == Some(&b',') {
            *pos += 1;
        }
    }
}

// Helper constructors for building JSON trees cleanly
pub fn j_str(s: impl Into<String>) -> JsonVal {
    JsonVal::Str(s.into())
}

pub fn j_bool(b: bool) -> JsonVal {
    JsonVal::Bool(b)
}

pub fn j_num(n: impl Into<f64>) -> JsonVal {
    JsonVal::Num(n.into())
}

pub fn j_arr(items: impl Into<Vec<JsonVal>>) -> JsonVal {
    JsonVal::Arr(items.into())
}

pub fn j_obj(entries: impl Into<Vec<(&'static str, JsonVal)>>) -> JsonVal {
    JsonVal::Obj(
        entries
            .into()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
//  Pure-Rust Base64 Encoder (for MCP image/png Screenshot Responses)
// ─────────────────────────────────────────────────────────────────────────────
const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
//  Table-Driven MCP Tool & Resource Specifications
// ─────────────────────────────────────────────────────────────────────────────
struct ToolParamSpec {
    name: &'static str,
    kind: &'static str,
    desc: &'static str,
    required: bool,
}

struct ToolSpec {
    name: &'static str,
    desc: &'static str,
    params: &'static [ToolParamSpec],
}

struct ResourceSpec {
    uri: &'static str,
    name: &'static str,
    mime: &'static str,
    desc: &'static str,
}

const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "flutter_status",
        desc: "Get live status of the running flutterff-rs simulator session (URL, port, window size, mode, and log count).",
        params: &[],
    },
    ToolSpec {
        name: "flutter_launch",
        desc: "Launch a new flutterff-rs GTK3 + WebKit2 simulator window for a Flutter project.",
        params: &[
            ToolParamSpec {
                name: "project_dir",
                kind: "string",
                desc: "Path to the Flutter project directory (defaults to current working directory).",
                required: false,
            },
            ToolParamSpec {
                name: "size",
                kind: "string",
                desc: "Device preset (mobile, mobile-small, iphone, tablet, desktop) or custom WxH (e.g. 430x932).",
                required: false,
            },
            ToolParamSpec {
                name: "port",
                kind: "integer",
                desc: "HTTP port for the Flutter web-server (default: 8080).",
                required: false,
            },
            ToolParamSpec {
                name: "offline",
                kind: "boolean",
                desc: "Force offline mode (--no-pub --no-web-resources-cdn).",
                required: false,
            },
            ToolParamSpec {
                name: "profile",
                kind: "boolean",
                desc: "Run Flutter in --profile mode.",
                required: false,
            },
            ToolParamSpec {
                name: "flavor",
                kind: "string",
                desc: "Custom Flutter build flavor.",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_hot_reload",
        desc: "Trigger an instant Hot Reload ('r') in the running flutterff-rs session.",
        params: &[],
    },
    ToolSpec {
        name: "flutter_hot_restart",
        desc: "Trigger a crash-safe WebView Hot Restart ('R') in the running flutterff-rs session.",
        params: &[],
    },
    ToolSpec {
        name: "flutter_resize",
        desc: "Resize the running flutterff-rs simulator window to a preset (mobile, mobile-small, iphone, tablet, desktop) or custom WxH (e.g. 390x844).",
        params: &[ToolParamSpec {
            name: "size",
            kind: "string",
            desc: "Preset name or WxH dimensions.",
            required: true,
        }],
    },
    ToolSpec {
        name: "flutter_screenshot",
        desc: "Capture a native Cairo PNG screenshot of the live Flutter WebView, saving it to screenshots/ and optionally returning inline base64 image data.",
        params: &[ToolParamSpec {
            name: "include_image",
            kind: "boolean",
            desc: "Include inline base64 image/png content in the tool response (default: true).",
            required: false,
        }],
    },
    ToolSpec {
        name: "flutter_logs",
        desc: "Read recent formatted Flutter and WebView runtime logs (INF, WRN, ERR, DBG).",
        params: &[
            ToolParamSpec {
                name: "limit",
                kind: "integer",
                desc: "Maximum number of recent log lines to return (default: 50).",
                required: false,
            },
            ToolParamSpec {
                name: "level",
                kind: "string",
                desc: "Optional filter by log tag (ERR, WRN, INF, DBG, HOT, WIN, SCR).",
                required: false,
            },
            ToolParamSpec {
                name: "clear",
                kind: "boolean",
                desc: "Clear the log buffer after reading (default: false).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_ui_tree",
        desc: "Inspect the live Flutter Semantics & DOM UI tree (page-agent style). Auto-enables Flutter Web semantics and returns indexed interactive widgets ([0], [1], [2]...) with roles, labels, values, bounding-box coordinates, and active pointer mode (desktop 'cursor' vs mobile 'touch' dot).",
        params: &[
            ToolParamSpec {
                name: "include_static",
                kind: "boolean",
                desc: "Include static text/label semantics nodes in addition to interactive widgets (default: false).",
                required: false,
            },
            ToolParamSpec {
                name: "overlay",
                kind: "boolean",
                desc: "Render visual [0], [1] Set-of-Mark bounding-box badges over the simulator window (default: false).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_tap",
        desc: "Tap or click a Flutter widget by its UI tree index ([0], [1]...), visible text/label, or viewport (x, y) coordinates. Automatically uses a glowing Touch Dot (●) + touch events on mobile/tablet viewports and an Electric Cyan Cursor (↖) + mouse events on desktop viewports.",
        params: &[
            ToolParamSpec {
                name: "index",
                kind: "integer",
                desc: "Element index from flutter_ui_tree (e.g. 0, 1, 2).",
                required: false,
            },
            ToolParamSpec {
                name: "text",
                kind: "string",
                desc: "Case-insensitive widget label or text substring to tap (e.g. 'Sign In').",
                required: false,
            },
            ToolParamSpec {
                name: "x",
                kind: "integer",
                desc: "Explicit viewport X coordinate (used when index and text are omitted).",
                required: false,
            },
            ToolParamSpec {
                name: "y",
                kind: "integer",
                desc: "Explicit viewport Y coordinate (used when index and text are omitted).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_type",
        desc: "Type text into a Flutter TextField or HTML input. Can optionally tap a target widget by index or label first to focus it, clear existing text, and press Enter to submit.",
        params: &[
            ToolParamSpec {
                name: "text",
                kind: "string",
                desc: "Text string to type into the input field.",
                required: true,
            },
            ToolParamSpec {
                name: "index",
                kind: "integer",
                desc: "Optional UI tree index of the textfield to tap/focus before typing.",
                required: false,
            },
            ToolParamSpec {
                name: "target_text",
                kind: "string",
                desc: "Optional label or placeholder of the textfield to tap/focus before typing.",
                required: false,
            },
            ToolParamSpec {
                name: "clear",
                kind: "boolean",
                desc: "Clear existing field value before typing (default: true).",
                required: false,
            },
            ToolParamSpec {
                name: "submit",
                kind: "boolean",
                desc: "Dispatch Enter key after typing to submit (default: false).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_scroll",
        desc: "Scroll or swipe the Flutter viewport or a specific scrollable widget. Animates a touch swipe dot on mobile or cursor wheel scroll on desktop.",
        params: &[
            ToolParamSpec {
                name: "dx",
                kind: "integer",
                desc: "Horizontal scroll delta in pixels (default: 0).",
                required: false,
            },
            ToolParamSpec {
                name: "dy",
                kind: "integer",
                desc: "Vertical scroll delta in pixels (positive = scroll down, negative = scroll up, default: 300).",
                required: false,
            },
            ToolParamSpec {
                name: "index",
                kind: "integer",
                desc: "Optional UI tree index of the scrollable container.",
                required: false,
            },
            ToolParamSpec {
                name: "x",
                kind: "integer",
                desc: "Optional viewport X coordinate to scroll at.",
                required: false,
            },
            ToolParamSpec {
                name: "y",
                kind: "integer",
                desc: "Optional viewport Y coordinate to scroll at.",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_press_key",
        desc: "Dispatch a keyboard key press (e.g. 'Enter', 'Escape', 'Backspace', 'Tab', 'ArrowDown', 'ArrowUp') to the active Flutter widget.",
        params: &[ToolParamSpec {
            name: "key",
            kind: "string",
            desc: "Key name to press (e.g. 'Enter', 'Escape', 'Tab', 'Backspace').",
            required: true,
        }],
    },
    ToolSpec {
        name: "flutter_inspect_overlay",
        desc: "Toggle the visual Set-of-Mark ([0], [1]...) widget bounding-box overlay and configure the pointer mode ('auto', 'cursor' for desktop arrow, or 'touch' for mobile dot).",
        params: &[
            ToolParamSpec {
                name: "enabled",
                kind: "boolean",
                desc: "Enable or disable the [0], [1] bounding-box inspector overlay (toggles if omitted).",
                required: false,
            },
            ToolParamSpec {
                name: "pointer_mode",
                kind: "string",
                desc: "Pointer visualization & event mode: 'auto' (desktop=cursor, mobile=touch dot), 'cursor', or 'touch'.",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_eval_js",
        desc: "Execute a JavaScript snippet inside the running WebKit2 WebView and return the evaluated result.",
        params: &[ToolParamSpec {
            name: "script",
            kind: "string",
            desc: "JavaScript code to evaluate in the WebView (use `return ...` to return a value).",
            required: true,
        }],
    },
    ToolSpec {
        name: "flutter_stop",
        desc: "Gracefully stop the running flutterff-rs simulator and Flutter daemon.",
        params: &[],
    },
];

/// Maps MCP tool names to in-page `window.__flutterff` action names
const WEB_AGENT_TOOL_MAP: &[(&str, &str)] = &[
    ("flutter_ui_tree", "ui_tree"),
    ("flutter_tap", "tap"),
    ("flutter_type", "type_text"),
    ("flutter_scroll", "scroll"),
    ("flutter_press_key", "press_key"),
    ("flutter_inspect_overlay", "inspect_overlay"),
    ("flutter_eval_js", "eval_js"),
];

const RESOURCE_SPECS: &[ResourceSpec] = &[
    ResourceSpec {
        uri: "flutterff://status",
        name: "flutterff-rs Live Status",
        mime: "application/json",
        desc: "Current simulator status, window size, URL, and mode.",
    },
    ResourceSpec {
        uri: "flutterff://ui_tree",
        name: "flutterff-rs Live UI Semantics Tree",
        mime: "text/plain",
        desc: "Page-agent style indexed Flutter Semantics & DOM widget tree ([0], [1]...).",
    },
    ResourceSpec {
        uri: "flutterff://logs",
        name: "flutterff-rs Runtime Logs",
        mime: "text/plain",
        desc: "Recent filtered Flutter & WebView logs.",
    },
    ResourceSpec {
        uri: "flutterff://presets",
        name: "flutterff-rs Device Presets",
        mime: "application/json",
        desc: "Built-in mobile, tablet, and desktop viewport presets.",
    },
];

fn build_tools_schema() -> JsonVal {
    let tools: Vec<JsonVal> = TOOL_SPECS
        .iter()
        .map(|spec| {
            let props: Vec<(&'static str, JsonVal)> = spec
                .params
                .iter()
                .map(|p| {
                    (
                        p.name,
                        j_obj([("type", j_str(p.kind)), ("description", j_str(p.desc))]),
                    )
                })
                .collect();
            let req: Vec<JsonVal> = spec
                .params
                .iter()
                .filter(|p| p.required)
                .map(|p| j_str(p.name))
                .collect();

            let mut schema_fields = vec![("type", j_str("object")), ("properties", j_obj(props))];
            if !req.is_empty() {
                schema_fields.push(("required", j_arr(req)));
            }

            j_obj([
                ("name", j_str(spec.name)),
                ("description", j_str(spec.desc)),
                ("inputSchema", j_obj(schema_fields)),
            ])
        })
        .collect();
    j_arr(tools)
}

fn build_resources_schema() -> JsonVal {
    let resources: Vec<JsonVal> = RESOURCE_SPECS
        .iter()
        .map(|r| {
            j_obj([
                ("uri", j_str(r.uri)),
                ("name", j_str(r.name)),
                ("mimeType", j_str(r.mime)),
                ("description", j_str(r.desc)),
            ])
        })
        .collect();
    j_arr(resources)
}

// ─────────────────────────────────────────────────────────────────────────────
//  IPC Bridge (Between Running GTK Simulator & MCP Server)
// ─────────────────────────────────────────────────────────────────────────────
pub enum IpcCommand {
    Status,
    HotReload,
    HotRestart,
    Resize {
        size: String,
    },
    Screenshot,
    Logs {
        limit: usize,
        level: Option<String>,
        clear: bool,
    },
    WebAgent {
        payload: JsonVal,
    },
    Stop,
}

impl IpcCommand {
    fn from_json(req: &JsonVal) -> Option<Self> {
        let action = req.get("action").and_then(JsonVal::as_str)?;
        if WEB_AGENT_TOOL_MAP.iter().any(|&(_, a)| a == action) {
            return Some(Self::WebAgent {
                payload: req.clone(),
            });
        }
        match action {
            "status" => Some(Self::Status),
            "hot_reload" => Some(Self::HotReload),
            "hot_restart" => Some(Self::HotRestart),
            "resize" => Some(Self::Resize {
                size: req
                    .get("size")
                    .and_then(JsonVal::as_str)
                    .unwrap_or(DEFAULT_PRESET)
                    .to_string(),
            }),
            "screenshot" => Some(Self::Screenshot),
            "logs" => Some(Self::Logs {
                limit: req
                    .get("limit")
                    .and_then(JsonVal::as_u64)
                    .unwrap_or(DEFAULT_LOG_LIMIT) as usize,
                level: req
                    .get("level")
                    .and_then(JsonVal::as_str)
                    .map(str::to_string),
                clear: req.get("clear").and_then(JsonVal::as_bool).unwrap_or(false),
            }),
            "stop" => Some(Self::Stop),
            _ => None,
        }
    }
}

pub struct IpcRequest {
    pub cmd: IpcCommand,
    pub reply_tx: mpsc::SyncSender<JsonVal>,
}

pub fn spawn_ipc_listener(ipc_tx: mpsc::Sender<IpcRequest>) {
    let sock = socket_path();
    let _ = fs::remove_file(&sock);
    let Ok(listener) = UnixListener::bind(&sock) else {
        return;
    };

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = ipc_tx.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok()
                    && let Ok(req_json) = JsonVal::parse(&line)
                    && let Some(cmd) = IpcCommand::from_json(&req_json)
                {
                    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                    if tx.send(IpcRequest { cmd, reply_tx }).is_ok()
                        && let Ok(resp) = reply_rx.recv_timeout(IPC_TIMEOUT)
                    {
                        let mut w = &stream;
                        let _ = writeln!(w, "{}", resp.to_json());
                        let _ = w.flush();
                    }
                }
            });
        }
    });
}

fn send_ipc(req: JsonVal) -> Result<JsonVal, String> {
    let sock = socket_path();
    let mut stream = UnixStream::connect(&sock).map_err(|_| {
        format!(
            "No running {} GUI instance found. Use `flutter_launch` or start `{}` first.",
            PKG_NAME, PKG_NAME
        )
    })?;
    let _ = stream.set_read_timeout(Some(IPC_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IPC_TIMEOUT));
    writeln!(stream, "{}", req.to_json()).map_err(|e| format!("IPC write failed: {}", e))?;
    stream
        .flush()
        .map_err(|e| format!("IPC flush failed: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("IPC read failed: {}", e))?;
    JsonVal::parse(&line)
}

fn forward_web_agent(action: &'static str, args: &JsonVal) -> JsonVal {
    let mut entries = match args {
        JsonVal::Obj(list) => list.clone(),
        _ => Vec::new(),
    };
    entries.insert(0, ("action".to_string(), j_str(action)));
    match send_ipc(JsonVal::Obj(entries)) {
        Ok(resp) => {
            let is_err = resp.get("ok").and_then(JsonVal::as_bool) == Some(false);
            if action == "ui_tree"
                && !is_err
                && let Some(summary) = resp.get("summary").and_then(JsonVal::as_str)
            {
                return tool_text_result(format!("{}\n\n{}", summary, resp.to_json()), false);
            }
            tool_text_result(resp.to_json(), is_err)
        }
        Err(e) => tool_text_result(e, true),
    }
}

fn forward_ipc_action(action: &'static str, mut extra: Vec<(&'static str, JsonVal)>) -> JsonVal {
    extra.insert(0, ("action", j_str(action)));
    match send_ipc(j_obj(extra)) {
        Ok(resp) => {
            let is_err = resp.get("ok").and_then(JsonVal::as_bool) == Some(false);
            tool_text_result(resp.to_json(), is_err)
        }
        Err(e) => tool_text_result(e, true),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Native Rust MCP Server (`flutterff-rs --mcp` / `flutterff-rs mcp`)
// ─────────────────────────────────────────────────────────────────────────────
pub fn run_mcp_server() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(msg) = JsonVal::parse(trimmed) else {
            continue;
        };

        let Some(req_id) = msg.get("id").cloned() else {
            continue;
        };
        let method = msg.get("method").and_then(JsonVal::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(JsonVal::Null);

        let response = match method {
            "initialize" => rpc_ok(
                req_id,
                j_obj([
                    ("protocolVersion", j_str(MCP_PROTOCOL_VERSION)),
                    (
                        "capabilities",
                        j_obj([
                            ("tools", j_obj([("listChanged", j_bool(false))])),
                            ("resources", j_obj([("listChanged", j_bool(false))])),
                        ]),
                    ),
                    (
                        "serverInfo",
                        j_obj([("name", j_str(PKG_NAME)), ("version", j_str(VERSION))]),
                    ),
                ]),
            ),
            "ping" => rpc_ok(req_id, j_obj([])),
            "tools/list" => rpc_ok(req_id, j_obj([("tools", build_tools_schema())])),
            "tools/call" => handle_tool_call(req_id, &params),
            "resources/list" => rpc_ok(req_id, j_obj([("resources", build_resources_schema())])),
            "resources/read" => handle_resource_read(req_id, &params),
            other => rpc_err(req_id, -32601, &format!("Method not found: {}", other)),
        };

        let _ = writeln!(stdout, "{}", response.to_json());
        let _ = stdout.flush();
    }
}

fn rpc_ok(id: JsonVal, result: JsonVal) -> JsonVal {
    j_obj([("jsonrpc", j_str("2.0")), ("id", id), ("result", result)])
}

fn rpc_err(id: JsonVal, code: i32, message: &str) -> JsonVal {
    j_obj([
        ("jsonrpc", j_str("2.0")),
        ("id", id),
        (
            "error",
            j_obj([("code", j_num(code)), ("message", j_str(message))]),
        ),
    ])
}

fn tool_text_result(text: impl Into<String>, is_error: bool) -> JsonVal {
    j_obj([
        (
            "content",
            j_arr([j_obj([("type", j_str("text")), ("text", j_str(text))])]),
        ),
        ("isError", j_bool(is_error)),
    ])
}

fn resource_content_response(req_id: JsonVal, uri: &str, mime: &str, text: String) -> JsonVal {
    rpc_ok(
        req_id,
        j_obj([(
            "contents",
            j_arr([j_obj([
                ("uri", j_str(uri)),
                ("mimeType", j_str(mime)),
                ("text", j_str(text)),
            ])]),
        )]),
    )
}

fn handle_resource_read(req_id: JsonVal, params: &JsonVal) -> JsonVal {
    let uri = params.get("uri").and_then(JsonVal::as_str).unwrap_or("");
    let Some(spec) = RESOURCE_SPECS.iter().find(|r| r.uri == uri) else {
        return rpc_err(req_id, -32602, &format!("Unknown resource URI: {}", uri));
    };

    let text = match spec.uri {
        "flutterff://presets" => {
            let presets: Vec<JsonVal> = DEVICE_PRESETS
                .iter()
                .map(|&(name, (w, h))| {
                    j_obj([
                        ("name", j_str(name)),
                        ("width", j_num(w)),
                        ("height", j_num(h)),
                    ])
                })
                .collect();
            j_arr(presets).to_json()
        }
        "flutterff://status" => send_ipc(j_obj([("action", j_str("status"))]))
            .unwrap_or_else(|e| j_obj([("running", j_bool(false)), ("message", j_str(e))]))
            .to_json(),
        "flutterff://ui_tree" => send_ipc(j_obj([("action", j_str("ui_tree"))]))
            .ok()
            .and_then(|resp| {
                resp.get("summary")
                    .and_then(JsonVal::as_str)
                    .map(String::from)
            })
            .unwrap_or_else(|| "No running simulator session.".to_string()),
        "flutterff://logs" => send_ipc(j_obj([
            ("action", j_str("logs")),
            ("limit", j_num((DEFAULT_LOG_LIMIT * 2) as f64)),
        ]))
        .ok()
        .and_then(|resp| resp.get("logs").and_then(JsonVal::as_str).map(String::from))
        .unwrap_or_default(),
        _ => String::new(),
    };

    resource_content_response(req_id, spec.uri, spec.mime, text)
}

fn handle_tool_call(req_id: JsonVal, params: &JsonVal) -> JsonVal {
    let name = params.get("name").and_then(JsonVal::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| j_obj([]));

    // Validate required parameters from TOOL_SPECS automatically
    if let Some(spec) = TOOL_SPECS.iter().find(|t| t.name == name) {
        for param in spec.params.iter().filter(|p| p.required) {
            if args.get(param.name).is_none() {
                return rpc_ok(
                    req_id,
                    tool_text_result(format!("Missing required '{}' argument", param.name), true),
                );
            }
        }
    }

    if let Some(&(_, agent_action)) = WEB_AGENT_TOOL_MAP.iter().find(|&&(t, _)| t == name) {
        return rpc_ok(req_id, forward_web_agent(agent_action, &args));
    }

    let result = match name {
        "flutter_status" => match send_ipc(j_obj([("action", j_str("status"))])) {
            Ok(resp) => tool_text_result(resp.to_json(), false),
            Err(e) => tool_text_result(
                j_obj([("running", j_bool(false)), ("info", j_str(e))]).to_json(),
                false,
            ),
        },
        "flutter_launch" => handle_launch_tool(&args),
        "flutter_hot_reload" => forward_ipc_action("hot_reload", vec![]),
        "flutter_hot_restart" => forward_ipc_action("hot_restart", vec![]),
        "flutter_stop" => forward_ipc_action("stop", vec![]),
        "flutter_resize" => {
            let size = args
                .get("size")
                .and_then(JsonVal::as_str)
                .unwrap_or(DEFAULT_PRESET);
            forward_ipc_action("resize", vec![("size", j_str(size))])
        }
        "flutter_screenshot" => {
            let include_img = args
                .get("include_image")
                .and_then(JsonVal::as_bool)
                .unwrap_or(true);
            match send_ipc(j_obj([("action", j_str("screenshot"))])) {
                Ok(resp) => {
                    if !resp.get("ok").and_then(JsonVal::as_bool).unwrap_or(false) {
                        return rpc_ok(req_id, tool_text_result(resp.to_json(), true));
                    }
                    let path = resp
                        .get("path")
                        .and_then(JsonVal::as_str)
                        .unwrap_or("")
                        .to_string();
                    let mut content = vec![j_obj([
                        ("type", j_str("text")),
                        ("text", j_str(resp.to_json())),
                    ])];
                    if include_img
                        && !path.is_empty()
                        && let Ok(bytes) = fs::read(&path)
                    {
                        content.push(j_obj([
                            ("type", j_str("image")),
                            ("data", j_str(base64_encode(&bytes))),
                            ("mimeType", j_str("image/png")),
                        ]));
                    }
                    j_obj([("content", j_arr(content)), ("isError", j_bool(false))])
                }
                Err(e) => tool_text_result(e, true),
            }
        }
        "flutter_logs" => {
            let limit = args
                .get("limit")
                .and_then(JsonVal::as_u64)
                .unwrap_or(DEFAULT_LOG_LIMIT);
            let clear = args
                .get("clear")
                .and_then(JsonVal::as_bool)
                .unwrap_or(false);
            let mut req_fields = vec![
                ("action", j_str("logs")),
                ("limit", j_num(limit as f64)),
                ("clear", j_bool(clear)),
            ];
            if let Some(level) = args.get("level").and_then(JsonVal::as_str)
                && !level.is_empty()
            {
                req_fields.push(("level", j_str(level)));
            }
            match send_ipc(j_obj(req_fields)) {
                Ok(resp) => tool_text_result(
                    resp.get("logs")
                        .and_then(JsonVal::as_str)
                        .unwrap_or("(no logs)"),
                    false,
                ),
                Err(e) => tool_text_result(e, true),
            }
        }
        other => tool_text_result(format!("Unknown tool '{}'", other), true),
    };

    rpc_ok(req_id, result)
}

fn handle_launch_tool(args: &JsonVal) -> JsonVal {
    if let Ok(existing) = send_ipc(j_obj([("action", j_str("status"))])) {
        return tool_text_result(
            format!("{} is already running: {}", PKG_NAME, existing.to_json()),
            false,
        );
    }

    let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from(PKG_NAME));
    let mut cmd = Command::new(exe);

    if let Some(dir) = args.get("project_dir").and_then(JsonVal::as_str) {
        cmd.current_dir(dir);
    }
    for (key, flag) in [("size", "--size"), ("flavor", "--flavor")] {
        if let Some(val) = args.get(key).and_then(JsonVal::as_str) {
            cmd.args([flag, val]);
        }
    }
    if let Some(port) = args.get("port").and_then(JsonVal::as_u64) {
        cmd.args(["--port", &port.to_string()]);
    }
    for (key, flag) in [("offline", "--offline"), ("profile", "--profile")] {
        if args.get(key).and_then(JsonVal::as_bool).unwrap_or(false) {
            cmd.arg(flag);
        }
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            for _ in 0..LAUNCH_POLL_ATTEMPTS {
                thread::sleep(LAUNCH_POLL_INTERVAL);
                if let Ok(status) = send_ipc(j_obj([("action", j_str("status"))])) {
                    return tool_text_result(status.to_json(), false);
                }
            }
            tool_text_result(
                j_obj([
                    ("ok", j_bool(true)),
                    ("pid", j_num(pid as f64)),
                    ("status", j_str("spawned (initializing)")),
                ])
                .to_json(),
                false,
            )
        }
        Err(e) => tool_text_result(format!("Failed to launch {}: {}", PKG_NAME, e), true),
    }
}
