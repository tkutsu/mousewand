//! What one gesture fires, given the gestures for the window it started on.
//!
//! Oneshot strokes fire on release, and so do shapes, when no stroke
//! matches what was drawn. With neither, the oneshot stroke the drawing
//! comes closest to fires, if it's close enough. Repeat and hold strokes
//! fire as soon as they match exactly and then lock: recognition stops,
//! and every `step` of motion in the stroke's last direction, and every
//! wheel notch, fires again. Wheel and click gestures fire on each notch
//! or click.

use crate::config::{Action, Gesture, Input, Kind, Wheel};
use crate::keys::{Combo, Mods};
use crate::shape;
use crate::stroke::{Drawing, Stroke};

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
    /// The modifier keys held on the keyboard: only gestures `with` exactly
    /// these can fire.
    mods: Mods,
    /// Given up on: nothing fires any more.
    abandoned: bool,
    /// How far a drawing may stray from a oneshot stroke, in degrees.
    tolerance: f64,
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
    /// `tolerance` is the `stroke_tolerance` setting.
    pub fn new(gestures: Vec<Gesture>, tolerance: f64) -> Self {
        Session {
            gestures,
            locked: None,
            fired: false,
            mods: Mods::NONE,
            abandoned: false,
            tolerance,
        }
    }

    /// Sets the modifier keys held now.
    pub fn set_mods(&mut self, mods: Mods) {
        self.mods = mods;
    }

    pub fn mods(&self) -> Mods {
        self.mods
    }

    /// The gestures that could fire with the modifiers held now.
    pub fn candidates(&self) -> impl Iterator<Item = &Gesture> {
        self.gestures.iter().filter(|g| g.with == self.mods)
    }

    /// Gives up on the gesture, unless something has fired: nothing fires
    /// from now on. Returns whether it gave up.
    pub fn abandon(&mut self) -> bool {
        if self.fired || self.locked.is_some() {
            return false;
        }
        self.abandoned = true;
        true
    }

    pub fn abandoned(&self) -> bool {
        self.abandoned
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
        if self.locked.is_some() || self.fired || self.abandoned {
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
            axis: last.unit(),
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
        if self.abandoned {
            return Vec::new();
        }
        self.fire_once(&Input::Wheel(wheel))
    }

    pub fn click(&mut self, button: u16) -> Vec<Fire> {
        if self.locked.is_some() || self.abandoned {
            return Vec::new();
        }
        self.fire_once(&Input::Click(button))
    }

    /// The trigger was released with this drawing. A stroke that matches
    /// wins, being exact; else the shape the path looks like; else the
    /// closest stroke.
    pub fn end(&mut self, drawing: &Drawing) -> Vec<Fire> {
        if self.locked.is_some() {
            return self.cancel();
        }
        // After a wheel or click gesture, the motion was incidental.
        if self.fired || self.abandoned || drawing.stroke.is_empty() {
            return Vec::new();
        }
        let exact = Input::Stroke(drawing.stroke.clone());
        if self.find(&exact).is_some() {
            return self.fire_once(&exact);
        }
        let shapes = self.candidates().filter_map(|g| match &g.input {
            Input::Shape(shape) => Some(shape),
            _ => None,
        });
        let shape = shape::recognize(shapes, &drawing.path).cloned().map(Input::Shape);
        match shape.or_else(|| self.closest(drawing)) {
            Some(input) => self.fire_once(&input),
            None => Vec::new(),
        }
    }

    /// The oneshot stroke the drawing strays least from, within the
    /// tolerance. Repeat and hold strokes take only exact matches: they
    /// lock mid-gesture, where the drawing isn't finished yet.
    fn closest(&self, drawing: &Drawing) -> Option<Input> {
        self.candidates()
            .filter(|g| !g.kind.continuous())
            .filter_map(|g| match &g.input {
                Input::Stroke(stroke) => Some((drawing.deviation(stroke), &g.input)),
                _ => None,
            })
            .filter(|&(deviation, _)| deviation <= self.tolerance)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, input)| input.clone())
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
        self.candidates().find(|g| &g.input == input)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::stroke::{Dir, Recognizer};

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

    /// The default `stroke_tolerance`.
    const TOLERANCE: f64 = 30.0;

    fn session() -> Session {
        Session::new(Config::parse(GESTURES, &[], &[]).unwrap().gestures, TOLERANCE)
    }

    fn s(stroke: &str) -> Stroke {
        stroke.parse().unwrap()
    }

    /// Straight lines, each `n` moves of `(dx, dy)`.
    fn drawing(moves: &[(f64, f64, usize)]) -> Drawing {
        let mut rec = Recognizer::new(30.0);
        for &(dx, dy, n) in moves {
            (0..n).for_each(|_| rec.push(dx, dy));
        }
        rec.finish()
    }

    /// The stroke drawn as it's spelled.
    fn drawn(stroke: &str) -> Drawing {
        let line = |d: Dir| (3.0 * d.unit().0, 3.0 * d.unit().1, 40);
        let moves: Vec<_> = s(stroke).0.into_iter().map(line).collect();
        let drawing = drawing(&moves);
        assert_eq!(drawing.stroke, s(stroke));
        drawing
    }

    /// A line `degrees` counter-clockwise from right.
    fn line(degrees: f64) -> Drawing {
        let (sin, cos) = degrees.to_radians().sin_cos();
        drawing(&[(3.0 * cos, -3.0 * sin, 40)])
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
        assert_eq!(session.end(&drawn("L")), [keys("alt+Left")]);
        assert!(session.matched());
    }

    #[test]
    fn shapes_fire_when_no_stroke_matches() {
        let moves: Vec<_> = (0..40)
            .map(|i| (i as f64 / 40.0 * std::f64::consts::TAU).sin_cos())
            .map(|(sin, cos)| (-15.7 * sin, 9.4 * cos, 1))
            .collect();
        let circle = drawing(&moves);
        let settings = Fire::Run(Action::Command("mousewand-settings".into()));
        let gestures = Config::parse(GESTURES, &[], &[]).unwrap().gestures_for("x");
        let mut session = Session::new(gestures.clone(), TOLERANCE);
        assert_eq!(session.end(&circle), [settings]);
        // "L" is bound, so it wins whatever the path looks like.
        let mut session = Session::new(gestures, TOLERANCE);
        let mut left = drawn("L");
        left.path = circle.path;
        assert_eq!(session.end(&left), [keys("alt+Left")]);
    }

    #[test]
    fn the_closest_oneshot_fires_when_nothing_matches() {
        // Left, leaning 25° up: drawn as 7, but closest to L.
        let leaning = line(155.0);
        assert_eq!(leaning.stroke, s("7"));
        assert_eq!(session().end(&leaning), [keys("alt+Left")]);
        // Leaning 40° up is too far from L.
        assert_eq!(session().end(&line(140.0)), []);
        // Up, leaning right, is closest to U, which repeats: those match only exactly.
        assert_eq!(session().end(&line(65.0)), []);
        // With no tolerance, only exact strokes fire.
        let gestures = Config::parse(GESTURES, &[], &[]).unwrap().gestures;
        assert_eq!(Session::new(gestures, 0.0).end(&leaning), []);
    }

    #[test]
    fn unknown_stroke_fires_nothing() {
        let mut session = session();
        assert_eq!(session.end(&drawn("DR")), []);
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
        assert_eq!(session.end(&drawn("UR")), []);
    }

    #[test]
    fn hold_presses_then_releases() {
        let mut session = session();
        let alt: Combo = "alt".parse().unwrap();
        assert_eq!(session.stroke(&s("R")), [Fire::Hold(alt.clone()), keys("Tab")]);
        assert_eq!(session.motion(60.0, 0.0), [keys("Tab")]);
        assert_eq!(session.end(&drawn("R")), [Fire::Release(alt.clone())]);
        assert_eq!(session.cancel(), [], "keys are released only once");

        let mut session = self::session();
        session.stroke(&s("R"));
        assert_eq!(session.cancel(), [Fire::Release(alt)]);
    }

    #[test]
    fn modifiers_pick_the_gesture() {
        let gestures = r#"
            [[gesture]]
            stroke = "L"
            action = { keys = "alt+Left" }

            [[gesture]]
            stroke = "L"
            with = "shift"
            action = { keys = "ctrl+shift+Tab" }
        "#;
        let gestures = Config::parse(gestures, &[], &[]).unwrap().gestures;
        let mut session = Session::new(gestures.clone(), TOLERANCE);
        session.set_mods("shift".parse().unwrap());
        assert_eq!(session.end(&drawn("L")), [keys("ctrl+shift+Tab")]);
        let mut session = Session::new(gestures.clone(), TOLERANCE);
        assert_eq!(session.end(&drawn("L")), [keys("alt+Left")]);
        let mut session = Session::new(gestures, TOLERANCE);
        session.set_mods("ctrl".parse().unwrap());
        assert_eq!(session.end(&drawn("L")), [], "ctrl+L is no gesture");
    }

    #[test]
    fn abandoned_fires_nothing_but_only_before_a_fire() {
        let mut session = session();
        assert!(session.abandon());
        assert_eq!(session.end(&drawn("L")), []);
        assert_eq!(session.wheel(Wheel::Down), []);
        let mut session = self::session();
        session.stroke(&s("U"));
        assert!(!session.abandon(), "a repeat has taken over");
        assert_eq!(session.wheel(Wheel::Up), [keys("ctrl+Tab")]);
    }

    #[test]
    fn wheel_and_click_fire_each_time_and_skip_the_stroke() {

        let mut session = session();
        assert_eq!(session.wheel(Wheel::Down), [command("scrolled")]);
        assert_eq!(session.wheel(Wheel::Down), [command("scrolled")]);
        assert_eq!(session.wheel(Wheel::Up), []);
        assert_eq!(session.click(crate::config::BTN_RIGHT), [command("rocked")]);
        assert_eq!(session.stroke(&s("U")), [], "no repeat after a wheel gesture");
        assert_eq!(session.end(&drawn("L")), []);
    }
}
