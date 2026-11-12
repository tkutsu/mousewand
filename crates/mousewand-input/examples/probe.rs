//! Test mousewand without touching the desktop.
//!
//!   probe capture SECS   grab mousewand's virtual device and print what it sends,
//!                        so nothing reaches the compositor meanwhile
//!   probe play SCRIPT    create a fake mouse named "mousewand test mouse" and play
//!                        SCRIPT, e.g. "down; move -6 0 40; up"
//!
//! Script steps, separated by `;`: `down`, `up` (the middle button),
//! `press KEY`, `release KEY` (left, right, middle, side or extra on the mouse;
//! shift, ctrl, alt or super on a fake keyboard), `wheel N` (N notches,
//! negative is down), `move DX DY N` (N motion events, 4 ms apart), `circle R`
//! (a circle of radius R, anticlockwise from its bottom), `wait MS`, and
//! `run COMMAND` (e.g. a screenshot mid-stroke).
//! Point mousewand at the fake mouse with `devices = ["mousewand test mouse"]`.
//! mousewand looks for new keyboards every 5 s, so wait that long before the
//! first stroke that needs a modifier.

use std::thread::sleep;
use std::time::{Duration, Instant};

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["capture", secs] => capture(secs.parse()?),
        ["play", script] => play(script),
        _ => anyhow::bail!("usage: probe capture SECS | probe play SCRIPT"),
    }
}

fn capture(secs: u64) -> anyhow::Result<()> {
    let (_, mut dev) = evdev::raw_stream::enumerate()
        .find(|(_, d)| d.name() == Some(mousewand_input::VIRTUAL_NAME))
        .ok_or_else(|| anyhow::anyhow!("mousewand's virtual device isn't there; is mousewand running?"))?;
    dev.grab()?;
    let fd = std::os::fd::AsRawFd::as_raw_fd(&dev);
    // Non-blocking, so the loop can stop on time.
    unsafe { libc_fcntl_nonblock(fd) };
    let start = Instant::now();
    println!("capturing");
    while start.elapsed() < Duration::from_secs(secs) {
        match dev.fetch_events() {
            Ok(events) => {
                for e in events.filter(|e| e.event_type() != EventType::SYNCHRONIZATION) {
                    println!(
                        "{:>6}ms {:?} {} {}",
                        start.elapsed().as_millis(),
                        e.event_type(),
                        e.code(),
                        e.value()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => sleep(Duration::from_millis(2)),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

unsafe fn libc_fcntl_nonblock(fd: i32) {
    unsafe extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const O_NONBLOCK: i32 = 0o4000;
    unsafe {
        let flags = fcntl(fd, F_GETFL);
        fcntl(fd, F_SETFL, flags | O_NONBLOCK);
    }
}

fn play(script: &str) -> anyhow::Result<()> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in 0x110..=0x117 {
        keys.insert(KeyCode::new(code));
    }
    let mut axes = AttributeSet::<RelativeAxisCode>::new();
    axes.insert(RelativeAxisCode::REL_X);
    axes.insert(RelativeAxisCode::REL_Y);
    axes.insert(RelativeAxisCode::REL_WHEEL);
    let mut dev = VirtualDevice::builder()?
        .name("mousewand test mouse")
        .with_keys(&keys)?
        .with_relative_axes(&axes)?
        .build()?;
    // mousewand counts a device with letters and modifiers as a keyboard.
    let mut typing = AttributeSet::<KeyCode>::new();
    for code in [
        KeyCode::KEY_A,
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_LEFTMETA,
    ] {
        typing.insert(code);
    }
    let mut keyboard = VirtualDevice::builder()?
        .name("mousewand test keyboard")
        .with_keys(&typing)?
        .build()?;
    // Give mousewand's hotplug watcher time to grab it.
    sleep(Duration::from_millis(1500));

    for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let words: Vec<&str> = step.split_whitespace().collect();
        match words.as_slice() {
            ["down"] => press(&mut dev, &mut keyboard, "middle", 1)?,
            ["up"] => press(&mut dev, &mut keyboard, "middle", 0)?,
            ["press", name] => press(&mut dev, &mut keyboard, name, 1)?,
            ["release", name] => press(&mut dev, &mut keyboard, name, 0)?,
            ["wheel", n] => dev.emit(&[InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_WHEEL.0,
                n.parse()?,
            )])?,
            ["wait", ms] => sleep(Duration::from_millis(ms.parse()?)),
            ["run", ..] => {
                let command = step.trim_start_matches("run").trim();
                std::process::Command::new("sh").args(["-c", command]).status()?;
            }
            ["move", dx, dy, n] => {
                let (dx, dy, n): (i32, i32, u32) = (dx.parse()?, dy.parse()?, n.parse()?);
                for _ in 0..n {
                    nudge(&mut dev, dx, dy)?;
                }
            }
            ["circle", r] => {
                let r: f64 = r.parse()?;
                // Whole-pixel steps whose sum stays on the circle.
                let (mut x, mut y) = (0, 0);
                let steps = 120;
                for i in 1..=steps {
                    let t = std::f64::consts::TAU * f64::from(i) / f64::from(steps);
                    let (tx, ty) = ((r * t.sin()).round() as i32, (-r * (1.0 - t.cos())).round() as i32);
                    nudge(&mut dev, tx - x, ty - y)?;
                    (x, y) = (tx, ty);
                }
            }
            _ => anyhow::bail!("bad step {step:?}"),
        }
    }
    sleep(Duration::from_millis(300));
    Ok(())
}

/// Presses (1) or releases (0) a mouse button or a modifier key.
fn press(mouse: &mut VirtualDevice, keyboard: &mut VirtualDevice, name: &str, value: i32) -> anyhow::Result<()> {
    let (device, code) = match name {
        "left" => (mouse, KeyCode::BTN_LEFT),
        "right" => (mouse, KeyCode::BTN_RIGHT),
        "middle" => (mouse, KeyCode::BTN_MIDDLE),
        "side" => (mouse, KeyCode::BTN_SIDE),
        "extra" => (mouse, KeyCode::BTN_EXTRA),
        "shift" => (keyboard, KeyCode::KEY_LEFTSHIFT),
        "ctrl" => (keyboard, KeyCode::KEY_LEFTCTRL),
        "alt" => (keyboard, KeyCode::KEY_LEFTALT),
        "super" => (keyboard, KeyCode::KEY_LEFTMETA),
        _ => anyhow::bail!("unknown key {name:?}"),
    };
    device.emit(&[InputEvent::new(EventType::KEY.0, code.0, value)])?;
    Ok(())
}

/// One motion event, then the 4 ms a real mouse at 250 Hz leaves between them.
fn nudge(dev: &mut VirtualDevice, dx: i32, dy: i32) -> std::io::Result<()> {
    dev.emit(&[
        InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_X.0, dx),
        InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_Y.0, dy),
    ])?;
    sleep(Duration::from_millis(4));
    Ok(())
}
