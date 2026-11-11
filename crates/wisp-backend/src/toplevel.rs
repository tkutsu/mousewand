//! Any Wayland compositor that lists its windows to clients: niri, labwc,
//! river, COSMIC and other wlroots compositors.
//!
//! These protocols say which window is focused but not which is under the
//! cursor, so gestures target the focused window. With focus-follows-mouse
//! that is the one under the cursor.
//!
//! The list comes from `wlr-foreign-toplevel-management`, or on COSMIC from
//! `ext-foreign-toplevel-list` plus COSMIC's toplevel info and management.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, bail};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1 as ext_handle, ext_foreign_toplevel_list_v1 as ext_list,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as wlr_handle, zwlr_foreign_toplevel_manager_v1 as wlr_manager,
};

use crate::cosmic::toplevel_info::{
    zcosmic_toplevel_handle_v1 as cosmic_handle, zcosmic_toplevel_info_v1 as cosmic_info,
};
use crate::cosmic::toplevel_management::zcosmic_toplevel_manager_v1 as cosmic_manager;
use crate::{Backend, Target, Window};

/// Which compositor, for `dispatch`.
#[derive(Clone, Debug, PartialEq)]
pub enum Flavor {
    /// niri, whose socket is passed on to `niri msg`.
    Niri(std::path::PathBuf),
    Other,
}

pub struct Toplevels {
    conn: Connection,
    shared: Arc<Mutex<Shared>>,
    flavor: Flavor,
}

#[derive(Default)]
struct Shared {
    seat: Option<wl_seat::WlSeat>,
    cosmic_info: Option<cosmic_info::ZcosmicToplevelInfoV1>,
    cosmic_manager: Option<cosmic_manager::ZcosmicToplevelManagerV1>,
    /// Keyed by the handle's protocol id: wlr handles, or ext handles on COSMIC.
    windows: HashMap<u32, Entry>,
    /// COSMIC handle id → ext handle id.
    cosmic_ids: HashMap<u32, u32>,
}

struct Entry {
    app_id: String,
    title: String,
    activated: bool,
    handle: Handle,
}

enum Handle {
    Wlr(wlr_handle::ZwlrForeignToplevelHandleV1),
    /// `None` if COSMIC's info global is missing.
    Cosmic(Option<cosmic_handle::ZcosmicToplevelHandleV1>),
}

struct State(Arc<Mutex<Shared>>);

const ACTIVATED: u32 = 2;

fn activated(states: &[u8]) -> bool {
    states
        .as_chunks::<4>()
        .0
        .iter()
        .any(|&c| u32::from_ne_bytes(c) == ACTIVATED)
}

impl Toplevels {
    pub fn connect(flavor: Flavor) -> anyhow::Result<Self> {
        let conn = Connection::connect_to_env().context("no Wayland display")?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut state = State(shared.clone());
        {
            let mut s = shared.lock().unwrap();
            s.seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=1, ()).ok();
            if globals
                .bind::<wlr_manager::ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ())
                .is_ok()
            {
                log::info!("using wlr-foreign-toplevel");
            } else if let Ok(info) = globals.bind::<cosmic_info::ZcosmicToplevelInfoV1, _, _>(&qh, 2..=3, ()) {
                // The info global must be known before the list sends windows.
                s.cosmic_info = Some(info);
                s.cosmic_manager = globals.bind(&qh, 1..=4, ()).ok();
                globals
                    .bind::<ext_list::ExtForeignToplevelListV1, _, _>(&qh, 1..=1, ())
                    .context("COSMIC without ext-foreign-toplevel-list")?;
                log::info!("using COSMIC toplevel info");
            } else {
                bail!("the compositor doesn't list its windows (no foreign-toplevel protocol)");
            }
        }
        queue.roundtrip(&mut state)?;
        thread::Builder::new().name("toplevels".into()).spawn(move || {
            while queue.blocking_dispatch(&mut state).is_ok() {}
            log::warn!("lost the Wayland connection for the window list");
        })?;
        Ok(Toplevels { conn, shared, flavor })
    }
}

/// What to do so the focused window is the one under the cursor.
fn focus_follows_mouse_note() -> String {
    let desktop = crate::desktop();
    if desktop.contains("wayfire") {
        return "Wayfire's IPC is off, so gestures act on the focused window wherever you draw. Add `ipc` and \
                `ipc-rules` to the `plugins` line of `wayfire.ini` to make them act on the window under the cursor."
            .into();
    }
    let (name, how) = if desktop.contains("labwc") {
        (
            "labwc",
            "add `<focus><followMouse>yes</followMouse></focus>` to `~/.config/labwc/rc.xml`",
        )
    } else if desktop.contains("cosmic") {
        (
            "COSMIC",
            "turn on focus follows cursor in COSMIC Settings, under window management",
        )
    } else {
        (
            "This compositor",
            "turn on its focus-follows-mouse setting, if it has one",
        )
    };
    format!(
        "{name} only tells other programs which window is focused, so gestures act on that window wherever you \
         draw. To make it the window under the cursor, {how}."
    )
}

impl Backend for Toplevels {
    fn setup(&mut self) -> Vec<String> {
        match self.flavor {
            Flavor::Niri(_) => crate::niri::setup(),
            Flavor::Other => vec![focus_follows_mouse_note()],
        }
    }

    fn pointer_target(&self) -> anyhow::Result<Target> {
        let shared = self.shared.lock().unwrap();
        let window = shared.windows.iter().find(|(_, e)| e.activated).map(|(id, e)| Window {
            id: id.to_string(),
            class: e.app_id.clone(),
            title: e.title.clone(),
        });
        // The overlay lands on the focused output by default.
        Ok(Target { window, monitor: None })
    }

