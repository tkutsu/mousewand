//! Config types and per-app gesture lookup.
//!
//! `config.toml` holds settings and global gestures; each `apps/<name>.toml`
//! holds the gestures for one app, matched by window class. Parsing is pure:
//! the caller reads the files and passes their text in.

use std::str::FromStr;

use serde::{Deserialize, Deserializer, de};

use crate::keys::Combo;
use crate::stroke::Stroke;

#[derive(Clone, Debug)]
pub struct Config {
    pub settings: Settings,
    pub gestures: Vec<Gesture>,
    pub apps: Vec<App>,
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            trigger: Trigger::default(),
            threshold: 8.0,
            hold_ms: 300,
            min_segment: 30.0,
            devices: Vec::new(),
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
            _ => return Err(format!("unknown trigger {s:?}: use middle, side, extra, forward, back or task")),
        }))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gesture {
    #[serde(deserialize_with = "parsed")]
    pub stroke: Stroke,
    pub action: Action,
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
        let apps = apps
            .iter()
            .map(|(stem, text)| {
                let file: AppFile = toml::from_str(text).map_err(|e| format!("apps/{stem}.toml: {e}"))?;
                let classes = if file.classes.is_empty() { vec![stem.clone()] } else { file.classes };
                Ok(App { classes, gestures: file.gestures })
            })
            .collect::<Result<_, String>>()?;
        Ok(Config { settings: main.settings, gestures: main.gestures, apps })
    }

    /// The action for a stroke over a window of `class`. App gestures win
    /// over global ones with the same stroke.
    pub fn action(&self, class: &str, stroke: &Stroke) -> Option<&Action> {
        self.apps
            .iter()
            .filter(|app| app.classes.iter().any(|c| c.eq_ignore_ascii_case(class)))
            .find_map(|app| find(&app.gestures, stroke))
            .or_else(|| find(&self.gestures, stroke))
    }
}

fn find<'a>(gestures: &'a [Gesture], stroke: &Stroke) -> Option<&'a Action> {
    gestures.iter().find(|g| &g.stroke == stroke).map(|g| &g.action)
}

impl Default for Config {
    fn default() -> Self {
        Config { settings: Settings::default(), gestures: Vec::new(), apps: Vec::new() }
    }
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
    "#;

    const FIREFOX: &str = r#"
        match = ["firefox", "librewolf"]

        [[gesture]]
        stroke = "L"
        action = { keys = "alt+Left" }
    "#;

    fn config() -> Config {
        Config::parse(MAIN, &[("firefox".into(), FIREFOX.into()), ("kitty".into(), String::new())]).unwrap()
    }

    #[test]
    fn settings() {
        let c = config();
        assert_eq!(c.settings.trigger, Trigger(0x113));
        assert_eq!(c.settings.threshold, 10.0);
        assert_eq!(c.settings.hold_ms, 300);
    }

    #[test]
    fn app_overrides_global() {
        let c = config();
        let l = "L".parse().unwrap();
        assert_eq!(c.action("LibreWolf", &l), Some(&Action::Keys("alt+Left".parse().unwrap())));
        assert_eq!(c.action("kitty", &l), Some(&Action::Dispatch("workspace -1".into())));
        assert_eq!(c.action("kitty", &"DR".parse().unwrap()), Some(&Action::Command("notify-send hi".into())));
        assert_eq!(c.action("kitty", &"U".parse().unwrap()), None);
    }

    #[test]
    fn stem_is_the_default_match() {
        assert_eq!(config().apps[1].classes, ["kitty"]);
    }

    #[test]
    fn errors_name_the_file() {
        let bad = Config::parse("trigger = \"left\"", &[]).unwrap_err();
        assert!(bad.contains("config.toml") && bad.contains("normal clicks"), "{bad}");
        let bad = Config::parse("", &[("x".into(), "[[gesture]]\nstroke = \"L\"\naction = { keys = \"ctrl+wat\" }".into())])
            .unwrap_err();
        assert!(bad.contains("apps/x.toml") && bad.contains("wat"), "{bad}");
        assert!(Config::parse("thresold = 3", &[]).is_err());
    }
}
