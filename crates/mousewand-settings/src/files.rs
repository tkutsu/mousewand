//! The config directory as the settings UI sees it, and saving edits back.
//!
//! The files stay the source of truth. Saving rewrites only what the UI
//! edits (the settings keys and the gesture list) through `toml_edit`, so
//! comments and anything else in the files survive. Every save is checked
//! with the daemon's own parser before anything touches the disk.

use std::path::{Path, PathBuf};

use mousewand_core::shape::Shape;
use mousewand_core::{Config, Theme};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, value};

/// One gesture, as plain strings for the form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gesture {
    /// "stroke", "shape", "wheel" or "click".
    pub input: String,
    /// The stroke, shape name, wheel direction or clicked button.
    pub value: String,
    /// "oneshot", "repeat" or "hold": `repeat = true`, or `hold` in the action.
    pub kind: String,
    pub step: Option<f64>,
    /// The keys a "hold" gesture holds down.
    pub hold: Option<String>,
    /// "keys", "command" or "dispatch".
    pub action: String,
    pub target: String,
    /// What hints call it.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub trigger: String,
    pub threshold: f64,
    pub hold_ms: u64,
    pub min_segment: f64,
    pub theme: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// The file stem in `apps/`.
    pub file: String,
    pub classes: Vec<String>,
    pub gestures: Vec<Gesture>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub classes: Vec<String>,
    pub gestures: Vec<Gesture>,
}

/// Everything the window shows.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub dir: String,
    pub settings: Settings,
    pub global: Vec<Gesture>,
    /// The app for the window the settings gesture was drawn over, if any.
    pub app: Option<App>,
    pub themes: Vec<String>,
    /// The built-in shapes, then the drawn ones.
    pub shapes: Vec<String>,
    pub presets: Vec<Preset>,
}

pub struct Dir(pub PathBuf);

