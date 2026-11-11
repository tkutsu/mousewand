//! What the trigger button does: a click, a normal hold, or a gesture.
//!
//! The input thread feeds trigger presses, releases and motion in, with the
//! current time, and acts on the outputs. Motion is always forwarded to the
//! cursor; the trigger button itself is held back until it's clear the
//! press isn't a gesture. While the trigger is down for a gesture, wheel
//! notches and other buttons belong to mousewand too: see [`Machine::claim`].

use std::time::Duration;

use crate::config::{Settings, Wheel};
use crate::shape::Point;
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
    /// The trigger was released and the gesture is over: its stroke, and
    /// its path for shapes.
    End(Stroke, Vec<Point>),
    /// The gesture was abandoned (e.g. its mouse was unplugged): fire nothing.
    Cancel,
    /// Pointer motion during a gesture, for repeat steps.
    Motion(f64, f64),
    /// A wheel notch during a gesture.
    Wheel(Wheel),
    /// Another button clicked during a gesture.
    Click(u16),
}

/// Input that can belong to a gesture instead of the app.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Extra {
    Wheel(Wheel),
    /// A button press other than the trigger.
    Button(u16),
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
        /// Whether a wheel notch or another button went to the gesture.
        claimed: bool,
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
            // Moved, but drew nothing and claimed nothing: a click made
            // while the mouse was moving, which shouldn't be lost.
            State::Gesturing {
                rec, claimed: false, ..
            } if rec.stroke().is_empty() => {
                vec![Output::Cancel, Output::Press, Output::Release]
            }
            State::Gesturing { rec, .. } => vec![Output::End(rec.stroke(), rec.path().to_vec())],
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
                self.begin()
            }
            State::Gesturing { rec, shown, .. } => {
                rec.push(dx, dy);
                let mut out = vec![Output::Motion(dx, dy)];
                let stroke = rec.stroke();
                if stroke != *shown {
                    *shown = stroke.clone();
                    out.push(Output::Update(stroke));
                }
                out
            }
            State::Idle | State::Holding => Vec::new(),
        }
    }

    /// Whether wheel notches and other buttons belong to mousewand right now.
    pub fn claims(&self) -> bool {
        matches!(self.state, State::Pending { .. } | State::Gesturing { .. })
    }

    /// Claims a wheel notch or button press for the gesture. `None` means
    /// it isn't mousewand's: forward it. A claimed press also turns a pending
    /// press into a gesture, so the trigger click is never replayed.
    pub fn claim(&mut self, extra: Extra) -> Option<Vec<Output>> {
        let mut out = match self.state {
            State::Pending { .. } => self.begin(),
            State::Gesturing { .. } => Vec::new(),
            State::Idle | State::Holding => return None,
        };
        if let State::Gesturing { claimed, .. } = &mut self.state {
            *claimed = true;
        }
        out.push(match extra {
            Extra::Wheel(wheel) => Output::Wheel(wheel),
            Extra::Button(code) => Output::Click(code),
        });
        Some(out)
    }

    /// Pending → Gesturing.
    fn begin(&mut self) -> Vec<Output> {
        let State::Pending { rec, .. } = std::mem::replace(&mut self.state, State::Idle) else {
            unreachable!("begin only runs on a pending press");
        };
        let shown = rec.stroke();
        let mut out = vec![Output::Begin];
        if !shown.is_empty() {
            out.push(Output::Update(shown.clone()));
        }
        self.state = State::Gesturing {
            rec,
            shown,
            claimed: false,
        };
        out
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

    /// The stroke, if the outputs are just the end of a gesture.
    fn ended(out: Vec<Output>) -> Option<Stroke> {
        match out.as_slice() {
            [End(stroke, path)] => {
                assert!(!path.is_empty());
                Some(stroke.clone())
            }
            _ => None,
        }
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
        let updates: Vec<_> = (0..10)
            .flat_map(|_| m.motion(-5.0, 0.0))
            .filter(|o| !matches!(o, Motion(..)))
            .collect();
        assert_eq!(updates, [Update("L".parse().unwrap())]);
        assert_eq!(m.motion(-1.0, 2.0), [Motion(-1.0, 2.0)]);
        assert_eq!(ended(m.release()), Some("L".parse().unwrap()));
    }

    #[test]
    fn click_while_moving_is_still_a_click() {
        let mut m = machine();
        m.press(ms(0));
        assert_eq!(m.motion(9.0, 0.0), [Begin], "the trail shows");
        assert_eq!(m.release(), [Cancel, Press, Release], "but nothing was drawn");
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
    fn wheel_and_buttons_belong_to_the_gesture() {
        let mut m = machine();
        assert_eq!(m.claim(Extra::Wheel(crate::Wheel::Up)), None, "idle: scroll normally");

        m.press(ms(0));
        assert_eq!(
            m.claim(Extra::Wheel(crate::Wheel::Up)),
            Some(vec![Begin, Wheel(crate::Wheel::Up)]),
            "a pending press becomes a gesture"
        );
        assert_eq!(m.claim(Extra::Button(0x110)), Some(vec![Click(0x110)]));
        assert_eq!(ended(m.release()), Some(Stroke::default()), "no click replay");

        m.press(ms(0));
        m.tick(ms(300));
        assert_eq!(
            m.claim(Extra::Wheel(crate::Wheel::Down)),
            None,
            "a plain hold scrolls normally"
        );
    }

    #[test]
    fn stray_release_does_nothing() {
        assert_eq!(machine().release(), []);
    }
}
