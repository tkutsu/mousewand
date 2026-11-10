//! The wisp daemon: grabs the mice, turns trigger gestures into actions for
//! the window under the cursor, and reloads its config as it changes.

mod config;
mod daemon;

use std::sync::mpsc;
use std::thread;

use anyhow::Context;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use wisp_core::Output;
use wisp_input::Input;
use wisp_overlay::Overlay;

use daemon::Daemon;

enum Msg {
    Gesture(Output),
    ConfigChanged,
    Quit,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let dir = config::dir()?;
    let config = config::load(&dir).with_context(|| format!("loading {}", dir.display()))?;
    log::info!("config from {}", dir.display());
    let backend = wisp_backend::detect()?;

    let (tx, rx) = mpsc::channel();
    let gestures = tx.clone();
    let input = Input::start(config.settings.clone(), move |output| {
        let _ = gestures.send(Msg::Gesture(output));
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

    let overlay = Overlay::start().inspect_err(|e| log::warn!("no trail: {e:#}")).ok();
    if let Some(overlay) = &overlay {
        overlay.set_theme(config.theme.clone());
    }
    let mut daemon = Daemon::new(config, backend, input, overlay);
    for msg in rx {
        match msg {
            Msg::Gesture(output) => daemon.on_output(output),
            Msg::ConfigChanged => match config::load(&dir) {
                Ok(new) => {
                    log::info!("config reloaded");
                    daemon.input.set_settings(new.settings.clone());
                    if let Some(overlay) = &daemon.overlay {
                        overlay.set_theme(new.theme.clone());
                    }
                    daemon.config = new;
                }
                Err(e) => log::error!("keeping the last good config: {e:#}"),
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
