//! Mouse input: grab physical mice, forward everything through one virtual
//! device except the trigger button, and decide per press whether it was a
//! click, a hold or a gesture. The same virtual device types key combos.

mod devices;

use std::collections::HashSet;
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode, RelativeAxisCode};
use wisp_core::keys::Combo;
use wisp_core::{Extra, Machine, Output, Settings, Wheel};

/// The virtual device's name, which is also how wisp avoids grabbing it.
pub const VIRTUAL_NAME: &str = "wisp virtual mouse";

/// Identifies one grabbed physical mouse.
type DeviceId = u64;

enum Msg {
    Batch(DeviceId, Vec<InputEvent>),
    /// The mouse was unplugged or failed.
    Gone(DeviceId),
    Settings(Settings),
    Type(Combo),
    Hold(Combo),
    Unhold(Combo),
    /// Release everything wisp holds down, then confirm.
    Shutdown(SyncSender<()>),
}

/// Talks to the running input thread.
#[derive(Clone)]
pub struct Input {
    tx: Sender<Msg>,
}

impl Input {
    /// Grabs the mice and starts the input thread. Gesture outputs go to
    /// `on_gesture`; clicks and holds are handled here.
    pub fn start(settings: Settings, on_gesture: impl Fn(Output) + Send + 'static) -> anyhow::Result<Input> {
        let device = virtual_device().context("creating the uinput device (is /dev/uinput writable?)")?;
        let (tx, rx) = mpsc::channel();
        devices::watch(settings.devices.clone(), tx.clone())?;

        thread::Builder::new().name("input".into()).spawn(move || {
            let mut router = Router {
                device,
                machine: Machine::new(&settings),
                settings,
                trigger_owner: None,
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
                    Ok(Msg::Type(combo)) => router.type_combo(&combo),
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

    /// Presses and releases a key combo on the virtual device.
    pub fn type_combo(&self, combo: Combo) {
        let _ = self.tx.send(Msg::Type(combo));
    }

    /// Presses a combo's keys and keeps them down until `unhold`.
    pub fn hold(&self, combo: Combo) {
        let _ = self.tx.send(Msg::Hold(combo));
    }

    pub fn unhold(&self, combo: Combo) {
        let _ = self.tx.send(Msg::Unhold(combo));
    }

    /// Releases every key and button wisp holds down and stops forwarding.
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
    /// The mouse whose trigger press the machine is handling.
    trigger_owner: Option<DeviceId>,
    /// Buttons each mouse holds down. The virtual device sends a press for
    /// the first holder and a release for the last, so two mice never send
    /// a double press or an early release.
    held: HashSet<(DeviceId, u16)>,
    /// Buttons pressed during a gesture: wisp's, so their release is too.
    claimed: HashSet<(DeviceId, u16)>,
    /// Keys a hold gesture keeps down.
    holding: Vec<u16>,
    on_gesture: F,
}

impl<F: Fn(Output)> Router<F> {
    fn batch(&mut self, id: DeviceId, events: Vec<InputEvent>, now: Duration) -> std::io::Result<()> {
        let trigger = self.settings.trigger.0;
        let mut forward = Vec::with_capacity(events.len());
        let mut outputs = Vec::new();
        let (mut dx, mut dy) = (0.0, 0.0);
        for event in events {
            let (code, value) = (event.code(), event.value());
            match event.event_type() {
                EventType::KEY if code == trigger => outputs.extend(self.trigger(id, value, now)),
                EventType::KEY if self.claimed.contains(&(id, code)) => {
                    if value == 0 {
                        self.claimed.remove(&(id, code));
                    }
                }
                EventType::KEY if value == 1 && self.machine.claims() => {
                    outputs.extend(self.machine.claim(Extra::Button(code)).unwrap_or_default());
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
                            outputs.extend(self.machine.claim(Extra::Wheel(wheel)).unwrap_or_default());
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
            outputs.extend(self.machine.motion(dx, dy));
        }
        self.act(outputs)
    }

    /// Only the mouse that pressed the trigger can release it; a second
    /// mouse's trigger is ignored while the first is down.
    fn trigger(&mut self, id: DeviceId, value: i32, now: Duration) -> Vec<Output> {
        match (value, self.trigger_owner) {
            (1, None) => {
                self.trigger_owner = Some(id);
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
                Output::Press => self.key(self.settings.trigger.0, 1)?,
                Output::Release => self.key(self.settings.trigger.0, 0)?,
                gesture => (self.on_gesture)(gesture),
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
