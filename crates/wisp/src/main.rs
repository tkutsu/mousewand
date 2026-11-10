//! The wisp daemon: grabs the mice, turns trigger strokes into actions for
//! the window under the cursor, and reloads its config as it changes.

mod config;

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::Context;
use wisp_backend::{Backend, Point, Window};
use wisp_core::{Action, Config, Output, Stroke};
use wisp_input::Input;
use wisp_overlay::Overlay;

enum Msg {
    Gesture(Output),
    ConfigChanged,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

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

    let overlay = Overlay::start().inspect_err(|e| log::warn!("no trail: {e:#}")).ok();

    // The window the current gesture started on.
    let mut target: Option<Window> = None;
    for msg in rx {
        match msg {
            Msg::Gesture(Output::Begin) => {
                let cursor = backend.cursor();
                if let Some(overlay) = &overlay {
                    overlay.begin(cursor.as_ref().ok().and_then(|&p| monitor_at(backend.as_ref(), p)));
                }
                let window = window_at(backend.as_ref(), cursor);
                log::debug!("gesture over {:?}", window.class);
                target = Some(window);
            }
            Msg::Gesture(Output::Update(stroke)) => log::debug!("stroke {stroke}"),
            Msg::Gesture(Output::End(stroke)) => {
                let matched = target
                    .take()
                    .is_some_and(|window| fire(&config, backend.as_ref(), &input, &window, &stroke));
                if let Some(overlay) = &overlay {
                    overlay.end(matched);
                }
            }
            Msg::Gesture(Output::Cancel) => {
                target = None;
                if let Some(overlay) = &overlay {
                    overlay.end(true);
                }
            }
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

fn monitor_at(backend: &dyn Backend, p: Point) -> Option<String> {
    let monitors = backend
        .monitors()
        .inspect_err(|e| log::warn!("can't list monitors: {e:#}"))
        .ok()?;
    monitors.into_iter().find(|m| m.contains(p)).map(|m| m.name)
}

/// The window at the cursor. Over the desktop, or when the lookup fails,
/// a window without an id or class: it still gets the global gestures.
fn window_at(backend: &dyn Backend, cursor: anyhow::Result<Point>) -> Window {
    let found = cursor.and_then(|p| backend.window_at(p));
    found
        .unwrap_or_else(|e| {
            log::warn!("can't find the window under the cursor: {e:#}");
            None
        })
        .unwrap_or(Window {
            id: String::new(),
            class: String::new(),
            title: String::new(),
        })
}

/// Runs the stroke's action. Returns whether the stroke matched a gesture.
fn fire(config: &Config, backend: &dyn Backend, input: &Input, window: &Window, stroke: &Stroke) -> bool {
    if stroke.is_empty() {
        return false;
    }
    let Some(action) = config.action(&window.class, stroke) else {
        log::info!("{stroke} over {:?}: no gesture", window.class);
        return false;
    };
    log::info!("{stroke} over {:?}: {action:?}", window.class);
    let result = match action {
        Action::Keys(combo) => {
            // The keys must reach the window the gesture was drawn over,
            // even if another window has focus.
            let focused = if window.id.is_empty() {
                Ok(())
            } else {
                backend.focus(window)
            };
            focused.map(|()| input.type_combo(combo.clone()))
        }
        Action::Command(command) => spawn(command, window),
        Action::Dispatch(command) => backend.dispatch(command),
    };
    if let Err(e) = result {
        log::error!("{stroke}: {e:#}");
    }
    true
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
