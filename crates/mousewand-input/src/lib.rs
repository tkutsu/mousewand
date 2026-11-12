//! Mouse input: grab physical mice, forward everything through one virtual
//! device except the trigger button, and decide per press whether it was a
//! click, a hold or a gesture. The same virtual device types key combos.

mod devices;
mod keyboard;

pub use keyboard::{Keyboards, readable as keyboards_readable};

use std::collections::HashSet;
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode, RelativeAxisCode};
use mousewand_core::keys::Combo;
use mousewand_core::{Extra, Machine, Output, Settings, Trigger, Wheel};

/// The virtual device's name, which is also how mousewand avoids grabbing it.
pub const VIRTUAL_NAME: &str = "mousewand virtual mouse";

/// Identifies one grabbed physical mouse.
type DeviceId = u64;

enum Msg {
    Batch(DeviceId, Vec<InputEvent>),
    /// The mouse was unplugged or failed.
    Gone(DeviceId),
    Settings(Settings),
    /// Types the combo, then confirms it went out.
    Type(Combo, SyncSender<()>),
    Hold(Combo),
    /// The pending press belongs to the app under the cursor.
    Pass,
    /// Paused, the triggers are ordinary buttons.
    Paused(bool),
    Unhold(Combo),
    /// Release everything mousewand holds down, then confirm.
    Shutdown(SyncSender<()>),
}

/// Talks to the running input thread.
#[derive(Clone)]
pub struct Input {
    tx: Sender<Msg>,
}