impl Dir {
    fn main_path(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    fn app_path(&self, file: &str) -> PathBuf {
        self.0.join("apps").join(format!("{file}.toml"))
    }

    fn shape_path(&self, name: &str) -> PathBuf {
        self.0.join("shapes").join(format!("{name}.toml"))
    }

    pub fn load(&self, class: Option<&str>) -> Result<State, String> {
        let main = read(&self.main_path())?;
        let table: toml::Table = toml::from_str(&main).map_err(|e| format!("config.toml: {e}"))?;
        let config = Config::parse(&main, &[], &toml_files(&self.0.join("shapes"))?)?;
        let s = &config.settings;
        let settings = Settings {
            // By its usual name, so "side" shows as "back".
            trigger: s.trigger.to_string(),
            threshold: s.threshold,
            hold_ms: s.hold_ms,
            min_segment: s.min_segment,
            theme: s.theme.clone(),
        };
        let app = match class.filter(|c| !c.is_empty()) {
            Some(class) => Some(self.app_for(class)?),
            None => None,
        };
        Ok(State {
            dir: self.0.display().to_string(),
            settings,
            global: gestures_of(&table)?,
            app,
            themes: self.themes(),
            shapes: config.shapes.iter().map(|s| s.name.clone()).collect(),
            presets: presets()?,
        })
    }

    /// The app file whose `match` lists `class`, or a new one named after it.
    fn app_for(&self, class: &str) -> Result<App, String> {
        for (file, text) in self.app_files()? {
            let table: toml::Table = toml::from_str(&text).map_err(|e| format!("apps/{file}.toml: {e}"))?;
            let classes = match table.get("match").and_then(|m| m.as_array()) {
                Some(list) => list.iter().filter_map(|c| c.as_str().map(String::from)).collect(),
                None => vec![file.clone()],
            };
            if classes.iter().any(|c| c.eq_ignore_ascii_case(class)) {
                return Ok(App {
                    file,
                    classes,
                    gestures: gestures_of(&table)?,
                });
            }
        }
        Ok(App {
            file: file_name(class),
            classes: vec![class.into()],
            gestures: Vec::new(),
        })
    }

    fn app_files(&self) -> Result<Vec<(String, String)>, String> {
        toml_files(&self.0.join("apps"))
    }

    /// Saves a drawn shape as `shapes/<name>.toml`, its points rounded to
    /// whole pixels, and returns the name it got.
    pub fn save_shape(&self, name: &str, examples: &[Vec<[f64; 2]>]) -> Result<String, String> {
        let name = file_name(name.trim());
        if name.is_empty() {
            return Err("the shape needs a name".into());
        }
        let examples: Vec<String> = examples
            .iter()
            .map(|e| {
                let points: Vec<String> = e
                    .iter()
                    .map(|[x, y]| format!("[{}, {}]", x.round(), y.round()))
                    .collect();
                format!("  [{}],\n", points.join(", "))
            })
            .collect();
        let text = format!(
            "# Drawn in mousewand-settings. Use it with shape = \"{name}\".\nexamples = [\n{}]\n",
            examples.concat()
        );
        Shape::parse(&name, &text)?;
        write(&self.shape_path(&name), &text)?;
        Ok(name)
    }

    fn themes(&self) -> Vec<String> {
        let mut themes = vec!["default".to_string()];
        if let Ok(entries) = std::fs::read_dir(self.0.join("themes")) {
            for path in entries.filter_map(|e| Some(e.ok()?.path())) {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                if path.extension().is_some_and(|ext| ext == "toml") && stem != "default" {
                    themes.push(stem);
                }
            }
        }
        themes[1..].sort();
        themes
    }

    /// Writes the edits if the whole config, as the daemon would read it,
    /// still parses. Nothing is written otherwise.
    pub fn save(&self, settings: &Settings, global: &[Gesture], app: Option<&App>) -> Result<(), String> {
        let main = edit_main(&read(&self.main_path())?, settings, global)?;
        let mut apps = self.app_files()?;
        let app_text = match app {
            Some(app) => {
                let path = self.app_path(&app.file);
                let text = edit_app(&read(&path)?, app)?;
                apps.retain(|(file, _)| file != &app.file);
                apps.push((app.file.clone(), text.clone()));
                Some((path, text))
            }
            None => None,
        };
        let config = Config::parse(&main, &apps, &toml_files(&self.0.join("shapes"))?)?;
        if config.settings.theme != "default" {
            let name = &config.settings.theme;
            let path = self.0.join("themes").join(format!("{name}.toml"));
            let text = std::fs::read_to_string(&path).map_err(|e| format!("theme {name:?}: {e}"))?;
            Theme::parse(&text).map_err(|e| format!("themes/{name}.toml: {e}"))?;
        }
        write(&self.main_path(), &main)?;
        if let Some((path, text)) = app_text {
            write(&path, &text)?;
        }
        Ok(())
    }
}

/// Each `*.toml` in `dir` as `(file stem, text)`, sorted. No directory is
/// no files.
fn toml_files(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    for path in entries.filter_map(|e| Some(e.ok()?.path())) {
        if path.extension().is_some_and(|ext| ext == "toml") {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            files.push((stem, read(&path)?));
        }
    }
    files.sort();
    Ok(files)
}

/// A missing file reads as empty.
fn read(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        other => other.map_err(|e| format!("reading {}: {e}", path.display())),
    }
}

/// Writes through a temporary file, so the daemon never reads half a file.
fn write(path: &Path, text: &str) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("writing {}: {e}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(fail)?;
    std::fs::rename(&tmp, path).map_err(fail)
}

