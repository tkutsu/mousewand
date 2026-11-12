//! Direction strokes and the recognizer that turns pointer motion into them.
//!
//! A stroke is a sequence of eight directions. Cardinals are letters (`U D L R`),
//! diagonals are numpad digits (`7` up-left, `9` up-right, `1` down-left,
//! `3` down-right), so `"DR"` reads "down, then right" and `"3"` is one
//! down-right diagonal.
//!
//! A drawing matches the stroke it spells exactly. Failing that, it may
//! match a stroke it comes close to, by [`Drawing::deviation`]: a rounded
//! corner drawn as `U9R` still reads as `UR`.

use std::fmt;
use std::str::FromStr;

use crate::shape::Point;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl Dir {
    /// Counter-clockwise from right, in 45° steps.
    const BY_ANGLE: [Dir; 8] = [
        Dir::Right,
        Dir::UpRight,
        Dir::Up,
        Dir::UpLeft,
        Dir::Left,
        Dir::DownLeft,
        Dir::Down,
        Dir::DownRight,
    ];

    /// The direction of a motion vector in screen coordinates (y grows downward).
    pub fn of(dx: f64, dy: f64) -> Dir {
        let angle = (-dy).atan2(dx).to_degrees().rem_euclid(360.0);
        let sector = ((angle + 22.5) / 45.0) as usize % 8;
        Self::BY_ANGLE[sector]
    }

    /// Unit vector in screen coordinates.
    pub fn unit(self) -> Vec2 {
        let d = std::f64::consts::FRAC_1_SQRT_2;
        match self {
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

    pub fn symbol(self) -> char {
        match self {
            Dir::Up => 'U',
            Dir::Down => 'D',
            Dir::Left => 'L',
            Dir::Right => 'R',
            Dir::UpLeft => '7',
            Dir::UpRight => '9',
            Dir::DownLeft => '1',
            Dir::DownRight => '3',
        }
    }

    pub fn from_symbol(c: char) -> Option<Dir> {
        Some(match c.to_ascii_uppercase() {
            'U' => Dir::Up,
            'D' => Dir::Down,
            'L' => Dir::Left,
            'R' => Dir::Right,
            '7' => Dir::UpLeft,
            '9' => Dir::UpRight,
            '1' => Dir::DownLeft,
            '3' => Dir::DownRight,
            _ => return None,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Stroke(pub Vec<Dir>);

impl Stroke {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Stroke {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.iter().try_for_each(|d| write!(f, "{}", d.symbol()))
    }
}

impl FromStr for Stroke {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let dirs = s
            .chars()
            .map(|c| {
                Dir::from_symbol(c).ok_or_else(|| format!("bad stroke {s:?}: {c:?} is not one of U D L R 7 9 1 3"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if dirs.is_empty() {
            return Err("empty stroke".into());
        }
        if dirs.windows(2).any(|w| w[0] == w[1]) {
            return Err(format!("bad stroke {s:?}: repeated directions merge into one"));
        }
        Ok(Stroke(dirs))
    }
}

/// Motion is sampled into chunks this long before it gets a direction,
/// so single-count mouse jitter can't flip it.
const SAMPLE: f64 = 8.0;

/// A run shorter than this share of the drawing is noise, however long it
/// is: the same drawing reads the same at any size, and with any mouse's
/// resolution. A tenth keeps the eight runs of a circle.
const NOISE_SHARE: f64 = 0.1;

/// Turns relative motion into a stroke, and keeps the path for shapes.
///
/// Motion is cut into short samples, and consecutive samples with the same
/// direction add up into runs, each kept as a vector. A run shorter than
/// `min_segment`, or than [`NOISE_SHARE`] of all the runs, is noise: the
/// shortest one merges into whichever neighbour points the most like it,
/// and the merged vector gets its direction anew. A wavy line's wobbles
/// cancel out that way instead of splitting it up.
#[derive(Clone, Debug)]
pub struct Recognizer {
    min_segment: f64,
    pending: Vec2,
    runs: Vec<Vec2>,
    /// Where each sample ended, from the press at (0, 0).
    path: Vec<Point>,
}

pub type Vec2 = (f64, f64);

fn len(v: Vec2) -> f64 {
    v.0.hypot(v.1)
}

fn dir(v: Vec2) -> Dir {
    Dir::of(v.0, v.1)
}

fn add(a: Vec2, b: Vec2) -> Vec2 {
    (a.0 + b.0, a.1 + b.1)
}

/// Cosine of the angle between two vectors.
fn alignment(a: Vec2, b: Vec2) -> f64 {
    (a.0 * b.0 + a.1 * b.1) / (len(a) * len(b)).max(f64::EPSILON)
}

impl Recognizer {
    pub fn new(min_segment: f64) -> Self {
        Recognizer {
            min_segment,
            pending: (0.0, 0.0),
            runs: Vec::new(),
            path: vec![(0.0, 0.0)],
        }
    }

    pub fn push(&mut self, dx: f64, dy: f64) {
        self.pending = add(self.pending, (dx, dy));
        if len(self.pending) < SAMPLE {
            return;
        }
        let sample = std::mem::take(&mut self.pending);
        let last = *self.path.last().expect("starts with the press");
        self.path.push(add(last, sample));
        match self.runs.last_mut() {
            Some(last) if dir(*last) == dir(sample) => *last = add(*last, sample),
            _ => self.runs.push(sample),
        }
    }

    pub fn stroke(&self) -> Stroke {
        Stroke(self.runs().into_iter().map(dir).collect())
    }

    /// The gesture is over: what was drawn.
    pub fn finish(self) -> Drawing {
        let runs = self.runs();
        Drawing {
            stroke: Stroke(runs.iter().copied().map(dir).collect()),
            runs,
            path: self.path,
        }
    }

    /// The runs left once noise has merged away, one per direction of the stroke.
    fn runs(&self) -> Vec<Vec2> {
        let mut runs = self.runs.clone();
        let total: f64 = runs.iter().map(|&run| len(run)).sum();
        let noise = self.min_segment.max(NOISE_SHARE * total);
        while runs.len() > 1 {
            let Some(i) = (0..runs.len())
                .filter(|&i| len(runs[i]) < noise)
                .min_by(|&a, &b| len(runs[a]).total_cmp(&len(runs[b])))
            else {
                break;
            };
            let neighbour = [i.checked_sub(1), Some(i + 1).filter(|&j| j < runs.len())]
                .into_iter()
                .flatten()
                .max_by(|&a, &b| alignment(runs[i], runs[a]).total_cmp(&alignment(runs[i], runs[b])))
                .expect("more than one run");
            let short = runs.remove(i);
            let into = if neighbour > i { neighbour - 1 } else { neighbour };
            runs[into] = add(runs[into], short);
            merge_equal_neighbours(&mut runs);
        }
        runs.retain(|&run| len(run) >= noise);
        merge_equal_neighbours(&mut runs);
        runs
    }
}

/// A finished gesture's motion.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    pub stroke: Stroke,
    /// The motion behind each of the stroke's directions.
    runs: Vec<Vec2>,
    /// Where each sample ended, from the press at (0, 0), for
    /// [`crate::shape::recognize`].
    pub path: Vec<Point>,
}

impl Drawing {
    /// How far the drawing strays from `stroke`, in degrees: the
    /// length-weighted root mean square angle between each run and the
    /// direction it's given. Runs are given to the stroke's directions in
    /// order, each direction gets at least one, and the way that strays
    /// least counts. So a short diagonal at a corner costs little, while a
    /// direction that was never drawn makes it infinite.
    pub fn deviation(&self, stroke: &Stroke) -> f64 {
        let dirs = &stroke.0;
        let total: f64 = self.runs.iter().map(|&run| len(run)).sum();
        // best[j]: the least cost so far with the latest run given to dirs[j].
        let mut best = vec![f64::INFINITY; dirs.len()];
        for (i, &run) in self.runs.iter().enumerate() {
            let weight = len(run) / total;
            for j in (0..dirs.len()).rev() {
                let from = match j {
                    0 if i == 0 => 0.0,
                    0 => f64::INFINITY,
                    _ => best[j - 1],
                };
                let angle = alignment(run, dirs[j].unit()).clamp(-1.0, 1.0).acos().to_degrees();
                best[j] = best[j].min(from) + weight * angle * angle;
            }
        }
        best.last().map_or(f64::INFINITY, |cost| cost.sqrt())
    }
}

fn merge_equal_neighbours(runs: &mut Vec<Vec2>) {
    let mut merged: Vec<Vec2> = Vec::with_capacity(runs.len());
    for &run in runs.iter() {
        match merged.last_mut() {
            Some(last) if dir(*last) == dir(run) => *last = add(*last, run),
            _ => merged.push(run),
        }
    }
    *runs = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default `stroke_tolerance`.
    const TOLERANCE: f64 = 30.0;

    fn drawing(moves: &[(f64, f64, usize)]) -> Drawing {
        let mut r = Recognizer::new(30.0);
        for &(dx, dy, n) in moves {
            for _ in 0..n {
                r.push(dx, dy);
            }
        }
        r.finish()
    }

    fn draw(moves: &[(f64, f64, usize)]) -> String {
        drawing(moves).stroke.to_string()
    }

    /// How far the drawing strays from each of these strokes.
    fn deviations(moves: &[(f64, f64, usize)], strokes: &[&str]) -> Vec<f64> {
        let drawing = drawing(moves);
        strokes.iter().map(|s| drawing.deviation(&s.parse().unwrap())).collect()
    }

    #[test]
    fn directions() {
        assert_eq!(Dir::of(1.0, 0.0), Dir::Right);
        assert_eq!(Dir::of(0.0, -1.0), Dir::Up);
        assert_eq!(Dir::of(-1.0, 0.1), Dir::Left);
        assert_eq!(Dir::of(1.0, 1.0), Dir::DownRight);
        assert_eq!(Dir::of(-1.0, -1.0), Dir::UpLeft);
        assert_eq!(Dir::of(1.0, -0.3), Dir::Right);
    }

    #[test]
    fn straight_lines() {
        assert_eq!(draw(&[(-3.0, 0.0, 60)]), "L");
        assert_eq!(draw(&[(0.0, 2.0, 60)]), "D");
        assert_eq!(draw(&[(2.0, 2.0, 40)]), "3");
    }

    #[test]
    fn corners() {
        assert_eq!(draw(&[(0.0, 3.0, 30), (3.0, 0.0, 30)]), "DR");
        assert_eq!(draw(&[(0.0, -3.0, 30), (0.0, 3.0, 30), (0.0, -3.0, 30)]), "UDU");
    }

    #[test]
    fn w_is_four_diagonals() {
        let w = [(3.0, 3.0, 15), (3.0, -3.0, 15), (3.0, 3.0, 15), (3.0, -3.0, 15)];
        assert_eq!(draw(&w), "3939");
    }

    #[test]
    fn too_short_is_empty() {
        assert_eq!(draw(&[(1.0, 0.0, 20)]), "");
    }

    #[test]
    fn wobble_does_not_split_a_line() {
        // A leftward line that drifts up and down across the L/7 and L/1 borders.
        let mut moves = Vec::new();
        for i in 0..30 {
            let dy = if i % 2 == 0 { -4.0 } else { 4.0 };
            moves.push((-9.0, dy, 1));
        }
        assert_eq!(draw(&moves), "L");
    }

    #[test]
    fn short_hook_at_the_end_is_ignored() {
        assert_eq!(draw(&[(-3.0, 0.0, 60), (0.0, 3.0, 5)]), "L");
    }

    #[test]
    fn reads_the_same_at_any_size() {
        // An L with a slightly rounded corner, as drawn small, and big
        // (or with a high-resolution mouse), where the corner alone is
        // longer than `min_segment`.
        for size in [1, 4, 10] {
            let corner = [(0.0, -3.0, 40 * size), (2.0, -2.0, 5 * size), (3.0, 0.0, 40 * size)];
            assert_eq!(draw(&corner), "UR", "at {size}×");
        }
    }

    #[test]
    fn a_wiggle_before_a_long_line_is_absorbed() {
        assert_eq!(draw(&[(0.0, 3.0, 13), (3.0, 0.0, 200)]), "R");
        // A short leg still counts.
        assert_eq!(draw(&[(0.0, 3.0, 15), (3.0, 0.0, 80)]), "DR");
    }

    #[test]
    fn a_circle_of_motion_is_a_circle() {
        use crate::shape::{Shape, recognize};
        let mut r = Recognizer::new(30.0);
        for i in 0..120 {
            let (sin, cos) = (i as f64 / 120.0 * std::f64::consts::TAU).sin_cos();
            r.push(-5.0 * sin, 4.0 * cos);
        }
        assert!(r.stroke().0.len() >= 6, "{}", r.stroke());
        assert!(recognize(&[Shape::built_in("circle").unwrap()], &r.finish().path).is_some());
    }

    #[test]
    fn an_exact_drawing_does_not_stray() {
        let up_right = [(0.0, -3.0, 40), (3.0, 0.0, 40)];
        assert_eq!(draw(&up_right), "UR");
        let d = deviations(&up_right, &["UR"])[0];
        assert!(d < 2.0, "{d}");
    }

    #[test]
    fn a_rounded_corner_is_close_to_the_corner() {
        let rounded = [(0.0, -3.0, 30), (2.0, -2.0, 15), (3.0, 0.0, 30)];
        assert_eq!(draw(&rounded), "U9R");
        let d = deviations(&rounded, &["UR", "9", "U", "R"]);
        assert!(d[0] < TOLERANCE, "{d:?}");
        assert!(d[1..].iter().all(|&far| far > TOLERANCE), "{d:?}");
    }

    #[test]
    fn a_leaning_line_is_close_to_its_direction() {
        // Up, leaning 28° to the right: a diagonal, but closer to up.
        let (sin, cos) = 28f64.to_radians().sin_cos();
        let leaning = [(3.0 * sin, -3.0 * cos, 40), (3.0, 0.0, 40)];
        assert_eq!(draw(&leaning), "9R");
        let d = deviations(&leaning, &["UR", "R"]);
        assert!(d[0] < TOLERANCE && d[1] > TOLERANCE, "{d:?}");
    }

    #[test]
    fn every_direction_must_be_drawn_and_none_added() {
        let up = [(0.0, -3.0, 40)];
        assert_eq!(deviations(&up, &["UR"]), [f64::INFINITY]);
        let up_left = [(0.0, -3.0, 40), (-3.0, 0.0, 40)];
        let d = deviations(&up_left, &["U", "UR", "U7"]);
        assert!(d.iter().all(|&far| far > TOLERANCE), "{d:?}");
        assert_eq!(Drawing::default().deviation(&"U".parse().unwrap()), f64::INFINITY);
    }

    #[test]
    fn parse_and_print() {
        let s: Stroke = "ud3".parse().unwrap();
        assert_eq!(s.to_string(), "UD3");
        assert!("UUD".parse::<Stroke>().is_err());
        assert!("UX".parse::<Stroke>().is_err());
        assert!("".parse::<Stroke>().is_err());
    }
}
