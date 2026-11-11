//! Turns gesture outputs from the input thread into actions.

use std::process::{Command, Stdio};
use std::thread;

use anyhow::Context;
use wisp_backend::{Backend, Target, Window};
use wisp_core::{Action, Config, Fire, Output, Session};
use wisp_input::Input;
use wisp_overlay::Overlay;

pub struct Daemon {
    pub config: Config,
    pub backend: Box<dyn Backend>,
    pub input: Input,
    pub overlay: Option<Overlay>,
    active: Option<Active>,
}

/// The gesture in progress.
struct Active {
    window: Window,
    session: Session,
    /// Whether the window has been focused for this gesture yet.
    focused: bool,
}

impl Daemon {
    pub fn new(config: Config, backend: Box<dyn Backend>, input: Input, overlay: Option<Overlay>) -> Self {
        Daemon {
            config,
            backend,
            input,
            overlay,
            active: None,
        }
    }

    pub fn on_output(&mut self, output: Output) {
        match output {
            Output::Begin => self.begin(),
            Output::Update(stroke) => {
                log::debug!("stroke {stroke}");
                self.run(|s| s.stroke(&stroke));
            }
            Output::Motion(dx, dy) => self.run(|s| s.motion(dx, dy)),
            Output::Wheel(wheel) => self.run(|s| s.wheel(wheel)),
            Output::Rocker(button) => self.run(|s| s.rocker(button)),
            Output::End(stroke) => {
                self.run(|s| s.end(&stroke));
                let matched = self.active.take().is_some_and(|a| a.session.matched());
                if !matched && !stroke.is_empty() {
                    log::info!("{stroke}: no gesture");
                }
                self.overlay(|o| o.end(matched));
            }
            Output::Cancel => {
                self.run(Session::cancel);
                self.active = None;
                self.overlay(|o| o.end(true));
            }
            Output::Press | Output::Release => {}
        }
    }

    /// Releases anything a gesture holds and stops input forwarding.
    pub fn quit(&mut self) {
        self.run(Session::cancel);
        self.input.shutdown();
        if let Some(overlay) = self.overlay.take() {
            overlay.shutdown();
        }
    }

    /// Over the desktop, or when the lookup fails, the window has no id or
    /// class: it still gets the global gestures.
    fn begin(&mut self) {
        let target = self.backend.pointer_target().unwrap_or_else(|e| {
            log::warn!("can't find the window under the cursor: {e:#}");
            Target::default()
        });
        self.overlay(|o| o.begin(target.monitor));
        let window = target.window.unwrap_or_default();
        log::debug!("gesture over {:?}", window.class);
        self.active = Some(Active {
            session: Session::new(self.config.gestures_for(&window.class)),
            window,
            focused: false,
        });
    }

    fn overlay(&self, f: impl FnOnce(&Overlay)) {
        if let Some(overlay) = &self.overlay {
            f(overlay);
        }
    }

    /// Feeds the gesture's session and carries out what it fires.
    fn run(&mut self, step: impl FnOnce(&mut Session) -> Vec<Fire>) {
        let Some(active) = &mut self.active else { return };
        let was_locked = active.session.locked();
        for fire in step(&mut active.session) {
            if was_locked
                && matches!(fire, Fire::Run(_))
                && let Some(overlay) = &self.overlay
            {
                overlay.tick();
            }
            let result = match fire {
                Fire::Run(action) => run(action, active, self.backend.as_ref(), &self.input),
                Fire::Hold(keys) => focus(active, self.backend.as_ref()).map(|()| self.input.hold(keys)),
                Fire::Release(keys) => {
                    self.input.unhold(keys);
                    Ok(())
                }
            };
            if let Err(e) = result {
                log::error!("{e:#}");
            }
        }
    }
}

fn run(action: Action, active: &mut Active, backend: &dyn Backend, input: &Input) -> anyhow::Result<()> {
    log::info!("{:?}: {action:?}", active.window.class);
    match action {
        Action::Keys(combo) => focus(active, backend).map(|()| input.type_combo(combo)),
        Action::Command(command) => spawn(&command, &active.window),
        // Dispatchers act on the active window, so it must be this one too.
        Action::Dispatch(command) => focus(active, backend).and_then(|()| backend.dispatch(&command)),
    }
}

/// Keys and dispatchers must reach the window the gesture was drawn over,
/// even if another window has focus. Focuses once per gesture, not on every
/// repeat.
fn focus(active: &mut Active, backend: &dyn Backend) -> anyhow::Result<()> {
    if !active.focused && !active.window.id.is_empty() {
        backend.focus(&active.window)?;
    }
    active.focused = true;
    Ok(())
}

/// Runs a shell command without waiting for it. It sees the window in
/// `WISP_CLASS` and `WISP_TITLE`, and wisp's own directory comes first in
/// `PATH`, so `wisp-settings` is found even when a service's PATH is bare.
fn spawn(command: &str, window: &Window) -> anyhow::Result<()> {
    let mut path: Vec<_> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .into_iter()
        .collect();
    path.extend(std::env::var_os("PATH").iter().flat_map(std::env::split_paths));
    let mut child = Command::new("sh")
        .args(["-c", command])
        .env("PATH", std::env::join_paths(path)?)
        .env("WISP_CLASS", &window.class)
        .env("WISP_TITLE", &window.title)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("running {command:?}"))?;
    thread::spawn(move || child.wait());
    Ok(())
}
