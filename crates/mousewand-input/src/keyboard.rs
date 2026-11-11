//! Which modifier keys are down on the keyboards, for gestures `with` them.
//!
//! Nothing is read from the keyboards' event streams, so nothing typed is
//! ever seen: each check asks the kernel which keys are down right now.
//! Opening keyboards needs read access to them, which the `input` group or
//! `dist/71-mousewand-keyboard.rules` gives.

use std::time::{Duration, Instant};

use evdev::{Device, KeyCode};
use mousewand_core::keys::Mods;

use crate::VIRTUAL_NAME;

/// The keyboards found so far, looked for again now and then for ones
/// plugged in later.
pub struct Keyboards {
    devices: Vec<Device>,
    scanned: Option<Instant>,
}

/// How long a scan for keyboards holds before the next one.
const RESCAN: Duration = Duration::from_secs(5);

impl Default for Keyboards {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboards {
    pub fn new() -> Self {
        Keyboards {
            devices: Vec::new(),
            scanned: None,
        }
    }

    /// The modifiers held on any keyboard, or `None` if none can be read.
    pub fn mods(&mut self) -> Option<Mods> {
        if self.scanned.is_none_or(|at| at.elapsed() > RESCAN) {
            self.devices = keyboards();
            self.scanned = Some(Instant::now());
        }
        let mut readable = false;
        let mut held = Vec::new();
        // An unplugged keyboard fails to answer; it drops out.
        self.devices.retain(|device| match device.get_key_state() {
            Ok(keys) => {
                readable = true;
                held.extend(keys.iter().map(|k| k.code()));
                true
            }
            Err(_) => false,
        });
        readable.then(|| Mods::of_keys(held))
    }

    /// Waits until no modifier is held, for up to `limit`.
    pub fn wait_for_release(&mut self, limit: Duration) {
        let start = Instant::now();
        while self.mods().is_some_and(|m| !m.is_empty()) && start.elapsed() < limit {
            std::thread::sleep(Duration::from_millis(15));
        }
    }
}

/// Whether mousewand could read any keyboard.
pub fn readable() -> bool {
    !keyboards().is_empty()
}

/// Every device with letter keys and modifiers, except mousewand's own.
fn keyboards() -> Vec<Device> {
    evdev::enumerate()
        .map(|(_, device)| device)
        .filter(|device| {
            let typing = device.supported_keys().is_some_and(|keys| {
                keys.contains(KeyCode::KEY_A) && Mods::codes().any(|c| keys.contains(KeyCode::new(c)))
            });
            typing && device.name() != Some(VIRTUAL_NAME)
        })
        .collect()
}