impl Input {
    /// Grabs the mice and starts the input thread. Gesture outputs go to
    /// `on_gesture`, with the trigger that started them: the main one or the
    /// global one. Clicks and holds are handled here.
    pub fn start(settings: Settings, on_gesture: impl Fn(Output, Trigger) + Send + 'static) -> anyhow::Result<Input> {
        let device = virtual_device().context("creating the uinput device (is /dev/uinput writable?)")?;
        let (tx, rx) = mpsc::channel();
        devices::watch(settings.devices.clone(), tx.clone())?;

        thread::Builder::new().name("input".into()).spawn(move || {
            let mut router = Router {
                device,
                machine: Machine::new(&settings),
                active: settings.trigger,
                settings,
                trigger_owner: None,
                paused: false,
                held: HashSet::new(),
                claimed: HashSet::new(),
                holding: Vec::new(),
                on_gesture,
            };
            let start = Instant::now();
            loop {
                let msg = match router.machine.deadline() {
                    Some(deadline) => rx.recv_timeout(deadline.saturating_sub(start.elapsed())),
                    None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };
                let result = match msg {
                    Ok(Msg::Batch(id, events)) => router.batch(id, events, start.elapsed()),
                    Ok(Msg::Gone(id)) => router.gone(id),
                    Ok(Msg::Settings(settings)) => {
                        router.settings = settings;
                        Ok(())
                    }
                    Ok(Msg::Type(combo, done)) => {
                        let result = router.type_combo(&combo);
                        let _ = done.send(());
                        result
                    }
                    Ok(Msg::Pass) => {
                        let outputs = router.machine.pass();
                        router.act(outputs)
                    }
                    Ok(Msg::Paused(paused)) => {
                        router.paused = paused;
                        Ok(())
                    }
                    Ok(Msg::Hold(combo)) => router.hold(&combo),
                    Ok(Msg::Unhold(combo)) => router.unhold(&combo),
                    Ok(Msg::Shutdown(done)) => {
                        if let Err(e) = router.release_everything() {
                            log::error!("releasing keys on shutdown: {e}");
                        }
                        let _ = done.send(());
                        return;
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let outputs = router.machine.tick(start.elapsed());
                        router.act(outputs)
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                if let Err(e) = result {
                    log::error!("uinput: {e}");
                }
            }
        })?;
        Ok(Input { tx })
    }

    /// New settings apply from the next press. `devices` only affects mice
    /// plugged in afterwards.
    pub fn set_settings(&self, settings: Settings) {
        let _ = self.tx.send(Msg::Settings(settings));
    }

    /// Presses and releases a key combo on the virtual device, and returns
    /// once it has, so the next step of a sequence comes after it.
    pub fn type_combo(&self, combo: Combo) {
        let (done, wait) = mpsc::sync_channel(1);
        if self.tx.send(Msg::Type(combo, done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_secs(1));
        }
    }

    /// Hands the trigger press in progress to the app, as an ordinary
    /// button press.
    pub fn pass(&self) {
        let _ = self.tx.send(Msg::Pass);
    }

    /// Paused, every trigger press goes to the app as it is, from the next one.
    pub fn set_paused(&self, paused: bool) {
        let _ = self.tx.send(Msg::Paused(paused));
    }

    /// Presses a combo's keys and keeps them down until `unhold`.
    pub fn hold(&self, combo: Combo) {
        let _ = self.tx.send(Msg::Hold(combo));
    }

    pub fn unhold(&self, combo: Combo) {
        let _ = self.tx.send(Msg::Unhold(combo));
    }

    /// Releases every key and button mousewand holds down and stops forwarding.
    /// Waits up to a second for the input thread to confirm.
    pub fn shutdown(&self) {
        let (done, wait) = mpsc::sync_channel(1);
        if self.tx.send(Msg::Shutdown(done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_secs(1));
        }
    }
}

struct Router<F> {
    device: VirtualDevice,
    settings: Settings,
    machine: Machine,
    /// The trigger the machine is handling, or handled last.
    active: Trigger,
    /// The mouse whose trigger press the machine is handling.
    trigger_owner: Option<DeviceId>,
    /// Whether the triggers are ordinary buttons for now.
    paused: bool,
    /// Buttons each mouse holds down. The virtual device sends a press for
    /// the first holder and a release for the last, so two mice never send
    /// a double press or an early release.
    held: HashSet<(DeviceId, u16)>,
    /// Buttons pressed during a gesture: mousewand's, so their release is too.
    claimed: HashSet<(DeviceId, u16)>,
    /// Keys a hold gesture keeps down.
    holding: Vec<u16>,
    on_gesture: F,
}

impl<F: Fn(Output, Trigger)> Router<F> {
    fn batch(&mut self, id: DeviceId, events: Vec<InputEvent>, now: Duration) -> std::io::Result<()> {
        let mut forward = Vec::with_capacity(events.len());
        let mut outputs = Vec::new();
        let (mut dx, mut dy) = (0.0, 0.0);
        for event in events {
            let (code, value) = (event.code(), event.value());
            match event.event_type() {
                // First, so the other trigger, pressed during a gesture, is
                // released as the claimed button it became.
                EventType::KEY if self.claimed.contains(&(id, code)) => {
                    if value == 0 {
                        self.claimed.remove(&(id, code));
                    }
                }
                EventType::KEY if self.is_trigger(code) => outputs.extend(self.trigger(id, code, value, now)),
                EventType::KEY if value == 1 && self.machine.claims() => {
                    outputs.extend(self.machine.claim(Extra::Button(code), now).unwrap_or_default());
                    self.claimed.insert((id, code));
                }
                EventType::KEY => {
                    if self.track(id, code, value) {
                        forward.push(event);
                    }
                }
                EventType::RELATIVE if is_wheel(code) && self.machine.claims() => {
                    if let Some(wheel) = notch(code, value) {
                        for _ in 0..value.unsigned_abs() {
                            outputs.extend(self.machine.claim(Extra::Wheel(wheel), now).unwrap_or_default());
                        }
                    }
                }
                EventType::RELATIVE if event.code() == RelativeAxisCode::REL_X.0 => {
                    dx += event.value() as f64;
                    forward.push(event);
                }
                EventType::RELATIVE if event.code() == RelativeAxisCode::REL_Y.0 => {
                    dy += event.value() as f64;
                    forward.push(event);
                }
                _ => forward.push(event),
            }
        }
        if !forward.is_empty() {
            self.device.emit(&forward)?;
        }
        if dx != 0.0 || dy != 0.0 {
            outputs.extend(self.machine.motion(dx, dy, now));
        }
        self.act(outputs)
    }

    /// Either trigger while none is down, unless paused; then only the one
    /// that is.
    fn is_trigger(&self, code: u16) -> bool {
        match self.trigger_owner {
            Some(_) => code == self.active.0,
            None if self.paused => false,
            None => code == self.settings.trigger.0 || self.settings.global_trigger.is_some_and(|t| t.0 == code),
        }
    }

    /// Only the mouse that pressed the trigger can release it; a second
    /// mouse's trigger is ignored while the first is down.
    fn trigger(&mut self, id: DeviceId, code: u16, value: i32, now: Duration) -> Vec<Output> {
        match (value, self.trigger_owner) {
            (1, None) => {
                self.trigger_owner = Some(id);
                self.active = Trigger(code);
                // Each press starts with the latest settings.
                self.machine = Machine::new(&self.settings);
                self.machine.press(now)
            }
            (0, Some(owner)) if owner == id => {
                self.trigger_owner = None;
                self.machine.release()
            }
            _ => Vec::new(),
        }
    }

    /// Records a button change and says whether to forward it.
    fn track(&mut self, id: DeviceId, code: u16, value: i32) -> bool {
        let others = self.held.iter().any(|&(dev, c)| c == code && dev != id);
        match value {
            1 => self.held.insert((id, code)) && !others,
            0 => self.held.remove(&(id, code)) && !others,
            _ => !others,
        }
    }

    /// Releases what an unplugged mouse held, so no button stays down.
    fn gone(&mut self, id: DeviceId) -> std::io::Result<()> {
        let codes: Vec<u16> = self
            .held
            .iter()
            .filter(|&&(dev, _)| dev == id)
            .map(|&(_, c)| c)
            .collect();
        for code in codes {
            if self.track(id, code, 0) {
                self.key(code, 0)?;
            }
        }
        self.claimed.retain(|&(dev, _)| dev != id);
        if self.trigger_owner == Some(id) {
            self.trigger_owner = None;
            let outputs = self.machine.cancel();
            self.act(outputs)?;
        }
        Ok(())
    }

    fn hold(&mut self, combo: &Combo) -> std::io::Result<()> {
        for &code in &combo.codes {
            self.holding.push(code);
            self.key(code, 1)?;
        }
        Ok(())
    }

    fn unhold(&mut self, combo: &Combo) -> std::io::Result<()> {
        let mut result = Ok(());
        for &code in combo.codes.iter().rev() {
            if let Some(i) = self.holding.iter().rposition(|&c| c == code) {
                self.holding.remove(i);
                result = result.and(self.key(code, 0));
            }
        }
        result
    }

    /// Leaves nothing pressed: held keys, a held trigger, forwarded buttons.
    fn release_everything(&mut self) -> std::io::Result<()> {
        let mut result = Ok(());
        for code in std::mem::take(&mut self.holding).into_iter().rev() {
            result = result.and(self.key(code, 0));
        }
        let outputs = self.machine.cancel();
        result = result.and(self.act(outputs));
        let buttons: HashSet<u16> = self.held.drain().map(|(_, code)| code).collect();
        for code in buttons {
            result = result.and(self.key(code, 0));
        }
        result
    }

    fn act(&mut self, outputs: Vec<Output>) -> std::io::Result<()> {
        for output in outputs {
            match output {
                Output::Press => self.key(self.active.0, 1)?,
                Output::Release => self.key(self.active.0, 0)?,
                gesture => (self.on_gesture)(gesture, self.active),
            }
        }
        Ok(())
    }

    fn key(&mut self, code: u16, value: i32) -> std::io::Result<()> {
        self.device.emit(&[InputEvent::new(EventType::KEY.0, code, value)])
    }

    /// Presses the combo, then releases every key even if a press failed.
    fn type_combo(&mut self, combo: &Combo) -> std::io::Result<()> {
        let mut result = combo.codes.iter().try_for_each(|&code| self.key(code, 1));
        for &code in combo.codes.iter().rev() {
            result = result.and(self.key(code, 0));
        }
        result
    }
}

fn is_wheel(code: u16) -> bool {
    [
        RelativeAxisCode::REL_WHEEL,
        RelativeAxisCode::REL_HWHEEL,
        RelativeAxisCode::REL_WHEEL_HI_RES,
        RelativeAxisCode::REL_HWHEEL_HI_RES,
    ]
    .iter()
    .any(|axis| axis.0 == code)
}

/// The direction of a low-resolution wheel event. High-resolution events
/// return `None`: they're swallowed with their notch but not counted twice.
fn notch(code: u16, value: i32) -> Option<Wheel> {
    match (code, value.signum()) {
        (c, 1) if c == RelativeAxisCode::REL_WHEEL.0 => Some(Wheel::Up),
        (c, -1) if c == RelativeAxisCode::REL_WHEEL.0 => Some(Wheel::Down),
        (c, 1) if c == RelativeAxisCode::REL_HWHEEL.0 => Some(Wheel::Right),
        (c, -1) if c == RelativeAxisCode::REL_HWHEEL.0 => Some(Wheel::Left),
        _ => None,
    }
}

fn virtual_device() -> std::io::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    // Keyboard keys for combos, then the mouse buttons for forwarding.
    for code in (1..=248).chain(0x110..=0x117) {
        keys.insert(KeyCode::new(code));
    }
    let mut axes = AttributeSet::<RelativeAxisCode>::new();
    for axis in [
        RelativeAxisCode::REL_X,
        RelativeAxisCode::REL_Y,
        RelativeAxisCode::REL_WHEEL,
        RelativeAxisCode::REL_HWHEEL,
        RelativeAxisCode::REL_WHEEL_HI_RES,
        RelativeAxisCode::REL_HWHEEL_HI_RES,
    ] {
        axes.insert(axis);
    }
    VirtualDevice::builder()?
        .name(VIRTUAL_NAME)
        .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x1209, 0x5717, 1))
        .with_keys(&keys)?
        .with_relative_axes(&axes)?
        .build()
}
