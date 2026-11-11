//! What one gesture fires, given the gestures for the window it started on.
//!
//! Oneshot strokes fire on release. Repeat and hold strokes fire as soon as
//! they match and then lock: recognition stops, and every `step` of motion
//! in the stroke's last direction, and every wheel notch, fires again.
//! Wheel and click gestures fire on each notch or click.

use crate::config::{Action, Gesture, Input, Kind, Wheel};
use crate::keys::Combo;
use crate::stroke::{Dir, Stroke};

#[derive(Clone, Debug, PartialEq)]
pub enum Fire {
    Run(Action),
    /// Press these keys and keep them down.
    Hold(Combo),
    /// Release keys held by `Hold`.
    Release(Combo),
}

#[derive(Debug)]
pub struct Session {
    gestures: Vec<Gesture>,
    locked: Option<Locked>,
    /// Whether anything has fired yet.
    fired: bool,
}

#[derive(Debug)]
struct Locked {
    action: Action,
    held: Option<Combo>,
    /// Unit vector of the stroke's last direction.
    axis: (f64, f64),
    step: f64,
    travel: f64,
}

impl Session {
    pub fn new(gestures: Vec<Gesture>) -> Self {
        Session {
            gestures,
            locked: None,
            fired: false,
        }
    }

    /// Whether the gesture matched anything, for feedback.
    pub fn matched(&self) -> bool {
        self.fired
    }

    /// Whether a repeat or hold gesture has taken over.
    pub fn locked(&self) -> bool {
        self.locked.is_some()
    }

    /// The live stroke changed.
    pub fn stroke(&mut self, stroke: &Stroke) -> Vec<Fire> {
        if self.locked.is_some() || self.fired {
            return Vec::new();
        }
        let Some(gesture) = self
            .find(&Input::Stroke(stroke.clone()))
            .filter(|g| g.kind.continuous())
        else {
            return Vec::new();
        };
        let (held, step) = match &gesture.kind {
            Kind::Repeat { step } => (None, *step),
            Kind::Hold { keys, step } => (Some(keys.clone()), *step),
            Kind::Oneshot => unreachable!("filtered above"),
        };
        let last = *stroke.0.last().expect("strokes aren't empty");
        let action = gesture.action.clone();
        let mut fires: Vec<Fire> = held.iter().cloned().map(Fire::Hold).collect();
        fires.push(Fire::Run(action.clone()));
        self.locked = Some(Locked {
            action,
            held,
            axis: unit(last),
            step,
            travel: 0.0,
        });
        self.fired = true;
        fires
    }

    /// Pointer motion during the gesture.
    pub fn motion(&mut self, dx: f64, dy: f64) -> Vec<Fire> {
        let Some(locked) = &mut self.locked else {
            return Vec::new();
        };
        // Backing up doesn't count against the next step.
        locked.travel = (locked.travel + dx * locked.axis.0 + dy * locked.axis.1).max(0.0);
        let mut fires = Vec::new();
        while locked.travel >= locked.step {
            locked.travel -= locked.step;
            fires.push(Fire::Run(locked.action.clone()));
        }
        fires
    }

    pub fn wheel(&mut self, wheel: Wheel) -> Vec<Fire> {
        if let Some(locked) = &self.locked {
            return vec![Fire::Run(locked.action.clone())];
        }
        self.fire_once(&Input::Wheel(wheel))
    }

    pub fn click(&mut self, button: u16) -> Vec<Fire> {
        if self.locked.is_some() {
            return Vec::new();
        }
        self.fire_once(&Input::Click(button))
    }

    /// The trigger was released with this final stroke.
    pub fn end(&mut self, stroke: &Stroke) -> Vec<Fire> {
        if self.locked.is_some() {
            return self.cancel();
        }
        // After a wheel or click gesture, the motion was incidental.
        if self.fired || stroke.is_empty() {
            return Vec::new();
        }
        self.fire_once(&Input::Stroke(stroke.clone()))
    }

    /// Ends the gesture without firing; releases held keys.
    pub fn cancel(&mut self) -> Vec<Fire> {
        self.locked
            .take()
            .and_then(|locked| locked.held)
            .map(Fire::Release)
            .into_iter()
            .collect()
    }

    fn find(&self, input: &Input) -> Option<&Gesture> {
        self.gestures.iter().find(|g| &g.input == input)
    }

