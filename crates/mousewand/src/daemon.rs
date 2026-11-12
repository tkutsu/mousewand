//! Turns gesture outputs from the input thread into actions.

use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use mousewand_backend::{Backend, Target, Window};
use mousewand_core::{Action, Config, Fire, Gesture, Hint, Output, Session, Stroke, Trail, Trigger};
use mousewand_input::{Input, Keyboards};

pub struct Daemon {
    pub config: Config,
    pub backend: Box<dyn Backend>,
    pub input: Input,
    pub overlay: Option<Box<dyn Trail>>,
    keyboards: Keyboards,
    /// Whether the notification that keyboards can't be read went out.
    told_keyboard: bool,
    paused: bool,
    /// The target looked up when the trigger went down, for the gesture.
    pressed: Option<Target>,
    /// The press went to the app as it is: nothing until the next one.
    passed: bool,
    active: Option<Active>,
}

/// The gesture in progress.
struct Active {
    window: Window,
    session: Session,
    /// Whether the window has been focused for this gesture yet.
    focused: bool,
    stroke: Stroke,
    /// What fired last, for the hint.
    fired: Option<String>,
}

/// How long a command in a sequence may run before the next step goes on
/// without it.
const STEP_TIMEOUT: Duration = Duration::from_secs(3);
/// A pause between steps, so the app has dealt with keys before the next.
const STEP_GAP: Duration = Duration::from_millis(40);
/// How long keys wait for the modifiers of a `with` gesture to be let go.
const RELEASE_TIMEOUT: Duration = Duration::from_millis(1500);

impl Daemon {
    pub fn new(config: Config, backend: Box<dyn Backend>, input: Input, overlay: Option<Box<dyn Trail>>) -> Self {
        Daemon {
            config,
            backend,
            input,
            overlay,
            keyboards: Keyboards::new(),
            told_keyboard: false,
            paused: false,
            pressed: None,
            passed: false,
            active: None,
        }
    }

    pub fn on_output(&mut self, output: Output, trigger: Trigger) {
        if self.passed && !matches!(output, Output::Down) {
            return;
        }
        match output {
            Output::Down => self.down(),
            Output::Begin => self.begin(Some(trigger) == self.config.settings.global_trigger),
            Output::Update(stroke) => {
                log::debug!("stroke {stroke}");
                self.read_mods();
                if let Some(active) = &mut self.active {
                    active.stroke = stroke.clone();
                }
                self.run(|s| s.stroke(&stroke));
                self.show_hint(false);
            }
            Output::Motion(dx, dy) => self.run(|s| s.motion(dx, dy)),
            Output::Still => self.still(),
            Output::Wheel(wheel) => self.run(|s| s.wheel(wheel)),
            Output::Click(button) => self.run(|s| s.click(button)),
            Output::End(stroke, path) => {
                if self.active.as_ref().is_some_and(|a| a.session.abandoned()) {
                    self.active = None;
                    return;
                }
                self.read_mods();
                if let Some(active) = &mut self.active {
                    active.stroke = stroke.clone();
                }
                self.run(|s| s.end(&stroke, &path));
                self.show_hint(true);
                let matched = self.active.take().is_some_and(|a| a.session.matched());
                if !matched && !stroke.is_empty() {
                    log::info!("{stroke}: no gesture");
                }
                self.overlay(|o| o.end(matched));
            }
            Output::Cancel => {
                let abandoned = self.active.as_ref().is_some_and(|a| a.session.abandoned());
                self.run(Session::cancel);
                self.active = None;
                if !abandoned {
                    self.overlay(|o| o.end(true));
                }
            }
            Output::Press | Output::Release => {}
        }
    }

