//! One gesture's trail over time, whatever surface it's drawn on: points,
//! repeat-step rings, and the fade after release.

use std::time::Instant;

use mousewand_core::Theme;

use crate::trail::{self, Style, Trail, Vertex};

pub struct Scene {
    trail: Trail,
    /// Ring pulses: when, in seconds since `start`, and where.
    pulses: Vec<(f32, (f32, f32))>,
    start: Instant,
    ended: Option<(Instant, bool)>,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            trail: Trail::default(),
            pulses: Vec::new(),
            start: Instant::now(),
            ended: None,
        }
    }

    /// A pointer position, in surface coordinates. Ignored after release.
    pub fn push(&mut self, x: f32, y: f32) {
        if self.ended.is_none() {
            self.trail.push(x, y, self.start.elapsed().as_secs_f32());
        }
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
