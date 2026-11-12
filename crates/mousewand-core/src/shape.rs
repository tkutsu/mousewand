//! Shapes: gestures matched by how the drawing looks, not by its directions.
//!
//! A drawing is compared to each shape's examples with $P (Vatavu, Anthony
//! and Wobbrock, 2012): both become clouds of evenly spaced points, and the
//! distance is how far the drawing's points must move to land on the
//! example's. A cloud forgets where the drawing started and which way it
//! went, so a circle matches whichever way round it's drawn.
//!
//! Before comparing, each cloud is centred and stretched until it spreads
//! equally in every direction, so a squashed circle at any tilt becomes a
//! round one. The stretch stops at [`MAX_STRETCH`]: a straight line stays a
//! line instead of its wobbles being blown up into a blob.

use std::f64::consts::TAU;
use std::sync::Arc;

pub type Point = (f64, f64);

/// Points per cloud.
const N: usize = 32;
/// How much a cloud's narrow side may be stretched to match its wide side.
const MAX_STRETCH: f64 = 3.0;
/// The weighted mean distance per point, in units of the clouds' spread,
/// under which a drawing matches a shape. Against the circle, circles of
/// every kind in the tests score up to 0.21 and other drawings from 0.29,
/// except a closed square, which is round enough at 0.18.
const THRESHOLD: f64 = 0.25;

/// A named shape and the examples it was taught with.
#[derive(Clone, Debug)]
pub struct Shape {
    pub name: String,
    examples: Arc<[Cloud]>,
}

/// Shapes are told apart by name.
impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Shape {}

/// The shapes that need no examples. A file in `shapes/` replaces them.
pub const BUILT_IN: [&str; 8] = [
    "circle", "triangle", "square", "star", "heart", "check", "spiral", "infinity",
];

impl Shape {
    pub fn built_in(name: &str) -> Option<Shape> {
        // Screen coordinates: y grows downwards.
        let curve = |n: usize, f: &dyn Fn(f64) -> Point| (0..=n).map(|i| f(i as f64 / n as f64)).collect();
        let points: Vec<Point> = match name {
            "circle" => curve(64, &|t| ((t * TAU).cos(), (t * TAU).sin())),
            "triangle" => vec![(0.0, -1.0), (0.87, 0.5), (-0.87, 0.5), (0.0, -1.0)],
            "square" => vec![(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)],
            // In one line, the way it's drawn by hand: each point to the
            // one after next.
            "star" => (0..=5)
                .map(|i| (i * 2 % 5) as f64 / 5.0 * TAU)
                .map(|a| (a.sin(), -a.cos()))
                .collect(),
            "heart" => curve(64, &|t| {
                let t = t * TAU;
                let x = 16.0 * t.sin().powi(3);
                let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                (x / 16.0, -y / 16.0)
            }),
            "check" => vec![(-1.0, 0.0), (-0.4, 0.6), (1.0, -1.0)],
            // A turn and a half, from the middle out.
            "spiral" => curve(64, &|t| {
                let a = t * 1.75 * TAU;
                (t * a.cos(), t * a.sin())
            }),
            // A figure of eight on its side.
            "infinity" => curve(64, &|t| {
                let a = t * TAU;
                (a.sin(), (2.0 * a).sin() / 2.0)
            }),
            _ => return None,
        };
        Shape::new(name, &[points]).ok()
    }

    /// A shape taught by drawing it, from a `shapes/<name>.toml` file:
    /// `examples = [[[x, y], …], …]`.
    pub fn parse(name: &str, text: &str) -> Result<Shape, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            examples: Vec<Vec<[f64; 2]>>,
        }
        let file: File =
            toml::from_str(text).map_err(|e| crate::config::toml_error(&format!("shapes/{name}.toml"), text, &e))?;
        let examples: Vec<Vec<Point>> = file
            .examples
            .into_iter()
            .map(|e| e.into_iter().map(|[x, y]| (x, y)).collect())
            .collect();
        Shape::new(name, &examples).map_err(|e| format!("shapes/{name}.toml: {e}"))
    }

    /// Just the name, as a gesture gives it, until the config looks it up.
    /// It matches nothing.
    pub(crate) fn unresolved(name: &str) -> Shape {
        Shape {
            name: name.into(),
            examples: Arc::new([]),
        }
    }

    fn new(name: &str, examples: &[Vec<Point>]) -> Result<Shape, String> {
        if examples.is_empty() {
            return Err("a shape needs at least one example".into());
        }
        let examples = examples
            .iter()
            .map(|e| Cloud::of(e).ok_or("an example is too small to be a shape"))
            .collect::<Result<_, _>>()?;
        Ok(Shape {
            name: name.into(),
            examples,
        })
    }

    /// The first example as it's compared: points in drawing order,
    /// centred, evened out and about 1 across. Enough to draw a thumbnail.
    pub fn outline(&self) -> &[Point] {
        self.examples.first().map_or(&[], |e| &e.0)
    }

    /// How far `drawing` is from the closest example.
    fn distance(&self, drawing: &Cloud) -> f64 {
        self.examples
            .iter()
            .map(|e| drawing.distance(e))
            .fold(f64::INFINITY, f64::min)
    }
}