/// A class as a file name: `org.gnome.Nautilus` stays, slashes don't.
fn file_name(class: &str) -> String {
    class
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn gestures_of(table: &toml::Table) -> Result<Vec<Gesture>, String> {
    let Some(list) = table.get("gesture") else {
        return Ok(Vec::new());
    };
    let list = list.as_array().ok_or("`gesture` must be a list of tables")?;
    list.iter()
        .map(|g| {
            g.as_table()
                .ok_or("each gesture must be a table".to_string())
                .and_then(gesture_of)
        })
        .collect()
}

fn gesture_of(table: &toml::Table) -> Result<Gesture, String> {
    let text = |key: &str| table.get(key).and_then(|v| v.as_str()).map(String::from);
    let (input, value) = ["stroke", "shape", "wheel", "click"]
        .into_iter()
        .find_map(|key| Some((key.to_string(), text(key)?)))
        .ok_or("a gesture needs a stroke, shape, wheel or click")?;
    let action = table
        .get("action")
        .and_then(|a| a.as_table())
        .ok_or("a gesture needs an action")?;
    let (what, target) = ["keys", "command", "dispatch"]
        .into_iter()
        .find_map(|key| Some((key.to_string(), action.get(key)?.as_str()?.to_string())))
        .ok_or("an action needs keys, a command or a dispatch")?;
    let hold = action.get("hold").and_then(|h| h.as_str()).map(String::from);
    let repeat = table.get("repeat").and_then(|r| r.as_bool()) == Some(true);
    Ok(Gesture {
        input,
        value,
        kind: match (&hold, repeat) {
            (Some(_), _) => "hold",
            (None, true) => "repeat",
            (None, false) => "oneshot",
        }
        .into(),
        step: table
            .get("step")
            .and_then(|s| s.as_float().or(s.as_integer().map(|i| i as f64))),
        hold,
        action: what,
        target,
        name: text("name"),
    })
}

fn document(text: &str, name: &str) -> Result<DocumentMut, String> {
    text.parse().map_err(|e| format!("{name}: {e}"))
}

fn edit_main(text: &str, settings: &Settings, global: &[Gesture]) -> Result<String, String> {
    let mut doc = document(text, "config.toml")?;
    doc["trigger"] = value(&settings.trigger);
    doc["threshold"] = value(settings.threshold);
    doc["hold_ms"] = value(settings.hold_ms as i64);
    doc["min_segment"] = value(settings.min_segment);
    doc["theme"] = value(&settings.theme);
    set_gestures(&mut doc, global);
    Ok(doc.to_string())
}

fn edit_app(text: &str, app: &App) -> Result<String, String> {
    let mut doc = document(text, &format!("apps/{}.toml", app.file))?;
    // A file named after its only class doesn't need `match`.
    let implied = app.classes.len() == 1 && app.classes[0] == app.file;
    if !(implied && doc.get("match").is_none()) {
        doc["match"] = value(app.classes.iter().collect::<Array>());
    }
    set_gestures(&mut doc, &app.gestures);
    Ok(doc.to_string())
}

fn set_gestures(doc: &mut DocumentMut, gestures: &[Gesture]) {
    doc.remove("gesture");
    if gestures.is_empty() {
        return;
    }
    let mut list = ArrayOfTables::new();
    for g in gestures {
        let mut table = Table::new();
        table[&g.input] = value(&g.value);
        if let Some(name) = g.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            table["name"] = value(name);
        }
        let continuous = g.input == "stroke" && g.kind != "oneshot";
        if continuous {
            // Holding keys implies repeating.
            if g.kind == "repeat" {
                table["repeat"] = value(true);
            }
            if let Some(step) = g.step {
                table["step"] = value(step);
            }
        }
        let mut action = InlineTable::new();
        action.insert(&g.action, g.target.as_str().into());
        if continuous
            && g.kind == "hold"
            && let Some(hold) = &g.hold
        {
            action.insert("hold", hold.as_str().into());
        }
        table["action"] = Item::Value(action.into());
        list.push(table);
    }
    doc.insert("gesture", Item::ArrayOfTables(list));
}

#[derive(Deserialize)]
struct PresetFile {
    preset: Vec<toml::Table>,
}

