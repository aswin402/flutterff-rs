use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

use crate::config::{DEFAULT_PRESET, PKG_NAME, spawn_compact_thread};

use super::json::{JsonVal, j_arr, j_bool, j_obj, j_str};
use super::schema::WEB_AGENT_TOOL_MAP;

pub const DEFAULT_LOG_LIMIT: u64 = 50;
pub const IPC_TIMEOUT: Duration = Duration::from_secs(5);

static SOCKET_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn socket_path() -> &'static Path {
    SOCKET_PATH.get_or_init(|| {
        let base = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        base.join(format!("{}.sock", PKG_NAME))
    })
}

pub fn cleanup_socket() {
    let _ = fs::remove_file(socket_path());
}

pub enum IpcCommand {
    Status,
    HotReload,
    HotRestart,
    ToggleWatch {
        enabled: Option<bool>,
    },
    Navigate {
        url: Option<String>,
        nav: Option<String>,
    },
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
    pub fn from_json(req: &JsonVal) -> Option<Self> {
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
            "toggle_watch" => Some(Self::ToggleWatch {
                enabled: req.get("enabled").and_then(JsonVal::as_bool),
            }),
            "navigate" => Some(Self::Navigate {
                url: req.get("url").and_then(JsonVal::as_str).map(str::to_string),
                nav: req.get("nav").and_then(JsonVal::as_str).map(str::to_string),
            }),
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

pub fn spawn_ipc_listener<W>(ipc_tx: mpsc::Sender<IpcRequest>, wake_main: W)
where
    W: Fn() + Send + 'static,
{
    let sock = socket_path();
    let _ = fs::remove_file(sock);
    let Ok(listener) = UnixListener::bind(sock) else {
        return;
    };

    spawn_compact_thread("ff-ipc", move || {
        let mut line = String::with_capacity(256);
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(IPC_TIMEOUT));
            let _ = stream.set_write_timeout(Some(IPC_TIMEOUT));
            line.clear();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut line).is_ok()
                && let Ok(req_json) = JsonVal::parse(&line)
                && let Some(cmd) = IpcCommand::from_json(&req_json)
            {
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                if ipc_tx.send(IpcRequest { cmd, reply_tx }).is_ok() {
                    wake_main();
                    if let Ok(resp) = reply_rx.recv_timeout(IPC_TIMEOUT) {
                        let mut w = &stream;
                        let _ = writeln!(w, "{}", resp.to_json());
                        let _ = w.flush();
                    }
                }
            }
        }
    });
}

pub fn send_ipc(req: JsonVal) -> Result<JsonVal, String> {
    let sock = socket_path();
    let mut stream = UnixStream::connect(sock).map_err(|_| {
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

pub fn tool_text_result(text: impl Into<String>, is_error: bool) -> JsonVal {
    j_obj([
        (
            "content",
            j_arr([j_obj([("type", j_str("text")), ("text", j_str(text))])]),
        ),
        ("isError", j_bool(is_error)),
    ])
}

pub fn forward_ipc_action(
    action: &'static str,
    mut extra: Vec<(&'static str, JsonVal)>,
) -> JsonVal {
    extra.insert(0, ("action", j_str(action)));
    match send_ipc(j_obj(extra)) {
        Ok(resp) => {
            let is_err = resp.get("ok").and_then(JsonVal::as_bool) == Some(false);
            tool_text_result(resp.to_json(), is_err)
        }
        Err(e) => tool_text_result(e, true),
    }
}

pub fn forward_web_agent(action: &'static str, args: &JsonVal) -> JsonVal {
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
