//! Config types and per-app gesture lookup.
//!
//! `config.toml` holds settings, the gestures every app gets unless it
//! overrides them, the `[global]` gestures no app overrides, and the `[hint]`
//! style. Each `apps/<name>.toml` holds the gestures for one app, matched by
//! window class. Parsing is pure: the caller reads the files and passes their
//! text in.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, de};

use crate::keys::Combo;
use crate::stroke::Stroke;

#[derive(Clone, Debug, Default)]
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
}

/// How the trail looks. Every field is optional.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    #[serde(deserialize_with = "parsed")]
    pub color: Color,
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
            color: Color([0.45, 0.75, 1.0]),
            rejected: Color([1.0, 0.35, 0.35]),
            width: 26.0,
            life_ms: 450,
            fade_ms: 300,
            glow: 0.6,
        }
    }
}

impl Theme {
    pub fn parse(text: &str) -> Result<Theme, String> {
        let theme: Theme = toml::from_str(text).map_err(|e| e.to_string())?;
        if theme.width <= 0.0 || theme.life_ms == 0 {
            return Err("width and life_ms must be positive".into());
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
        let input = match (raw.stroke, raw.wheel, raw.click) {
            (Some(stroke), None, None) => Input::Stroke(stroke.parse()?),
            (None, Some(wheel), None) => Input::Wheel(match wheel.to_ascii_lowercase().as_str() {
                "up" => Wheel::Up,
                "down" => Wheel::Down,
                "left" => Wheel::Left,
                "right" => Wheel::Right,
                _ => return Err(format!("unknown wheel {wheel:?}: use up, down, left or right")),
            }),
            (None, None, Some(button)) => Input::Click(match button.to_ascii_lowercase().as_str() {
                "left" => BTN_LEFT,
                "right" => BTN_RIGHT,
                _ => return Err(format!("unknown click {button:?}: use left or right")),
            }),
            _ => return Err("a gesture needs exactly one of stroke, wheel or click".into()),
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
    /// Parses `config.toml` and the app files, given as `(file stem, text)`.
    /// An app file without `match` matches the class named by its stem.
    pub fn parse(main: &str, apps: &[(String, String)]) -> Result<Config, String> {
        let mut main: MainFile = toml::from_str(main).map_err(|e| format!("config.toml: {e}"))?;
        let in_main = |e: String| format!("config.toml: {e}");
        main.hint.check().map_err(in_main)?;
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
                let file: AppFile = toml::from_str(text).map_err(|e| format!("apps/{stem}.toml: {e}"))?;
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
        })
    }

    /// The gestures for the trigger over a window of `class`: the global
    /// ones unless they have their own trigger, its app's, the shared ones
    /// the app doesn't override, then the settings gestures whose strokes
    /// nothing else uses.
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
        for settings in SETTINGS_STROKES.map(settings_gesture) {
            if !gestures.iter().any(|g| g.input == settings.input) {
                gestures.push(settings);
            }
        }
        gestures
    }

    /// The gestures for the global trigger, if `[global]` has one.
    pub fn global_gestures(&self) -> Vec<Gesture> {
        self.global.clone()
    }
}

/// The strokes that open the settings app on the window under the cursor:
/// an M, drawn with straight sides (↑↘↗↓) or all in diagonals (↗↘↗↘).
pub const SETTINGS_STROKES: [&str; 2] = ["U39D", "9393"];

