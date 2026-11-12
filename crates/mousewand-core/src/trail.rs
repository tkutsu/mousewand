//! What draws the gesture trail. Compositors with layer-shell get mousewand's
//! own GPU overlay; GNOME gets its Shell extension.

use crate::config::{HintStyle, Theme};
use crate::hint::Hint;

pub trait Trail: Send {
    /// Starts a trail at the cursor, on the named monitor if known.
    fn begin(&self, monitor: Option<String>);
    /// The mouse moved by this much mid-gesture. A trail that learns where
    /// the cursor is only once its surface is up draws the stroke so far
    /// from these.
    fn moved(&self, _dx: f64, _dy: f64) {}
    /// Pulses a ring at the cursor, for each repeat step.
    fn tick(&self);
    /// Fades the trail out, red if nothing matched.
    fn end(&self, matched: bool);
    /// Applies from the next gesture.
    fn set_theme(&self, theme: Theme);
    /// How hints look; they're shown only if `style.enabled`.
    fn set_hint_style(&self, style: HintStyle);
    /// Shows or updates the hint for the gesture being drawn. It fades with
    /// the trail.
    fn hint(&self, hint: Hint);
    /// Stops drawing for good, before the process exits.
    fn shutdown(self: Box<Self>) {}
}
