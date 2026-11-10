//! The pure part of wisp: config, stroke recognition and the trigger state
//! machine. No I/O, so all of it is unit-tested.

pub mod config;
pub mod keys;
pub mod machine;
pub mod session;
pub mod stroke;

pub use config::{Action, Color, Config, Gesture, Input, Kind, Settings, Theme, Trigger, Wheel};
pub use machine::{Extra, Machine, Output};
pub use session::{Fire, Session};
pub use stroke::{Dir, Stroke};
