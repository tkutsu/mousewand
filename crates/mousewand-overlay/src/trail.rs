//! The ribbon's geometry: recent pointer positions turned into a triangle
//! strip that narrows and fades toward its tail.

/// Points closer than this to the previous one are skipped.
const MIN_STEP: f32 = 3.0;
/// How many points on each side of a point are averaged into it. Mice
/// report whole counts, so raw points zigzag a pixel or two around the
/// line the hand drew.
const SMOOTHING: usize = 4;
/// How long a repeat-step ring takes to expand and fade, in seconds.
pub const RING_LIFE: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    /// Clip-space position.
    pub pos: [f32; 2],
    /// -1 on one edge of the ribbon, 1 on the other.
    pub edge: f32,
    pub alpha: f32,
    pub color: [f32; 3],
    pub glow: f32,
}

/// How to paint one frame.
#[derive(Clone, Copy, Debug)]
pub struct Style<'a> {
    /// From head to tail, at least one.
    pub colors: &'a [[f32; 3]],
    pub glow: f32,
    /// Scales every alpha, for the fade after release.
    pub fade: f32,
    /// Width at the head, glow included, in logical pixels.
    pub width: f32,
    /// How long a point stays on screen, in seconds.
    pub life: f32,
}

impl Style<'_> {
    /// The color at `life`: the first at the head (1), blending evenly
    /// through the rest to the last at the tail (0).
    pub fn color(&self, life: f32) -> [f32; 3] {
        let last = self.colors.len() - 1;
        if last == 0 {
            return self.colors[0];
        }
        let x = (1.0 - life).clamp(0.0, 1.0) * last as f32;
        let i = (x as usize).min(last - 1);
        let (a, b, f) = (self.colors[i], self.colors[i + 1], x - i as f32);
        std::array::from_fn(|c| a[c] + (b[c] - a[c]) * f)
    }
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f32,
    y: f32,
    t: f32,
}

#[derive(Default, Debug)]
pub struct Trail {
    points: Vec<Point>,
}

impl Trail {
    pub fn push(&mut self, x: f32, y: f32, t: f32) {
        if let Some(last) = self.points.last()
            && (last.x - x).hypot(last.y - y) < MIN_STEP
        {
            return;
        }
        self.points.push(Point { x, y, t });
    }

    /// The newest point.
    pub fn head(&self) -> Option<(f32, f32)> {
        self.points.last().map(|p| (p.x, p.y))
    }

    /// The strip for time `now` on a `width`×`height` surface.
    pub fn mesh(&mut self, now: f32, (width, height): (f32, f32), style: &Style) -> Vec<Vertex> {
        self.points.retain(|p| now - p.t < style.life);
        let points = self.smoothed();
        let n = points.len();
        if n < 2 {
            return Vec::new();
        }
        let clip = |x: f32, y: f32| [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0];
        let mut vertices = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (prev, next) = (points[i.saturating_sub(1)], points[(i + 1).min(n - 1)]);
            let (tx, ty) = (next.x - prev.x, next.y - prev.y);
            let len = tx.hypot(ty).max(f32::EPSILON);
            let (nx, ny) = (-ty / len, tx / len);

            let p = points[i];
            // 1 at the head, falling to 0 as the point expires: the ribbon
            // narrows and fades evenly in time, however fast it was drawn.
            let life = 1.0 - (now - p.t) / style.life;
            let half = style.width / 2.0 * life.powf(0.7);
            let alpha = life * style.fade;
            for side in [-1.0, 1.0] {
                vertices.push(Vertex {
                    pos: clip(p.x + nx * half * side, p.y + ny * half * side),
                    edge: side,
                    alpha,
                    color: style.color(life),
                    glow: style.glow,
                });
            }
        }
        vertices
    }

    /// The points, each averaged with its neighbours, weighted by nearness.
    /// The window shrinks toward the ends, so the head stays on the cursor.
    fn smoothed(&self) -> Vec<Point> {
        let n = self.points.len();
        (0..n)
            .map(|i| {
                let k = SMOOTHING.min(i).min(n - 1 - i);
                let (mut x, mut y, mut total) = (0.0, 0.0, 0.0);
                for j in i - k..=i + k {
                    let weight = (k + 1 - i.abs_diff(j)) as f32;
                    x += self.points[j].x * weight;
                    y += self.points[j].y * weight;
                    total += weight;
                }
                Point {
                    x: x / total,
                    y: y / total,
                    t: self.points[i].t,
                }
            })
            .collect()
    }
}

