//! Test wisp without touching the desktop.
//!
//!   probe capture SECS   grab wisp's virtual device and print what it sends,
//!                        so nothing reaches the compositor meanwhile
//!   probe play SCRIPT    create a fake mouse named "wisp test mouse" and play
//!                        SCRIPT, e.g. "down; move -6 0 40; up"
//!
//! Script steps, separated by `;`: `down`, `up` (the middle button),
//! `move DX DY N` (N motion events, 4 ms apart), `wait MS`.
//! Point wisp at the fake mouse with `devices = ["wisp test mouse"]`.

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
        .find(|(_, d)| d.name() == Some(wisp_input::VIRTUAL_NAME))
        .ok_or_else(|| anyhow::anyhow!("wisp's virtual device isn't there; is wisp running?"))?;
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
                    println!("{:>6}ms {:?} {} {}", start.elapsed().as_millis(), e.event_type(), e.code(), e.value());
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
        .name("wisp test mouse")
        .with_keys(&keys)?
        .with_relative_axes(&axes)?
        .build()?;
    // Give wisp's hotplug watcher time to grab it.
    sleep(Duration::from_millis(1500));

    let button = |dev: &mut VirtualDevice, value| dev.emit(&[InputEvent::new(EventType::KEY.0, KeyCode::BTN_MIDDLE.0, value)]);
    for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let words: Vec<&str> = step.split_whitespace().collect();
        match words.as_slice() {
            ["down"] => button(&mut dev, 1)?,
            ["up"] => button(&mut dev, 0)?,
            ["wait", ms] => sleep(Duration::from_millis(ms.parse()?)),
            ["move", dx, dy, n] => {
                let (dx, dy, n): (i32, i32, u32) = (dx.parse()?, dy.parse()?, n.parse()?);
                for _ in 0..n {
                    dev.emit(&[
                        InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_X.0, dx),
                        InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_Y.0, dy),
                    ])?;
                    sleep(Duration::from_millis(4));
                }
            }
            _ => anyhow::bail!("bad step {step:?}"),
        }
    }
    sleep(Duration::from_millis(300));
    Ok(())
}
