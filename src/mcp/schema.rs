use super::json::{JsonVal, j_arr, j_obj, j_str};

pub struct ToolParamSpec {
    pub name: &'static str,
    pub kind: &'static str,
    pub desc: &'static str,
    pub required: bool,
}

pub struct ToolSpec {
    pub name: &'static str,
    pub desc: &'static str,
    pub params: &'static [ToolParamSpec],
}

pub struct ResourceSpec {
    pub uri: &'static str,
    pub name: &'static str,
    pub mime: &'static str,
    pub desc: &'static str,
}

pub const TOOL_SPECS: &[ToolSpec] = &[
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
            ToolParamSpec {
                name: "landscape",
                kind: "boolean",
                desc: "Launch in landscape orientation (swaps width and height).",
                required: false,
            },
            ToolParamSpec {
                name: "watch",
                kind: "boolean",
                desc: "Enable automatic inotify Hot Reload when lib/*.dart files are saved (default: true).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_hot_reload",
        desc: "Trigger an instant Hot Reload ('r') in the running flutterff-rs session, or toggle the automatic inotify .dart file-save watcher via 'auto_watch'.",
        params: &[ToolParamSpec {
            name: "auto_watch",
            kind: "boolean",
            desc: "Optional: enable (true) or pause (false) automatic inotify Hot Reload on lib/*.dart save.",
            required: false,
        }],
    },
    ToolSpec {
        name: "flutter_hot_restart",
        desc: "Trigger a crash-safe WebView Hot Restart ('R') in the running flutterff-rs session.",
        params: &[],
    },
    ToolSpec {
        name: "flutter_resize",
        desc: "Resize or rotate the running flutterff-rs simulator window. Pass a preset (mobile, mobile-small, iphone, tablet, desktop), custom WxH (e.g. 390x844), or 'rotate' / 'landscape' / 'portrait'.",
        params: &[ToolParamSpec {
            name: "size",
            kind: "string",
            desc: "Preset name, WxH dimensions, or 'rotate' / 'landscape' / 'portrait'.",
            required: true,
        }],
    },
    ToolSpec {
        name: "flutter_responsive_audit",
        desc: "Automatically step through all 5 viewport presets (mobile-small, iphone, mobile, tablet, desktop), check for Flutter RenderFlex overflow errors and horizontally clipped interactive widgets at each size, and restore the original viewport.",
        params: &[],
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
        desc: "Inspect the live Flutter Semantics & DOM UI tree (page-agent & obscura style). Auto-enables Flutter Web semantics and returns indexed interactive widgets ([0], [1], [2]...) with roles, labels, values, bounding-box coordinates, and active pointer mode.",
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
            ToolParamSpec {
                name: "max_chars",
                kind: "integer",
                desc: "Character budget for the returned summary to save agent context tokens (default: 4000).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_tap",
        desc: "Tap or click a Flutter widget by its UI tree index ([0], [1]...), visible text/label, or viewport (x, y) coordinates. Automatically uses a Red-Orange Touch Dot (●) + touch events on mobile/tablet viewports and a Red-Orange Bibata Cursor (↖) + mouse events on desktop viewports.",
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
        name: "flutter_fill_form",
        desc: "Fill multiple Flutter TextFields/inputs and optionally tap a submit button in a single call (Obscura browser_fill_form style). Saves N round-trips when testing multi-field forms.",
        params: &[
            ToolParamSpec {
                name: "fields",
                kind: "array",
                desc: "Array of field objects: [{ index?: number, target_text?: string, value: string, clear?: boolean }].",
                required: true,
            },
            ToolParamSpec {
                name: "submit_index",
                kind: "integer",
                desc: "Optional UI tree index of the submit button to tap after filling all fields.",
                required: false,
            },
            ToolParamSpec {
                name: "submit_text",
                kind: "string",
                desc: "Optional visible label of the submit button to tap after filling all fields (e.g. 'Sign In', 'Submit').",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_wait_for",
        desc: "Wait until a widget label/text appears (or disappears) in the Flutter UI tree or a JS expression becomes truthy (Obscura wait_for_text style).",
        params: &[
            ToolParamSpec {
                name: "text",
                kind: "string",
                desc: "Case-insensitive text/label substring to wait for in the Flutter UI tree or page text.",
                required: false,
            },
            ToolParamSpec {
                name: "role",
                kind: "string",
                desc: "Optional widget role to match (e.g. 'button', 'textfield', 'dialog').",
                required: false,
            },
            ToolParamSpec {
                name: "gone",
                kind: "boolean",
                desc: "If true, wait for the matching text/widget to disappear instead of appear (default: false).",
                required: false,
            },
            ToolParamSpec {
                name: "timeout_ms",
                kind: "integer",
                desc: "Maximum time to wait in milliseconds (default: 4000).",
                required: false,
            },
            ToolParamSpec {
                name: "poll_ms",
                kind: "integer",
                desc: "Polling interval in milliseconds (default: 100).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_network",
        desc: "Inspect recent HTTP fetch and XMLHttpRequest calls made by the Flutter Web app (method, URL, status code, latency in ms, and error state).",
        params: &[
            ToolParamSpec {
                name: "limit",
                kind: "integer",
                desc: "Maximum number of recent network requests to return (default: 50).",
                required: false,
            },
            ToolParamSpec {
                name: "clear",
                kind: "boolean",
                desc: "Clear the captured network request buffer after reading (default: false).",
                required: false,
            },
        ],
    },
    ToolSpec {
        name: "flutter_scroll",
        desc: "Scroll or swipe the Flutter viewport or a specific scrollable widget using semantic directions ('down', 'up', 'top', 'bottom', 'left', 'right') or pixel deltas (dx, dy).",
        params: &[
            ToolParamSpec {
                name: "direction",
                kind: "string",
                desc: "Semantic scroll direction: 'down', 'up', 'top', 'bottom', 'left', or 'right'.",
                required: false,
            },
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
pub const WEB_AGENT_TOOL_MAP: &[(&str, &str)] = &[
    ("flutter_ui_tree", "ui_tree"),
    ("flutter_tap", "tap"),
    ("flutter_type", "type_text"),
    ("flutter_fill_form", "fill_form"),
    ("flutter_wait_for", "wait_for"),
    ("flutter_network", "network"),
    ("flutter_scroll", "scroll"),
    ("flutter_press_key", "press_key"),
    ("flutter_inspect_overlay", "inspect_overlay"),
    ("flutter_eval_js", "eval_js"),
];

pub const RESOURCE_SPECS: &[ResourceSpec] = &[
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
        uri: "flutterff://network",
        name: "flutterff-rs Live Network Requests",
        mime: "application/json",
        desc: "Captured fetch & XMLHttpRequest calls made by the Flutter Web app.",
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

pub fn build_tools_schema() -> JsonVal {
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

pub fn build_resources_schema() -> JsonVal {
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