impl Trail {
    /// A semicircle that rounds off the head, the same width as the ribbon
    /// there. Its centre has `edge` 0 and its rim 1, so the shader shades it
    /// radially and it blends into the ribbon. Call after `mesh`.
    pub fn cap(&self, now: f32, (width, height): (f32, f32), style: &Style) -> Vec<Vertex> {
        const SEGMENTS: usize = 12;
        let n = self.points.len();
        if n < 2 {
            return Vec::new();
        }
        let points = self.smoothed();
        let (head, prev) = (points[n - 1], points[n - 2]);
        let forward = (head.y - prev.y).atan2(head.x - prev.x);
        let life = 1.0 - (now - head.t) / style.life;
        let radius = style.width / 2.0 * life.powf(0.7);
        let clip = |x: f32, y: f32| [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0];
        let vertex = |x, y, edge| Vertex {
            pos: clip(x, y),
            edge,
            alpha: life * style.fade,
            color: style.color(life),
            glow: style.glow,
        };
        // A strip alternating rim and centre: every other triangle is
        // degenerate, the rest fan out from the head.
        (0..=SEGMENTS)
            .flat_map(|i| {
                let angle = forward - std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / SEGMENTS as f32;
                let (sin, cos) = angle.sin_cos();
                [
                    vertex(head.x + cos * radius, head.y + sin * radius, 1.0),
                    vertex(head.x, head.y, 0.0),
                ]
            })
            .collect()
    }
}

/// A ring `age` seconds into its pulse: it grows from the cursor and fades.
pub fn ring((cx, cy): (f32, f32), age: f32, (width, height): (f32, f32), style: &Style) -> Vec<Vertex> {
    const SEGMENTS: usize = 40;
    let progress = (age / RING_LIFE).clamp(0.0, 1.0);
    let radius = 8.0 + 28.0 * progress.sqrt();
    let half = 7.0 * (1.0 - progress) + 2.0;
    let alpha = (1.0 - progress) * style.fade;
    let clip = |x: f32, y: f32| [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0];
    (0..=SEGMENTS)
        .flat_map(|i| {
            let angle = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let (sin, cos) = angle.sin_cos();
            [-1.0, 1.0].map(|side| {
                let r = radius + half * side;
                Vertex {
                    pos: clip(cx + cos * r, cy + sin * r),
                    edge: side,
                    alpha,
                    color: style.color(1.0),
                    glow: style.glow,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLE: Style = Style {
        colors: &[[1.0; 3]],
        glow: 0.6,
        fade: 1.0,
        width: 26.0,
        life: 0.45,
    };

    #[test]
    fn strip_narrows_toward_the_tail() {
        let mut trail = Trail::default();
        for i in 0..10 {
            trail.push(i as f32 * 10.0, 100.0, i as f32 * 0.01);
        }
        let mesh = trail.mesh(0.1, (200.0, 200.0), &STYLE);
        assert_eq!(mesh.len(), 20);
        let width = |i: usize| (mesh[i * 2].pos[1] - mesh[i * 2 + 1].pos[1]).abs();
        assert!(width(9) > width(5) && width(5) > width(1));
        assert!(mesh[19].alpha > mesh[0].alpha);
    }

    #[test]
    fn colors_blend_from_head_to_tail() {
        let style = Style {
            colors: &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            ..STYLE
        };
        assert_eq!(style.color(1.0), [1.0, 0.0, 0.0]);
        assert_eq!(style.color(0.5), [0.0, 1.0, 0.0]);
        assert_eq!(style.color(0.0), [0.0, 0.0, 1.0]);
        assert_eq!(style.color(0.75), [0.5, 0.5, 0.0]);
        assert_eq!(STYLE.color(0.3), [1.0; 3]);
    }

    #[test]
    fn ring_grows_and_fades() {
        let early = ring((50.0, 50.0), 0.0, (100.0, 100.0), &STYLE);
        let late = ring((50.0, 50.0), 0.25, (100.0, 100.0), &STYLE);
        assert_eq!(early.len(), late.len());
        // The outer vertex at angle 0 moves outward.
        assert!(late[1].pos[0] > early[1].pos[0]);
        assert!(late[0].alpha < early[0].alpha);
    }

    #[test]
    fn old_points_expire_and_close_ones_are_skipped() {
        let mut trail = Trail::default();
        trail.push(0.0, 0.0, 0.0);
        trail.push(0.5, 0.0, 0.0);
        trail.push(50.0, 0.0, 0.3);
        trail.push(90.0, 0.0, 0.4);
        assert_eq!(trail.points.len(), 3);
        assert_eq!(trail.mesh(0.5, (100.0, 100.0), &STYLE).len(), 4);
        assert!(trail.mesh(1.0, (100.0, 100.0), &STYLE).is_empty());
    }
}
