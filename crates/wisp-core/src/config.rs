//! Config types and per-app gesture lookup.
//!
//! `config.toml` holds settings and global gestures; each `apps/<name>.toml`
//! holds the gestures for one app, matched by window class. Parsing is pure:
//! the caller reads the files and passes their text in.

use std::str::FromStr;

use serde::{Deserialize, Deserializer, de};

use crate::keys::Combo;
use crate::stroke::Stroke;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub settings: Settings,
    pub gestures: Vec<Gesture>,
    pub apps: Vec<App>,
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

impl FromStr for Trigger {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        Ok(Trigger(match s.to_ascii_lowercase().as_str() {
            "middle" => BTN_MIDDLE,
            "side" => 0x113,
            "extra" => 0x114,
            "forward" => 0x115,
            "back" => 0x116,
            "task" => 0x117,
            "left" | "right" => return Err(format!("{s:?} can't be the trigger: it would stop normal clicks")),
            _ => {
                return Err(format!(
                    "unknown trigger {s:?}: use middle, side, extra, forward, back or task"
                ));
            }
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawGesture")]
pub struct Gesture {
    pub input: Input,
    pub kind: Kind,
    pub action: Action,
}

/// What the user does while holding the trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Stroke(Stroke),
    /// One wheel notch.
    Wheel(Wheel),
    /// A press of another button.
    Rocker(u16),
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
    /// holding alt and tapping tab is a real alt-tab switcher.
    Hold { keys: Combo, step: f64 },
}

impl Kind {
    pub fn continuous(&self) -> bool {
        !matches!(self, Kind::Oneshot)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Action {
    /// A key combo sent to the window the gesture started on.
    #[serde(deserialize_with = "parsed")]
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
    rocker: Option<String>,
    kind: Option<String>,
    step: Option<f64>,
    hold: Option<String>,
    action: Action,
}

const DEFAULT_STEP: f64 = 80.0;

impl TryFrom<RawGesture> for Gesture {
    type Error = String;

    fn try_from(raw: RawGesture) -> Result<Gesture, String> {
        let input = match (raw.stroke, raw.wheel, raw.rocker) {
            (Some(stroke), None, None) => Input::Stroke(stroke.parse()?),
            (None, Some(wheel), None) => Input::Wheel(match wheel.to_ascii_lowercase().as_str() {
                "up" => Wheel::Up,
                "down" => Wheel::Down,
                "left" => Wheel::Left,
                "right" => Wheel::Right,
                _ => return Err(format!("unknown wheel {wheel:?}: use up, down, left or right")),
            }),
            (None, None, Some(button)) => Input::Rocker(match button.to_ascii_lowercase().as_str() {
                "left" => BTN_LEFT,
                "right" => BTN_RIGHT,
                _ => return Err(format!("unknown rocker {button:?}: use left or right")),
            }),
            _ => return Err("a gesture needs exactly one of stroke, wheel or rocker".into()),
        };
        let step = raw.step.unwrap_or(DEFAULT_STEP);
        if step <= 0.0 {
            return Err(format!("step must be positive, not {step}"));
        }
        let kind = match (raw.kind.as_deref(), raw.hold) {
            (None | Some("oneshot"), None) => Kind::Oneshot,
            (Some("repeat"), None) => Kind::Repeat { step },
            (Some("hold"), Some(keys)) => Kind::Hold {
                keys: keys.parse()?,
                step,
            },
            (Some("hold"), None) => return Err("a hold gesture needs `hold`, the keys to hold down".into()),
            (_, Some(_)) => return Err("`hold` only goes with kind = \"hold\"".into()),
            (Some(other), None) => return Err(format!("unknown kind {other:?}: use oneshot, repeat or hold")),
        };
        if kind.continuous() && !matches!(input, Input::Stroke(_)) {
            return Err("repeat and hold gestures need a stroke".into());
        }
        if !kind.continuous() && raw.step.is_some() {
            return Err("`step` only goes with repeat and hold gestures".into());
        }
        Ok(Gesture {
            input,
            kind,
            action: raw.action,
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
        let main: MainFile = toml::from_str(main).map_err(|e| format!("config.toml: {e}"))?;
        check_prefixes(&main.gestures).map_err(|e| format!("config.toml: {e}"))?;
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
                check_prefixes(&merged(&app.gestures, &main.gestures)).map_err(|e| format!("apps/{stem}.toml: {e}"))?;
                Ok(app)
            })
            .collect::<Result<_, String>>()?;
        Ok(Config {
            settings: main.settings,
            gestures: main.gestures,
            apps,
            theme: Theme::default(),
        })
    }

    /// The gestures for a window of `class`: its app's, then the global ones
    /// the app doesn't override, then the settings gesture unless something
    /// else uses its stroke.
    pub fn gestures_for(&self, class: &str) -> Vec<Gesture> {
        let app = self
            .apps
            .iter()
            .find(|app| app.classes.iter().any(|c| c.eq_ignore_ascii_case(class)));
        let mut gestures = merged(app.map_or(&[], |a| &a.gestures), &self.gestures);
        let settings = settings_gesture();
        if !gestures.iter().any(|g| g.input == settings.input) {
            gestures.push(settings);
        }
        gestures
    }
}

/// The stroke that opens the settings app on the window under the cursor:
/// a W, ↘↗↘↗.
pub const SETTINGS_STROKE: &str = "3939";

/// Opens `wisp-settings`, which reads the window class from `WISP_CLASS`.
fn settings_gesture() -> Gesture {
    Gesture {
        input: Input::Stroke(SETTINGS_STROKE.parse().expect("a valid stroke")),
        kind: Kind::Oneshot,
        action: Action::Command("wisp-settings".into()),
    }
}

fn merged(app: &[Gesture], global: &[Gesture]) -> Vec<Gesture> {
    let overridden = |g: &Gesture| app.iter().any(|a| a.input == g.input);
    app.iter()
        .chain(global.iter().filter(|g| !overridden(g)))
        .cloned()
        .collect()
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
        kind = "repeat"
        step = 60
        action = { keys = "ctrl+Tab" }

        [[gesture]]
        rocker = "left"
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
            action_for(&c, "firefox", Input::Rocker(BTN_LEFT)),
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
            kind = "hold"
            hold = "alt"
            action = { keys = "Tab" }
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
                .any(|g| g.input == stroke("3939") && g.action == Action::Command("wisp-settings".into()))
        };
        assert!(opens_settings(&config(), "kitty"));
        let taken = "[[gesture]]\nstroke = \"3939\"\naction = { keys = \"a\" }";
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
        assert!(bad("wheel = \"up\"\nkind = \"repeat\"").contains("need a stroke"));
        assert!(bad("stroke = \"L\"\nkind = \"hold\"").contains("needs `hold`"));
        assert!(bad("stroke = \"L\"\nhold = \"alt\"").contains("only goes with"));
        assert!(bad("stroke = \"L\"\nstep = 5").contains("only goes with"));
        assert!(bad("stroke = \"L\"\nkind = \"spin\"").contains("unknown kind"));
        assert!(bad("rocker = \"middle\"").contains("unknown rocker"));
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
    fn continuous_gestures_cant_prefix_others() {
        let main = r#"
            [[gesture]]
            stroke = "L"
            kind = "repeat"
            action = { keys = "a" }
        "#;
        assert!(Config::parse(main, &[]).is_ok());
        let longer = "[[gesture]]\nstroke = \"LD\"\naction = { keys = \"b\" }";
        let bad = Config::parse(main, &[("x".into(), longer.into())]).unwrap_err();
        assert!(bad.contains("apps/x.toml") && bad.contains("LD"), "{bad}");
        // A oneshot may prefix: it fires on release, after the whole stroke.
        let oneshot = main.replace("kind = \"repeat\"\n", "");
        assert!(Config::parse(&oneshot, &[("x".into(), longer.into())]).is_ok());
    }
}
