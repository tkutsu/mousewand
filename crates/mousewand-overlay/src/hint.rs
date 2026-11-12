//! The hint panel, painted on the CPU into an image that the GPU shows as
//! one quad. It changes a few times per gesture at most, so painting is
//! cheap next to the trail's frames.
//!
//! Arrows are drawn as paths, so they look the same in every font, and so are
//! the outline of a shape that fired and the mouse of a wheel or click
//! gesture. Text uses the font fontconfig picks for `style.font`.

use std::sync::atomic::{AtomicU64, Ordering};

use ab_glyph::{Font, FontVec, OutlineCurve, PxScale, ScaleFont};
use mousewand_core::config::BTN_LEFT;
use mousewand_core::hint::{Hint, State};
use mousewand_core::shape::Point;
use mousewand_core::{Align, Dir, HintStyle, Input, Rgba, Wheel};
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

/// A row: arrows, a shape or a mouse, then a label, then how many times
/// it fired.
struct Row<'a> {
    mark: Mark<'a>,
    label: Option<&'a str>,
    /// "3×", for a repeat.
    count: Option<String>,
    size: f32,
    arrow: [f32; 4],
    text: [f32; 4],
    /// A possible next move, not the stroke so far.
    next: bool,
}

/// What a row draws before its label.
#[derive(Clone, Copy)]
enum Mark<'a> {
    Arrows(&'a [Dir]),
    /// A shape's outline.
    Shape(&'a [Point]),
    Mouse(Part),
}

/// The part of the mouse a wheel or click gesture uses.
#[derive(Clone, Copy)]
enum Part {
    Left,
    Right,
    Wheel(Wheel),
}

impl<'a> Mark<'a> {
    fn of(input: &'a Input) -> Mark<'a> {
        match input {
            Input::Stroke(stroke) => Mark::Arrows(&stroke.0),
            Input::Shape(shape) => Mark::Shape(shape.outline()),
            Input::Wheel(wheel) => Mark::Mouse(Part::Wheel(*wheel)),
            Input::Click(BTN_LEFT) => Mark::Mouse(Part::Left),
            Input::Click(_) => Mark::Mouse(Part::Right),
        }
    }

    /// How many arrow squares it takes: a wheel's arrow tucks in close to
    /// its mouse.
    fn squares(self) -> f32 {
        match self {
            Mark::Arrows(arrows) => arrows.len() as f32,
            Mark::Mouse(Part::Wheel(_)) => 1.75,
            _ => 1.0,
        }
    }
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
            mark: hint.input.as_ref().map_or(Mark::Arrows(&hint.stroke.0), Mark::of),
            label,
            count: (hint.count > 1).then(|| format!("{}×", hint.count)),
            size,
            arrow,
            text,
            next: false,
        });
        let next = hint.next.iter().map(|(rest, label)| Row {
            mark: Mark::of(rest),
            label: Some(label),
            count: None,
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

        // A shape or a click takes one arrow's square, a wheel a bit more.
        // Next moves line their labels up after the longest stroke.
        let arrows_width = |row: &Row| row.mark.squares() * row.size * 1.05;
        let next_arrows = rows.iter().filter(|r| r.next).map(arrows_width).fold(0.0, f32::max);
        let gap = |row: &Row| {
            if row.mark.squares() == 0.0 {
                0.0
            } else {
                row.size * 0.45
            }
        };
        let count_gap = |row: &Row| row.size * 0.4;
        let widths: Vec<f32> = rows
            .iter()
            .map(|row| {
                let arrows = if row.next { next_arrows } else { arrows_width(row) };
                let label = row.label.map_or(0.0, |l| gap(row) + text_width(font, l, row.size));
                let count = row
                    .count
                    .as_deref()
                    .map_or(0.0, |c| count_gap(row) + text_width(font, c, row.size));
                arrows + label + count
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
            let top = y + (h - row.size) / 2.0;
            let square = row.size * 1.05;
            match row.mark {
                Mark::Arrows(arrows) => {
                    for (i, &dir) in arrows.iter().enumerate() {
                        draw_arrow(&mut pixmap, dir, x + i as f32 * square, top, row.size, row.arrow);
                    }
                }
                Mark::Shape(outline) => draw_shape(&mut pixmap, outline, x, top, row.size, row.arrow),
                Mark::Mouse(part) => draw_mouse(&mut pixmap, part, x, top, row.size, row.arrow),
            }
            x += row.mark.squares() * square;
            if row.next {
                x = pad + next_arrows;
            }
            if let Some(font) = font {
                if let Some(label) = row.label {
                    x += gap(row);
                    draw_text(&mut pixmap, font, label, x, y + h / 2.0, row.size, row.text);
                    x += text_width(Some(font), label, row.size);
                }
                // In the arrows' color, so it reads as part of the gesture.
                if let Some(count) = &row.count {
                    x += count_gap(row);
                    draw_text(&mut pixmap, font, count, x, y + h / 2.0, row.size, row.arrow);
                }
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
    let mut p = PathBuilder::new();
    add_rounded_rect(&mut p, (0.0, 0.0), w, h, r);
    p.finish()
}

/// How far a quarter circle's cubic puts its control points from the ends,
/// per unit of radius.
const QUARTER: f32 = 0.5523;

/// A rounded rect with its top-left corner at `(x, y)`, as a subpath of `p`.
fn add_rounded_rect(p: &mut PathBuilder, (x, y): (f32, f32), w: f32, h: f32, r: f32) {
    let k = r * QUARTER;
    let (x1, y1) = (x + w, y + h);
    p.move_to(x + r, y);
    p.line_to(x1 - r, y);
    p.cubic_to(x1 - r + k, y, x1, y + r - k, x1, y + r);
    p.line_to(x1, y1 - r);
    p.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    p.line_to(x + r, y1);
    p.cubic_to(x + r - k, y1, x, y1 - r + k, x, y1 - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    p.close();
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
    pixmap.stroke_path(&path, &paint(color), &pen(0.12), transform, None);
}

/// A shape's outline, fitted into a `size` square at (x, y), in a finer
/// line than an arrow's so its detail stays open.
fn draw_shape(pixmap: &mut Pixmap, outline: &[Point], x: f32, y: f32, size: f32, color: [f32; 4]) {
    let (x0, x1, y0, y1) = outline.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY),
        |(x0, x1, y0, y1), &(px, py)| (x0.min(px), x1.max(px), y0.min(py), y1.max(py)),
    );
    let span = (x1 - x0).max(y1 - y0).max(f64::EPSILON);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    // Into the middle of a unit square.
    let at = |(px, py): Point| {
        (
            (0.5 + (px - cx) / span * 0.84) as f32,
            (0.5 + (py - cy) / span * 0.84) as f32,
        )
    };
    let mut p = PathBuilder::new();
    for (i, &point) in outline.iter().enumerate() {
        let (px, py) = at(point);
        match i {
            0 => p.move_to(px, py),
            _ => p.line_to(px, py),
        }
    }
    let Some(path) = p.finish() else { return };
    let transform = Transform::from_scale(size, size).post_translate(x, y);
    pixmap.stroke_path(&path, &paint(color), &pen(0.09), transform, None);
}

/// A mouse in a `size` square at (x, y): its outline faint and the part a
/// gesture uses solid, a button, or the wheel, with an arrow after it for
/// the way it turns.
fn draw_mouse(pixmap: &mut Pixmap, part: Part, x: f32, y: f32, size: f32, color: [f32; 4]) {
    // The body, in a unit square, and the line under its buttons.
    let (x0, y0, w, h, r) = (0.2, 0.04, 0.6, 0.92, 0.24);
    let buttons = 0.42;
    let at = Transform::from_scale(size, size).post_translate(x, y);
    let solid = paint(color);

    // One path, so the faint lines don't darken where they cross.
    let mut p = PathBuilder::new();
    add_rounded_rect(&mut p, (x0, y0), w, h, r);
    p.move_to(x0, buttons);
    p.line_to(x0 + w, buttons);
    p.move_to(0.5, y0);
    p.line_to(0.5, buttons);
    if let Some(outline) = p.finish() {
        let faint = [color[0], color[1], color[2], color[3] * 0.6];
        pixmap.stroke_path(&outline, &paint(faint), &pen(0.08), at, None);
    }

    match part {
        Part::Left | Part::Right => {
            // The left button; the right one is its mirror image.
            let k = r * QUARTER;
            let mut p = PathBuilder::new();
            p.move_to(0.5, y0);
            p.line_to(x0 + r, y0);
            p.cubic_to(x0 + r - k, y0, x0, y0 + r - k, x0, y0 + r);
            p.line_to(x0, buttons);
            p.line_to(0.5, buttons);
            p.close();
            let mirror = match part {
                Part::Right => Transform::from_row(-1.0, 0.0, 0.0, 1.0, 1.0, 0.0),
                _ => Transform::identity(),
            };
            if let Some(button) = p.finish() {
                pixmap.fill_path(&button, &solid, FillRule::Winding, mirror.post_concat(at), None);
            }
        }
        Part::Wheel(wheel) => {
            let mut p = PathBuilder::new();
            add_rounded_rect(&mut p, (0.43, 0.12), 0.14, 0.22, 0.07);
            if let Some(wheel) = p.finish() {
                pixmap.fill_path(&wheel, &solid, FillRule::Winding, at, None);
            }
            let dir = match wheel {
                Wheel::Up => Dir::Up,
                Wheel::Down => Dir::Down,
                Wheel::Left => Dir::Left,
                Wheel::Right => Dir::Right,
            };
            draw_arrow(pixmap, dir, x + size * 0.75, y, size, color);
        }
    }
}

/// A round line `width` wide, in a unit square.
fn pen(width: f32) -> tiny_skia::Stroke {
    tiny_skia::Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    }
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
    fn a_fired_shape_takes_one_arrow_s_room() {
        let config = Config::parse(
            "[[gesture]]\nshape = \"circle\"\nname = \"Menu\"\naction = { keys = \"F10\" }\n\n\
             [[gesture]]\nstroke = \"DLURD\"\nname = \"Menu\"\naction = { keys = \"F10\" }",
            &[],
            &[],
        )
        .unwrap();
        let stroke: Stroke = "DLURD".parse().unwrap();
        let mut painter = Painter::default();
        let mut width = |gesture| {
            painter
                .paint(&Hint::fired(&stroke, gesture, 1), &config.hint, 1.0)
                .unwrap()
                .width
        };
        let (shape, arrows) = (width(&config.gestures[0]), width(&config.gestures[1]));
        // Four arrows fewer, give or take rounding.
        let fewer = (arrows - shape) as f32 - config.hint.size * 1.05 * 4.0;
        assert!(fewer.abs() <= 1.0, "{arrows} with arrows, {shape} with the shape");
    }

    #[test]
    fn a_miss_is_only_red_arrows() {
        let style = HintStyle::default();
        let stroke: Stroke = "LR".parse().unwrap();
        let mut painter = Painter::default();
        let config = Config::parse(
            "[[gesture]]\nstroke = \"LR\"\nname = \"Something long\"\naction = { keys = \"F5\" }",
            &[],
            &[],
        )
        .unwrap();
        let missed = painter.paint(&Hint::missed(&stroke), &style, 1.0);
        let fired = painter.paint(&Hint::fired(&stroke, &config.gestures[0], 1), &style, 1.0);
        assert!(missed.unwrap().width < fired.unwrap().width, "no label on a miss");
        assert!(painter.paint(&Hint::missed(&Stroke::default()), &style, 1.0).is_none());
    }

    #[test]
    fn a_wheel_takes_its_arrow_s_room_and_a_repeat_counts() {
        let config = Config::parse(
            "[[gesture]]\nwheel = \"up\"\nname = \"Louder\"\naction = { keys = \"F5\" }\n\n\
             [[gesture]]\nclick = \"left\"\nname = \"Louder\"\naction = { keys = \"F6\" }",
            &[],
            &[],
        )
        .unwrap();
        let mut painter = Painter::default();
        let mut width = |gesture, count| {
            painter
                .paint(&Hint::fired(&Stroke::default(), gesture, count), &config.hint, 1.0)
                .unwrap()
                .width
        };
        let (wheel, click) = (&config.gestures[0], &config.gestures[1]);
        let more = (width(wheel, 1) - width(click, 1)) as f32 - config.hint.size * 1.05 * 0.75;
        assert!(
            more.abs() <= 1.0,
            "a wheel is a mouse and three quarters of an arrow wider"
        );
        assert!(width(wheel, 3) > width(wheel, 1), "3× after the label");
        assert_eq!(width(wheel, 1), width(wheel, 0), "once is no count");
    }
}
