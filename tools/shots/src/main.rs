//! Compares what mousewand drew on each compositor with what it drew on Hyprland.
//!
//!   mousewand-shots SHOTS_DIR REPORT.html
//!
//! The VM suite saves `COMPOSITOR-SCENE-before.png` and `COMPOSITOR-SCENE-mid.png`
//! for each scene: the screen just before a stroke, and while it's held. Their
//! difference is what mousewand drew, whatever the windows and decorations
//! around it look like. Each compositor's overlay must then roughly match
//! Hyprland's: the trail and the hint there at all, in the same place, the
//! same shape and the same color. Exact pixels never match, since GNOME and X11
//! draw with their own renderers, so the checks are tolerant. The report shows
//! every overlay beside Hyprland's; the exit status is 1 if any check failed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

const REFERENCE: &str = "hyprland";
/// A pixel is mousewand's when a channel changed by more than this.
const CHANGED: u8 = 40;
/// The hint sits in the bottom quarter of the screen, the trail above it.
const HINT_TOP: f64 = 0.75;
/// Each region must have at least this share of the reference's pixels...
const MIN_SHARE: f64 = 0.25;
/// ...its middle no further than this from the reference's, in pixels...
const MAX_SHIFT: f64 = 40.0;
/// ...overlap it this much, once both are grown by `GROW` pixels to forgive
/// a width or font that differs a little...
const MIN_OVERLAP: f64 = 0.5;
const GROW: usize = 6;
/// ...and have a mean color this close, as a distance in 0–255 RGB.
const MAX_COLOR: f64 = 45.0;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, report] = args.as_slice() else {
        bail!("usage: mousewand-shots SHOTS_DIR REPORT.html");
    };
    let dir = Path::new(dir);
    let scenes = find(dir)?;
    if scenes.is_empty() {
        bail!("no screenshots in {}; run tools/vm/vm.sh test first", dir.display());
    }

    // Every overlay, by scene, then by compositor.
    let mut overlays: BTreeMap<String, BTreeMap<String, Overlay>> = BTreeMap::new();
    for ((compositor, scene), (before, mid)) in &scenes {
        let overlay = Overlay::new(&read(before)?, &read(mid)?)
            .with_context(|| format!("{compositor} {scene}"))?;
        overlay.save(&dir.join(format!("{compositor}-{scene}-overlay.png")))?;
        overlays.entry(scene.clone()).or_default().insert(compositor.clone(), overlay);
    }

    let mut rows = Vec::new();
    let mut failed = false;
    for (scene, by_compositor) in &overlays {
        let Some(reference) = by_compositor.get(REFERENCE) else {
            println!("{scene}: no {REFERENCE} shots to compare with; run tools/vm/vm.sh test {REFERENCE}");
            failed = true;
            continue;
        };
        for (compositor, overlay) in by_compositor {
            let checks = if compositor == REFERENCE {
                reference.sanity()
            } else {
                overlay.compare(reference)
            };
            let ok = checks.iter().all(|c| c.ok);
            failed |= !ok;
            let summary: Vec<String> = checks.iter().filter(|c| !c.ok).map(|c| c.to_string()).collect();
            println!(
                "{compositor:>9} {scene:<8} {}",
                if ok { "ok".into() } else { format!("FAILED: {}", summary.join("; ")) }
            );
            rows.push(Row {
                scene: scene.clone(),
                compositor: compositor.clone(),
                checks,
            });
        }
    }

    std::fs::write(report, html(dir, Path::new(report), &rows))?;
    println!("report: {report}");
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

/// The before and mid shots of each compositor's scenes.
fn find(dir: &Path) -> anyhow::Result<BTreeMap<(String, String), (PathBuf, PathBuf)>> {
    let mut scenes = BTreeMap::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let Some(base) = name.strip_suffix("-before.png") else { continue };
        let Some((compositor, scene)) = base.split_once('-') else { continue };
        let mid = dir.join(format!("{base}-mid.png"));
        if mid.exists() {
            scenes.insert((compositor.to_owned(), scene.to_owned()), (path.clone(), mid));
        }
    }
    Ok(scenes)
}

struct Image {
    width: usize,
    height: usize,
    /// RGB, row by row.
    rgb: Vec<u8>,
}

fn read(path: &Path) -> anyhow::Result<Image> {
    let file = std::io::BufReader::new(File::open(path).with_context(|| format!("opening {}", path.display()))?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().context("image too large")?];
    let info = reader.next_frame(&mut buf)?;
    let channels = info.color_type.samples();
    if channels < 3 {
        bail!("{} isn't in color", path.display());
    }
    let rgb = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    Ok(Image {
        width: info.width as usize,
        height: info.height as usize,
        rgb,
    })
}

/// What mousewand drew in one scene: the pixels the stroke changed.
struct Overlay {
    width: usize,
    height: usize,
    mid: Vec<u8>,
    drawn: Vec<bool>,
}

/// One part of the overlay, the trail or the hint.
struct Region {
    pixels: usize,
    middle: (f64, f64),
    color: [f64; 3],
    grown: Vec<bool>,
}

