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
    /// The trigger went down. What it becomes isn't known yet, but the
    /// window under the cursor can be looked up, and its app can pass the
    /// press through: see [`Machine::pass`].
    Down,
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
    /// The pointer has held still mid-gesture for `cancel_ms`: sent once
    /// per gesture, which may cancel it.
    Still,
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
        still: Still,
    },
}

/// How long the pointer has been still during a gesture.
#[derive(Debug)]
struct Still {
    since: Duration,
    /// Motion since `since`: a sensor's jitter doesn't count as moving.
    drift: (f64, f64),
    /// Whether `Output::Still` went out.
    sent: bool,
}

/// Motion under this, in device units, still counts as holding still.
const JITTER: f64 = 4.0;

#[derive(Debug)]
pub struct Machine {
    threshold: f64,
    hold: Duration,
    /// How long holding still mid-gesture takes to send `Still`, if ever.
    cancel: Option<Duration>,
    min_segment: f64,
    state: State,
}

impl Machine {
    pub fn new(settings: &Settings) -> Self {
        Machine {
            threshold: settings.threshold,
            hold: Duration::from_millis(settings.hold_ms),
            cancel: (settings.cancel_ms > 0).then(|| Duration::from_millis(settings.cancel_ms)),
            min_segment: settings.min_segment,
            state: State::Idle,
        }
    }

    /// When `tick` must be called next, if a press is pending or a gesture
    /// could be held still.
    pub fn deadline(&self) -> Option<Duration> {
        match &self.state {
            State::Pending { since, .. } => Some(*since + self.hold),
            State::Gesturing { still, .. } if !still.sent => self.cancel.map(|c| still.since + c),
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
            return vec![Output::Down];
        }
        Vec::new()
    }