    /// Pauses gestures, or resumes them: paused, the triggers are ordinary
    /// buttons.
    pub fn toggle(&mut self) {
        self.paused = !self.paused;
        self.input.set_paused(self.paused);
        log::info!("{}", if self.paused { "paused" } else { "resumed" });
        let (title, body) = match self.paused {
            true => (
                "mousewand paused",
                "The trigger is an ordinary button until you resume.",
            ),
            false => ("mousewand resumed", "Gestures work again."),
        };
        if let Err(e) = crate::notes::notify(title, body, false) {
            log::warn!("can't show a notification: {e:#}");
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

    /// Looks up what's under the cursor as soon as the trigger goes down.
    /// An app with gestures turned off gets the press back right away.
    fn down(&mut self) {
        self.passed = false;
        self.active = None;
        let target = self.backend.pointer_target().unwrap_or_else(|e| {
            log::warn!("can't find the window under the cursor: {e:#}");
            Target::default()
        });
        let class = target.window.as_ref().map_or("", |w| w.class.as_str());
        if !self.config.enabled_for(class) {
            log::debug!("gestures are off over {class:?}");
            self.passed = true;
            self.input.pass();
            return;
        }
        self.pressed = Some(target);
    }

    /// Over the desktop, or when the lookup fails, the window has no id or
    /// class: it still gets the shared and global gestures. The global
    /// trigger gets only the global gestures, and keys go to whichever
    /// window has focus.
    fn begin(&mut self, global: bool) {
        let target = self.pressed.take().unwrap_or_default();
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
            session: Session::new(gestures),
            window,
            focused: global,
            stroke: Stroke::default(),
            fired: None,
        });
        self.read_mods();
        self.show_hint(false);
    }

    /// Holding still mid-stroke gives up on it, unless it has fired.
    fn still(&mut self) {
        let Some(active) = &mut self.active else { return };
        if active.stroke.is_empty() || !active.session.abandon() {
            return;
        }
        log::info!("{}: cancelled", active.stroke);
        if self.config.hint.enabled
            && let Some(overlay) = &self.overlay
        {
            overlay.hint(Hint::cancelled(&active.stroke));
        }
        self.overlay(|o| o.end(false));
    }

    /// Tells the session which modifiers are held, if any gesture cares.
    fn read_mods(&mut self) {
        let Some(active) = &mut self.active else { return };
        if !self.config.uses_modifiers() {
            return;
        }
        match self.keyboards.mods() {
            Some(mods) => active.session.set_mods(mods),
            None if !self.told_keyboard => {
                self.told_keyboard = true;
                log::warn!("can't read the keyboard, so gestures `with` modifiers never fire");
                let body = "Gestures with Shift, Ctrl, Alt or Super need to see which keys are down. \
                            The README's Modifiers section shows how to allow it.";
                if let Err(e) = crate::notes::notify("mousewand: can't read the keyboard", body, false) {
                    log::warn!("can't show a notification: {e:#}");
                }
            }
            None => {}
        }
    }

