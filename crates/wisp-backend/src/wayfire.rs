//! Wayfire, over its IPC socket. Needs the `ipc` and `ipc-rules` plugins.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Backend, Target, Window};

pub struct Wayfire {
    socket: PathBuf,
}

#[derive(Deserialize)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct View {
    id: i64,
    title: String,
    app_id: String,
    geometry: Rect,
    output_id: i64,
    role: String,
    mapped: bool,
    minimized: bool,
    always_on_top: bool,
    last_focus_timestamp: i64,
}

#[derive(Deserialize)]
struct Output {
    id: i64,
    name: String,
    geometry: Rect,
}

/// `$WAYFIRE_SOCKET`, else the newest `wayfire-*.socket` in the runtime dir.
pub fn find_socket() -> Option<PathBuf> {
    if let Some(socket) = std::env::var_os("WAYFIRE_SOCKET").map(PathBuf::from) {
        return Some(socket);
    }
    crate::newest_in_runtime_dir(|name| name.starts_with("wayfire-") && name.ends_with(".socket"))
}

impl Wayfire {
    pub fn connect() -> anyhow::Result<Self> {
        let socket = find_socket().context("no Wayfire IPC socket; enable the ipc and ipc-rules plugins")?;
        log::info!("wayfire socket: {}", socket.display());
        Ok(Wayfire { socket })
    }

    /// One call: a little-endian length, then JSON, each way.
    fn call(&self, method: &str, data: Value) -> anyhow::Result<Value> {
        let mut stream =
            UnixStream::connect(&self.socket).with_context(|| format!("connecting to {}", self.socket.display()))?;
        let body = serde_json::to_vec(&json!({ "method": method, "data": data }))?;
        stream.write_all(&(body.len() as u32).to_le_bytes())?;
        stream.write_all(&body)?;
        let mut len = [0; 4];
        stream.read_exact(&mut len)?;
        let mut reply = vec![0; u32::from_le_bytes(len) as usize];
        stream.read_exact(&mut reply)?;
        let reply: Value = serde_json::from_slice(&reply)?;
        if let Some(error) = reply.get("error") {
            bail!("wayfire {method}: {error}");
        }
        Ok(reply)
    }
}

/// The topmost window at `p`: always-on-top first, then the most recently
/// focused. View geometry is relative to its output.
fn topmost<'a>(views: &'a [View], outputs: &[Output], p: (f64, f64)) -> Option<&'a View> {
    views
        .iter()
        .filter(|v| v.role == "toplevel" && v.mapped && !v.minimized)
        .filter(|v| {
            let Some(output) = outputs.iter().find(|o| o.id == v.output_id) else {
                return false;
            };
            v.geometry.contains((p.0 - output.geometry.x, p.1 - output.geometry.y))
        })
        .max_by_key(|v| (v.always_on_top, v.last_focus_timestamp))
}

impl Backend for Wayfire {
    fn pointer_target(&self) -> anyhow::Result<Target> {
        let pos = self.call("window-rules/get_cursor_position", json!({}))?;
        let p = (
            pos.pointer("/pos/x").and_then(Value::as_f64).unwrap_or_default(),
            pos.pointer("/pos/y").and_then(Value::as_f64).unwrap_or_default(),
        );
        let views: Vec<View> = serde_json::from_value(self.call("window-rules/list-views", json!({}))?)?;
        let outputs: Vec<Output> = serde_json::from_value(self.call("window-rules/list-outputs", json!({}))?)?;
        Ok(Target {
            window: topmost(&views, &outputs, p).map(|v| Window {
                id: v.id.to_string(),
                class: v.app_id.clone(),
                title: v.title.clone(),
            }),
            monitor: outputs.iter().find(|o| o.geometry.contains(p)).map(|o| o.name.clone()),
        })
    }

    fn focus(&self, window: &Window) -> anyhow::Result<()> {
        self.call("window-rules/focus-view", json!({ "id": window.id.parse::<i64>()? }))?;
        Ok(())
    }

    /// A raw IPC call: the method, then optional JSON data, e.g.
    /// `"wm-actions/set-fullscreen {\"view_id\": 3, \"state\": true}"`.
    fn dispatch(&self, command: &str) -> anyhow::Result<()> {
        let (method, data) = command.split_once(' ').unwrap_or((command, "{}"));
        self.call(
            method,
            serde_json::from_str(data).context("dispatch data must be JSON")?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: i64, x: f64, focus: i64) -> View {
        View {
            id,
            title: String::new(),
            app_id: format!("app{id}"),
            geometry: Rect {
                x,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            output_id: 1,
            role: "toplevel".into(),
            mapped: true,
            minimized: false,
            always_on_top: false,
            last_focus_timestamp: focus,
        }
    }

    #[test]
    fn picks_the_most_recently_focused_view_under_the_cursor() {
        let outputs = [Output {
            id: 1,
            name: "DP-1".into(),
            geometry: Rect {
                x: 1000.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
        }];
        let views = [view(1, 0.0, 5), view(2, 50.0, 9), view(3, 500.0, 1)];
        let at = |x| topmost(&views, &outputs, (x, 10.0)).map(|v| v.id);
        assert_eq!(at(1010.0), Some(1));
        assert_eq!(at(1060.0), Some(2));
        assert_eq!(at(1300.0), None);
        assert_eq!(at(10.0), None, "outside the output");
    }
}