    /// Gives this press to the app: it becomes an ordinary held button,
    /// pressed late, until release. For apps with gestures turned off.
    pub fn pass(&mut self) -> Vec<Output> {
        match self.state {
            State::Pending { .. } | State::Gesturing { .. } => {
                self.state = State::Holding;
                vec![Output::Press]
            }
            State::Idle | State::Holding => Vec::new(),
        }
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

    pub fn motion(&mut self, dx: f64, dy: f64, now: Duration) -> Vec<Output> {
        match &mut self.state {
            State::Pending { moved, rec, .. } => {
                rec.push(dx, dy);
                moved.0 += dx;
                moved.1 += dy;
                if moved.0.hypot(moved.1) <= self.threshold {
                    return Vec::new();
                }
                self.begin(now)
            }
            State::Gesturing { rec, shown, still, .. } => {
                still.drift.0 += dx;
                still.drift.1 += dy;
                if still.drift.0.hypot(still.drift.1) > JITTER {
                    still.since = now;
                    still.drift = (0.0, 0.0);
                }
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
    pub fn claim(&mut self, extra: Extra, now: Duration) -> Option<Vec<Output>> {
        let mut out = match self.state {
            State::Pending { .. } => self.begin(now),
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
    fn begin(&mut self, now: Duration) -> Vec<Output> {
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
            still: Still {
                since: now,
                drift: (0.0, 0.0),
                sent: false,
            },
        };
        out
    }

    pub fn tick(&mut self, now: Duration) -> Vec<Output> {
        if !self.deadline().is_some_and(|deadline| now >= deadline) {
            return Vec::new();
        }
        match &mut self.state {
            State::Pending { .. } => {
                self.state = State::Holding;
                vec![Output::Press]
            }
            State::Gesturing { still, .. } => {
                still.sent = true;
                vec![Output::Still]
            }
            State::Idle | State::Holding => Vec::new(),
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
        assert_eq!(m.press(ms(0)), [Down]);
        assert_eq!(m.motion(2.0, 1.0, ms(0)), []);
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
        assert_eq!(m.motion(-50.0, 0.0, ms(0)), [], "a hold drags; it never turns into a gesture");
        assert_eq!(m.release(), [Release]);
    }

    #[test]
    fn moving_past_the_threshold_starts_a_gesture() {
        let mut m = machine();
        m.press(ms(0));
        assert_eq!(m.motion(-5.0, 0.0, ms(0)), []);
        assert_eq!(m.motion(-5.0, 0.0, ms(0)), [Begin]);
        assert_eq!(m.tick(ms(300)), [], "a gesture never times out into a press");
        let updates: Vec<_> = (0..10)
            .flat_map(|_| m.motion(-5.0, 0.0, ms(0)))
            .filter(|o| !matches!(o, Motion(..)))
            .collect();
        assert_eq!(updates, [Update("L".parse().unwrap())]);
        assert_eq!(m.motion(-1.0, 2.0, ms(0)), [Motion(-1.0, 2.0)]);
        assert_eq!(ended(m.release()), Some("L".parse().unwrap()));
    }

    #[test]
    fn holding_still_mid_gesture_says_so_once() {
        let mut m = machine();
        m.press(ms(0));
        m.motion(-40.0, 0.0, ms(10));
        assert_eq!(m.deadline(), Some(ms(710)));
        // Moving on resets the clock; a jitter doesn't.
        m.motion(-40.0, 0.0, ms(500));
        m.motion(1.0, 1.0, ms(900));
        assert_eq!(m.tick(ms(1199)), []);
        assert_eq!(m.tick(ms(1200)), [Still]);
        assert_eq!(m.deadline(), None, "only once");
        assert_eq!(ended(m.release()), Some("L".parse().unwrap()), "release still ends it");

        let mut never = Machine::new(&Settings {
            cancel_ms: 0,
            ..Settings::default()
        });
        never.press(ms(0));
        never.motion(-40.0, 0.0, ms(0));
        assert_eq!(never.deadline(), None);
    }

    #[test]
    fn a_press_can_be_passed_to_the_app() {
        let mut m = machine();
        assert_eq!(m.press(ms(0)), [Down]);
        assert_eq!(m.pass(), [Press]);
        assert_eq!(m.motion(-50.0, 0.0, ms(5)), [], "a drag, not a gesture");
        assert_eq!(m.release(), [Release]);

        m.press(ms(0));
        m.motion(-50.0, 0.0, ms(5));
        assert_eq!(m.pass(), [Press], "even once it looked like a gesture");
        assert_eq!(m.release(), [Release]);
        assert_eq!(m.pass(), [], "too late: it's over");
    }

    #[test]
    fn click_while_moving_is_still_a_click() {
        let mut m = machine();
        m.press(ms(0));
        assert_eq!(m.motion(9.0, 0.0, ms(0)), [Begin], "the trail shows");
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
        m.motion(20.0, 0.0, ms(0));
        assert_eq!(m.cancel(), [Cancel]);
        assert_eq!(m.release(), []);
    }

    #[test]
    fn wheel_and_buttons_belong_to_the_gesture() {
        let mut m = machine();
        assert_eq!(m.claim(Extra::Wheel(crate::Wheel::Up), ms(0)), None, "idle: scroll normally");

        m.press(ms(0));
        assert_eq!(
            m.claim(Extra::Wheel(crate::Wheel::Up), ms(0)),
            Some(vec![Begin, Wheel(crate::Wheel::Up)]),
            "a pending press becomes a gesture"
        );
        assert_eq!(m.claim(Extra::Button(0x110), ms(0)), Some(vec![Click(0x110)]));
        assert_eq!(ended(m.release()), Some(Stroke::default()), "no click replay");

        m.press(ms(0));
        m.tick(ms(300));
        assert_eq!(
            m.claim(Extra::Wheel(crate::Wheel::Down), ms(0)),
            None,
            "a plain hold scrolls normally"
        );
    }

    #[test]
    fn stray_release_does_nothing() {
        assert_eq!(machine().release(), []);
    }
}