    /// Sends the hint for the gesture in progress, if hints are on.
    fn show_hint(&self, ended: bool) {
        let (Some(active), Some(overlay)) = (&self.active, &self.overlay) else {
            return;
        };
        if !self.config.hint.enabled || active.session.abandoned() {
            return;
        }
        let hint = match (&active.fired, ended) {
            (Some(label), _) => Hint::fired(&active.stroke, label.clone()),
            (None, true) => Hint::missed(&active.stroke),
            (None, false) => {
                let candidates: Vec<Gesture> = active.session.candidates().cloned().collect();
                Hint::drawing(&candidates, &active.stroke, self.config.hint.next)
            }
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
                active.fired = active
                    .session
                    .candidates()
                    .find(|g| &g.action == action)
                    .map(Gesture::label);
                // Shift held on the keyboard would turn ctrl+t into
                // ctrl+shift+t: keys wait until it's let go.
                if !active.session.mods().is_empty() {
                    self.keyboards.wait_for_release(RELEASE_TIMEOUT);
                }
            }
            if was_locked
                && matches!(fire, Fire::Run(_))
                && let Some(overlay) = &self.overlay
            {
                overlay.tick();
            }
            let result = match fire {
                Fire::Run(action) => {
                    log::info!("{:?}: {action:?}", active.window.class);
                    run(&action, false, active, self.backend.as_ref(), &self.input)
                }
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

/// Carries out one action. In a sequence, a command finishes before the
/// next step, up to [`STEP_TIMEOUT`]: `wait` says so.
fn run(action: &Action, wait: bool, active: &mut Active, backend: &dyn Backend, input: &Input) -> anyhow::Result<()> {
    match action {
        Action::Keys(combo) => focus(active, backend).map(|()| input.type_combo(combo.clone())),
        Action::Command(command) => spawn(command, &active.window, wait),
        // Dispatchers act on the active window, so it must be this one too.
        Action::Dispatch(command) => focus(active, backend).and_then(|()| backend.dispatch(command)),
        Action::Wait(ms) => {
            thread::sleep(Duration::from_millis(*ms));
            Ok(())
        }
        Action::Sequence(steps) => {
            for (i, step) in steps.iter().enumerate() {
                if i > 0 {
                    thread::sleep(STEP_GAP);
                }
                let last = i + 1 == steps.len();
                run(step, !last, active, backend, input)?;
            }
            Ok(())
        }
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

/// Runs a shell command, waiting for it only if `wait`. It sees the window
/// in `MOUSEWAND_CLASS` and `MOUSEWAND_TITLE`, the selected text in
/// `MOUSEWAND_SELECTION` if it mentions it, and mousewand's directory in
/// `PATH`. A command that isn't installed gets a notification, since a
/// gesture that does nothing looks like mousewand's fault.
fn spawn(command: &str, window: &Window, wait: bool) -> anyhow::Result<()> {
    let mut sh = Command::new("sh");
    sh.args(["-c", command])
        .env("PATH", path()?)
        .env("MOUSEWAND_CLASS", &window.class)
        .env("MOUSEWAND_TITLE", &window.title)
        .stdin(Stdio::null());
    if command.contains("MOUSEWAND_SELECTION") {
        sh.env("MOUSEWAND_SELECTION", selection());
    }
    let mut child = sh.spawn().with_context(|| format!("running {command:?}"))?;
    if wait {
        let start = Instant::now();
        while start.elapsed() < STEP_TIMEOUT {
            if let Some(status) = child.try_wait()? {
                if status.code() == Some(127) {
                    tell_not_found(command);
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        log::info!("{command:?} is still running; the next step goes on without it");
    }
    reap(child, command.to_owned());
    Ok(())
}

/// The text selected last, in any window: the primary selection, which
/// selecting text sets without copying it. Empty if there's none, or if
/// the app holding it doesn't answer within [`SELECTION_TIMEOUT`].
fn selection() -> String {
    let (program, args): (&str, &[&str]) = match std::env::var_os("WAYLAND_DISPLAY") {
        Some(_) => ("wl-paste", &["--primary", "--no-newline"]),
        None => ("xclip", &["-o", "-selection", "primary"]),
    };
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(e) => {
            log::warn!("can't read the selection with {program}: {e}");
            return String::new();
        }
    };
    // Read on a thread, so a long selection can't fill the pipe and stall.
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(SELECTION_TIMEOUT) {
        Ok(Ok(out)) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Ok(Err(e)) => {
            log::warn!("can't read the selection with {program}: {e}");
            String::new()
        }
        Err(_) => {
            log::warn!("{program} took too long to read the selection");
            // SAFETY: kill only sends a signal; the reader thread reaps it.
            unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
            String::new()
        }
    }
}

/// How long the app holding the selection gets to hand it over.
const SELECTION_TIMEOUT: Duration = Duration::from_millis(300);

/// Waits for the command in the background, to tell if it wasn't found.
fn reap(mut child: Child, command: String) {
    thread::spawn(move || {
        // sh exits with 127 when it can't find the program.
        if child.wait().is_ok_and(|status| status.code() == Some(127)) {
            tell_not_found(&command);
        }
    });
}

fn tell_not_found(command: &str) {
    if let Err(e) = crate::notes::notify("mousewand: command not found", &not_found(command), false) {
        log::warn!("can't show a notification: {e:#}");
    }
}

fn not_found(command: &str) -> String {
    if command == "mousewand-settings" {
        return "Drawing a circle opens the settings app, mousewand-settings, which isn't installed. \
                The README's Settings app section shows how to build it."
            .into();
    }
    format!("A gesture ran `{command}`, and the shell couldn't find it.")
}
