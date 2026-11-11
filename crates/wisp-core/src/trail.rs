//! What draws the gesture trail. Compositors with layer-shell get wisp's
//! own GPU overlay; GNOME gets its Shell extension.

use crate::config::Theme;

pub trait Trail: Send {
    /// Starts a trail at the cursor, on the named monitor if known.
    fn begin(&self, monitor: Option<String>);
    /// Pulses a ring at the cursor, for each repeat step.
    fn tick(&self);
    /// Fades the trail out, red if nothing matched.
    fn end(&self, matched: bool);
    /// Applies from the next gesture.
    fn set_theme(&self, theme: Theme);
    /// Stops drawing for good, before the process exits.
    fn shutdown(self: Box<Self>) {}
}
