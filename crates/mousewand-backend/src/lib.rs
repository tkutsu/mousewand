//! What mousewand needs from the compositor, behind one trait so each compositor
//! gets its own implementation.

mod cosmic;
mod gnome;
mod hyprland;
mod kwin;
mod niri;
mod sway;
mod toplevel;
mod x11;

pub use gnome::Gnome;
pub use hyprland::Hyprland;
pub use kwin::KWin;
pub use sway::Sway;
pub use toplevel::{Flavor, Toplevels};
pub use x11::X11;

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Window {
    /// The compositor's handle for the window, e.g. a Hyprland address.
    pub id: String,
    pub class: String,
    pub title: String,
}

/// Where a gesture starts.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Target {
    /// The window under the cursor; `None` over the desktop.
    pub window: Option<Window>,
    /// The monitor under the cursor, by connector name (the same as
    /// `wl_output.name`), if the compositor says.
    pub monitor: Option<String>,
}

pub trait Backend: Send {
    /// The window and monitor under the cursor.
    fn pointer_target(&self) -> anyhow::Result<Target>;
    /// Gives a window keyboard focus.
    fn focus(&self, window: &Window) -> anyhow::Result<()>;
    /// Runs a compositor command, written the compositor's way.
    fn dispatch(&self, command: &str) -> anyhow::Result<()>;
    /// A trail the compositor draws itself, for compositors without
    /// layer-shell. `None` means mousewand's own overlay.
    fn trail(&self) -> Option<Box<dyn mousewand_core::Trail>> {
        None
    }

    /// Sets up what this compositor needs, where mousewand can do it itself, and
    /// returns notes on what the user should know or still do: one
    /// sentence or a few each, with code in `backticks`. Runs once, at start.
    fn setup(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// `$XDG_CURRENT_DESKTOP`, lowercased, e.g. "labwc:wlroots".
fn desktop() -> String {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase()
}

/// `$XDG_<KIND>_HOME`, else `~/<fallback>`.
fn xdg_home(kind: &str, fallback: &str) -> Option<std::path::PathBuf> {
    match std::env::var_os(format!("XDG_{kind}_HOME")) {
        Some(dir) => Some(dir.into()),
        None => Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join(fallback)),
    }
}

/// Picks the backend for the running compositor: `$MOUSEWAND_BACKEND` if set,
/// else whichever compositor answers on its socket or D-Bus name. Neither
/// needs the compositor's environment, so this works from a service started
/// before the compositor exported it.
pub fn detect() -> anyhow::Result<Box<dyn Backend>> {
    match std::env::var("MOUSEWAND_BACKEND").ok().as_deref() {
        Some("hyprland") => return Ok(Box::new(Hyprland::connect()?)),
        Some("sway") => return Ok(Box::new(Sway::connect()?)),
        Some("kwin") => return Ok(Box::new(KWin::connect()?)),
        Some("gnome") => return Ok(Box::new(Gnome::connect()?)),
        Some("niri") => {
            let socket = niri_socket().ok_or_else(|| anyhow::anyhow!("no niri socket found"))?;
            return Ok(Box::new(Toplevels::connect(Flavor::Niri(socket))?));
        }
        Some("toplevel") => return Ok(Box::new(Toplevels::connect(Flavor::Other)?)),
        Some("x11") => return Ok(Box::new(X11::connect()?)),
        Some(other) => {
            anyhow::bail!("unknown MOUSEWAND_BACKEND {other:?}: use hyprland, sway, niri, kwin, gnome, toplevel or x11")
        }
        None => {}
    }
    // An X11 session: an X display and no Wayland one.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some() {
        return Ok(Box::new(X11::connect()?));
    }
    if hyprland::find_socket().is_ok_and(|s| answers(&s)) {
        return Ok(Box::new(Hyprland::connect()?));
    }
    if sway::find_socket().is_some_and(|s| answers(&s)) {
        return Ok(Box::new(Sway::connect()?));
    }
    if let Some(socket) = niri_socket().filter(|s| answers(s)) {
        return Ok(Box::new(Toplevels::connect(Flavor::Niri(socket))?));
    }
    if let Ok(bus) = zbus::blocking::Connection::session() {
        if has_owner(&bus, "org.kde.KWin") {
            return Ok(Box::new(KWin::connect()?));
        }
        // Also before mousewand's extension runs: its setup installs it.
        if has_owner(&bus, gnome::NAME) || has_owner(&bus, "org.gnome.Shell") {
            return Ok(Box::new(Gnome::connect()?));
        }
    }
    // labwc, COSMIC and other compositors that list their windows.
    Toplevels::connect(Flavor::Other)
        .map(|t| Box::new(t) as Box<dyn Backend>)
        .map_err(|e| {
            e.context("no supported compositor found (Hyprland, sway, niri, KWin, GNOME, or one with foreign-toplevel)")
        })
}

/// `$NIRI_SOCKET`, else the newest `niri.*.sock` in the runtime directory.
fn niri_socket() -> Option<std::path::PathBuf> {
    if let Some(socket) = std::env::var_os("NIRI_SOCKET").map(std::path::PathBuf::from) {
        return Some(socket);
    }
    newest_in_runtime_dir(|name| name.starts_with("niri.") && name.ends_with(".sock"))
}

/// The most recently modified file in `$XDG_RUNTIME_DIR` whose name matches.
fn newest_in_runtime_dir(matches: impl Fn(&str) -> bool) -> Option<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    std::fs::read_dir(runtime)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            matches(path.file_name()?.to_str()?).then_some(())?;
            Some((path.metadata().ok()?.modified().ok()?, path))
        })
        .max()
        .map(|(_, path)| path)
}

/// A socket file can outlive its compositor.
fn answers(socket: &std::path::Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_ok()
}

/// Whether a D-Bus name has an owner on the session bus.
fn has_owner(bus: &zbus::blocking::Connection, name: &str) -> bool {
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(bus) else {
        return false;
    };
    let Ok(name) = name.try_into() else { return false };
    dbus.name_has_owner(name).unwrap_or(false)
}
