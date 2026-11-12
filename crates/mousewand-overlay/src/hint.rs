//! The hint panel, painted on the CPU into an image that the GPU shows as
//! one quad. It changes a few times per gesture at most, so painting is
//! cheap next to the trail's frames.
//!
//! Arrows are drawn as paths, so they look the same in every font. Text uses
//! the font fontconfig picks for `style.font`.

use std::sync::atomic::{AtomicU64, Ordering};

use ab_glyph::{Font, FontVec, OutlineCurve, PxScale, ScaleFont};
use mousewand_core::hint::{Hint, State};
use mousewand_core::{Align, Dir, HintStyle, Rgba};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Transform};

/// Premultiplied RGBA pixels.
pub struct Image {
    /// Changes with every new image, so the GPU knows when to upload.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Where the main line's panel sits in the image, top and height: it's
    /// what `place` puts in position, with the next moves hanging off it.
    pub main: (f32, f32),
}

/// Paints hints, with the font for the current style.
#[derive(Default)]
pub struct Painter {
    /// The family asked for, and its font if fontconfig found one.
    font: Option<(String, Option<FontVec>)>,
}

/// A row: arrows, then a label.
struct Row<'a> {
    arrows: &'a [Dir],
    label: Option<&'a str>,
    size: f32,
    arrow: [f32; 4],
    text: [f32; 4],
    /// A possible next move, not the stroke so far.
    next: bool,
}

/// Whether the next moves go above the main line: at the bottom of the
/// screen there's no room under it.
fn next_above(style: &HintStyle) -> bool {
    style.position.vertical == Align::End
}

impl Painter {
    /// The hint at `scale` (physical pixels per logical one), or `None` if
    /// there's nothing to show.
    pub fn paint(&mut self, hint: &Hint, style: &HintStyle, scale: f32) -> Option<Image> {
        let font = self.font(&style.font);
        let size = style.size * scale;
        let small = size * 0.82;
        let dim = |c: Rgba, f: f32| [c.0[0], c.0[1], c.0[2], c.0[3] * f];
        // A miss shows its arrows in the failed color.
        let (arrow, text) = match hint.state {
            State::Drawing => (style.glyph.0, dim(style.text, 0.75)),
            State::Fired => (style.glyph.0, style.text.0),
            State::Missed => (style.failed.0, style.failed.0),
        };
        // A miss has a label only when it was cancelled.
        let label = hint.label.as_deref();
        let main = (!hint.stroke.0.is_empty() || label.is_some()).then(|| Row {
            arrows: &hint.stroke.0,
            label,
            size,
            arrow,
            text,
            next: false,
        });
        let next = hint.next.iter().map(|(rest, label)| Row {
            arrows: &rest.0,
            label: Some(label),
            size: small,
            arrow: dim(style.glyph, 0.85),
            text: dim(style.text, 0.7),
            next: true,
        });
        let above = next_above(style);
        let rows: Vec<Row> = match above {
            true => next.chain(main).collect(),
            false => main.into_iter().chain(next).collect(),
        };
        if rows.is_empty() {
            return None;
        }

        // Next moves line their labels up after the longest stroke.
        let arrows_width = |row: &Row| row.arrows.len() as f32 * row.size * 1.05;
        let next_arrows = rows.iter().filter(|r| r.next).map(arrows_width).fold(0.0, f32::max);
        let gap = |row: &Row| if row.arrows.is_empty() { 0.0 } else { row.size * 0.45 };
        let widths: Vec<f32> = rows
            .iter()
            .map(|row| {
                let arrows = if row.next { next_arrows } else { arrows_width(row) };
                let label = row.label.map_or(0.0, |l| gap(row) + text_width(font, l, row.size));
                arrows + label
            })
            .collect();
        let pad = style.padding * scale;
        let line = |row: &Row| row.size * 1.45;
        // Between the main line and the next moves, if there are both.
        let mixed = rows.windows(2).any(|w| w[0].next != w[1].next);
        let spacing = if mixed { size * 0.35 } else { 0.0 };
        let width = (widths.iter().fold(0.0, |a: f32, &b| a.max(b)) + pad * 2.0).ceil();
        let height = (rows.iter().map(line).sum::<f32>() + spacing + pad * 2.0).ceil();
        // The panel the main line alone would make, at the end it's on.
        let main_height = size * 1.45 + pad * 2.0;
        let main_top = if above { height - main_height } else { 0.0 };
        let mut pixmap = Pixmap::new(width as u32, height as u32)?;

        let radius = (style.radius * scale).min(width / 2.0).min(height / 2.0);
        if let Some(path) = rounded_rect(width, height, radius) {
            pixmap.fill_path(
                &path,
                &paint(style.background.0),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        let mut y = pad;
        for (i, row) in rows.iter().enumerate() {
            if i > 0 && row.next != rows[i - 1].next {
                y += spacing;
            }
            let h = line(row);
            let mut x = pad;
            for &dir in row.arrows {
                draw_arrow(&mut pixmap, dir, x, y + (h - row.size) / 2.0, row.size, row.arrow);
                x += row.size * 1.05;
            }
            if row.next {
                x = pad + next_arrows;
            }
            if let (Some(label), Some(font)) = (row.label, font) {
                x += gap(row);
                draw_text(&mut pixmap, font, label, x, y + h / 2.0, row.size, row.text);
            }
            y += h;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Some(Image {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            width: pixmap.width(),
            height: pixmap.height(),
            pixels: pixmap.take(),
            main: (main_top, main_height),
        })
    }

    fn font(&mut self, family: &str) -> Option<&FontVec> {
        if self.font.as_ref().is_none_or(|(f, _)| f != family) {
            let font = load_font(family);
            if font.is_none() {
                log::warn!("no font for {family:?}: hints show arrows only");
            }
            self.font = Some((family.to_string(), font));
        }
        self.font.as_ref().and_then(|(_, font)| font.as_ref())
    }
}

/// The file fontconfig matches for `family`, the way every app finds fonts.
fn load_font(family: &str) -> Option<FontVec> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", family])
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let (file, index) = text.split_once('\n')?;
    let data = std::fs::read(file).ok()?;
    FontVec::try_from_vec_and_index(data, index.trim().parse().unwrap_or(0)).ok()
}

fn paint(color: [f32; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        (color[0] * 255.0).round() as u8,
        (color[1] * 255.0).round() as u8,
        (color[2] * 255.0).round() as u8,
        (color[3] * 255.0).round() as u8,
    );
    paint.anti_alias = true;
    paint
}

fn rounded_rect(w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if r <= 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, w, h)?));
    }
    // Quarter circles as cubics.
    let k = r * 0.5523;
    let mut p = PathBuilder::new();
    p.move_to(r, 0.0);
    p.line_to(w - r, 0.0);
    p.cubic_to(w - r + k, 0.0, w, r - k, w, r);
    p.line_to(w, h - r);
    p.cubic_to(w, h - r + k, w - r + k, h, w - r, h);
    p.line_to(r, h);
    p.cubic_to(r - k, h, 0.0, h - r + k, 0.0, h - r);
    p.line_to(0.0, r);
    p.cubic_to(0.0, r - k, r - k, 0.0, r, 0.0);
    p.close();
    p.finish()
}

