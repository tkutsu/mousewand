//! Finding, grabbing and reading physical mice, including hotplugged ones.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use evdev::raw_stream::RawDevice;
use evdev::{EventType, InputEvent, KeyCode, RelativeAxisCode, SynchronizationCode};
use notify::{EventKind, RecursiveMode, Watcher};

use crate::{Msg, VIRTUAL_NAME};

/// Grabs every matching mouse now and as it's plugged in, and sends each
/// event batch (one `SYN_REPORT` frame) to `tx`.
pub fn watch(filter: Vec<String>, tx: Sender<Msg>) -> anyhow::Result<()> {
    let grabbed = Arc::new(Mutex::new(HashSet::new()));
    for (path, dev) in evdev::raw_stream::enumerate() {
        try_grab(path, dev, &filter, &tx, &grabbed);
    }

    let (fs_tx, fs_rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(fs_tx)?;
    watcher
        .watch(Path::new("/dev/input"), RecursiveMode::NonRecursive)
        .context("watching /dev/input for new mice")?;
    thread::Builder::new().name("hotplug".into()).spawn(move || {
        let _watcher = watcher;
        for event in fs_rx.into_iter().flatten() {
            if !matches!(event.kind, EventKind::Create(_)) {
                continue;
            }
            for path in event.paths.into_iter().filter(|p| is_event_node(p)) {
                // udev sets the node's permissions just after creating it.
                for _ in 0..20 {
                    if let Ok(dev) = RawDevice::open(&path) {
                        try_grab(path.clone(), dev, &filter, &tx, &grabbed);
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    })?;
    Ok(())
}

fn is_event_node(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("event"))
}

fn is_mouse(dev: &RawDevice, filter: &[String]) -> bool {
    let name = dev.name().unwrap_or_default();
    let moves = dev
        .supported_relative_axes()
        .is_some_and(|axes| axes.contains(RelativeAxisCode::REL_X) && axes.contains(RelativeAxisCode::REL_Y));
    let clicks = dev
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::BTN_LEFT));
    let wanted = filter.is_empty() || filter.iter().any(|f| name.contains(f.as_str()));
    name != VIRTUAL_NAME && moves && clicks && wanted
}

fn try_grab(
    path: PathBuf,
    mut dev: RawDevice,
    filter: &[String],
    tx: &Sender<Msg>,
    grabbed: &Arc<Mutex<HashSet<PathBuf>>>,
) {
    if !is_mouse(&dev, filter) || !grabbed.lock().unwrap().insert(path.clone()) {
        return;
    }
    let name = dev.name().unwrap_or("?").to_string();
    if let Err(e) = dev.grab() {
        log::warn!("can't grab {name} ({}): {e}", path.display());
        grabbed.lock().unwrap().remove(&path);
        return;
    }
    log::info!("grabbed {name} ({})", path.display());
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, grabbed) = (tx.clone(), grabbed.clone());
    let spawned = thread::Builder::new().name(format!("read {name}")).spawn(move || {
        if let Err(e) = read(id, &mut dev, &tx) {
            log::info!("released {name}: {e}");
        }
        grabbed.lock().unwrap().remove(&path);
        let _ = tx.send(Msg::Gone(id));
    });
    if let Err(e) = spawned {
        log::warn!("can't start a reader thread: {e}");
    }
}

/// Reads until the device goes away or the input thread stops listening.
fn read(id: u64, dev: &mut RawDevice, tx: &Sender<Msg>) -> anyhow::Result<()> {
    let mut batch = Vec::new();
    loop {
        for event in dev.fetch_events()? {
            if event.event_type() != EventType::SYNCHRONIZATION {
                batch.push(event);
            } else if event.code() == SynchronizationCode::SYN_DROPPED.0 {
                batch.clear();
            } else if event.code() == SynchronizationCode::SYN_REPORT.0 && !batch.is_empty() {
                let events: Vec<InputEvent> = std::mem::take(&mut batch);
                tx.send(Msg::Batch(id, events)).context("input thread stopped")?;
            }
        }
    }
}
