//! The pure part of mousewand: config, stroke recognition and the trigger state
//! machine. No I/O, so all of it is unit-tested.

pub mod config;
pub mod hint;
pub mod keys;
pub mod machine;
pub mod session;
pub mod shape;
pub mod stroke;
pub mod trail;

pub use config::{
    Action, Align, Color, Config, Gesture, HintStyle, Input, Kind, Position, Rgba, Settings, Theme, Trigger, Wheel,
};
pub use hint::Hint;
pub use machine::{Extra, Machine, Output};
pub use session::{Fire, Session};
pub use stroke::{Dir, Stroke};
pub use trail::Trail;
