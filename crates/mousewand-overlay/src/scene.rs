//! One gesture's trail over time, whatever surface it's drawn on: points,
//! repeat-step rings, and the fade after release.

use std::time::Instant;

use mousewand_core::Theme;

use crate::trail::{self, Style, Trail, Vertex};

pub struct Scene {
    trail: Trail,
    /// The stroke drawn before the compositor first said where the pointer
    /// is, from the mouse's own motion, relative to where it began: points
    /// and their times. `None` once placed.
    unplaced: Option<Vec<(f32, f32, f32)>>,
    /// Ring pulses: when, in seconds since `start`, and where.
    pulses: Vec<(f32, (f32, f32))>,
    start: Instant,
    ended: Option<(Instant, bool)>,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            trail: Trail::default(),
            unplaced: Some(vec![(0.0, 0.0, 0.0)]),
            pulses: Vec::new(),
            start: Instant::now(),
            ended: None,
        }
    }

    /// The mouse moved by this much. Surfaces can take a while to appear,
    /// longer on a busy compositor, and get no pointer positions until then:
    /// motion fills in the stroke drawn meanwhile.
    pub fn moved(&mut self, dx: f32, dy: f32) {
        let t = self.start.elapsed().as_secs_f32();
        if let Some(path) = &mut self.unplaced {
            let &(x, y, _) = path.last().expect("starts with a point");
            path.push((x + dx, y + dy, t));
        }
    }

    /// A pointer position, in surface coordinates. Ignored after release.
    /// The first one places the stroke drawn so far, ending here.
    pub fn push(&mut self, x: f32, y: f32) {
        if self.ended.is_some() {
            return;
        }
        if let Some(path) = self.unplaced.take() {
            let &(ex, ey, _) = path.last().expect("starts with a point");
            for (px, py, t) in path {
                self.trail.push(x - ex + px, y - ey + py, t);
            }
        }
        self.trail.push(x, y, self.start.elapsed().as_secs_f32());
    }

    /// Pulses a ring at the head.
    pub fn tick(&mut self) {
        if let Some(head) = self.trail.head() {
            self.pulses.push((self.start.elapsed().as_secs_f32(), head));
        }
    }

    pub fn end(&mut self, matched: bool) {
        self.ended.get_or_insert((Instant::now(), matched));
    }

    /// 1 while drawing, falling to 0 over the fade after release.
    pub fn fade(&self, theme: &Theme) -> f32 {
        match self.ended {
            Some((at, _)) => 1.0 - at.elapsed().as_secs_f32() * 1000.0 / theme.fade_ms.max(1) as f32,
            None => 1.0,
        }
    }

    /// The strips to draw now on a surface of `size`, or `None` once the
    /// fade is over. `output` turns a theme color into what the surface
    /// takes (linear on sRGB surfaces).
    pub fn strips(
        &mut self,
        size: (f32, f32),
        theme: &Theme,
        output: impl Fn([f32; 3]) -> [f32; 3],
    ) -> Option<Vec<Vec<Vertex>>> {
        let fade = self.fade(theme);
        if fade <= 0.0 {
            return None;
        }
        let colors: Vec<_> = match self.ended {
            Some((_, false)) => vec![output(theme.rejected.0)],
            _ => theme.color.0.iter().map(|c| output(c.0)).collect(),
        };
        let style = Style {
            colors: &colors,
            glow: theme.glow,
            fade,
            width: theme.width,
            life: theme.life_ms as f32 / 1000.0,
        };
        let now = self.start.elapsed().as_secs_f32();
        self.pulses.retain(|&(t, _)| now - t < trail::RING_LIFE);
        let mut strips = vec![self.trail.mesh(now, size, &style), self.trail.cap(now, size, &style)];
        strips.extend(
            self.pulses
                .iter()
                .map(|&(t, center)| trail::ring(center, now - t, size, &style)),
        );
        Some(strips)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_before_the_first_position_is_placed_to_end_there() {
        let mut scene = Scene::new();
        scene.moved(0.0, 60.0);
        scene.moved(80.0, 0.0);
        scene.push(500.0, 400.0);
        assert_eq!(scene.trail.points(), [(420.0, 340.0), (420.0, 400.0), (500.0, 400.0)]);
    }

    #[test]
    fn motion_after_the_first_position_is_left_to_the_compositor() {
        let mut scene = Scene::new();
        scene.push(500.0, 400.0);
        scene.moved(80.0, 0.0);
        scene.push(510.0, 400.0);
        assert_eq!(scene.trail.points(), [(500.0, 400.0), (510.0, 400.0)]);
    }

    #[test]
    fn nothing_is_placed_after_release() {
        let mut scene = Scene::new();
        scene.moved(0.0, 60.0);
        scene.end(true);
        scene.push(500.0, 400.0);
        assert_eq!(scene.trail.points(), []);
    }
}