impl Overlay {
    fn new(before: &Image, mid: &Image) -> anyhow::Result<Overlay> {
        if (before.width, before.height) != (mid.width, mid.height) {
            bail!("the before and mid shots differ in size");
        }
        let drawn = before
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(mid.rgb.as_chunks::<3>().0)
            .map(|(a, b)| a.iter().zip(b).any(|(a, b)| a.abs_diff(*b) > CHANGED))
            .collect();
        Ok(Overlay {
            width: mid.width,
            height: mid.height,
            mid: mid.rgb.clone(),
            drawn,
        })
    }

    /// The trail, above the hint, or the hint.
    fn region(&self, hint: bool) -> Region {
        let top = (self.height as f64 * HINT_TOP) as usize;
        let rows = if hint { top..self.height } else { 0..top };
        let mut mask = vec![false; self.drawn.len()];
        let (mut pixels, mut x, mut y, mut color) = (0, 0.0, 0.0, [0.0; 3]);
        for row in rows {
            for col in 0..self.width {
                let i = row * self.width + col;
                if self.drawn[i] {
                    mask[i] = true;
                    pixels += 1;
                    x += col as f64;
                    y += row as f64;
                    for (c, v) in color.iter_mut().zip(&self.mid[i * 3..i * 3 + 3]) {
                        *c += f64::from(*v);
                    }
                }
            }
        }
        let n = pixels.max(1) as f64;
        Region {
            pixels,
            middle: (x / n, y / n),
            color: color.map(|c| c / n),
            grown: grow(&mask, self.width, self.height, GROW),
        }
    }

    /// The reference must show something to compare with.
    fn sanity(&self) -> Vec<Check> {
        [false, true]
            .into_iter()
            .map(|hint| {
                let pixels = self.region(hint).pixels;
                Check::new(part(hint), "drawn", pixels > 200, format!("{pixels} px"))
            })
            .collect()
    }

    fn compare(&self, reference: &Overlay) -> Vec<Check> {
        let mut checks = Vec::new();
        if (self.width, self.height) != (reference.width, reference.height) {
            checks.push(Check::new("screen", "size", false, format!("{}×{}", self.width, self.height)));
            return checks;
        }
        for hint in [false, true] {
            let (this, that) = (self.region(hint), reference.region(hint));
            let share = this.pixels as f64 / that.pixels.max(1) as f64;
            let name = part(hint);
            checks.push(Check::new(name, "drawn", share >= MIN_SHARE, format!("{:.0}% of the pixels", share * 100.0)));
            if this.pixels == 0 {
                continue;
            }
            let shift = (this.middle.0 - that.middle.0).hypot(this.middle.1 - that.middle.1);
            checks.push(Check::new(name, "place", shift <= MAX_SHIFT, format!("{shift:.0} px off")));
            let overlap = overlap(&this.grown, &that.grown);
            checks.push(Check::new(name, "shape", overlap >= MIN_OVERLAP, format!("{:.0}% overlap", overlap * 100.0)));
            let color = this
                .color
                .iter()
                .zip(&that.color)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            checks.push(Check::new(name, "color", color <= MAX_COLOR, format!("{color:.0} off")));
        }
        checks
    }

    /// The overlay alone, see-through where mousewand drew nothing.
    fn save(&self, path: &Path) -> anyhow::Result<()> {
        let file = BufWriter::new(File::create(path).with_context(|| format!("writing {}", path.display()))?);
        let mut encoder = png::Encoder::new(file, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        let rgba: Vec<u8> = self
            .mid
            .as_chunks::<3>()
            .0
            .iter()
            .zip(&self.drawn)
            .flat_map(|(p, &d)| [p[0], p[1], p[2], if d { 255 } else { 0 }])
            .collect();
        encoder.write_header()?.write_image_data(&rgba)?;
        Ok(())
    }
}

fn part(hint: bool) -> &'static str {
    if hint { "hint" } else { "trail" }
}

/// The mask with every set pixel grown into a square of `r` around it.
fn grow(mask: &[bool], width: usize, height: usize, r: usize) -> Vec<bool> {
    // Rows, then columns: a square in two passes of a line each.
    let mut rows = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            if mask[y * width + x] {
                for v in &mut rows[y * width + x.saturating_sub(r)..y * width + (x + r + 1).min(width)] {
                    *v = true;
                }
            }
        }
    }
    let mut out = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            if rows[y * width + x] {
                for yy in y.saturating_sub(r)..(y + r + 1).min(height) {
                    out[yy * width + x] = true;
                }
            }
        }
    }
    out
}

/// Intersection over union.
fn overlap(a: &[bool], b: &[bool]) -> f64 {
    let (both, either) = a.iter().zip(b).fold((0, 0), |(both, either), (&a, &b)| {
        (both + usize::from(a && b), either + usize::from(a || b))
    });
    both as f64 / either.max(1) as f64
}

struct Check {
    part: &'static str,
    name: &'static str,
    ok: bool,
    detail: String,
}

impl Check {
    fn new(part: &'static str, name: &'static str, ok: bool, detail: String) -> Check {
        Check { part, name, ok, detail }
    }
}

impl std::fmt::Display for Check {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} {}: {}", self.part, self.name, self.detail)
    }
}

