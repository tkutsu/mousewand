//! The pure part of wisp: config, stroke recognition and the trigger state
//! machine. No I/O, so all of it is unit-tested.

pub mod config;
pub mod keys;
pub mod machine;
pub mod stroke;

pub use config::{Action, Config, Settings, Trigger};
pub use machine::{Machine, Output};
pub use stroke::{Dir, Stroke};