fn presets() -> Result<Vec<Preset>, String> {
    let file: PresetFile = toml::from_str(include_str!("../presets.toml")).map_err(|e| e.to_string())?;
    file.preset
        .iter()
        .map(|p| {
            let classes = p
                .get("classes")
                .and_then(|c| c.as_array())
                .map(|list| list.iter().filter_map(|c| c.as_str().map(String::from)).collect())
                .unwrap_or_default();
            Ok(Preset {
                name: p.get("name").and_then(|n| n.as_str()).unwrap_or("?").into(),
                classes,
                gestures: gestures_of(p)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory per test: tests run in parallel.
    fn dir(test: &str) -> (Dir, PathBuf) {
        let path = std::env::temp_dir().join(format!("mousewand-settings-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("apps")).unwrap();
        (Dir(path.clone()), path)
    }

    fn keys(stroke: &str, combo: &str) -> Gesture {
        Gesture {
            input: "stroke".into(),
            value: stroke.into(),
            kind: "oneshot".into(),
            step: None,
            hold: None,
            action: "keys".into(),
            target: combo.into(),
            name: None,
        }
    }

    #[test]
    fn presets_are_valid_gestures() {
        let presets = presets().unwrap();
        assert!(presets.len() >= 5);
        for preset in presets {
            let app = App {
                file: "x".into(),
                classes: preset.classes.clone(),
                gestures: preset.gestures.clone(),
            };
            let text = edit_app("", &app).unwrap();
            Config::parse("", &[("x".into(), text)], &[]).unwrap_or_else(|e| panic!("{}: {e}", preset.name));
        }
    }

    #[test]
    fn round_trip_keeps_comments_and_other_files() {
        let (dir, path) = dir("round-trip");
        std::fs::write(
            path.join("config.toml"),
            "# my settings\ntrigger = \"side\"\n\n[[gesture]]\nstroke = \"L\"\naction = { keys = \"a\" }\n\n\
             [hint]\nenabled = true\n\n[[global.gesture]]\nstroke = \"9\"\naction = { keys = \"super+Tab\" }\n",
        )
        .unwrap();
        std::fs::write(
            path.join("apps/browser.toml"),
            "# browsers\nmatch = [\"firefox\"]\n\n[[gesture]]\nwheel = \"up\"\naction = { command = \"x\" }\n",
        )
        .unwrap();

        let state = dir.load(Some("Firefox")).unwrap();
        assert_eq!(state.settings.trigger, "back", "evdev's name shows as the usual one");
        assert_eq!(state.global, [keys("L", "a")]);
        let mut app = state.app.unwrap();
        assert_eq!(app.file, "browser");
        assert_eq!(app.gestures[0].input, "wheel");

        app.gestures.push(Gesture {
            kind: "repeat".into(),
            step: Some(50.0),
            name: Some("Next tab".into()),
            ..keys("U", "ctrl+Tab")
        });
        app.gestures.push(Gesture {
            kind: "hold".into(),
            hold: Some("alt".into()),
            ..keys("7", "Tab")
        });
        dir.save(&state.settings, &[keys("R", "b")], Some(&app)).unwrap();

        let main = std::fs::read_to_string(path.join("config.toml")).unwrap();
        assert!(main.starts_with("# my settings"), "{main}");
        let browser = std::fs::read_to_string(path.join("apps/browser.toml")).unwrap();
        assert!(browser.starts_with("# browsers"), "{browser}");
        assert!(browser.contains("name = \"Next tab\""), "{browser}");
        assert!(browser.contains("repeat = true"), "{browser}");
        assert!(
            browser.contains("action = { keys = \"Tab\", hold = \"alt\" }"),
            "{browser}"
        );
        // Sections the app doesn't edit stay as they were.
        assert!(main.contains("[hint]") && main.contains("[[global.gesture]]"), "{main}");
        let again = dir.load(Some("firefox")).unwrap();
        assert_eq!(again.global, [keys("R", "b")]);
        let gestures = again.app.unwrap().gestures;
        assert_eq!(gestures.len(), 3);
        assert_eq!(gestures[1].name.as_deref(), Some("Next tab"));
        assert_eq!(gestures[1].kind, "repeat");
        assert_eq!(
            (gestures[2].kind.as_str(), gestures[2].hold.as_deref()),
            ("hold", Some("alt"))
        );
    }

    #[test]
    fn drawn_shapes_save_and_list() {
        let (dir, path) = dir("shapes");
        let triangle = vec![[0.0, 0.0], [100.4, 0.0], [50.0, 90.0], [0.0, 0.0]];
        let name = dir.save_shape("my triangle", &[triangle.clone(), triangle]).unwrap();
        assert_eq!(name, "my_triangle");
        let text = std::fs::read_to_string(path.join("shapes/my_triangle.toml")).unwrap();
        assert!(text.contains("[100, 0]"), "{text}");
        assert_eq!(dir.load(None).unwrap().shapes, ["circle", "my_triangle"]);

        let gesture = Gesture {
            input: "shape".into(),
            ..keys("my_triangle", "ctrl+t")
        };
        dir.save(&dir.load(None).unwrap().settings, std::slice::from_ref(&gesture), None)
            .unwrap();
        assert_eq!(dir.load(None).unwrap().global, [gesture]);

        assert!(dir.save_shape("dot", &[vec![[5.0, 5.0], [5.0, 5.0]]]).is_err());
        assert!(dir.save_shape(" ", &[vec![[0.0, 0.0], [9.0, 9.0]]]).is_err());
        assert!(!path.join("shapes/dot.toml").exists());
    }

    #[test]
    fn invalid_saves_write_nothing() {
        let (dir, path) = dir("invalid");
        let state = dir.load(Some("kitty")).unwrap();
        let app = state.app.unwrap();
        assert_eq!(app.file, "kitty");
        let bad = dir.save(&state.settings, &[keys("L", "ctrl+nope")], Some(&app));
        assert!(bad.unwrap_err().contains("nope"));
        assert!(!path.join("config.toml").exists());

        let mut settings = state.settings.clone();
        settings.theme = "missing".into();
        assert!(dir.save(&settings, &[], None).is_err());

        dir.save(&state.settings, &[keys("L", "alt+Left")], Some(&app)).unwrap();
        let kitty = std::fs::read_to_string(path.join("apps/kitty.toml")).unwrap();
        assert!(
            !kitty.contains("match"),
            "a file named after its class needs no match: {kitty}"
        );
    }
}
