//! What the trigger button does: a click, a normal hold, or a gesture.
//!
//! The input thread feeds trigger presses, releases and motion in, with the
//! current time, and acts on the outputs. Motion is always forwarded to the
//! cursor; only the trigger button itself is held back until it's clear the
//! press isn't a gesture.

use std::time::Duration;

use crate::config::Settings;
use crate::stroke::{Recognizer, Stroke};

#[derive(Clone, PartialEq, Debug)]
pub enum Output {
    /// Send a real trigger press: a replayed click, or a hold without movement.
    Press,
    /// Send a real trigger release.
    Release,
    /// The cursor moved past the threshold: a gesture has started.
    Begin,
    /// The recognized stroke changed mid-gesture.
    Update(Stroke),
    /// The trigger was released and the gesture is over.
    End(Stroke),
    /// The gesture was abandoned (e.g. its mouse was unplugged): fire nothing.
    Cancel,
}

#[derive(Debug)]
enum State {
    Idle,
    /// Pressed, and not yet a click, hold or gesture.
    Pending {
        since: Duration,
        moved: (f64, f64),
        rec: Recognizer,
    },
    /// Held without moving: behaves as a normal button until release.
    Holding,
    Gesturing {
        rec: Recognizer,
        shown: Stroke,
    },
}

#[derive(Debug)]
pub struct Machine {
    threshold: f64,
    hold: Duration,
    min_segment: f64,
    state: State,
}

impl Machine {
    pub fn new(settings: &Settings) -> Self {
        Machine {
            threshold: settings.threshold,
            hold: Duration::from_millis(settings.hold_ms),
            min_segment: settings.min_segment,
            state: State::Idle,
        }
    }

    /// When `tick` must be called next, if a press is pending.
    pub fn deadline(&self) -> Option<Duration> {
        match self.state {
            State::Pending { since, .. } => Some(since + self.hold),
            _ => None,
        }
    }

    pub fn press(&mut self, now: Duration) -> Vec<Output> {
        if let State::Idle = self.state {
            let rec = Recognizer::new(self.min_segment);
            self.state = State::Pending {
                since: now,
                moved: (0.0, 0.0),
                rec,
            };
        }
        Vec::new()
    }

    pub fn release(&mut self) -> Vec<Output> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle => Vec::new(),
            State::Pending { .. } => vec![Output::Press, Output::Release],
            State::Holding => vec![Output::Release],
            State::Gesturing { rec, .. } => vec![Output::End(rec.stroke())],
        }
    }

    /// Drops the current press without clicking or firing, and releases the
    /// trigger if it was sent as held.
    pub fn cancel(&mut self) -> Vec<Output> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle | State::Pending { .. } => Vec::new(),
            State::Holding => vec![Output::Release],
            State::Gesturing { .. } => vec![Output::Cancel],
        }
    }

    pub fn motion(&mut self, dx: f64, dy: f64) -> Vec<Output> {
        match &mut self.state {
            State::Pending { moved, rec, .. } => {
                rec.push(dx, dy);
                moved.0 += dx;
                moved.1 += dy;
                if moved.0.hypot(moved.1) <= self.threshold {
                    return Vec::new();
                }
                let rec = rec.clone();
                let shown = rec.stroke();
                let mut out = vec![Output::Begin];
                if !shown.is_empty() {
                    out.push(Output::Update(shown.clone()));
                }
                self.state = State::Gesturing { rec, shown };
                out
            }
            State::Gesturing { rec, shown } => {
                rec.push(dx, dy);
                let stroke = rec.stroke();
                if stroke == *shown {
                    return Vec::new();
                }
                *shown = stroke.clone();
                vec![Output::Update(stroke)]
            }
            State::Idle | State::Holding => Vec::new(),
        }
    }

    pub fn tick(&mut self, now: Duration) -> Vec<Output> {
        match self.deadline() {
            Some(deadline) if now >= deadline => {
                self.state = State::Holding;
                vec![Output::Press]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Output::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn machine() -> Machine {
        Machine::new(&Settings::default())
    }

    #[test]
    fn quick_click_is_replayed() {
        let mut m = machine();
        assert_eq!(m.press(ms(0)), []);
        assert_eq!(m.motion(2.0, 1.0), []);
        assert_eq!(m.release(), [Press, Release]);
    }

    #[test]
    fn still_hold_becomes_a_normal_press() {
        let mut m = machine();
        m.press(ms(0));
        assert_eq!(m.deadline(), Some(ms(300)));
        assert_eq!(m.tick(ms(299)), []);
        assert_eq!(m.tick(ms(300)), [Press]);
        assert_eq!(m.deadline(), None);
        assert_eq!(m.motion(-50.0, 0.0), [], "a hold drags; it never turns into a gesture");
        assert_eq!(m.release(), [Release]);
    }

    #[test]
    fn moving_past_the_threshold_starts_a_gesture() {
        let mut m = machine();
        m.press(ms(0));
        assert_eq!(m.motion(-5.0, 0.0), []);
        assert_eq!(m.motion(-5.0, 0.0), [Begin]);
        assert_eq!(m.tick(ms(1000)), [], "a gesture never times out into a press");
        let updates: Vec<_> = (0..10).flat_map(|_| m.motion(-5.0, 0.0)).collect();
        assert_eq!(updates, [Update("L".parse().unwrap())]);
        assert_eq!(m.release(), [End("L".parse().unwrap())]);
    }

    #[test]
    fn tiny_gesture_ends_empty() {
        let mut m = machine();
        m.press(ms(0));
        m.motion(9.0, 0.0);
        assert_eq!(m.release(), [End(Stroke::default())]);
    }

    #[test]
    fn cancel_never_clicks_and_never_leaves_the_trigger_down() {
        let mut m = machine();
        m.press(ms(0));
        assert_eq!(m.cancel(), []);
        m.press(ms(0));
        m.tick(ms(300));
        assert_eq!(m.cancel(), [Release]);
        m.press(ms(0));
        m.motion(20.0, 0.0);
        assert_eq!(m.cancel(), [Cancel]);
        assert_eq!(m.release(), []);
    }

    #[test]
    fn stray_release_does_nothing() {
        assert_eq!(machine().release(), []);
    }
}