/// The shape `path` looks most like, if it looks enough like one.
pub fn recognize<'a>(shapes: impl IntoIterator<Item = &'a Shape>, path: &[Point]) -> Option<&'a Shape> {
    let drawing = Cloud::of(path)?;
    shapes
        .into_iter()
        .map(|s| (s, s.distance(&drawing)))
        .filter(|&(_, d)| d < THRESHOLD)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(s, _)| s)
}

/// Whether `drawing` looks like one of `examples`, as a gesture would have
/// to: a way to turn away a drawing that doesn't look like the others while
/// teaching a shape.
pub fn alike(examples: &[Vec<Point>], drawing: &[Point]) -> bool {
    Shape::new("", examples).is_ok_and(|shape| recognize([&shape], drawing).is_some())
}

/// `N` points, evenly spaced along the drawing, centred and evened out.
#[derive(Clone, Debug)]
struct Cloud(Vec<Point>);

impl Cloud {
    /// `None` for a path that doesn't go anywhere.
    fn of(path: &[Point]) -> Option<Cloud> {
        even(&resample(path)?).map(Cloud)
    }

    /// $P's greedy matching, tried from several starting points both ways.
    fn distance(&self, other: &Cloud) -> f64 {
        let step = (N as f64).sqrt() as usize;
        (0..N)
            .step_by(step)
            .flat_map(|start| {
                [
                    match_from(&self.0, &other.0, start),
                    match_from(&other.0, &self.0, start),
                ]
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// Each point of `a` in turn, from `start`, takes its nearest unclaimed
/// point of `b`. Earlier points weigh more, since their choice was freer.
fn match_from(a: &[Point], b: &[Point], start: usize) -> f64 {
    let mut taken = [false; N];
    let mut sum = 0.0;
    for k in 0..N {
        let p = a[(start + k) % N];
        let (j, d) = (0..N)
            .filter(|&j| !taken[j])
            .map(|j| (j, dist(p, b[j])))
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .expect("as many points as there are unclaimed ones");
        taken[j] = true;
        sum += (1.0 - k as f64 / N as f64) * d;
    }
    // The weights add up to (N + 1) / 2: a mean, per point.
    sum / ((N + 1) as f64 / 2.0)
}

fn dist(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// `N` points evenly spaced along `path`.
fn resample(path: &[Point]) -> Option<Vec<Point>> {
    let length: f64 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
    if length <= f64::EPSILON {
        return None;
    }
    let interval = length / (N - 1) as f64;
    let mut out = vec![path[0]];
    let mut covered = 0.0;
    let mut prev = path[0];
    for &next in &path[1..] {
        let mut d = dist(prev, next);
        while covered + d >= interval && out.len() < N {
            let t = (interval - covered) / d;
            let p = (prev.0 + t * (next.0 - prev.0), prev.1 + t * (next.1 - prev.1));
            out.push(p);
            prev = p;
            d = dist(prev, next);
            covered = 0.0;
        }
        covered += d;
        prev = next;
    }
    // Rounding can leave the last point out.
    while out.len() < N {
        out.push(*path.last().expect("not empty"));
    }
    Some(out)
}

/// Centres the points and stretches them along their narrow axis, at
/// whatever tilt, until they spread the same in every direction, then
/// scales them to a spread of 1. The narrow axis stretches at most
/// [`MAX_STRETCH`] times more than the wide one.
fn even(points: &[Point]) -> Option<Vec<Point>> {
    let n = points.len() as f64;
    let (cx, cy) = points.iter().fold((0.0, 0.0), |(x, y), p| (x + p.0 / n, y + p.1 / n));
    let centred: Vec<Point> = points.iter().map(|p| (p.0 - cx, p.1 - cy)).collect();
    let (a, b, d) = centred.iter().fold((0.0, 0.0, 0.0), |(a, b, d), p| {
        (a + p.0 * p.0 / n, b + p.0 * p.1 / n, d + p.1 * p.1 / n)
    });
    // The covariance [[a, b], [b, d]]: its eigenvalues are the spread along
    // the wide and narrow axes, and `angle` is the wide axis's direction.
    let mid = (a + d) / 2.0;
    let half = ((a - d) / 2.0).hypot(b);
    let wide = mid + half;
    if wide <= f64::EPSILON {
        return None;
    }
    let narrow = (mid - half).max(wide / (MAX_STRETCH * MAX_STRETCH));
    let angle = 0.5 * (2.0 * b).atan2(a - d);
    let (sin, cos) = angle.sin_cos();
    let (sx, sy) = (1.0 / wide.sqrt(), 1.0 / narrow.sqrt());
    Some(
        centred
            .iter()
            .map(|&(x, y)| {
                // Into the axes' frame, scaled, and back.
                let (u, v) = (x * cos + y * sin, -x * sin + y * cos);
                let (u, v) = (u * sx, v * sy);
                (u * cos - v * sin, u * sin + v * cos)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ellipse with radii `rx` and `ry`, tilted by `tilt`, from angle
    /// `from` through `turn` radians, with a wobble of `wobble` × radius.
    fn ellipse(rx: f64, ry: f64, tilt: f64, from: f64, turn: f64, wobble: f64) -> Vec<Point> {
        let (s, c) = tilt.sin_cos();
        (0..=80)
            .map(|i| {
                let t = from + turn * i as f64 / 80.0;
                let r = 1.0 + wobble * (7.0 * t).sin();
                let (x, y) = (rx * r * t.cos(), ry * r * t.sin());
                (x * c - y * s + 300.0, x * s + y * c + 200.0)
            })
            .collect()
    }

    /// Straight segments through `corners`, sampled every 8 units like the
    /// recognizer does.
    fn polyline(corners: &[Point]) -> Vec<Point> {
        let mut out = vec![corners[0]];
        for w in corners.windows(2) {
            let steps = (dist(w[0], w[1]) / 8.0).ceil() as usize;
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                out.push((w[0].0 + t * (w[1].0 - w[0].0), w[0].1 + t * (w[1].1 - w[0].1)));
            }
        }
        out
    }

    fn circle_distance(path: &[Point]) -> f64 {
        Shape::built_in("circle").unwrap().distance(&Cloud::of(path).unwrap())
    }

    fn is_circle(path: &[Point]) -> bool {
        recognize(&[Shape::built_in("circle").unwrap()], path).is_some()
    }

    #[test]
    fn circles_however_drawn() {
        for (what, path) in [
            ("round", ellipse(100.0, 100.0, 0.0, 0.0, TAU, 0.0)),
            ("backwards from the top", ellipse(80.0, 80.0, 0.0, -1.6, -TAU, 0.0)),
            ("wide", ellipse(150.0, 70.0, 0.0, 0.0, TAU, 0.0)),
            ("tall and tilted", ellipse(60.0, 160.0, 0.6, 1.0, TAU, 0.0)),
            ("wobbly", ellipse(100.0, 90.0, 0.3, 0.0, TAU, 0.08)),
            ("not quite closed", ellipse(100.0, 100.0, 0.0, 0.0, TAU * 0.9, 0.0)),
            ("overdrawn", ellipse(100.0, 100.0, 0.0, 0.0, TAU * 1.2, 0.0)),
            ("tiny", ellipse(25.0, 20.0, 0.0, 0.0, TAU, 0.0)),
            ("a 4:1 ellipse", ellipse(200.0, 50.0, 0.0, 0.0, TAU, 0.0)),
            ("three quarters", ellipse(100.0, 100.0, 0.0, 0.0, TAU * 0.75, 0.0)),
        ] {
            assert!(is_circle(&path), "{what}: {:.3}", circle_distance(&path));
        }
    }

    #[test]
    fn other_drawings_are_not_circles() {
        for (what, path) in [
            ("a line", polyline(&[(0.0, 0.0), (300.0, 10.0)])),
            ("a wobbly line", ellipse(150.0, 6.0, 0.0, 0.0, TAU * 0.5, 0.0)),
            ("an L", polyline(&[(0.0, 0.0), (0.0, 200.0), (150.0, 200.0)])),
            ("a V", polyline(&[(0.0, 0.0), (80.0, 200.0), (160.0, 0.0)])),
            (
                "an M",
                polyline(&[(0.0, 200.0), (0.0, 0.0), (80.0, 120.0), (160.0, 0.0), (160.0, 200.0)]),
            ),
            ("there and back", polyline(&[(0.0, 0.0), (0.0, 200.0), (0.0, 0.0)])),
            (
                "a Z",
                polyline(&[(0.0, 0.0), (150.0, 0.0), (0.0, 150.0), (150.0, 150.0)]),
            ),
            ("a half circle", ellipse(100.0, 100.0, 0.0, 0.0, TAU * 0.5, 0.0)),
            (
                "three sides of a box",
                polyline(&[(0.0, 0.0), (0.0, 150.0), (150.0, 150.0), (150.0, 0.0)]),
            ),
        ] {
            assert!(!is_circle(&path), "{what}: {:.3}", circle_distance(&path));
        }
    }

    /// Each built-in shape drawn by hand: bigger, squashed, from another
    /// corner or the other way round, a little wobbly.
    #[test]
    fn built_in_shapes_tell_each_other_apart() {
        let all: Vec<Shape> = BUILT_IN.iter().filter_map(|n| Shape::built_in(n)).collect();
        assert_eq!(all.len(), BUILT_IN.len());
        let wobble = |path: Vec<Point>| -> Vec<Point> {
            path.iter()
                .enumerate()
                .map(|(i, &(x, y))| (x + 3.0 * (i as f64 * 1.7).sin(), y + 3.0 * (i as f64 * 2.3).cos()))
                .collect()
        };
        let scaled = |path: &[Point], sx: f64, sy: f64| -> Vec<Point> {
            path.iter().map(|&(x, y)| (300.0 + sx * x, 200.0 + sy * y)).collect()
        };
        let drawn: [(&str, Vec<Point>); 10] = [
            ("circle", ellipse(100.0, 80.0, 0.3, 1.0, -TAU, 0.04)),
            ("circle", ellipse(90.0, 90.0, 0.0, 0.0, TAU * 0.92, 0.0)),
            (
                "triangle",
                polyline(&[(100.0, 200.0), (300.0, 200.0), (200.0, 30.0), (95.0, 195.0)]),
            ),
            (
                "square",
                polyline(&[(0.0, 0.0), (160.0, 0.0), (160.0, 150.0), (0.0, 150.0), (0.0, 4.0)]),
            ),
            (
                "star",
                polyline(&[(-58.0, 81.0), (0.0, -100.0), (58.0, 81.0), (-95.0, -31.0), (95.0, -31.0), (-58.0, 81.0)]),
            ),
            ("heart", scaled(Shape::built_in("heart").unwrap().outline(), 90.0, 80.0)),
            ("check", polyline(&[(0.0, 60.0), (50.0, 120.0), (170.0, -30.0)])),
            ("spiral", {
                let mut out: Vec<Point> = (0..=80)
                    .map(|i| i as f64 / 80.0)
                    .map(|t| (t, t * 1.6 * TAU))
                    .map(|(t, a)| (120.0 * t * a.cos(), -120.0 * t * a.sin()))
                    .collect();
                out.reverse();
                out
            }),
            (
                "infinity",
                (0..=80)
                    .map(|i| i as f64 / 80.0 * TAU)
                    .map(|a| (150.0 * a.sin(), -50.0 * (2.0 * a).sin()))
                    .collect(),
            ),
            (
                "check",
                polyline(&[(0.0, 0.0), (30.0, 40.0), (110.0, -70.0)]),
            ),
        ];
        for (name, path) in drawn {
            let path = wobble(path);
            let cloud = Cloud::of(&path).unwrap();
            let scores: Vec<String> = all
                .iter()
                .map(|s| format!("{} {:.3}", s.name, s.distance(&cloud)))
                .collect();
            let found = recognize(&all, &path).map(|s| s.name.as_str());
            assert_eq!(found, Some(name), "{}", scores.join(", "));
        }
    }

    #[test]
    fn taught_shapes() {
        let text = "examples = [[[0, 0], [100, 0], [50, 90], [0, 0]], [[0, 0], [90, 5], [45, 80], [2, 3]]]";
        let triangle = Shape::parse("triangle", text).unwrap();
        let circle = Shape::built_in("circle").unwrap();
        let shapes = [circle, triangle];
        let drawn = polyline(&[(0.0, 0.0), (200.0, 0.0), (100.0, 170.0), (0.0, 0.0)]);
        assert_eq!(recognize(&shapes, &drawn).map(|s| s.name.as_str()), Some("triangle"));
        let round = ellipse(100.0, 100.0, 0.0, 0.0, TAU, 0.0);
        assert_eq!(recognize(&shapes, &round).map(|s| s.name.as_str()), Some("circle"));

        let round = |rx| ellipse(rx, 100.0, 0.0, 0.0, TAU, 0.0);
        assert!(alike(&[round(100.0), round(120.0)], &round(80.0)));
        assert!(!alike(
            &[round(100.0)],
            &polyline(&[(0.0, 0.0), (0.0, 200.0), (150.0, 200.0)])
        ));
        assert_eq!(shapes[0].outline().len(), N);

        assert!(Shape::parse("x", "examples = []").unwrap_err().contains("at least one"));
        assert!(
            Shape::parse("x", "examples = [[[1, 1], [1, 1]]]")
                .unwrap_err()
                .contains("too small")
        );
        assert!(Shape::parse("x", "points = []").unwrap_err().contains("shapes/x.toml"));
    }
}
