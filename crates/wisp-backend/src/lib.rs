//! What wisp needs from the compositor, behind one trait so each compositor
//! gets its own implementation.

mod gnome;
mod hyprland;
mod kwin;
mod sway;

pub use gnome::Gnome;
pub use hyprland::Hyprland;
pub use kwin::KWin;
pub use sway::Sway;

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
    /// layer-shell. `None` means wisp's own overlay.
    fn trail(&self) -> Option<Box<dyn wisp_core::Trail>> {
        None
    }
}

/// Picks the backend for the running compositor: `$WISP_BACKEND` if set,
/// else whichever compositor answers on its socket or D-Bus name. Neither
/// needs the compositor's environment, so this works from a service started
/// before the compositor exported it.
pub fn detect() -> anyhow::Result<Box<dyn Backend>> {
    match std::env::var("WISP_BACKEND").ok().as_deref() {
        Some("hyprland") => return Ok(Box::new(Hyprland::connect()?)),
        Some("sway") => return Ok(Box::new(Sway::connect()?)),
        Some("kwin") => return Ok(Box::new(KWin::connect()?)),
        Some("gnome") => return Ok(Box::new(Gnome::connect()?)),
        Some(other) => anyhow::bail!("unknown WISP_BACKEND {other:?}: use hyprland, sway, kwin or gnome"),
        None => {}
    }
    if hyprland::find_socket().is_ok_and(|s| answers(&s)) {
        return Ok(Box::new(Hyprland::connect()?));
    }
    if sway::find_socket().is_some_and(|s| answers(&s)) {
        return Ok(Box::new(Sway::connect()?));
    }
    if let Ok(bus) = zbus::blocking::Connection::session() {
        if has_owner(&bus, "org.kde.KWin") {
            return Ok(Box::new(KWin::connect()?));
        }
        // Without wisp's extension, this fails with how to install it.
        if has_owner(&bus, gnome::NAME) || has_owner(&bus, "org.gnome.Shell") {
            return Ok(Box::new(Gnome::connect()?));
        }
    }
    anyhow::bail!("no supported compositor found (Hyprland, sway, KWin or GNOME)")
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
