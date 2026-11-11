//! sway, and other compositors that speak its IPC, over `$SWAYSOCK`.
//!
//! sway's IPC can't report the cursor position, so the window under the
//! cursor is taken to be the focused one: with sway's default
//! `focus_follows_mouse yes`, moving onto a window focuses it, and a gesture
//! only starts once the pointer has moved.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::{Backend, Target, Window};

const RUN_COMMAND: u32 = 0;
const GET_OUTPUTS: u32 = 3;
const GET_TREE: u32 = 4;
const MAGIC: &[u8] = b"i3-ipc";

pub struct Sway {
    socket: PathBuf,
}

impl Sway {
    pub fn connect() -> anyhow::Result<Self> {
        let socket = find_socket().context("no sway socket found; is sway running?")?;
        log::info!("sway socket: {}", socket.display());
        Ok(Sway { socket })
    }

    fn request(&self, kind: u32, payload: &str) -> anyhow::Result<Value> {
        let mut stream =
            UnixStream::connect(&self.socket).with_context(|| format!("connecting to {}", self.socket.display()))?;
        let mut message = MAGIC.to_vec();
        message.extend((payload.len() as u32).to_ne_bytes());
        message.extend(kind.to_ne_bytes());
        message.extend(payload.as_bytes());
        stream.write_all(&message)?;

        let mut header = [0; 14];
        stream.read_exact(&mut header)?;
        if &header[..6] != MAGIC {
            bail!("not a sway IPC reply");
        }
        let len = u32::from_ne_bytes(header[6..10].try_into().expect("4 bytes"));
        let mut body = vec![0; len as usize];
        stream.read_exact(&mut body)?;
        Ok(serde_json::from_slice(&body)?)
    }

    fn command(&self, command: &str) -> anyhow::Result<()> {
        let replies: Vec<CommandReply> = serde_json::from_value(self.request(RUN_COMMAND, command)?)?;
        match replies.into_iter().find(|r| !r.success) {
            Some(failed) => bail!("sway {command:?}: {}", failed.error.unwrap_or_default()),
            None => Ok(()),
        }
    }
}

/// `$SWAYSOCK`, else the newest `sway-ipc.*.sock` in the runtime directory,
/// for when wisp runs as a service without sway's environment.
pub fn find_socket() -> Option<PathBuf> {
    if let Some(socket) = std::env::var_os("SWAYSOCK").map(PathBuf::from)
        && socket.exists()
    {
        return Some(socket);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    std::fs::read_dir(Path::new(&runtime))
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            if !(name.starts_with("sway-ipc.") && name.ends_with(".sock")) {
                return None;
            }
            Some((path.metadata().ok()?.modified().ok()?, path))
        })
        .max()
        .map(|(_, path)| path)
}

#[derive(Deserialize)]
struct CommandReply {
    success: bool,
    error: Option<String>,
}

/// The focused window in a `get_tree` reply.
fn focused_window(node: &Value) -> Option<Window> {
    let is_window = node.get("pid").is_some_and(|p| !p.is_null());
    if is_window && node.get("focused").and_then(Value::as_bool) == Some(true) {
        let text = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
        let app_id = text(node.get("app_id"));
        // XWayland windows have no app_id, but an X11 class.
        let class = if app_id.is_empty() {
            text(node.pointer("/window_properties/class"))
        } else {
            app_id
        };
        return Some(Window {
            id: node.get("id")?.as_i64()?.to_string(),
            class,
            title: text(node.get("name")),
        });
    }
    ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|key| node.get(*key)?.as_array())
        .flatten()
        .find_map(focused_window)
}

impl Backend for Sway {
    fn pointer_target(&self) -> anyhow::Result<Target> {
        let tree = self.request(GET_TREE, "")?;
        let outputs = self.request(GET_OUTPUTS, "")?;
        let monitor = outputs.as_array().into_iter().flatten().find_map(|o| {
            let focused = o.get("focused").and_then(Value::as_bool) == Some(true);
            focused.then(|| o.get("name")?.as_str().map(String::from)).flatten()
        });
        Ok(Target {
            window: focused_window(&tree),
            monitor,
        })
    }

    fn focus(&self, window: &Window) -> anyhow::Result<()> {
        self.command(&format!("[con_id={}] focus", window.id))
    }

    fn dispatch(&self, command: &str) -> anyhow::Result<()> {
        self.command(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_focused_window() {
        let tree = serde_json::json!({
            "id": 1, "type": "root", "focused": false,
            "nodes": [{
                "id": 3, "type": "output", "name": "Virtual-1", "focused": false,
                "nodes": [{
                    "id": 4, "type": "workspace", "focused": false,
                    "nodes": [
                        {"id": 7, "pid": 100, "app_id": "foot", "name": "~", "focused": false, "nodes": []},
                    ],
                    "floating_nodes": [
                        {"id": 9, "pid": 200, "app_id": null, "name": "xterm",
                         "window_properties": {"class": "XTerm"}, "focused": true, "nodes": []},
                    ]
                }]
            }]
        });
        let window = focused_window(&tree).unwrap();
        assert_eq!(
            window,
            Window {
                id: "9".into(),
                class: "XTerm".into(),
                title: "xterm".into()
            }
        );
    }

    #[test]
    fn a_focused_workspace_is_no_window() {
        let tree = serde_json::json!({
            "id": 1, "focused": false,
            "nodes": [{"id": 4, "type": "workspace", "focused": true, "nodes": []}]
        });
        assert_eq!(focused_window(&tree), None);
    }
}