/// An arrow pointing `dir`, filling a `size` square at (x, y).
fn draw_arrow(pixmap: &mut Pixmap, dir: Dir, x: f32, y: f32, size: f32, color: [f32; 4]) {
    // Pointing right in a unit square, then turned.
    let mut p = PathBuilder::new();
    p.move_to(0.18, 0.5);
    p.line_to(0.82, 0.5);
    p.move_to(0.56, 0.24);
    p.line_to(0.82, 0.5);
    p.line_to(0.56, 0.76);
    let Some(path) = p.finish() else { return };
    let angle = match dir {
        Dir::Right => 0.0,
        Dir::DownRight => 45.0,
        Dir::Down => 90.0,
        Dir::DownLeft => 135.0,
        Dir::Left => 180.0,
        Dir::UpLeft => 225.0,
        Dir::Up => 270.0,
        Dir::UpRight => 315.0,
    };
    let transform = Transform::from_rotate_at(angle, 0.5, 0.5)
        .post_scale(size, size)
        .post_translate(x, y);
    let stroke = tiny_skia::Stroke {
        width: 0.12,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint(color), &stroke, transform, None);
}

/// Text at `size` px, its middle on `mid_y`.
fn draw_text(pixmap: &mut Pixmap, font: &FontVec, text: &str, x: f32, mid_y: f32, size: f32, color: [f32; 4]) {
    let scaled = font.as_scaled(px_scale(font, size));
    let baseline = mid_y + (scaled.ascent() + scaled.descent()) / 2.0;
    let (sx, sy) = (scaled.h_scale_factor(), scaled.v_scale_factor());
    let mut pen = x;
    let mut prev = None;
    let mut path = PathBuilder::new();
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = prev {
            pen += scaled.kern(prev, id);
        }
        if let Some(outline) = font.outline(id) {
            let at = |p: ab_glyph::Point| (pen + p.x * sx, baseline - p.y * sy);
            let mut last = None;
            for curve in &outline.curves {
                let (start, end) = match curve {
                    OutlineCurve::Line(a, b) | OutlineCurve::Quad(a, _, b) | OutlineCurve::Cubic(a, _, _, b) => (a, b),
                };
                if last != Some(*start) {
                    if last.is_some() {
                        path.close();
                    }
                    let (x, y) = at(*start);
                    path.move_to(x, y);
                }
                match curve {
                    OutlineCurve::Line(_, b) => {
                        let (x, y) = at(*b);
                        path.line_to(x, y);
                    }
                    OutlineCurve::Quad(_, c, b) => {
                        let ((cx, cy), (x, y)) = (at(*c), at(*b));
                        path.quad_to(cx, cy, x, y);
                    }
                    OutlineCurve::Cubic(_, c1, c2, b) => {
                        let ((ax, ay), (bx, by), (x, y)) = (at(*c1), at(*c2), at(*b));
                        path.cubic_to(ax, ay, bx, by, x, y);
                    }
                }
                last = Some(*end);
            }
            if last.is_some() {
                path.close();
            }
        }
        pen += scaled.h_advance(id);
        prev = Some(id);
    }
    if let Some(path) = path.finish() {
        pixmap.fill_path(&path, &paint(color), FillRule::Winding, Transform::identity(), None);
    }
}

