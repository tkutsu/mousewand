//! Config types and per-app gesture lookup.
//!
//! `config.toml` holds settings, the gestures every app gets unless it
//! overrides them, the `[global]` gestures no app overrides, and the `[hint]`
//! style. Each `apps/<name>.toml` holds the gestures for one app, matched by
//! window class, and each `shapes/<name>.toml` a shape taught by drawing it.
//! Parsing is pure: the caller reads the files and passes their text in.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, de};

use crate::keys::Combo;
use crate::shape::{self, Shape};
use crate::stroke::Stroke;

#[derive(Clone, Debug)]
pub struct Config {
    pub settings: Settings,
    /// Every app gets these, unless it has its own for the same input.
    pub gestures: Vec<Gesture>,
    pub apps: Vec<App>,
    /// The same in every app, never overridden. With `settings.global_trigger`
    /// they belong to that button alone.
    pub global: Vec<Gesture>,
    pub hint: HintStyle,
    /// Set by the caller from `themes/<settings.theme>.toml`.
    pub theme: Theme,
    /// The built-in shapes and the taught ones.
    pub shapes: Vec<Shape>,
    /// What opens the settings app: a stroke or shape, a circle by default.
    pub open_settings: Input,
}

/// How the trail looks. Every field is optional.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    pub color: Gradient,
    /// The trail's color when the stroke matched nothing.
    #[serde(deserialize_with = "parsed")]
    pub rejected: Color,
    /// Width at the head, glow included, in logical pixels.
    pub width: f32,
    /// How long each point of the trail stays visible.
    pub life_ms: u32,
    /// How long the trail fades after release.
    pub fade_ms: u32,
    /// Glow strength, from 0 (a plain line) to about 1.
    pub glow: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            color: Gradient(vec![Color([0.45, 0.75, 1.0])]),
            rejected: Color([1.0, 0.35, 0.35]),
            width: 26.0,
            life_ms: 450,
            fade_ms: 300,
            glow: 0.6,
        }
    }
}

impl Theme {
    /// Parses `themes/<name>.toml`.
    pub fn parse(name: &str, text: &str) -> Result<Theme, String> {
        let file = format!("themes/{name}.toml");
        let theme: Theme = toml::from_str(text).map_err(|e| toml_error(&file, text, &e))?;
        if theme.width <= 0.0 || theme.life_ms == 0 {
            return Err(format!("{file}: width and life_ms must be more than 0"));
        }
        Ok(theme)
    }
}

/// An sRGB color written `"#rrggbb"`, stored as 0–1 floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub [f32; 3]);

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.is_ascii());
        let channel = |i: usize| {
            let h = hex?;
            u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f32 / 255.0)
        };
        match (channel(0), channel(2), channel(4)) {
            (Some(r), Some(g), Some(b)) => Ok(Color([r, g, b])),
            _ => Err(format!("bad color {s:?}: write it as \"#rrggbb\"")),
        }
    }
}

/// The trail's colors from head to tail, written as one `"#rrggbb"` or a
/// list of them. The trail blends evenly from one to the next.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient(pub Vec<Color>);

impl<'de> Deserialize<'de> for Gradient {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bad = || de::Error::custom("write color as \"#rrggbb\" or a list of them");
        let stops: Vec<String> = match toml::Value::deserialize(d)? {
            toml::Value::String(s) => vec![s],
            toml::Value::Array(list) => list
                .into_iter()
                .map(|v| v.as_str().map(String::from))
                .collect::<Option<_>>()
                .ok_or_else(bad)?,
            _ => return Err(bad()),
        };
        if stops.is_empty() {
            return Err(bad());
        }
        let colors = stops.iter().map(|s| s.parse()).collect::<Result<_, String>>();
        colors.map(Gradient).map_err(de::Error::custom)
    }
}

/// An sRGB color with alpha, written `"#rrggbb"` or `"#rrggbbaa"`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub [f32; 4]);

impl FromStr for Rgba {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("bad color {s:?}: write it as \"#rrggbb\" or \"#rrggbbaa\"");
        let hex = s.strip_prefix('#').filter(|h| h.is_ascii()).ok_or_else(bad)?;
        let alpha = match hex.len() {
            6 => 1.0,
            8 => u8::from_str_radix(&hex[6..], 16).map_err(|_| bad())? as f32 / 255.0,
            _ => return Err(bad()),
        };
        let Color([r, g, b]) = format!("#{}", &hex[..6]).parse::<Color>().map_err(|_| bad())?;
        Ok(Rgba([r, g, b, alpha]))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// The mouse button that starts a gesture.
    pub trigger: Trigger,
    /// Motion before a press turns into a gesture, in device units (≈ px).
    pub threshold: f64,
    /// Holding the trigger this long without moving makes it a normal press.
    pub hold_ms: u64,
    /// Shorter direction runs are treated as noise.
    pub min_segment: f64,
    /// Only grab mice whose name contains one of these. Empty grabs all mice.
    pub devices: Vec<String>,
    /// The trail theme: a file in `themes/` without `.toml`, or "default".
    pub theme: String,
    /// The button for the global gestures alone, from `[global]`.
    #[serde(skip)]
    pub global_trigger: Option<Trigger>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            trigger: Trigger::default(),
            threshold: 8.0,
            hold_ms: 300,
            min_segment: 30.0,
            devices: Vec::new(),
            theme: "default".into(),
            global_trigger: None,
        }
    }
}

