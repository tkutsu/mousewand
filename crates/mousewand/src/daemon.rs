//! Turns gesture outputs from the input thread into actions.

use std::process::{Command, Stdio};
use std::thread;

use anyhow::Context;
use mousewand_backend::{Backend, Target, Window};
use mousewand_core::{Action, Config, Fire, Gesture, Hint, Output, Session, Stroke, Trail, Trigger};
use mousewand_input::Input;

pub struct Daemon {
    pub config: Config,
    pub backend: Box<dyn Backend>,
    pub input: Input,
    pub overlay: Option<Box<dyn Trail>>,
    active: Option<Active>,
}

/// The gesture in progress.
struct Active {
    window: Window,
    session: Session,
    /// Whether the window has been focused for this gesture yet.
    focused: bool,
    /// The gestures the session draws from, for the hint.
    gestures: Vec<Gesture>,
    stroke: Stroke,
    /// What fired last, for the hint.
    fired: Option<String>,
}

impl Daemon {
    pub fn new(config: Config, backend: Box<dyn Backend>, input: Input, overlay: Option<Box<dyn Trail>>) -> Self {
        Daemon {
            config,
            backend,
            input,
            overlay,
            active: None,
        }
    }

    pub fn on_output(&mut self, output: Output, trigger: Trigger) {
        match output {
            Output::Begin => self.begin(Some(trigger) == self.config.settings.global_trigger),
            Output::Update(stroke) => {
                log::debug!("stroke {stroke}");
                if let Some(active) = &mut self.active {
                    active.stroke = stroke.clone();
                }
                self.run(|s| s.stroke(&stroke));
                self.show_hint(false);
            }
            Output::Motion(dx, dy) => self.run(|s| s.motion(dx, dy)),
            Output::Wheel(wheel) => self.run(|s| s.wheel(wheel)),
            Output::Rocker(button) => self.run(|s| s.rocker(button)),
            Output::End(stroke) => {
                if let Some(active) = &mut self.active {
                    active.stroke = stroke.clone();
                }
                self.run(|s| s.end(&stroke));
                self.show_hint(true);
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
    /// class: it still gets the shared and global gestures. The global
    /// trigger gets only the global gestures, and keys go to whichever
    /// window has focus.
    fn begin(&mut self, global: bool) {
        let target = self.backend.pointer_target().unwrap_or_else(|e| {
            log::warn!("can't find the window under the cursor: {e:#}");
            Target::default()
        });
        self.overlay(|o| o.begin(target.monitor));
        let window = target.window.unwrap_or_default();
        log::debug!(
            "gesture over {:?}{}",
            window.class,
            if global { ", global" } else { "" }
        );
        let gestures = match global {
            true => self.config.global_gestures(),
            false => self.config.gestures_for(&window.class),
        };
        self.active = Some(Active {
            session: Session::new(gestures.clone()),
            gestures,
            window,
            focused: global,
            stroke: Stroke::default(),
            fired: None,
        });
        self.show_hint(false);
    }

    /// Sends the hint for the gesture in progress, if hints are on.
    fn show_hint(&self, ended: bool) {
        let (Some(active), Some(overlay)) = (&self.active, &self.overlay) else {
            return;
        };
        if !self.config.hint.enabled {
            return;
        }
        let hint = match (&active.fired, ended) {
            (Some(label), _) => Hint::fired(&active.stroke, label.clone()),
            (None, true) => Hint::missed(&active.stroke),
            (None, false) => Hint::drawing(&active.gestures, &active.stroke, self.config.hint.next),
        };
        overlay.hint(hint);
    }

    fn overlay(&self, f: impl FnOnce(&dyn Trail)) {
        if let Some(overlay) = &self.overlay {
            f(overlay.as_ref());
        }
    }

    /// Feeds the gesture's session and carries out what it fires.
    fn run(&mut self, step: impl FnOnce(&mut Session) -> Vec<Fire>) {
        let Some(active) = &mut self.active else { return };
        let was_locked = active.session.locked();
        let mut fired = false;
        for fire in step(&mut active.session) {
            if let Fire::Run(action) = &fire {
                fired = true;
                active.fired = active.gestures.iter().find(|g| &g.action == action).map(Gesture::label);
            }
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
        if fired {
            self.show_hint(false);
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

/// `PATH` with mousewand's own directory first, so `mousewand-settings` is found even
/// when a service's PATH is bare.
pub fn path() -> anyhow::Result<std::ffi::OsString> {
    let mut path: Vec<_> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .into_iter()
        .collect();
    path.extend(std::env::var_os("PATH").iter().flat_map(std::env::split_paths));
    Ok(std::env::join_paths(path)?)
}

/// Runs a shell command without waiting for it. It sees the window in
/// `MOUSEWAND_CLASS` and `MOUSEWAND_TITLE`, and mousewand's directory in `PATH`. A command
/// that isn't installed gets a notification, since a gesture that does
/// nothing looks like mousewand's fault.
fn spawn(command: &str, window: &Window) -> anyhow::Result<()> {
    let mut child = Command::new("sh")
        .args(["-c", command])
        .env("PATH", path()?)
        .env("MOUSEWAND_CLASS", &window.class)
        .env("MOUSEWAND_TITLE", &window.title)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("running {command:?}"))?;
    let command = command.to_owned();
    thread::spawn(move || {
        // sh exits with 127 when it can't find the program.
        if child.wait().is_ok_and(|status| status.code() == Some(127))
            && let Err(e) = crate::notes::notify("mousewand: command not found", &not_found(&command), false)
        {
            log::warn!("can't show a notification: {e:#}");
        }
    });
    Ok(())
}

fn not_found(command: &str) -> String {
    if command == "mousewand-settings" {
        return "Drawing a W opens the settings app, mousewand-settings, which isn't installed. \
                The README's Settings app section shows how to build it."
            .into();
    }
    format!("A gesture ran `{command}`, and the shell couldn't find it.")
}
