//! Setup notes: what to know or do on this compositor, from the backend's
//! setup. They open in the settings app once, and again only when there's a
//! note that hasn't been shown, such as after switching compositors.
//!
//! In `$XDG_STATE_HOME/wisp`: `notes` holds the current ones, for the
//! settings app, and `seen-notes` every note shown so far.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;

pub fn show(notes: &[String]) {
    for note in notes {
        log::warn!("{note}");
    }
    if let Err(e) = try_show(notes) {
        log::warn!("can't show the setup notes: {e:#}");
    }
}

fn try_show(notes: &[String]) -> anyhow::Result<()> {
    let dir = state_dir().context("HOME is not set")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    // One note per line.
    let notes: Vec<String> = notes.iter().map(|n| n.replace('\n', " ")).collect();
    std::fs::write(dir.join("notes"), lines(&notes))?;
    let seen_path = dir.join("seen-notes");
    let mut seen: Vec<String> = std::fs::read_to_string(&seen_path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    if notes.iter().all(|n| seen.contains(n)) {
        return Ok(());
    }
    open(&dir, &notes)?;
    seen.extend(notes.into_iter().filter(|n| !seen.contains(n)).collect::<Vec<_>>());
    std::fs::write(seen_path, lines(&seen))?;
    Ok(())
}

fn lines(notes: &[String]) -> String {
    notes.iter().map(|n| format!("{n}\n")).collect()
}

/// The settings app, else a desktop notification.
fn open(dir: &Path, notes: &[String]) -> anyhow::Result<()> {
    let app = Command::new("wisp-settings")
        .arg("--notes")
        .env("PATH", crate::daemon::path()?)
        .stdin(Stdio::null())
        .spawn();
    match app {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
            Ok(())
        }
        Err(e) => {
            log::info!("no wisp-settings ({e}), so the notes go in a notification");
            notify("Setting up wisp", &notes.join("\n\n"), true)
                .with_context(|| format!("the notes are in {}", dir.join("notes").display()))
        }
    }
}

/// A desktop notification. A `sticky` one stays until dismissed. Backticks
/// are dropped: notifications show plain text.
pub fn notify(summary: &str, body: &str, sticky: bool) -> anyhow::Result<()> {
    let bus = zbus::blocking::Connection::session()?;
    let body = body.replace('`', "");
    let hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
    bus.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        // App, replaces, icon, summary, body, actions, hints, timeout (0:
        // stays, -1: the server's default).
        &(
            "wisp",
            0u32,
            "input-mouse",
            summary,
            body,
            Vec::<&str>::new(),
            hints,
            if sticky { 0i32 } else { -1 },
        ),
    )?;
    Ok(())
}

/// `$XDG_STATE_HOME/wisp`, else `~/.local/state/wisp`; the settings app
/// looks in the same place.
fn state_dir() -> Option<std::path::PathBuf> {
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(base) => base.into(),
        None => std::path::PathBuf::from(std::env::var_os("HOME")?).join(".local/state"),
    };
    Some(base.join("wisp"))
}
