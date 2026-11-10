//! What wisp needs from the compositor, behind one trait so each compositor
//! gets its own implementation.

mod hyprland;

pub use hyprland::Hyprland;

/// A point in the compositor's global, logical coordinates.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Window {
    /// The compositor's handle for the window, e.g. a Hyprland address.
    pub id: String,
    pub class: String,
    pub title: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Monitor {
    /// The connector name, the same as `wl_output.name`.
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Monitor {
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.x + self.width && p.y >= self.y && p.y < self.y + self.height
    }
}

pub trait Backend: Send {
    fn cursor(&self) -> anyhow::Result<Point>;
    /// The topmost window at a point, if any.
    fn window_at(&self, p: Point) -> anyhow::Result<Option<Window>>;
    /// Gives a window keyboard focus.
    fn focus(&self, w: &Window) -> anyhow::Result<()>;
    fn monitors(&self) -> anyhow::Result<Vec<Monitor>>;
    /// Runs a compositor command, written the compositor's way.
    fn dispatch(&self, command: &str) -> anyhow::Result<()>;
}

/// Picks the backend for the running compositor.
pub fn detect() -> anyhow::Result<Box<dyn Backend>> {
    Ok(Box::new(Hyprland::connect()?))
}