    fn fire_once(&mut self, input: &Input) -> Vec<Fire> {
        match self.find(input) {
            Some(gesture) => {
                let action = gesture.action.clone();
                self.fired = true;
                vec![Fire::Run(action)]
            }
            None => Vec::new(),
        }
    }
}

fn unit(dir: Dir) -> (f64, f64) {
    let d = std::f64::consts::FRAC_1_SQRT_2;
    match dir {
        Dir::Up => (0.0, -1.0),
        Dir::Down => (0.0, 1.0),
        Dir::Left => (-1.0, 0.0),
        Dir::Right => (1.0, 0.0),
        Dir::UpLeft => (-d, -d),
        Dir::UpRight => (d, -d),
        Dir::DownLeft => (-d, d),
        Dir::DownRight => (d, d),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    const GESTURES: &str = r#"
        [[gesture]]
        stroke = "L"
        action = { keys = "alt+Left" }

        [[gesture]]
        stroke = "U"
        repeat = true
        step = 50
        action = { keys = "ctrl+Tab" }

        [[gesture]]
        stroke = "R"
        step = 50
        action = { keys = "Tab", hold = "alt" }

        [[gesture]]
        wheel = "down"
        action = { command = "scrolled" }

        [[gesture]]
        click = "right"
        action = { command = "rocked" }
    "#;

    fn session() -> Session {
        Session::new(Config::parse(GESTURES, &[]).unwrap().gestures)
    }

    fn s(stroke: &str) -> Stroke {
        stroke.parse().unwrap()
    }

    fn keys(combo: &str) -> Fire {
        Fire::Run(Action::Keys(combo.parse().unwrap()))
    }

    fn command(c: &str) -> Fire {
        Fire::Run(Action::Command(c.into()))
    }

    #[test]
    fn oneshot_fires_on_release_only() {
        let mut session = session();
        assert_eq!(session.stroke(&s("L")), []);
        assert!(!session.matched());
        assert_eq!(session.end(&s("L")), [keys("alt+Left")]);
        assert!(session.matched());
    }

    #[test]
    fn unknown_stroke_fires_nothing() {
        let mut session = session();
        assert_eq!(session.end(&s("DR")), []);
        assert!(!session.matched());
    }

    #[test]
    fn repeat_fires_on_match_then_per_step() {
        let mut session = session();
        assert_eq!(session.stroke(&s("U")), [keys("ctrl+Tab")]);
        assert_eq!(session.motion(0.0, -30.0), []);
        assert_eq!(session.motion(0.0, -30.0), [keys("ctrl+Tab")]);
        assert_eq!(session.motion(0.0, -100.0), [keys("ctrl+Tab"), keys("ctrl+Tab")]);
        // Sideways motion doesn't count; backing up resets toward zero.
        assert_eq!(session.motion(200.0, 0.0), []);
        assert_eq!(session.motion(0.0, 500.0), []);
        assert_eq!(session.motion(0.0, -49.0), []);
        // Once locked, the stroke no longer matters.
        assert_eq!(session.stroke(&s("UR")), []);
        assert_eq!(session.wheel(Wheel::Up), [keys("ctrl+Tab")]);
        assert_eq!(session.end(&s("UR")), []);
    }

    #[test]
    fn hold_presses_then_releases() {
        let mut session = session();
        let alt: Combo = "alt".parse().unwrap();
        assert_eq!(session.stroke(&s("R")), [Fire::Hold(alt.clone()), keys("Tab")]);
        assert_eq!(session.motion(60.0, 0.0), [keys("Tab")]);
        assert_eq!(session.end(&s("R")), [Fire::Release(alt.clone())]);
        assert_eq!(session.cancel(), [], "keys are released only once");

        let mut session = self::session();
        session.stroke(&s("R"));
        assert_eq!(session.cancel(), [Fire::Release(alt)]);
    }

    #[test]
    fn wheel_and_click_fire_each_time_and_skip_the_stroke() {
        let mut session = session();
        assert_eq!(session.wheel(Wheel::Down), [command("scrolled")]);
        assert_eq!(session.wheel(Wheel::Down), [command("scrolled")]);
        assert_eq!(session.wheel(Wheel::Up), []);
        assert_eq!(session.click(crate::config::BTN_RIGHT), [command("rocked")]);
        assert_eq!(session.stroke(&s("U")), [], "no repeat after a wheel gesture");
        assert_eq!(session.end(&s("L")), []);
    }
}
