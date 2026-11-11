//! What the hint shows: the stroke so far, the gesture it matches, what it
//! could still become, and what fired.

use crate::config::{Gesture, Input};
use crate::stroke::Stroke;

/// How many possible next moves the hint lists at most.
pub const MAX_NEXT: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub stroke: Stroke,
    /// The gesture the stroke matches, or the one that fired.
    pub label: Option<String>,
    pub state: State,
    /// Gestures the stroke could still become: the rest of each stroke and
    /// its label, shortest first.
    pub next: Vec<(Stroke, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Drawing,
    /// Something fired; `label` says what.
    Fired,
    /// The gesture ended without firing anything.
    Missed,
}

impl Hint {
    /// Mid-stroke, among `gestures`. `next` lists what the stroke could
    /// still become.
    pub fn drawing(gestures: &[Gesture], stroke: &Stroke, next: bool) -> Hint {
        let strokes = || {
            gestures.iter().filter_map(|g| match &g.input {
                Input::Stroke(s) => Some((s, g)),
                _ => None,
            })
        };
        let label = strokes().find(|(s, _)| *s == stroke).map(|(_, g)| g.label());
        let mut next: Vec<(Stroke, String)> = match next {
            true => strokes()
                .filter(|(s, _)| s.0.len() > stroke.0.len() && s.0.starts_with(&stroke.0))
                .map(|(s, g)| (Stroke(s.0[stroke.0.len()..].to_vec()), g.label()))
                .collect(),
            false => Vec::new(),
        };
        next.sort_by_key(|(rest, _)| rest.0.len());
        next.truncate(MAX_NEXT);
        Hint {
            stroke: stroke.clone(),
            label,
            state: State::Drawing,
            next,
        }
    }

    pub fn fired(stroke: &Stroke, label: String) -> Hint {
        Hint {
            stroke: stroke.clone(),
            label: Some(label),
            state: State::Fired,
            next: Vec::new(),
        }
    }

    pub fn missed(stroke: &Stroke) -> Hint {
        Hint {
            stroke: stroke.clone(),
            label: None,
            state: State::Missed,
            next: Vec::new(),
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
            [(s("R"), "Close tab".to_string()), (s("RU"), "ctrl+shift+t".to_string())]
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
    fn empty_stroke_lists_every_stroke() {
        let hint = Hint::drawing(&gestures(), &Stroke::default(), true);
        assert_eq!(hint.next.len(), 3);
        assert_eq!(hint.next[0].0, s("D"));
    }
}
