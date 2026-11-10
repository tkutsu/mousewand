//! The wisp daemon: grabs the mice, turns trigger strokes into actions for
//! the window under the cursor, and reloads its config as it changes.

mod config;

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::Context;
use wisp_backend::{Backend, Window};
use wisp_core::{Action, Config, Output, Stroke};
use wisp_input::Input;

enum Msg {
    Gesture(Output),
    ConfigChanged,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let dir = config::dir()?;
    let mut config = config::load(&dir).with_context(|| format!("loading {}", dir.display()))?;
    log::info!("config from {}", dir.display());
    let backend = wisp_backend::detect()?;

    let (tx, rx) = mpsc::channel();
    let gestures = tx.clone();
    let input = Input::start(config.settings.clone(), move |output| {
        let _ = gestures.send(Msg::Gesture(output));
    })?;
    config::watch(&dir, move || {
        let _ = tx.send(Msg::ConfigChanged);
    })?;

    // The window the current gesture started on.
    let mut target: Option<Window> = None;
    for msg in rx {
        match msg {
            Msg::Gesture(Output::Begin) => {
                let window = window_under_cursor(backend.as_ref());
                log::debug!("gesture over {:?}", window.class);
                target = Some(window);
            }
            Msg::Gesture(Output::Update(stroke)) => log::debug!("stroke {stroke}"),
            Msg::Gesture(Output::End(stroke)) => {
                if let Some(window) = target.take() {
                    fire(&config, backend.as_ref(), &input, &window, &stroke);
                }
            }
            Msg::Gesture(Output::Cancel) => target = None,
            Msg::Gesture(Output::Press | Output::Release) => {}
            Msg::ConfigChanged => match config::load(&dir) {
                Ok(new) => {
                    log::info!("config reloaded");
                    input.set_settings(new.settings.clone());
                    config = new;
                }
                Err(e) => log::error!("keeping the last good config: {e:#}"),
            },
        }
    }
    Ok(())
}

/// The window under the cursor. Over the desktop, or when the lookup fails,
/// a window without an id or class: it still gets the global gestures.
fn window_under_cursor(backend: &dyn Backend) -> Window {
    let found = backend.cursor().and_then(|p| backend.window_at(p));
    found.unwrap_or_else(|e| {
        log::warn!("can't find the window under the cursor: {e:#}");
        None
    })
    .unwrap_or(Window { id: String::new(), class: String::new(), title: String::new() })
}

fn fire(config: &Config, backend: &dyn Backend, input: &Input, window: &Window, stroke: &Stroke) {
    if stroke.is_empty() {
        return;
    }
    let Some(action) = config.action(&window.class, stroke) else {
        log::info!("{stroke} over {:?}: no gesture", window.class);
        return;
    };
    log::info!("{stroke} over {:?}: {action:?}", window.class);
    let result = match action {
        Action::Keys(combo) => {
            // The keys must reach the window the gesture was drawn over,
            // even if another window has focus.
            let focused = if window.id.is_empty() { Ok(()) } else { backend.focus(window) };
            focused.map(|()| input.type_combo(combo.clone()))
        }
        Action::Command(command) => spawn(command, window),
        Action::Dispatch(command) => backend.dispatch(command),
    };
    if let Err(e) = result {
        log::error!("{stroke}: {e:#}");
    }
}

/// Runs a shell command without waiting for it. It sees the window in
/// `WISP_CLASS` and `WISP_TITLE`.
fn spawn(command: &str, window: &Window) -> anyhow::Result<()> {
    let mut child = Command::new("sh")
        .args(["-c", command])
        .env("WISP_CLASS", &window.class)
        .env("WISP_TITLE", &window.title)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("running {command:?}"))?;
    thread::spawn(move || child.wait());
    Ok(())
}