    fn focus(&self, window: &Window) -> anyhow::Result<()> {
        {
            let shared = self.shared.lock().unwrap();
            let seat = shared.seat.as_ref().context("no seat")?;
            let id: u32 = window.id.parse()?;
            match &shared.windows.get(&id).context("the window is gone")?.handle {
                Handle::Wlr(handle) => handle.activate(seat),
                Handle::Cosmic(Some(handle)) => shared
                    .cosmic_manager
                    .as_ref()
                    .context("COSMIC toplevel management is missing")?
                    .activate(handle, seat),
                Handle::Cosmic(None) => bail!("the window isn't ready yet"),
            }
        }
        self.conn.flush()?;
        Ok(())
    }

    fn dispatch(&self, command: &str) -> anyhow::Result<()> {
        let Flavor::Niri(socket) = &self.flavor else {
            bail!("this compositor has no dispatchers; use a keys or command action")
        };
        // `niri msg action focus-column-left`, etc.
        let status = std::process::Command::new("niri")
            .args(["msg", "action"])
            .args(command.split_whitespace())
            .env("NIRI_SOCKET", socket)
            .status()
            .context("running niri msg")?;
        if !status.success() {
            bail!("niri msg action {command}: {status}");
        }
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wlr_manager::ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &wlr_manager::ZwlrForeignToplevelManagerV1,
        event: wlr_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wlr_manager::Event::Toplevel { toplevel } = event {
            let entry = Entry {
                app_id: String::new(),
                title: String::new(),
                activated: false,
                handle: Handle::Wlr(toplevel.clone()),
            };
            state
                .0
                .lock()
                .unwrap()
                .windows
                .insert(toplevel.id().protocol_id(), entry);
        }
    }

    event_created_child!(State, wlr_manager::ZwlrForeignToplevelManagerV1, [
        wlr_manager::EVT_TOPLEVEL_OPCODE => (wlr_handle::ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<wlr_handle::ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &wlr_handle::ZwlrForeignToplevelHandleV1,
        event: wlr_handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut shared = state.0.lock().unwrap();
        let id = handle.id().protocol_id();
        if let wlr_handle::Event::Closed = event {
            shared.windows.remove(&id);
            handle.destroy();
            return;
        }
        let Some(entry) = shared.windows.get_mut(&id) else {
            return;
        };
        match event {
            wlr_handle::Event::Title { title } => entry.title = title,
            wlr_handle::Event::AppId { app_id } => entry.app_id = app_id,
            wlr_handle::Event::State { state } => entry.activated = activated(&state),
            _ => {}
        }
    }
}

impl Dispatch<ext_list::ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ext_list::ExtForeignToplevelListV1,
        event: ext_list::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let ext_list::Event::Toplevel { toplevel } = event else {
            return;
        };
        let mut shared = state.0.lock().unwrap();
        let id = toplevel.id().protocol_id();
        // COSMIC's own handle for this toplevel carries its activated state.
        let cosmic = shared
            .cosmic_info
            .as_ref()
            .map(|info| info.get_cosmic_toplevel(&toplevel, qh, ()));
        if let Some(cosmic) = &cosmic {
            shared.cosmic_ids.insert(cosmic.id().protocol_id(), id);
        }
        shared.windows.insert(
            id,
            Entry {
                app_id: String::new(),
                title: String::new(),
                activated: false,
                handle: Handle::Cosmic(cosmic),
            },
        );
    }

    event_created_child!(State, ext_list::ExtForeignToplevelListV1, [
        ext_list::EVT_TOPLEVEL_OPCODE => (ext_handle::ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ext_handle::ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ext_handle::ExtForeignToplevelHandleV1,
        event: ext_handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut shared = state.0.lock().unwrap();
        let id = handle.id().protocol_id();
        match event {
            ext_handle::Event::Closed => {
                shared.windows.remove(&id);
                shared.cosmic_ids.retain(|_, ext| *ext != id);
            }
            ext_handle::Event::Title { title } => {
                if let Some(entry) = shared.windows.get_mut(&id) {
                    entry.title = title;
                }
            }
            ext_handle::Event::AppId { app_id } => {
                if let Some(entry) = shared.windows.get_mut(&id) {
                    entry.app_id = app_id;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<cosmic_info::ZcosmicToplevelInfoV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &cosmic_info::ZcosmicToplevelInfoV1,
        _: cosmic_info::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(State, cosmic_info::ZcosmicToplevelInfoV1, [
        cosmic_info::EVT_TOPLEVEL_OPCODE => (cosmic_handle::ZcosmicToplevelHandleV1, ()),
    ]);
}

impl Dispatch<cosmic_handle::ZcosmicToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &cosmic_handle::ZcosmicToplevelHandleV1,
        event: cosmic_handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let cosmic_handle::Event::State { state: states } = event {
            let mut shared = state.0.lock().unwrap();
            if let Some(ext) = shared.cosmic_ids.get(&handle.id().protocol_id()).copied()
                && let Some(entry) = shared.windows.get_mut(&ext)
            {
                entry.activated = activated(&states);
            }
        }
    }
}

impl Dispatch<cosmic_manager::ZcosmicToplevelManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &cosmic_manager::ZcosmicToplevelManagerV1,
        _: cosmic_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_activated_state() {
        let states: Vec<u8> = [0u32, 2].iter().flat_map(|s| s.to_ne_bytes()).collect();
        assert!(super::activated(&states));
        assert!(!super::activated(&1u32.to_ne_bytes()));
    }
}
