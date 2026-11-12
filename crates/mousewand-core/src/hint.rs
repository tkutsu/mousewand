//! What the hint shows: the stroke so far, the gesture it matches, what it
//! could still become, and what fired, how many times.

use crate::config::{Gesture, Input};
use crate::stroke::Stroke;

/// How many possible next moves the hint lists at most.
pub const MAX_NEXT: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub stroke: Stroke,
    /// The gesture the stroke matches, or the one that fired.
    pub label: Option<String>,
    /// What fired when it isn't a stroke, shown in place of the stroke's
    /// arrows: a shape, a wheel notch or a click.
    pub input: Option<Input>,
    /// How many times in a row it fired: more than once is a repeat.
    pub count: u32,
    pub state: State,
    /// Gestures the stroke could still become, shortest first: the rest of
    /// each stroke and its label. Before any stroke, wheel and click
    /// gestures too.
    pub next: Vec<(Input, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Drawing,
    /// Something fired; `label` says what.
    Fired,
    /// Nothing fired, or mid-stroke, no stroke gesture is or starts with
    /// the stroke so far. `label` is set only when the gesture was cancelled.
    Missed,
}

impl Hint {
    /// Mid-stroke, among `gestures`. `next` lists what the stroke could
    /// still become. A stroke that can't become any stroke gesture is a
    /// miss right away; a shape may still match it on release.
    pub fn drawing(gestures: &[Gesture], stroke: &Stroke, next: bool) -> Hint {
        let strokes = || {
            gestures.iter().filter_map(|g| match &g.input {
                Input::Stroke(s) => Some((s, g)),
                _ => None,
            })
        };
        if !stroke.0.is_empty() && !strokes().any(|(s, _)| s.0.starts_with(&stroke.0)) {
            return Hint::missed(stroke);
        }
        let label = strokes().find(|(s, _)| *s == stroke).map(|(_, g)| g.label());
        let rests = strokes()
            .filter(|(s, _)| s.0.len() > stroke.0.len() && s.0.starts_with(&stroke.0))
            .map(|(s, g)| (Input::Stroke(Stroke(s.0[stroke.0.len()..].to_vec())), g.label()));
        let mouse = gestures
            .iter()
            .filter(|g| stroke.0.is_empty() && matches!(g.input, Input::Wheel(_) | Input::Click(_)))
            .map(|g| (g.input.clone(), g.label()));
        let mut next: Vec<(Input, String)> = match next {
            true => rests.chain(mouse).collect(),
            false => Vec::new(),
        };
        // A notch or click is one move, like one arrow.
        next.sort_by_key(|(input, _)| match input {
            Input::Stroke(rest) => rest.0.len(),
            _ => 1,
        });
        next.truncate(MAX_NEXT);
        Hint {
            stroke: stroke.clone(),
            label,
            input: None,
            count: 0,
            state: State::Drawing,
            next,
        }
    }

    /// `gesture` fired, `count` times in a row.
    pub fn fired(stroke: &Stroke, gesture: &Gesture, count: u32) -> Hint {
        Hint {
            stroke: stroke.clone(),
            label: Some(gesture.label()),
            input: match &gesture.input {
                Input::Stroke(_) => None,
                input => Some(input.clone()),
            },
            count,
            state: State::Fired,
            next: Vec::new(),
        }
    }

    pub fn missed(stroke: &Stroke) -> Hint {
        Hint {
            stroke: stroke.clone(),
            label: None,
            input: None,
            count: 0,
            state: State::Missed,
            next: Vec::new(),
        }
    }

    /// Given up on by holding still.
    pub fn cancelled(stroke: &Stroke) -> Hint {
        Hint {
            label: Some("Cancelled".into()),
            ..Hint::missed(stroke)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn gestures() -> Vec<Gesture> {
        Config::parse(
            r#"
            [[gesture]]
            stroke = "D"
            name = "Reload"
            action = { keys = "F5" }

            [[gesture]]
            stroke = "DR"
            name = "Close tab"
            action = { keys = "ctrl+w" }

            [[gesture]]
            stroke = "DRU"
            action = { keys = "ctrl+shift+t" }

            [[gesture]]
            wheel = "up"
            action = { keys = "ctrl+Tab" }
            "#,
            &[],
            &[],
        )
        .unwrap()
        .gestures
    }

    fn s(stroke: &str) -> Stroke {
        stroke.parse().unwrap()
    }

    #[test]
    fn matches_and_next_moves() {
        let hint = Hint::drawing(&gestures(), &s("D"), true);
        assert_eq!(hint.label.as_deref(), Some("Reload"));
        assert_eq!(
            hint.next,
            [
                (Input::Stroke(s("R")), "Close tab".to_string()),
                (Input::Stroke(s("RU")), "ctrl+shift+t".to_string())
            ]
        );
        let hint = Hint::drawing(&gestures(), &s("DRU"), true);
        assert_eq!(hint.label.as_deref(), Some("ctrl+shift+t"));
        assert!(hint.next.is_empty());
    }

    #[test]
    fn nothing_matches_and_next_can_be_off() {
        let hint = Hint::drawing(&gestures(), &s("L"), true);
        assert_eq!(hint.label, None);
        assert!(hint.next.is_empty());
        assert!(Hint::drawing(&gestures(), &s("D"), false).next.is_empty());
    }

    #[test]
    fn a_dead_end_is_a_miss_right_away() {
        assert_eq!(Hint::drawing(&gestures(), &s("L"), true).state, State::Missed);
        assert_eq!(Hint::drawing(&gestures(), &s("DL"), false).state, State::Missed);
        // Still on the way to DRU.
        assert_eq!(Hint::drawing(&gestures(), &s("DR"), false).state, State::Drawing);
        assert_eq!(
            Hint::drawing(&gestures(), &Stroke::default(), true).state,
            State::Drawing
        );
    }

    #[test]
    fn a_fired_shape_shows_instead_of_its_stroke() {
        let config = Config::parse(
            "[[gesture]]\nshape = \"circle\"\nname = \"Menu\"\naction = { keys = \"F10\" }",
            &[],
            &[],
        )
        .unwrap();
        let hint = Hint::fired(&s("DLUR"), &config.gestures[0], 1);
        assert_eq!(hint.label.as_deref(), Some("Menu"));
        assert!(matches!(
            hint.input,
            Some(Input::Shape(s)) if s.name == "circle" && !s.outline().is_empty()
        ));
        assert_eq!(Hint::fired(&s("D"), &gestures()[0], 1).input, None);
    }

    #[test]
    fn a_wheel_shows_and_counts() {
        let hint = Hint::fired(&s("D"), &gestures()[3], 3);
        assert_eq!(hint.input, Some(Input::Wheel(crate::Wheel::Up)));
        assert_eq!(hint.count, 3);
    }

    #[test]
    fn empty_stroke_lists_every_stroke_and_the_wheel() {
        let hint = Hint::drawing(&gestures(), &Stroke::default(), true);
        let next: Vec<&Input> = hint.next.iter().map(|(input, _)| input).collect();
        assert_eq!(
            next,
            [
                &Input::Stroke(s("D")),
                &Input::Wheel(crate::Wheel::Up),
                &Input::Stroke(s("DR")),
                &Input::Stroke(s("DRU"))
            ]
        );
        // Mid-stroke, only strokes.
        let hint = Hint::drawing(&gestures(), &s("D"), true);
        assert!(hint.next.iter().all(|(input, _)| matches!(input, Input::Stroke(_))));
    }
}
