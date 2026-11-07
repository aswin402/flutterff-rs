pub mod ipc;
pub mod json;
pub mod schema;

use std::env;
use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::config::{DEFAULT_PRESET, DEVICE_PRESETS, PKG_NAME, VERSION};

pub use ipc::{IpcCommand, IpcRequest, cleanup_socket, socket_path, spawn_ipc_listener};
pub use json::{JsonVal, base64_encode, j_arr, j_bool, j_num, j_obj, j_str};

use ipc::{DEFAULT_LOG_LIMIT, forward_ipc_action, forward_web_agent, send_ipc, tool_text_result};
use schema::{
    RESOURCE_SPECS, TOOL_SPECS, WEB_AGENT_TOOL_MAP, build_resources_schema, build_tools_schema,
};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const LAUNCH_POLL_ATTEMPTS: usize = 20;
const LAUNCH_POLL_INTERVAL: Duration = Duration::from_millis(150);

pub fn run_mcp_server() {
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    let mut stdout = std::io::stdout();
    let mut line = String::with_capacity(512);

    while handle.read_line(&mut line).is_ok_and(|n| n > 0) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }
        let Ok(msg) = JsonVal::parse(trimmed) else {
            line.clear();
            continue;
        };

        let Some(req_id) = msg.get("id").cloned() else {
            line.clear();
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
        line.clear();
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
