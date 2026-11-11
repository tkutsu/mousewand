//! What wisp needs from the compositor, behind one trait so each compositor
//! gets its own implementation.

mod hyprland;
mod kwin;
mod sway;

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
}

/// Picks the backend for the running compositor: `$WISP_BACKEND` if set
/// (hyprland or sway), else whichever compositor's socket answers. Sockets
/// are found without the compositor's environment, so this works from a
/// service started before the compositor exported it.
pub fn detect() -> anyhow::Result<Box<dyn Backend>> {
    match std::env::var("WISP_BACKEND").ok().as_deref() {
        Some("hyprland") => return Ok(Box::new(Hyprland::connect()?)),
        Some("sway") => return Ok(Box::new(Sway::connect()?)),
        Some("kwin") => return Ok(Box::new(KWin::connect()?)),
        Some(other) => anyhow::bail!("unknown WISP_BACKEND {other:?}: use hyprland, sway or kwin"),
        None => {}
    }
    if hyprland::find_socket().is_ok_and(|s| answers(&s)) {
        return Ok(Box::new(Hyprland::connect()?));
    }
    if sway::find_socket().is_some_and(|s| answers(&s)) {
        return Ok(Box::new(Sway::connect()?));
    }
    if kwin::running() {
        return Ok(Box::new(KWin::connect()?));
    }
    anyhow::bail!("no supported compositor found (Hyprland, sway or KWin)")
}

/// A socket file can outlive its compositor.
fn answers(socket: &std::path::Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_ok()
}