fn text_width(font: Option<&FontVec>, text: &str, size: f32) -> f32 {
    let Some(font) = font else { return 0.0 };
    let scaled = font.as_scaled(px_scale(font, size));
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = prev {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// `size` as an em size, the way CSS and toolkits mean it. ab_glyph's own
/// scale is the ascent-to-descent height, which runs larger than the em.
fn px_scale(font: &FontVec, size: f32) -> PxScale {
    let em = font.units_per_em().unwrap_or(1000.0);
    let height = font.height_unscaled();
    PxScale::from(size * height / em)
}

/// Where the hint's top-left corner goes on a `surface`-sized area. Its main
/// line goes where `position` says, so it stays put while the next moves
/// come and go.
pub fn place(style: &HintStyle, image: &Image, surface: (u32, u32), scale: f32) -> (f32, f32) {
    let margin = style.margin * scale;
    let at = |align: Align, size: f32, room: u32| match align {
        Align::Start => margin,
        Align::Center => (room as f32 - size) / 2.0,
        Align::End => room as f32 - size - margin,
    };
    let (main_top, main_height) = image.main;
    (
        at(style.position.horizontal, image.width as f32, surface.0).round(),
        (at(style.position.vertical, main_height, surface.1) - main_top).round(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mousewand_core::{Config, Stroke};

    #[test]
    fn paints_a_panel_and_places_it() {
        let config = Config::parse(
            r#"
            [hint]
            enabled = true

            [[gesture]]
            stroke = "D"
            name = "Reload"
            action = { keys = "F5" }

            [[gesture]]
            stroke = "DR"
            name = "Close tab"
            action = { keys = "ctrl+w" }
            "#,
            &[],
            &[],
        )
        .unwrap();
        let stroke: Stroke = "D".parse().unwrap();
        let hint = Hint::drawing(&config.gestures, &stroke, true);
        let mut painter = Painter::default();
        let image = painter.paint(&hint, &config.hint, 1.0).expect("a hint");
        assert!(image.width > 40 && image.height > 40);
        // The corner is rounded off, the middle is filled.
        assert_eq!(image.pixels[3], 0);
        let mid = ((image.height / 2 * image.width + image.width / 2) * 4 + 3) as usize;
        assert!(image.pixels[mid] > 200);
        let (x, y) = place(&config.hint, &image, (1000, 800), 1.0);
        assert_eq!(x, ((1000 - image.width) as f32 / 2.0).round());
        assert_eq!(
            y,
            (800 - image.height) as f32 - 64.0,
            "at the bottom, next moves go above"
        );

        let empty = Hint::drawing(&[], &Stroke::default(), true);
        assert!(painter.paint(&empty, &config.hint, 1.0).is_none());
    }

    #[test]
    fn the_main_line_stays_put() {
        let config = Config::parse(
            "[[gesture]]\nstroke = \"D\"\naction = { keys = \"F5\" }\n\n\
             [[gesture]]\nstroke = \"DR\"\naction = { keys = \"ctrl+w\" }",
            &[],
            &[],
        )
        .unwrap();
        let mut painter = Painter::default();
        let stroke: Stroke = "D".parse().unwrap();
        let mut paint_at = |position: &str, next: bool| {
            let style = HintStyle {
                position: position.parse().unwrap(),
                ..config.hint.clone()
            };
            let image = painter
                .paint(&Hint::drawing(&config.gestures, &stroke, next), &style, 1.0)
                .unwrap();
            let (_, y) = place(&style, &image, (1000, 800), 1.0);
            (y + image.main.0, image.height)
        };
        for position in ["top", "center", "bottom"] {
            let (alone, short) = paint_at(position, false);
            let (with_next, tall) = paint_at(position, true);
            assert!(tall > short);
            assert_eq!(alone, with_next, "{position}");
        }
    }

    #[test]
    fn a_miss_is_only_red_arrows() {
        let style = HintStyle::default();
        let stroke: Stroke = "LR".parse().unwrap();
        let mut painter = Painter::default();
        let missed = painter.paint(&Hint::missed(&stroke), &style, 1.0);
        let fired = painter.paint(&Hint::fired(&stroke, "Something long".into()), &style, 1.0);
        assert!(missed.unwrap().width < fired.unwrap().width, "no label on a miss");
        assert!(painter.paint(&Hint::missed(&Stroke::default()), &style, 1.0).is_none());
    }
}