/// Opens `mousewand-settings`, which reads the window class from `MOUSEWAND_CLASS`.
fn settings_gesture(stroke: &str) -> Gesture {
    Gesture {
        input: Input::Stroke(stroke.parse().expect("a valid stroke")),
        kind: Kind::Oneshot,
        action: Action::Command("mousewand-settings".into()),
        name: Some("mousewand settings".into()),
    }
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
        let c = Config::parse(hold, &[]).unwrap();
        assert_eq!(
            c.gestures[0].kind,
            Kind::Hold {
                keys: "alt".parse().unwrap(),
                step: 80.0
            }
        );
    }

    #[test]
    fn settings_gesture_is_built_in_unless_taken() {
        let opens_settings = |c: &Config, class: &str| {
            c.gestures_for(class)
                .iter()
                .any(|g| g.input == stroke("9393") && g.action == Action::Command("mousewand-settings".into()))
        };
        assert!(opens_settings(&config(), "kitty"));
        assert!(config().gestures_for("kitty").iter().any(|g| g.input == stroke("U39D")));
        let taken = "[[gesture]]\nstroke = \"9393\"\naction = { keys = \"a\" }";
        let c = Config::parse("", &[("kitty".into(), taken.into())]).unwrap();
        assert!(!opens_settings(&c, "kitty"));
        assert!(opens_settings(&c, "firefox"));
    }

    #[test]
    fn stem_is_the_default_match() {
        assert_eq!(config().apps[1].classes, ["kitty"]);
    }

    #[test]
    fn errors_name_the_file() {
        let bad = Config::parse("trigger = \"left\"", &[]).unwrap_err();
        assert!(bad.contains("config.toml") && bad.contains("normal clicks"), "{bad}");
        let bad = Config::parse(
            "",
            &[(
                "x".into(),
                "[[gesture]]\nstroke = \"L\"\naction = { keys = \"ctrl+wat\" }".into(),
            )],
        )
        .unwrap_err();
        assert!(bad.contains("apps/x.toml") && bad.contains("wat"), "{bad}");
        assert!(Config::parse("thresold = 3", &[]).is_err());
    }

    #[test]
    fn invalid_gestures() {
        let bad = |g: &str| Config::parse(&format!("[[gesture]]\n{g}\naction = {{ keys = \"a\" }}"), &[]).unwrap_err();
        assert!(bad("").contains("exactly one"));
        assert!(bad("stroke = \"L\"\nwheel = \"up\"").contains("exactly one"));
        assert!(bad("wheel = \"up\"\nrepeat = true").contains("only strokes"));
        assert!(bad("stroke = \"L\"\nstep = 5").contains("only goes with"));
        assert!(bad("click = \"middle\"").contains("unknown click"));
        // The old names are gone.
        assert!(bad("stroke = \"L\"\nkind = \"repeat\"").contains("unknown field"));
        assert!(bad("rocker = \"left\"").contains("unknown field"));

        let action = |a: &str| Config::parse(&format!("[[gesture]]\nstroke = \"L\"\naction = {a}"), &[]);
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
        assert!(Config::parse(no_repeat, &[]).unwrap_err().contains("always repeats"));
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
        let theme = Theme::parse("color = \"#ff8000\"\nwidth = 40").unwrap();
        assert_eq!(theme.color, Color([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(theme.width, 40.0);
        assert_eq!(theme.life_ms, Theme::default().life_ms);
        assert!(Theme::parse("color = \"orange\"").unwrap_err().contains("#rrggbb"));
        assert!(Theme::parse("width = 0").is_err());
        assert!(Theme::parse("colour = \"#ffffff\"").is_err());
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
        let c = Config::parse(main, &[]).unwrap();
        assert_eq!(c.settings.global_trigger, None);
        let gestures = c.gestures_for("firefox");
        assert_eq!(gestures[0].label(), "Next workspace");
        assert!(gestures.iter().any(|g| g.input == stroke("L")));
        assert!(c.global_gestures().len() == 1);

        let clash = "[[gesture]]\nstroke = \"R\"\naction = { keys = \"b\" }";
        let bad = Config::parse(main, &[("kitty".into(), clash.into())]).unwrap_err();
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
        let c = Config::parse(main, &[]).unwrap();
        assert_eq!(c.settings.global_trigger, Some(Trigger(0x114)));
        // The same stroke is free for the other trigger.
        assert_eq!(
            action_for(&c, "firefox", stroke("R")),
            Some(Action::Keys("alt+Right".parse().unwrap()))
        );
        assert_eq!(c.global_gestures()[0].action, Action::Dispatch("workspace +1".into()));
        let same = Config::parse("[global]\ntrigger = \"middle\"", &[]).unwrap_err();
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
            Config::parse("[hint]\nopacity = 2", &[])
                .unwrap_err()
                .contains("opacity")
        );
        assert!(Config::parse("[hint]\ncolour = \"#fff\"", &[]).is_err());
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
        assert!(Config::parse(main, &[]).is_ok());
        let longer = "[[gesture]]\nstroke = \"LD\"\naction = { keys = \"b\" }";
        let bad = Config::parse(main, &[("x".into(), longer.into())]).unwrap_err();
        assert!(bad.contains("apps/x.toml") && bad.contains("LD"), "{bad}");
        // A oneshot may prefix: it fires on release, after the whole stroke.
        let oneshot = main.replace("repeat = true\n", "");
        assert!(Config::parse(&oneshot, &[("x".into(), longer.into())]).is_ok());
    }
}
