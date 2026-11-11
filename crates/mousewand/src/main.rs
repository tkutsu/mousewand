//! The mousewand daemon: grabs the mice, turns trigger gestures into actions for
//! the window under the cursor, and reloads its config as it changes.

mod config;
mod daemon;
mod notes;

use std::sync::mpsc;
use std::thread;

use anyhow::Context;
use mousewand_core::{Output, Trail, Trigger};
use mousewand_input::Input;
use mousewand_overlay::{Overlay, X11Overlay};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

use daemon::Daemon;

enum Msg {
    Gesture(Output, Trigger),
    ConfigChanged,
    Quit,
}

fn main() -> anyhow::Result<()> {
    // zbus logs every call at info; wgpu logs the GL drivers it probes and
    // skips as errors.
    let filter = "info,zbus=warn,tracing=warn,wgpu_hal::gles=off";
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(filter))
        .format_timestamp_millis()
        .init();

    let dir = config::dir()?;
    let config = config::load(&dir).with_context(|| format!("loading {}", dir.display()))?;
    log::info!("config from {}", dir.display());
    let mut backend = mousewand_backend::detect()?;
    // Before the overlay: on X11 it can need the compositor this starts.
    notes::show(&backend.setup());

    let (tx, rx) = mpsc::channel();
    let gestures = tx.clone();
    let input = Input::start(config.settings.clone(), move |output, trigger| {
        let _ = gestures.send(Msg::Gesture(output, trigger));
    })?;
    let changed = tx.clone();
    config::watch(&dir, move || {
        let _ = changed.send(Msg::ConfigChanged);
    })?;
    // Quitting mid-gesture must not leave keys held down.
    let mut signals = signal_hook::iterator::Signals::new([SIGTERM, SIGINT, SIGHUP])?;
    thread::Builder::new().name("signals".into()).spawn(move || {
        if signals.forever().next().is_some() {
            let _ = tx.send(Msg::Quit);
        }
    })?;

    // The compositor's own trail (GNOME), else mousewand's overlay for the session.
    let overlay = backend.trail().or_else(|| {
        let overlay = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            Overlay::start().map(|o| Box::new(o) as Box<dyn Trail>)
        } else {
            X11Overlay::start().map(|o| Box::new(o) as Box<dyn Trail>)
        };
        overlay.inspect_err(|e| log::warn!("no trail: {e:#}")).ok()
    });
    if let Some(overlay) = &overlay {
        overlay.set_theme(config.theme.clone());
        overlay.set_hint_style(config.hint.clone());
    }
    let mut daemon = Daemon::new(config, backend, input, overlay);
    for msg in rx {
        match msg {
            Msg::Gesture(output, trigger) => daemon.on_output(output, trigger),
            Msg::ConfigChanged => match config::load(&dir) {
                Ok(new) => {
                    log::info!("config reloaded");
                    daemon.input.set_settings(new.settings.clone());
                    if let Some(overlay) = &daemon.overlay {
                        overlay.set_theme(new.theme.clone());
                        overlay.set_hint_style(new.hint.clone());
                    }
                    daemon.config = new;
                }
                // A save that breaks the config would otherwise look like
                // mousewand ignoring it.
                Err(e) => {
                    log::error!("keeping the last good config: {e:#}");
                    if let Err(e) = notes::notify("mousewand: config not applied", &format!("{e:#}"), false) {
                        log::warn!("can't show a notification: {e:#}");
                    }
                }
            },
            Msg::Quit => {
                log::info!("quitting");
                daemon.quit();
                break;
            }
        }
    }
    Ok(())
}