struct Row {
    scene: String,
    compositor: String,
    checks: Vec<Check>,
}

/// A page of every scene: each compositor's overlay beside Hyprland's, with
/// its checks. Images are linked relative to the report.
fn html(dir: &Path, report: &Path, rows: &[Row]) -> String {
    let base = report.parent().unwrap_or(Path::new("."));
    let link = |name: String| {
        let path = dir.join(name);
        path.strip_prefix(base).unwrap_or(&path).display().to_string()
    };
    let mut out = String::from(
        r#"<!doctype html><meta charset="utf-8"><title>mousewand screenshots</title>
<style>
:root { color-scheme: light dark; font: 14px system-ui, sans-serif; }
body { margin: 24px; } h2 { margin-top: 32px; }
table { border-collapse: collapse; } td, th { padding: 8px; vertical-align: top; text-align: left; }
tr + tr td { border-top: 1px solid #8884; }
img { width: 400px; display: block; border-radius: 6px; }
.overlay { background: repeating-conic-gradient(#8883 0 25%, #0000 0 50%) 0 0 / 16px 16px; }
.bad { color: #d33; } .good { color: #3a3; } ul { margin: 0; padding-left: 18px; }
</style>
<h1>mousewand screenshots</h1>
<p>What mousewand drew on each compositor (mid-stroke minus just before it), beside Hyprland's.</p>
"#,
    );
    let mut scene = "";
    for row in rows {
        if row.scene != scene {
            if !scene.is_empty() {
                out.push_str("</table>");
            }
            scene = &row.scene;
            let _ = write!(
                out,
                "<h2>{scene}</h2><table><tr><th>compositor</th><th>overlay</th><th>{REFERENCE}</th><th>screen</th><th>checks</th></tr>"
            );
        }
        let ok = row.checks.iter().all(|c| c.ok);
        let _ = write!(
            out,
            r#"<tr><td><b>{c}</b><br><span class="{class}">{verdict}</span></td>
<td><img class="overlay" src="{overlay}"></td><td><img class="overlay" src="{reference}"></td>
<td><img src="{mid}"></td><td><ul>"#,
            c = row.compositor,
            class = if ok { "good" } else { "bad" },
            verdict = if ok { "ok" } else { "failed" },
            overlay = link(format!("{}-{scene}-overlay.png", row.compositor)),
            reference = link(format!("{REFERENCE}-{scene}-overlay.png")),
            mid = link(format!("{}-{scene}-mid.png", row.compositor)),
        );
        for check in &row.checks {
            let _ = write!(
                out,
                r#"<li class="{}">{check}</li>"#,
                if check.ok { "good" } else { "bad" }
            );
        }
        out.push_str("</ul></td></tr>\n");
    }
    out.push_str("</table>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gray screen, with a square of `color` at (x, y) when given.
    fn screen(square: Option<(usize, usize, [u8; 3])>) -> Image {
        let (width, height) = (200, 100);
        let mut rgb = vec![60; width * height * 3];
        if let Some((x0, y0, color)) = square {
            for y in y0..y0 + 10 {
                for x in x0..x0 + 10 {
                    rgb[(y * width + x) * 3..][..3].copy_from_slice(&color);
                }
            }
        }
        Image { width, height, rgb }
    }

    fn checks(x: usize, color: [u8; 3]) -> Vec<(&'static str, bool)> {
        let reference = Overlay::new(&screen(None), &screen(Some((50, 20, [115, 191, 255])))).unwrap();
        let other = Overlay::new(&screen(None), &screen(Some((x, 20, color)))).unwrap();
        other
            .compare(&reference)
            .into_iter()
            .filter(|c| c.part == "trail")
            .map(|c| (c.name, c.ok))
            .collect()
    }

    #[test]
    fn the_same_drawing_matches() {
        assert!(checks(50, [115, 191, 255]).iter().all(|(_, ok)| *ok));
    }

    #[test]
    fn a_drawing_elsewhere_fails_its_place_and_shape() {
        let failed: Vec<_> = checks(140, [115, 191, 255]).into_iter().filter(|(_, ok)| !ok).collect();
        assert_eq!(failed, [("place", false), ("shape", false)]);
    }

    #[test]
    fn another_color_fails_only_the_color() {
        let failed: Vec<_> = checks(50, [255, 90, 90]).into_iter().filter(|(_, ok)| !ok).collect();
        assert_eq!(failed, [("color", false)]);
    }

    #[test]
    fn nothing_drawn_fails() {
        let reference = Overlay::new(&screen(None), &screen(Some((50, 20, [115, 191, 255])))).unwrap();
        let blank = Overlay::new(&screen(None), &screen(None)).unwrap();
        let drawn = blank.compare(&reference).into_iter().find(|c| c.part == "trail").unwrap();
        assert_eq!((drawn.name, drawn.ok), ("drawn", false));
    }

    #[test]
    fn growing_makes_squares() {
        let mut mask = vec![false; 25];
        mask[12] = true;
        assert_eq!(grow(&mask, 5, 5, 1).iter().filter(|&&v| v).count(), 9);
    }
}
