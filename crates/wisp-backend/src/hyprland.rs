//! Hyprland, over its request socket (`.socket.sock`).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::{Backend, Target, Window};

/// A point in Hyprland's global, logical coordinates.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, PartialEq, Debug)]
struct Monitor {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Monitor {
    fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.x + self.width && p.y >= self.y && p.y < self.y + self.height
    }
}

pub struct Hyprland {
    socket: PathBuf,
}

impl Hyprland {
    pub fn connect() -> anyhow::Result<Self> {
        let socket = find_socket()?;
        log::info!("hyprland socket: {}", socket.display());
        Ok(Hyprland { socket })
    }

    fn request(&self, command: &str) -> anyhow::Result<String> {
        let mut stream =
            UnixStream::connect(&self.socket).with_context(|| format!("connecting to {}", self.socket.display()))?;
        stream.write_all(command.as_bytes())?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply)?;
        Ok(reply)
    }

    fn json<T: DeserializeOwned>(&self, what: &str) -> anyhow::Result<T> {
        let reply = self.request(&format!("j/{what}"))?;
        serde_json::from_str(&reply).with_context(|| format!("parsing hyprctl {what}"))
    }

    fn dispatch_checked(&self, command: &str) -> anyhow::Result<()> {
        let reply = self.request(&format!("dispatch {command}"))?;
        if reply.trim() != "ok" {
            bail!("hyprland dispatch {command:?}: {}", reply.trim());
        }
        Ok(())
    }
}

/// `$HYPRLAND_INSTANCE_SIGNATURE` is missing when wisp runs as a service that
/// started before Hyprland exported it, so fall back to the newest instance.
pub fn find_socket() -> anyhow::Result<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    let hypr = Path::new(&runtime).join("hypr");
    if let Some(sig) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") {
        let socket = hypr.join(sig).join(".socket.sock");
        if socket.exists() {
            return Ok(socket);
        }
    }
    std::fs::read_dir(&hypr)
        .with_context(|| format!("no Hyprland instance in {}", hypr.display()))?
        .filter_map(|entry| {
            let socket = entry.ok()?.path().join(".socket.sock");
            let modified = socket.metadata().ok()?.modified().ok()?;
            Some((modified, socket))
        })
        .max()
        .map(|(_, socket)| socket)
        .context("no Hyprland socket found; is Hyprland running?")
}

#[derive(Deserialize)]
struct CursorPos {
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyprMonitor {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    transform: u8,
    active_workspace: WorkspaceRef,
    special_workspace: WorkspaceRef,
}

#[derive(Deserialize)]
struct WorkspaceRef {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Client {
    address: String,
    mapped: bool,
    hidden: bool,
    at: (f64, f64),
    size: (f64, f64),
    workspace: WorkspaceRef,
    floating: bool,
    pinned: bool,
    fullscreen: u8,
    class: String,
    title: String,
    #[serde(rename = "focusHistoryID")]
    focus_history_id: i64,
}

impl Client {
    fn contains(&self, p: Point) -> bool {
        let (x, y) = self.at;
        let (w, h) = self.size;
        p.x >= x && p.x < x + w && p.y >= y && p.y < y + h
    }
}

impl HyprMonitor {
    fn logical(&self) -> Monitor {
        // Transforms 1, 3, 5 and 7 rotate by 90° or 270°.
        let (w, h) = if self.transform % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        Monitor {
            name: self.name.clone(),
            x: self.x,
            y: self.y,
            width: w / self.scale,
            height: h / self.scale,
        }
    }
}

/// The topmost of the clients at `p`: special workspaces cover normal ones,
/// fullscreen covers tiled, floating sits above tiled, and among equals the
/// most recently focused is on top.
fn topmost<'a>(clients: &'a [Client], monitors: &[HyprMonitor], p: Point) -> Option<&'a Client> {
    let shown = |c: &Client| {
        c.pinned
            || monitors
                .iter()
                .any(|m| c.workspace.name == m.active_workspace.name || c.workspace.name == m.special_workspace.name)
    };
    clients
        .iter()
        .filter(|c| c.mapped && !c.hidden && c.contains(p) && shown(c))
        .min_by_key(|c| {
            let special = c.workspace.name.starts_with("special:");
            (
                !special,
                c.fullscreen == 0,
                !(c.floating || c.pinned),
                c.focus_history_id,
            )
        })
}

impl Backend for Hyprland {
    fn pointer_target(&self) -> anyhow::Result<Target> {
        let pos: CursorPos = self.json("cursorpos")?;
        let p = Point { x: pos.x, y: pos.y };
        let monitors: Vec<HyprMonitor> = self.json("monitors")?;
        let clients: Vec<Client> = self.json("clients")?;
        Ok(Target {
            window: topmost(&clients, &monitors, p).map(|c| Window {
                id: c.address.clone(),
                class: c.class.clone(),
                title: c.title.clone(),
            }),
            monitor: monitors
                .iter()
                .map(HyprMonitor::logical)
                .find(|m| m.contains(p))
                .map(|m| m.name),
        })
    }

    /// Lua-config Hyprland (0.55+) takes Lua dispatchers; older versions
    /// take the legacy `focuswindow` form.
    fn focus(&self, w: &Window) -> anyhow::Result<()> {
        self.dispatch_checked(&format!("hl.dsp.focus({{ window = \"address:{}\" }})", w.id))
            .or_else(|lua| {
                self.dispatch_checked(&format!("focuswindow address:{}", w.id))
                    .map_err(|_| lua)
            })
    }

    fn dispatch(&self, command: &str) -> anyhow::Result<()> {
        self.dispatch_checked(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(address: &str, workspace: &str, at: (f64, f64), floating: bool, focus: i64) -> Client {
        Client {
            address: address.into(),
            mapped: true,
            hidden: false,
            at,
            size: (100.0, 100.0),
            workspace: WorkspaceRef { name: workspace.into() },
            floating,
            pinned: false,
            fullscreen: 0,
            class: address.into(),
            title: String::new(),
            focus_history_id: focus,
        }
    }

    fn monitor(active: &str, special: &str) -> HyprMonitor {
        HyprMonitor {
            name: "DP-1".into(),
            x: 0.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
            scale: 1.0,
            transform: 0,
            active_workspace: WorkspaceRef { name: active.into() },
            special_workspace: WorkspaceRef { name: special.into() },
        }
    }

    #[test]
    fn floating_beats_tiled_and_hidden_workspaces_are_skipped() {
        let clients = [
            client("tiled", "1", (0.0, 0.0), false, 0),
            client("float", "1", (50.0, 50.0), true, 3),
            client("elsewhere", "2", (0.0, 0.0), true, 1),
            client("scratch", "special:term", (0.0, 0.0), true, 2),
        ];
        let at = |x, y| topmost(&clients, &[monitor("1", "")], Point { x, y }).map(|c| c.address.as_str());
        assert_eq!(at(10.0, 10.0), Some("tiled"));
        assert_eq!(at(60.0, 60.0), Some("float"));
        assert_eq!(at(500.0, 500.0), None);

        let open = [monitor("1", "special:term")];
        assert_eq!(
            topmost(&clients, &open, Point { x: 60.0, y: 60.0 }).map(|c| c.address.as_str()),
            Some("scratch")
        );
    }

    #[test]
    fn scaled_and_rotated_monitors() {
        let mut m = monitor("1", "");
        m.scale = 2.0;
        m.transform = 1;
        let l = m.logical();
        assert_eq!((l.width, l.height), (720.0, 1280.0));
    }
}
