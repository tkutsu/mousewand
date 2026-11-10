//! The ribbon's geometry: recent pointer positions turned into a triangle
//! strip that narrows and fades toward its tail.

/// How long a point stays on screen, in seconds.
pub const LIFE: f32 = 0.45;
/// Full width at the head, glow included, in logical pixels.
pub const WIDTH: f32 = 26.0;
/// Points closer than this to the previous one are skipped.
const MIN_STEP: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    /// Clip-space position.
    pub pos: [f32; 2],
    /// -1 on one edge of the ribbon, 1 on the other.
    pub edge: f32,
    pub alpha: f32,
    pub color: [f32; 3],
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

    /// The strip for time `now` on a `width`×`height` surface, with every
    /// alpha scaled by `fade`.
    pub fn mesh(&mut self, now: f32, (width, height): (f32, f32), fade: f32, color: [f32; 3]) -> Vec<Vertex> {
        self.points.retain(|p| now - p.t < LIFE);
        let points = &self.points;
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
            let life = 1.0 - (now - p.t) / LIFE;
            let half = WIDTH / 2.0 * life.powf(0.7);
            let alpha = life * fade;
            for side in [-1.0, 1.0] {
                vertices.push(Vertex {
                    pos: clip(p.x + nx * half * side, p.y + ny * half * side),
                    edge: side,
                    alpha,
                    color,
                });
            }
        }
        vertices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_narrows_toward_the_tail() {
        let mut trail = Trail::default();
        for i in 0..10 {
            trail.push(i as f32 * 10.0, 100.0, i as f32 * 0.01);
        }
        let mesh = trail.mesh(0.1, (200.0, 200.0), 1.0, [1.0; 3]);
        assert_eq!(mesh.len(), 20);
        let width = |i: usize| (mesh[i * 2].pos[1] - mesh[i * 2 + 1].pos[1]).abs();
        assert!(width(9) > width(5) && width(5) > width(1));
        assert!(mesh[19].alpha > mesh[0].alpha);
    }

    #[test]
    fn old_points_expire_and_close_ones_are_skipped() {
        let mut trail = Trail::default();
        trail.push(0.0, 0.0, 0.0);
        trail.push(0.5, 0.0, 0.0);
        trail.push(50.0, 0.0, 0.3);
        trail.push(90.0, 0.0, 0.4);
        assert_eq!(trail.points.len(), 3);
        assert_eq!(trail.mesh(0.5, (100.0, 100.0), 1.0, [1.0; 3]).len(), 4);
        assert!(trail.mesh(1.0, (100.0, 100.0), 1.0, [1.0; 3]).is_empty());
    }
}