/// A mouse button usable as the trigger. Left and right are excluded:
/// grabbing them would break normal clicking.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Trigger(pub u16);

impl Default for Trigger {
    fn default() -> Self {
        Trigger(BTN_MIDDLE)
    }
}

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;

/// The buttons a trigger can be, by the names browsers and games use: mouse 4
/// is the rear thumb button, mouse 5 the front one. Rarer codes are numbered on.
const BUTTONS: [(&str, u16); 6] = [
    ("middle", BTN_MIDDLE),
    ("back", 0x113),    // BTN_SIDE
    ("forward", 0x114), // BTN_EXTRA
    ("mouse6", 0x115),  // BTN_FORWARD
    ("mouse7", 0x116),  // BTN_BACK
    ("mouse8", 0x117),  // BTN_TASK
];

impl FromStr for Trigger {
    type Err = String;

    /// A name from [`BUTTONS`], "mouse 3" to "mouse 5", or evdev's "side"
    /// and "extra", as tools like evtest show them.
    fn from_str(s: &str) -> Result<Self, String> {
        let name = s.to_ascii_lowercase().replace(' ', "");
        let name = match name.as_str() {
            "mouse3" => "middle",
            "mouse4" | "side" => "back",
            "mouse5" | "extra" => "forward",
            other => other,
        };
        match BUTTONS.iter().find(|(n, _)| *n == name) {
            Some(&(_, code)) => Ok(Trigger(code)),
            None if name == "left" || name == "right" => {
                Err(format!("{s:?} can't be the trigger: it would stop normal clicks"))
            }
            None => Err(format!(
                "unknown button {s:?}: use middle, back (mouse 4), forward (mouse 5), mouse6, mouse7 \
                 or mouse8, or let the settings app detect it"
            )),
        }
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match BUTTONS.iter().find(|(_, code)| *code == self.0) {
            Some((name, _)) => f.write_str(name),
            None => write!(f, "button {:#x}", self.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawGesture")]
pub struct Gesture {
    pub input: Input,
    pub kind: Kind,
    pub action: Action,
    /// What the hint calls it, e.g. "Back".
    pub name: Option<String>,
}

impl Gesture {
    /// Its name, else what it does: the keys, command or dispatcher.
    pub fn label(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        let text = match &self.action {
            Action::Keys(combo) => combo.to_string(),
            Action::Command(command) | Action::Dispatch(command) => command.clone(),
        };
        // Long commands shorten to their first 40 characters.
        match text.char_indices().nth(40) {
            Some((end, _)) => format!("{}…", &text[..end]),
            None => text,
        }
    }
}

/// What the user does while holding the trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Stroke(Stroke),
    /// A drawing that looks like this shape, matched on release.
    Shape(Shape),
    /// One wheel notch.
    Wheel(Wheel),
    /// A click of another button, left or right.
    Click(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wheel {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Fires once, on release.
    Oneshot,
    /// Fires as soon as the stroke matches, then again every `step` of
    /// motion in the stroke's last direction and on every wheel notch.
    Repeat { step: f64 },
    /// Like `Repeat`, with `keys` held down from the match until release:
    /// holding alt and tapping tab is a real alt-tab switcher. Written as
    /// `hold` in the action.
    Hold { keys: Combo, step: f64 },
}

impl Kind {
    pub fn continuous(&self) -> bool {
        !matches!(self, Kind::Oneshot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A key combo sent to the window the gesture started on.
    Keys(Combo),
    /// A shell command.
    Command(String),
    /// A compositor dispatch, e.g. Hyprland's `workspace +1`.
    Dispatch(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGesture {
    stroke: Option<String>,
    shape: Option<String>,
    wheel: Option<String>,
    click: Option<String>,
    repeat: Option<bool>,
    step: Option<f64>,
    action: RawAction,
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    keys: Option<String>,
    command: Option<String>,
    dispatch: Option<String>,
    /// Keys held down from the match until the trigger is let go.
    hold: Option<String>,
}

const DEFAULT_STEP: f64 = 80.0;

impl TryFrom<RawGesture> for Gesture {
    type Error = String;

    fn try_from(raw: RawGesture) -> Result<Gesture, String> {
        let input = match (raw.stroke, raw.shape, raw.wheel, raw.click) {
            (Some(stroke), None, None, None) => Input::Stroke(stroke.parse()?),
            // Checked against the shapes there are once every file is read.
            (None, Some(name), None, None) => Input::Shape(Shape::unresolved(&name)),
            (None, None, Some(wheel), None) => Input::Wheel(match wheel.to_ascii_lowercase().as_str() {
                "up" => Wheel::Up,
                "down" => Wheel::Down,
                "left" => Wheel::Left,
                "right" => Wheel::Right,
                _ => return Err(format!("unknown wheel {wheel:?}: use up, down, left or right")),
            }),
            (None, None, None, Some(button)) => Input::Click(match button.to_ascii_lowercase().as_str() {
                "left" => BTN_LEFT,
                "right" => BTN_RIGHT,
                _ => return Err(format!("unknown click {button:?}: use left or right")),
            }),
            _ => return Err("a gesture needs exactly one of stroke, shape, wheel or click".into()),
        };
        let RawAction {
            keys,
            command,
            dispatch,
            hold,
        } = raw.action;
        let action = match (keys, command, dispatch) {
            (Some(keys), None, None) => Action::Keys(keys.parse()?),
            (None, Some(command), None) => Action::Command(command),
            (None, None, Some(dispatch)) => Action::Dispatch(dispatch),
            _ => return Err("an action needs exactly one of keys, command or dispatch".into()),
        };
        if hold.is_some() && !matches!(action, Action::Keys(_)) {
            return Err("`hold` only goes with keys".into());
        }
        let step = raw.step.unwrap_or(DEFAULT_STEP);
        if step <= 0.0 {
            return Err(format!("step must be positive, not {step}"));
        }
        // Holding keys only makes sense from the match on, so it repeats.
        let kind = match (hold, raw.repeat) {
            (Some(keys), None | Some(true)) => Kind::Hold {
                keys: keys.parse()?,
                step,
            },
            (Some(_), Some(false)) => return Err("a gesture that holds keys always repeats".into()),
            (None, Some(true)) => Kind::Repeat { step },
            (None, _) => Kind::Oneshot,
        };
        if kind.continuous() && !matches!(input, Input::Stroke(_)) {
            return Err("only strokes can repeat or hold keys".into());
        }
        if !kind.continuous() && raw.step.is_some() {
            return Err("`step` only goes with repeat = true or hold".into());
        }
        Ok(Gesture {
            input,
            kind,
            action,
            name: raw.name.filter(|n| !n.trim().is_empty()),
        })
    }
}

#[derive(Clone, Debug)]
pub struct App {
    /// Window classes this app matches, compared case-insensitively.
    pub classes: Vec<String>,
    pub gestures: Vec<Gesture>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MainFile {
    #[serde(flatten)]
    settings: Settings,
    #[serde(default, rename = "gesture")]
    gestures: Vec<Gesture>,
    #[serde(default)]
    global: GlobalFile,
    #[serde(default)]
    hint: HintStyle,
    open_settings: Option<RawOpen>,
}

/// `open_settings = { shape = "circle" }`, or a `stroke`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOpen {
    stroke: Option<String>,
    shape: Option<String>,
}

impl RawOpen {
    fn input(self, shapes: &[Shape]) -> Result<Input, String> {
        let mut input = match (self.stroke, self.shape) {
            (Some(stroke), None) => Input::Stroke(stroke.parse()?),
            (None, Some(shape)) => Input::Shape(Shape::unresolved(&shape)),
            _ => return Err("open_settings needs one stroke or one shape".into()),
        };
        resolve_input(&mut input, shapes)?;
        Ok(input)
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalFile {
    trigger: Option<Trigger>,
    #[serde(default, rename = "gesture")]
    gestures: Vec<Gesture>,
}

/// The hint: the stroke so far as arrows, what it could still become, and
/// what fired. Every field is optional; it's off unless `enabled`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HintStyle {
    pub enabled: bool,
    /// List the gestures the stroke could still become.
    pub next: bool,
    #[serde(deserialize_with = "parsed")]
    pub position: Position,
    /// Distance from the screen edge, in logical px.
    pub margin: f32,
    /// A font family, e.g. "Inter"; "sans-serif" picks the system's.
    pub font: String,
    /// Text size, in logical px. Arrows match it.
    pub size: f32,
    #[serde(deserialize_with = "parsed")]
    pub text: Rgba,
    /// The arrows.
    #[serde(deserialize_with = "parsed")]
    pub glyph: Rgba,
    #[serde(deserialize_with = "parsed")]
    pub background: Rgba,
    /// Of the whole hint, 0 to 1.
    pub opacity: f32,
    pub padding: f32,
    pub radius: f32,
}

impl Default for HintStyle {
    fn default() -> Self {
        HintStyle {
            enabled: false,
            next: true,
            position: Position::default(),
            margin: 64.0,
            font: "sans-serif".into(),
            size: 17.0,
            text: Rgba([0.95, 0.95, 0.97, 1.0]),
            glyph: Rgba([0.62, 0.77, 1.0, 1.0]),
            background: Rgba([0.11, 0.12, 0.15, 1.0]),
            opacity: 0.9,
            padding: 14.0,
            radius: 14.0,
        }
    }
}

/// Where on the screen the hint sits, written like "bottom center".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub vertical: Align,
    pub horizontal: Align,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

impl Default for Position {
    fn default() -> Self {
        Position {
            vertical: Align::End,
            horizontal: Align::Center,
        }
    }
}

impl FromStr for Position {
    type Err = String;

    /// One or two words in any order: top, bottom, left, right and center.
    /// A missing one is center: "top" is "top center".
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("bad position {s:?}: write it like \"bottom center\" or \"top right\"");
        let words: Vec<String> = s.split_whitespace().map(str::to_ascii_lowercase).collect();
        if words.is_empty() || words.len() > 2 {
            return Err(bad());
        }
        let (mut vertical, mut horizontal) = (None, None);
        let mut centers = 0;
        for word in &words {
            match word.as_str() {
                "top" if vertical.is_none() => vertical = Some(Align::Start),
                "bottom" if vertical.is_none() => vertical = Some(Align::End),
                "left" if horizontal.is_none() => horizontal = Some(Align::Start),
                "right" if horizontal.is_none() => horizontal = Some(Align::End),
                "center" | "middle" => centers += 1,
                _ => return Err(bad()),
            }
        }
        if centers > 2 - vertical.iter().count() - horizontal.iter().count() {
            return Err(bad());
        }
        Ok(Position {
            vertical: vertical.unwrap_or(Align::Center),
            horizontal: horizontal.unwrap_or(Align::Center),
        })
    }
}

impl HintStyle {
    fn check(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.opacity) {
            return Err(format!("hint opacity must be from 0 to 1, not {}", self.opacity));
        }
        if self.size <= 0.0 {
            return Err("hint size must be positive".into());
        }
        if self.margin < 0.0 || self.padding < 0.0 || self.radius < 0.0 {
            return Err("hint margin, padding and radius can't be negative".into());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppFile {
    #[serde(default, rename = "match")]
    classes: Vec<String>,
    #[serde(default, rename = "gesture")]
    gestures: Vec<Gesture>,
}

impl Config {
    /// Parses `config.toml`, the app files and the shape files, each given
    /// as `(file stem, text)`. An app file without `match` matches the
    /// class named by its stem.
    pub fn parse(main: &str, apps: &[(String, String)], shapes: &[(String, String)]) -> Result<Config, String> {
        let shapes = shapes_of(shapes)?;
        let mut main: MainFile = toml::from_str(main).map_err(|e| toml_error("config.toml", main, &e))?;
        let in_main = |e: String| format!("config.toml: {e}");
        let open_settings = match main.open_settings.take() {
            Some(raw) => raw.input(&shapes).map_err(in_main)?,
            None => Input::Shape(shapes.iter().find(|s| s.name == "circle").cloned().expect("built in")),
        };
        main.hint.check().map_err(in_main)?;
        resolve(&mut main.gestures, &shapes).map_err(in_main)?;
        resolve(&mut main.global.gestures, &shapes).map_err(in_main)?;
        let shared = match main.global.trigger {
            Some(trigger) if trigger == main.settings.trigger => {
                return Err(in_main(
                    "[global] has the same trigger as the other gestures; leave it out to share it".into(),
                ));
            }
            Some(_) => false,
            None => true,
        };
        main.settings.global_trigger = main.global.trigger;
        let global = main.global.gestures;
        if shared {
            check_global(&global, &main.gestures).map_err(in_main)?;
            check_prefixes(&[global.as_slice(), &main.gestures].concat()).map_err(in_main)?;
        } else {
            check_prefixes(&global).map_err(in_main)?;
            check_prefixes(&main.gestures).map_err(in_main)?;
        }
        let apps = apps
            .iter()
            .map(|(stem, text)| {
                let mut file: AppFile =
                    toml::from_str(text).map_err(|e| toml_error(&format!("apps/{stem}.toml"), text, &e))?;
                resolve(&mut file.gestures, &shapes).map_err(|e| format!("apps/{stem}.toml: {e}"))?;
                let classes = if file.classes.is_empty() {
                    vec![stem.clone()]
                } else {
                    file.classes
                };
                let app = App {
                    classes,
                    gestures: file.gestures,
                };
                let in_app = |e: String| format!("apps/{stem}.toml: {e}");
                let mut gestures = merged(&app.gestures, &main.gestures);
                if shared {
                    check_global(&global, &app.gestures).map_err(in_app)?;
                    gestures.extend(global.iter().cloned());
                }
                check_prefixes(&gestures).map_err(in_app)?;
                Ok(app)
            })
            .collect::<Result<_, String>>()?;
        Ok(Config {
            settings: main.settings,
            gestures: main.gestures,
            apps,
            global,
            hint: main.hint,
            theme: Theme::default(),
            shapes,
            open_settings,
        })
    }

    /// The gestures for the trigger over a window of `class`: the global
    /// ones unless they have their own trigger, its app's, the shared ones
    /// the app doesn't override, then the one that opens the settings app,
    /// if nothing else uses its input and this isn't the settings app.
    pub fn gestures_for(&self, class: &str) -> Vec<Gesture> {
        let app = self
            .apps
            .iter()
            .find(|app| app.classes.iter().any(|c| c.eq_ignore_ascii_case(class)));
        let mut gestures = match self.settings.global_trigger {
            Some(_) => Vec::new(),
            None => self.global.clone(),
        };
        gestures.extend(merged(app.map_or(&[], |a| &a.gestures), &self.gestures));
        let settings = Gesture {
            input: self.open_settings.clone(),
            kind: Kind::Oneshot,
            action: Action::Command("mousewand-settings".into()),
            name: Some("mousewand settings".into()),
        };
        let in_settings = SETTINGS_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(class));
        if !in_settings && !gestures.iter().any(|g| g.input == settings.input) {
            gestures.push(settings);
        }
        gestures
    }

    /// The gestures for the global trigger, if `[global]` has one.
    pub fn global_gestures(&self) -> Vec<Gesture> {
        self.global.clone()
    }
}

/// The settings app's window classes: its binary's name, and its app id.
pub const SETTINGS_CLASSES: [&str; 2] = ["mousewand-settings", "io.github.tkutsu.mousewand"];

/// A TOML error as one line, like "config.toml, line 4: there's no `colour`
/// here", instead of the parser's excerpt with carets under it.
pub fn toml_error(file: &str, text: &str, e: &toml::de::Error) -> String {
    // Errors under `flatten` come with an empty span at the start: no line.
    let at = match e.span() {
        Some(span) if span.end > 0 => {
            format!(
                ", line {}",
                text[..span.start.min(text.len())].matches('\n').count() + 1
            )
        }
        _ => String::new(),
    };
    format!("{file}{at}: {}", plain(e.message()))
}

/// Serde's wording, in plainer words.
fn plain(message: &str) -> String {
    let quoted = |s: &str| s.split('`').nth(1).map(str::to_owned);
    if let Some(rest) = message.strip_prefix("unknown field ") {
        let (field, expected) = rest.split_once(", expected ").unwrap_or((rest, ""));
        let field = quoted(field).unwrap_or_default();
        let options: Vec<String> = expected
            .replace("one of ", "")
            .replace('`', "")
            .split(", ")
            .filter(|o| !o.is_empty())
            .map(str::to_owned)
            .collect();
        let options = match options.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
            _ => options.concat(),
        };
        return match options.is_empty() {
            true => format!("there's no `{field}` here"),
            false => format!("there's no `{field}` here; it can be {options}"),
        };
    }
    if let Some(field) = message.strip_prefix("missing field ").and_then(quoted) {
        return format!("`{field}` is missing");
    }
    if let Some(rest) = message.strip_prefix("invalid type: ") {
        let (found, expected) = rest.split_once(", expected ").unwrap_or((rest, "something else"));
        let kind = |what: &str| match what.split_whitespace().next().unwrap_or(what) {
            "f32" | "f64" | "u8" | "u16" | "u32" | "u64" | "i32" | "i64" | "integer" | "float" => "a number",
            "a" if what.contains("string") => "text in quotes",
            "string" => "text in quotes",
            "boolean" | "bool" => "true or false",
            "a" if what.contains("sequence") => "a list in [ ]",
            "sequence" => "a list in [ ]",
            "map" | "struct" => "a table",
            _ => "something else",
        };
        return format!("this should be {}, not {}", kind(expected), kind(found));
    }
    message.trim().to_owned()
}

/// The built-in shapes no file replaces, then the taught ones.
fn shapes_of(files: &[(String, String)]) -> Result<Vec<Shape>, String> {
    let taught = files
        .iter()
        .map(|(name, text)| Shape::parse(name, text))
        .collect::<Result<Vec<_>, _>>()?;
    let mut shapes: Vec<Shape> = shape::BUILT_IN
        .into_iter()
        .filter(|&name| !taught.iter().any(|s| s.name == name))
        .filter_map(Shape::built_in)
        .collect();
    shapes.extend(taught);
    Ok(shapes)
}

/// Swaps each gesture's shape name for the shape.
fn resolve(gestures: &mut [Gesture], shapes: &[Shape]) -> Result<(), String> {
    gestures
        .iter_mut()
        .try_for_each(|g| resolve_input(&mut g.input, shapes))
}

fn resolve_input(input: &mut Input, shapes: &[Shape]) -> Result<(), String> {
    if let Input::Shape(named) = input {
        *named = shapes.iter().find(|s| *s == named).cloned().ok_or_else(|| {
            format!(
                "there's no shape called \"{}\": use circle, or draw it in the settings app",
                named.name
            )
        })?;
    }
    Ok(())
}

fn merged(app: &[Gesture], global: &[Gesture]) -> Vec<Gesture> {
    let overridden = |g: &Gesture| app.iter().any(|a| a.input == g.input);
    app.iter()
        .chain(global.iter().filter(|g| !overridden(g)))
        .cloned()
        .collect()
}

/// Global gestures sharing the trigger can't be overridden, so nothing else
/// may use their input.
fn check_global(global: &[Gesture], others: &[Gesture]) -> Result<(), String> {
    match others.iter().find(|o| global.iter().any(|g| g.input == o.input)) {
        Some(taken) => Err(format!(
            "{} is already a [global] gesture, which no app overrides",
            describe(&taken.input)
        )),
        None => Ok(()),
    }
}

fn describe(input: &Input) -> String {
    match input {
        Input::Stroke(stroke) => format!("stroke {stroke}"),
        Input::Shape(shape) => format!("shape {}", shape.name),
        Input::Wheel(wheel) => format!("wheel {wheel:?}").to_lowercase(),
        Input::Click(BTN_LEFT) => "click left".into(),
        Input::Click(_) => "click right".into(),
    }
}

/// A repeat or hold gesture fires as soon as its stroke matches, so a longer
/// stroke that starts with it could never be drawn.
fn check_prefixes(gestures: &[Gesture]) -> Result<(), String> {
    for short in gestures.iter().filter(|g| g.kind.continuous()) {
        let Input::Stroke(prefix) = &short.input else { continue };
        for long in gestures {
            if let Input::Stroke(stroke) = &long.input
                && stroke.0.len() > prefix.0.len()
                && stroke.0.starts_with(&prefix.0)
            {
                return Err(format!(
                    "{prefix} fires as soon as it's drawn, so {stroke}, which starts with it, never could"
                ));
            }
        }
    }
    Ok(())
}

/// Deserializes a string field through `FromStr`.
fn parsed<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr<Err = String>,
{
    String::deserialize(d)?.parse().map_err(de::Error::custom)
}

impl<'de> Deserialize<'de> for Trigger {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        parsed(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = r#"
        trigger = "side"
        threshold = 10

        [[gesture]]
        stroke = "L"
        action = { dispatch = "workspace -1" }

        [[gesture]]
        stroke = "DR"
        action = { command = "notify-send hi" }

        [[gesture]]
        wheel = "up"
        action = { keys = "ctrl+shift+Tab" }
    "#;

    const FIREFOX: &str = r#"
        match = ["firefox", "librewolf"]

        [[gesture]]
        stroke = "L"
        action = { keys = "alt+Left" }

        [[gesture]]
        stroke = "U"
        repeat = true
        step = 60
        action = { keys = "ctrl+Tab" }

        [[gesture]]
        click = "left"
        action = { keys = "ctrl+w" }
    "#;

    fn config() -> Config {
        Config::parse(
            MAIN,
            &[("firefox".into(), FIREFOX.into()), ("kitty".into(), String::new())],
            &[],
        )
        .unwrap()
    }

    fn stroke(s: &str) -> Input {
        Input::Stroke(s.parse().unwrap())
    }

    fn action_for(c: &Config, class: &str, input: Input) -> Option<Action> {
        c.gestures_for(class)
            .into_iter()
            .find(|g| g.input == input)
            .map(|g| g.action)
    }

    #[test]
    fn settings() {
        let c = config();
        assert_eq!(c.settings.trigger, Trigger(0x113));
        assert_eq!(c.settings.threshold, 10.0);
        assert_eq!(c.settings.hold_ms, 300);
        assert_eq!(c.settings.theme, "default");
    }

    #[test]
    fn app_overrides_global() {
        let c = config();
        assert_eq!(
            action_for(&c, "LibreWolf", stroke("L")),
            Some(Action::Keys("alt+Left".parse().unwrap()))
        );
        assert_eq!(
            action_for(&c, "kitty", stroke("L")),
            Some(Action::Dispatch("workspace -1".into()))
        );
        assert_eq!(
            action_for(&c, "kitty", stroke("DR")),
            Some(Action::Command("notify-send hi".into()))
        );
        assert_eq!(action_for(&c, "kitty", stroke("U")), None);
        assert_eq!(
            action_for(&c, "firefox", Input::Wheel(Wheel::Up)),
            Some(Action::Keys("ctrl+shift+Tab".parse().unwrap()))
        );
        assert_eq!(
            action_for(&c, "firefox", Input::Click(BTN_LEFT)),
            Some(Action::Keys("ctrl+w".parse().unwrap()))
        );
    }

    #[test]
    fn kinds() {
        let gestures = config().gestures_for("firefox");
        let repeat = gestures.iter().find(|g| g.input == stroke("U")).unwrap();
        assert_eq!(repeat.kind, Kind::Repeat { step: 60.0 });

        let hold = r#"
            [[gesture]]
            stroke = "R"
            action = { keys = "Tab", hold = "alt" }
        "#;
        let c = Config::parse(hold, &[], &[]).unwrap();
        assert_eq!(
            c.gestures[0].kind,
            Kind::Hold {
                keys: "alt".parse().unwrap(),
                step: 80.0
            }
        );
    }

    #[test]
    fn settings_gesture_is_a_circle_unless_taken() {
        let circle = |c: &Config| Input::Shape(c.shapes.iter().find(|s| s.name == "circle").unwrap().clone());
        let opens_settings = |c: &Config, class: &str| {
            c.gestures_for(class)
                .iter()
                .any(|g| g.input == circle(c) && g.action == Action::Command("mousewand-settings".into()))
        };
        assert!(opens_settings(&config(), "kitty"));
        let taken = "[[gesture]]\nshape = \"circle\"\naction = { keys = \"a\" }";
        let c = Config::parse("", &[("kitty".into(), taken.into())], &[]).unwrap();
        assert!(!opens_settings(&c, "kitty"));
        assert!(opens_settings(&c, "firefox"));
    }

    #[test]
    fn errors_are_one_plain_line() {
        let bad = Config::parse("trigger = \"back\"\ncolour = \"red\"", &[], &[]).unwrap_err();
        assert_eq!(bad, "config.toml: there's no `colour` here");
        let app = "match = [\"x\"]\n\n[[gesture]]\nstrok = \"L\"\naction = { keys = \"a\" }";
        let bad = Config::parse("", &[("x".into(), app.into())], &[]).unwrap_err();
        assert!(
            bad.starts_with("apps/x.toml, line 4: there's no `strok` here; it can be stroke, shape"),
            "{bad}"
        );
        assert!(bad.ends_with("action or name"), "{bad}");
        assert!(!bad.contains('\n') && !bad.contains('^'), "{bad}");
        let bad = Config::parse("[[gesture]]\nstroke = \"L\"", &[], &[]).unwrap_err();
        assert!(bad.contains("`action` is missing"), "{bad}");
        let bad = Config::parse("threshold = \"big\"", &[], &[]).unwrap_err();
        assert_eq!(bad, "config.toml: this should be a number, not text in quotes");
        let bad = Theme::parse("ember", "width = 0").unwrap_err();
        assert_eq!(bad, "themes/ember.toml: width and life_ms must be more than 0");
    }

    #[test]
    fn open_settings_is_a_circle_by_default_and_never_in_settings() {
        let opens = |c: &Config, class: &str| {
            c.gestures_for(class)
                .iter()
                .find(|g| g.action == Action::Command("mousewand-settings".into()))
                .map(|g| g.input.clone())
        };
        let c = config();
        assert!(matches!(opens(&c, "kitty"), Some(Input::Shape(s)) if s.name == "circle"));
        assert_eq!(opens(&c, "mousewand-settings"), None);
        assert_eq!(opens(&c, "io.github.tkutsu.mousewand"), None);

        let c = Config::parse("open_settings = { stroke = \"U39D\" }", &[], &[]).unwrap();
        assert_eq!(opens(&c, "kitty"), Some(stroke("U39D")));
        let bad = Config::parse("open_settings = { shape = \"star\" }", &[], &[]).unwrap_err();
        assert!(bad.contains("no shape called"), "{bad}");
        assert!(
            Config::parse("open_settings = {}", &[], &[])
                .unwrap_err()
                .contains("one stroke or one shape")
        );
    }

    #[test]
    fn shapes_are_built_in_or_taught() {
        let gesture = |shape: &str| format!("[[gesture]]\nshape = \"{shape}\"\naction = {{ keys = \"a\" }}");
        let triangle = (
            "triangle".to_string(),
            "examples = [[[0, 0], [100, 0], [50, 90], [0, 0]]]".to_string(),
        );
        let c = Config::parse(&gesture("triangle"), &[], std::slice::from_ref(&triangle)).unwrap();
        let names: Vec<&str> = c.shapes.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["circle", "triangle"]);
        assert!(matches!(&c.gestures[0].input, Input::Shape(s) if s.name == "triangle"));

        let bad = Config::parse(&gesture("star"), &[], &[]).unwrap_err();
        assert!(
            bad.contains("config.toml") && bad.contains("no shape called \"star\""),
            "{bad}"
        );
        let bad = Config::parse("", &[("x".into(), gesture("star"))], &[triangle]).unwrap_err();
        assert!(bad.contains("apps/x.toml"), "{bad}");
        let repeat = Config::parse(&gesture("circle").replace("action", "repeat = true\naction"), &[], &[]);
        assert!(repeat.unwrap_err().contains("only strokes"));
    }

    #[test]
    fn stem_is_the_default_match() {
        assert_eq!(config().apps[1].classes, ["kitty"]);
    }

    #[test]
    fn errors_name_the_file() {
        let bad = Config::parse("trigger = \"left\"", &[], &[]).unwrap_err();
        assert!(bad.contains("config.toml") && bad.contains("normal clicks"), "{bad}");
        let bad = Config::parse(
            "",
            &[(
                "x".into(),
                "[[gesture]]\nstroke = \"L\"\naction = { keys = \"ctrl+wat\" }".into(),
            )],
            &[],
        )
        .unwrap_err();
        assert!(bad.contains("apps/x.toml") && bad.contains("wat"), "{bad}");
        assert!(Config::parse("thresold = 3", &[], &[]).is_err());
    }

    #[test]
    fn invalid_gestures() {
        let bad =
            |g: &str| Config::parse(&format!("[[gesture]]\n{g}\naction = {{ keys = \"a\" }}"), &[], &[]).unwrap_err();
        assert!(bad("").contains("exactly one"));
        assert!(bad("stroke = \"L\"\nwheel = \"up\"").contains("exactly one"));
        assert!(bad("wheel = \"up\"\nrepeat = true").contains("only strokes"));
        assert!(bad("stroke = \"L\"\nstep = 5").contains("only goes with"));
        assert!(bad("click = \"middle\"").contains("unknown click"));
        // The old names are gone.
        assert!(bad("stroke = \"L\"\nkind = \"repeat\"").contains("there's no"));
        assert!(bad("rocker = \"left\"").contains("there's no"));

        let action = |a: &str| Config::parse(&format!("[[gesture]]\nstroke = \"L\"\naction = {a}"), &[], &[]);
        assert!(
            action("{ keys = \"a\", command = \"b\" }")
                .unwrap_err()
                .contains("exactly one")
        );
        assert!(
            action("{ command = \"b\", hold = \"alt\" }")
                .unwrap_err()
                .contains("only goes with keys")
        );
        assert!(action("{ keys = \"a\", hold = \"alt\" }").is_ok());
        let no_repeat = "[[gesture]]\nstroke = \"L\"\nrepeat = false\naction = { keys = \"a\", hold = \"alt\" }";
        assert!(
            Config::parse(no_repeat, &[], &[])
                .unwrap_err()
                .contains("always repeats")
        );
    }

    #[test]
    fn button_names() {
        let code = |s: &str| s.parse::<Trigger>().map(|t| t.0);
        assert_eq!(code("back"), Ok(0x113));
        assert_eq!(code("Mouse 4"), Ok(0x113));
        assert_eq!(code("side"), Ok(0x113));
        assert_eq!(code("forward"), Ok(0x114));
        assert_eq!(code("extra"), Ok(0x114));
        assert_eq!(code("mouse3"), Ok(0x112));
        assert_eq!(Trigger(0x113).to_string(), "back");
        assert_eq!(Trigger(0x117).to_string(), "mouse8");
        assert!(code("right").unwrap_err().contains("normal clicks"));
        assert!(code("thumb").unwrap_err().contains("detect"));
    }

    #[test]
    fn themes() {
        let theme = Theme::parse("x", "color = \"#ff8000\"\nwidth = 40").unwrap();
        assert_eq!(theme.color, Gradient(vec![Color([1.0, 128.0 / 255.0, 0.0])]));
        assert_eq!(theme.width, 40.0);
        assert_eq!(theme.life_ms, Theme::default().life_ms);
        let rainbow = Theme::parse("x", "color = [\"#ff0000\", \"#0000ff\"]").unwrap();
        assert_eq!(
            rainbow.color,
            Gradient(vec![Color([1.0, 0.0, 0.0]), Color([0.0, 0.0, 1.0])])
        );
        assert!(Theme::parse("x", "color = \"orange\"").unwrap_err().contains("#rrggbb"));
        assert!(
            Theme::parse("x", "color = [\"#ff0000\", \"red\"]")
                .unwrap_err()
                .contains("#rrggbb")
        );
        assert!(Theme::parse("x", "color = []").unwrap_err().contains("list"));
        assert!(Theme::parse("x", "color = 3").unwrap_err().contains("list"));
        for (name, text) in [
            ("ember", include_str!("../../../examples/themes/ember.toml")),
            ("fire", include_str!("../../../examples/themes/fire.toml")),
            ("laser", include_str!("../../../examples/themes/laser.toml")),
            ("mint", include_str!("../../../examples/themes/mint.toml")),
            ("rainbow", include_str!("../../../examples/themes/rainbow.toml")),
            ("sakura", include_str!("../../../examples/themes/sakura.toml")),
            ("sunset", include_str!("../../../examples/themes/sunset.toml")),
        ] {
            Theme::parse(name, text).unwrap();
        }
        assert!(Theme::parse("x", "width = 0").is_err());
        assert!(Theme::parse("x", "colour = \"#ffffff\"").is_err());
    }

    #[test]
    fn global_gestures_share_the_trigger_and_win() {
        let main = r#"
            [[gesture]]
            stroke = "L"
            action = { keys = "a" }

            [[global.gesture]]
            stroke = "R"
            name = "Next workspace"
            action = { dispatch = "workspace +1" }
        "#;
        let c = Config::parse(main, &[], &[]).unwrap();
        assert_eq!(c.settings.global_trigger, None);
        let gestures = c.gestures_for("firefox");
        assert_eq!(gestures[0].label(), "Next workspace");
        assert!(gestures.iter().any(|g| g.input == stroke("L")));
        assert!(c.global_gestures().len() == 1);

        let clash = "[[gesture]]\nstroke = \"R\"\naction = { keys = \"b\" }";
        let bad = Config::parse(main, &[("kitty".into(), clash.into())], &[]).unwrap_err();
        assert!(bad.contains("apps/kitty.toml") && bad.contains("[global]"), "{bad}");
    }

    #[test]
    fn global_gestures_can_have_their_own_trigger() {
        let main = r#"
            [global]
            trigger = "extra"

            [[global.gesture]]
            stroke = "R"
            action = { dispatch = "workspace +1" }

            [[gesture]]
            stroke = "R"
            action = { keys = "alt+Right" }
        "#;
        let c = Config::parse(main, &[], &[]).unwrap();
        assert_eq!(c.settings.global_trigger, Some(Trigger(0x114)));
        // The same stroke is free for the other trigger.
        assert_eq!(
            action_for(&c, "firefox", stroke("R")),
            Some(Action::Keys("alt+Right".parse().unwrap()))
        );
        assert_eq!(c.global_gestures()[0].action, Action::Dispatch("workspace +1".into()));
        let same = Config::parse("[global]\ntrigger = \"middle\"", &[], &[]).unwrap_err();
        assert!(same.contains("same trigger"), "{same}");
    }

    #[test]
    fn labels_and_hint_style() {
        let c = Config::parse(
            r#"
            [hint]
            enabled = true
            position = "top right"
            opacity = 0.5

            [[gesture]]
            stroke = "D"
            action = { command = "a very long command that goes on and on and on for a while" }
            "#,
            &[],
            &[],
        )
        .unwrap();
        assert!(c.hint.enabled && c.hint.next);
        assert_eq!(
            c.hint.position,
            Position {
                vertical: Align::Start,
                horizontal: Align::End
            }
        );
        assert_eq!(c.gestures[0].label().chars().count(), 41);
        assert_eq!(
            "center".parse::<Position>().unwrap(),
            Position {
                vertical: Align::Center,
                horizontal: Align::Center
            }
        );
        assert_eq!("left".parse::<Position>().unwrap().vertical, Align::Center);
        assert!("top bottom".parse::<Position>().is_err());
        assert!("center center center".parse::<Position>().is_err());
        assert!(
            Config::parse("[hint]\nopacity = 2", &[], &[])
                .unwrap_err()
                .contains("opacity")
        );
        assert!(Config::parse("[hint]\ncolour = \"#fff\"", &[], &[]).is_err());
        assert_eq!(
            "#ff000080".parse::<Rgba>().unwrap(),
            Rgba([1.0, 0.0, 0.0, 128.0 / 255.0])
        );
        assert_eq!("#00ff00".parse::<Rgba>().unwrap().0[3], 1.0);
        assert!("#00ff0".parse::<Rgba>().is_err() && "00ff00".parse::<Rgba>().is_err());
    }

    #[test]
    fn continuous_gestures_cant_prefix_others() {
        let main = r#"
            [[gesture]]
            stroke = "L"
            repeat = true
            action = { keys = "a" }
        "#;
        assert!(Config::parse(main, &[], &[]).is_ok());
        let longer = "[[gesture]]\nstroke = \"LD\"\naction = { keys = \"b\" }";
        let bad = Config::parse(main, &[("x".into(), longer.into())], &[]).unwrap_err();
        assert!(bad.contains("apps/x.toml") && bad.contains("LD"), "{bad}");
        // A oneshot may prefix: it fires on release, after the whole stroke.
        let oneshot = main.replace("repeat = true\n", "");
        assert!(Config::parse(&oneshot, &[("x".into(), longer.into())], &[]).is_ok());
    }
}
