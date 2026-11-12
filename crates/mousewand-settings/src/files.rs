//! The config directory as the settings UI sees it, and saving edits back.
//!
//! The files stay the source of truth. Saving rewrites only what the UI
//! edits (the settings keys and the gesture list) through `toml_edit`, so
//! comments and anything else in the files survive. Every save is checked
//! with the daemon's own parser before anything touches the disk.

use std::path::{Path, PathBuf};

use mousewand_core::config::{SETTINGS_CLASSES, toml_error};
use mousewand_core::shape::Shape;
use mousewand_core::{Config, Input, Theme};
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
    /// Modifiers held on the keyboard, like "ctrl+shift".
    #[serde(default)]
    pub with: Option<String>,
    /// "oneshot", "repeat" or "hold": `repeat = true`, or `hold` in the action.
    pub kind: String,
    pub step: Option<f64>,
    /// The keys a "hold" gesture holds down.
    pub hold: Option<String>,
    /// What it does, in turn: one step, or several.
    pub steps: Vec<Step>,
    /// What hints call it.
    #[serde(default)]
    pub name: Option<String>,
}

/// One step of an action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// "keys", "command", "dispatch" or "wait".
    pub action: String,
    /// The keys, command or dispatcher, or the milliseconds to wait.
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub trigger: String,
    pub threshold: f64,
    pub hold_ms: u64,
    /// Holding still this long mid-stroke cancels it; 0 never does.
    pub cancel_ms: u64,
    pub min_segment: f64,
    pub theme: String,
    /// The stroke or shape that opens this app.
    pub open_settings: Drawn,
}

/// A stroke or a shape, as `open_settings` takes one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Drawn {
    /// "stroke" or "shape".
    pub input: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// The file stem in `apps/`.
    pub file: String,
    pub classes: Vec<String>,
    /// Off, the trigger is an ordinary button over this app.
    pub enabled: bool,
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
    /// The gestures over this app's own window.
    pub settings_app: App,
    pub themes: Vec<String>,
    /// The built-in shapes, then the drawn ones.
    pub shapes: Vec<ShapeInfo>,
    pub presets: Vec<Preset>,
}

impl App {
    fn new(file: String, classes: Vec<String>) -> App {
        App {
            file,
            classes,
            enabled: true,
            gestures: Vec::new(),
        }
    }
}

/// A shape as the shapes list shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeInfo {
    pub name: String,
    /// Built in, not drawn: there's no file to redraw or remove.
    pub built_in: bool,
    /// Its first example, about 1 across, for a thumbnail.
    pub outline: Vec<[f64; 2]>,
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
        let table: toml::Table = toml::from_str(&main).map_err(|e| toml_error("config.toml", &main, &e))?;
        let config = Config::parse(&main, &[], &toml_files(&self.0.join("shapes"))?)?;
        let open_settings = match &config.open_settings {
            Input::Shape(shape) => ("shape", shape.name.clone()),
            Input::Stroke(stroke) => ("stroke", stroke.to_string()),
            other => unreachable!("open_settings is a stroke or shape, not {other:?}"),
        };
        let s = &config.settings;
        let settings = Settings {
            // By its usual name, so "side" shows as "back".
            trigger: s.trigger.to_string(),
            threshold: s.threshold,
            hold_ms: s.hold_ms,
            cancel_ms: s.cancel_ms,
            min_segment: s.min_segment,
            theme: s.theme.clone(),
            open_settings: Drawn {
                input: open_settings.0.into(),
                value: open_settings.1,
            },
        };
        let is_settings = |c: &str| SETTINGS_CLASSES.iter().any(|s| s.eq_ignore_ascii_case(c));
        let app = match class.filter(|c| !c.is_empty() && !is_settings(c)) {
            Some(class) => Some(self.app_for(class)?),
            None => None,
        };
        let mut settings_app = self.app_for(SETTINGS_CLASSES[0])?;
        if settings_app.gestures.is_empty() && settings_app.classes.len() == 1 {
            settings_app.classes = SETTINGS_CLASSES.map(String::from).to_vec();
        }
        Ok(State {
            dir: self.0.display().to_string(),
            settings,
            global: gestures_of(&table)?,
            app,
            settings_app,
            themes: self.themes(),
            shapes: config
                .shapes
                .iter()
                .map(|s| ShapeInfo {
                    name: s.name.clone(),
                    built_in: !self.shape_path(&s.name).exists(),
                    outline: s.outline().iter().map(|&(x, y)| [x, y]).collect(),
                })
                .collect(),
            presets: presets()?,
        })
    }

    /// The app file whose `match` lists `class`, or a new one named after it.
    fn app_for(&self, class: &str) -> Result<App, String> {
        for (file, text) in self.app_files()? {
            let table: toml::Table =
                toml::from_str(&text).map_err(|e| toml_error(&format!("apps/{file}.toml"), &text, &e))?;
            let classes = match table.get("match").and_then(|m| m.as_array()) {
                Some(list) => list.iter().filter_map(|c| c.as_str().map(String::from)).collect(),
                None => vec![file.clone()],
            };
            if classes.iter().any(|c| c.eq_ignore_ascii_case(class)) {
                return Ok(App {
                    enabled: table.get("enabled").and_then(|e| e.as_bool()) != Some(false),
                    gestures: gestures_of(&table)?,
                    ..App::new(file, classes)
                });
            }
        }
        Ok(App::new(file_name(class), vec![class.into()]))
    }

    fn app_files(&self) -> Result<Vec<(String, String)>, String> {
        toml_files(&self.0.join("apps"))
    }

    /// Saves a drawn shape as `shapes/<name>.toml`, its points rounded to
    /// whole pixels, and returns the name it got. Only a `redraw` may
    /// replace a shape that's there.
    pub fn save_shape(&self, name: &str, examples: &[Vec<[f64; 2]>], redraw: bool) -> Result<String, String> {
        let name = file_name(&name.trim().to_lowercase());
        if name.is_empty() {
            return Err("give the shape a name".into());
        }
        let taken = self.shape_path(&name).exists() || mousewand_core::shape::BUILT_IN.contains(&name.as_str());
        if taken && !redraw {
            return Err(format!("there's already a shape called {name}; pick another name"));
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

    /// Removes `shapes/<name>.toml`, unless a saved gesture still uses it.
    pub fn delete_shape(&self, name: &str) -> Result<(), String> {
        let path = self.shape_path(name);
        if !path.exists() {
            return Err(format!("{name} is built in, so it can't be removed"));
        }
        let mut shapes = toml_files(&self.0.join("shapes"))?;
        shapes.retain(|(stem, _)| stem != name);
        let main = read(&self.main_path())?;
        if let Err(e) = Config::parse(&main, &self.app_files()?, &shapes) {
            let file = e.split([':', ',']).next().unwrap_or("a file");
            return Err(match e.contains("no shape called") {
                true => format!("{name} is still used by a gesture in {file}; change that gesture first"),
                false => e,
            });
        }
        std::fs::remove_file(&path).map_err(|e| format!("removing {}: {e}", path.display()))
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
    pub fn save(&self, settings: &Settings, global: &[Gesture], edited: &[App]) -> Result<(), String> {
        let main = edit_main(&read(&self.main_path())?, settings, global)?;
        let mut apps = self.app_files()?;
        let mut writes = Vec::new();
        for app in edited {
            let path = self.app_path(&app.file);
            let old = read(&path)?;
            // An app with nothing in it needs no file.
            if old.is_empty() && app.gestures.is_empty() && app.enabled {
                continue;
            }
            let text = edit_app(&old, app)?;
            apps.retain(|(file, _)| file != &app.file);
            apps.push((app.file.clone(), text.clone()));
            writes.push((path, text));
        }
        let config = Config::parse(&main, &apps, &toml_files(&self.0.join("shapes"))?)?;
        if config.settings.theme != "default" {
            let name = &config.settings.theme;
            let path = self.0.join("themes").join(format!("{name}.toml"));
            let text = std::fs::read_to_string(&path).map_err(|e| format!("theme {name:?}: {e}"))?;
            Theme::parse(name, &text)?;
        }
        write(&self.main_path(), &main)?;
        for (path, text) in writes {
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
    let actions: Vec<&toml::Table> = match table.get("action") {
        Some(toml::Value::Table(one)) => vec![one],
        Some(toml::Value::Array(list)) => list.iter().filter_map(|a| a.as_table()).collect(),
        _ => return Err("a gesture needs an action".into()),
    };
    let steps = actions
        .iter()
        .map(|action| {
            let text = |key: &str| match action.get(key)? {
                toml::Value::String(s) => Some(s.clone()),
                toml::Value::Integer(n) => Some(n.to_string()),
                _ => None,
            };
            ["keys", "command", "dispatch", "wait"]
                .into_iter()
                .find_map(|key| {
                    Some(Step {
                        action: key.into(),
                        target: text(key)?,
                    })
                })
                .ok_or("an action needs keys, a command or a dispatch".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let hold = actions
        .first()
        .and_then(|a| a.get("hold"))
        .and_then(|h| h.as_str())
        .map(String::from);
    let repeat = table.get("repeat").and_then(|r| r.as_bool()) == Some(true);
    Ok(Gesture {
        input,
        value,
        with: text("with"),
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
        steps,
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
    // Written once it's been changed, like `open_settings`.
    let cancel_ms = settings.cancel_ms as i64;
    if cancel_ms != 0 || doc.contains_key("cancel_ms") {
        doc["cancel_ms"] = value(cancel_ms);
    }
    doc["min_segment"] = value(settings.min_segment);
    doc["theme"] = value(&settings.theme);
    // The default stays unwritten, so the file only says what differs.
    let open = &settings.open_settings;
    let default = open.input == "shape" && open.value == "circle";
    if !default || doc.contains_key("open_settings") {
        let mut table = InlineTable::new();
        table.insert(&open.input, open.value.as_str().into());
        doc["open_settings"] = Item::Value(table.into());
    }
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
    // On is the default, so only off is written.
    match app.enabled {
        true => {
            doc.remove("enabled");
        }
        false => doc["enabled"] = value(false),
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
        if let Some(with) = g.with.as_deref().filter(|w| !w.is_empty()) {
            table["with"] = value(with);
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
        let mut steps: Vec<InlineTable> = g.steps.iter().map(step_table).collect();
        if continuous
            && g.kind == "hold"
            && let (Some(hold), [only]) = (&g.hold, steps.as_mut_slice())
        {
            only.insert("hold", hold.as_str().into());
        }
        table["action"] = match steps.len() {
            1 => Item::Value(steps.remove(0).into()),
            _ => Item::Value(steps.into_iter().collect::<Array>().into()),
        };
        list.push(table);
    }
    doc.insert("gesture", Item::ArrayOfTables(list));
}

/// `{ keys = "ctrl+t" }`, or `{ wait = 200 }` with a number.
fn step_table(step: &Step) -> InlineTable {
    let mut table = InlineTable::new();
    match (step.action.as_str(), step.target.trim().parse::<i64>()) {
        ("wait", Ok(ms)) => table.insert("wait", ms.into()),
        _ => table.insert(&step.action, step.target.as_str().into()),
    };
    table
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
            with: None,
            kind: "oneshot".into(),
            step: None,
            hold: None,
            steps: vec![Step {
                action: "keys".into(),
                target: combo.into(),
            }],
            name: None,
        }
    }

    #[test]
    fn steps_modifiers_and_turning_an_app_off() {
        let (dir, path) = dir("steps");
        let state = dir.load(Some("blender")).unwrap();
        let mut app = state.app.unwrap();
        app.enabled = false;
        let mut copy = keys("U", "ctrl+c");
        copy.with = Some("shift".into());
        copy.steps.push(Step {
            action: "wait".into(),
            target: "150".into(),
        });
        copy.steps.push(Step {
            action: "command".into(),
            target: "notify-send copied".into(),
        });
        let mut settings = state.settings.clone();
        settings.cancel_ms = 700;
        dir.save(&settings, &[copy.clone()], std::slice::from_ref(&app)).unwrap();

        let main = std::fs::read_to_string(path.join("config.toml")).unwrap();
        assert!(main.contains("with = \"shift\""), "{main}");
        assert!(
            main.contains("action = [{ keys = \"ctrl+c\" }, { wait = 150 }, { command = \"notify-send copied\" }]"),
            "{main}"
        );
        assert!(main.contains("cancel_ms = 700"), "{main}");
        let blender = std::fs::read_to_string(path.join("apps/blender.toml")).unwrap();
        assert!(blender.contains("enabled = false"), "{blender}");

        let again = dir.load(Some("blender")).unwrap();
        assert_eq!(again.global, [copy]);
        assert_eq!(again.settings.cancel_ms, 700);
        let mut app = again.app.unwrap();
        assert!(!app.enabled);
        app.enabled = true;
        dir.save(&again.settings, &again.global, &[app]).unwrap();
        let blender = std::fs::read_to_string(path.join("apps/blender.toml")).unwrap();
        assert!(!blender.contains("enabled"), "{blender}");
    }

    #[test]
    fn presets_are_valid_gestures() {
        let presets = presets().unwrap();
        assert!(presets.len() >= 5);
        for preset in presets {
            let app = App {
                gestures: preset.gestures.clone(),
                ..App::new("x".into(), preset.classes.clone())
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
        dir.save(&state.settings, &[keys("R", "b")], std::slice::from_ref(&app))
            .unwrap();

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
        let name = dir
            .save_shape("My triangle", &[triangle.clone(), triangle.clone()], false)
            .unwrap();
        assert_eq!(name, "my_triangle");
        let again = dir.save_shape("my triangle", std::slice::from_ref(&triangle), false);
        assert!(again.unwrap_err().contains("already a shape called my_triangle"));
        assert!(
            dir.save_shape("circle", std::slice::from_ref(&triangle), false)
                .is_err()
        );
        dir.save_shape("my_triangle", &[triangle.clone(), triangle], true)
            .unwrap();
        let text = std::fs::read_to_string(path.join("shapes/my_triangle.toml")).unwrap();
        assert!(text.contains("[100, 0]"), "{text}");
        let shapes = dir.load(None).unwrap().shapes;
        let names: Vec<_> = shapes.iter().map(|s| (s.name.as_str(), s.built_in)).collect();
        assert_eq!(names.len(), mousewand_core::shape::BUILT_IN.len() + 1);
        assert_eq!(names[0], ("circle", true));
        assert_eq!(names.last(), Some(&("my_triangle", false)));
        assert_eq!(shapes.last().unwrap().outline.len(), 32);

        let gesture = Gesture {
            input: "shape".into(),
            ..keys("my_triangle", "ctrl+t")
        };
        dir.save(&dir.load(None).unwrap().settings, std::slice::from_ref(&gesture), &[])
            .unwrap();
        assert_eq!(dir.load(None).unwrap().global, [gesture]);

        assert!(dir.save_shape("dot", &[vec![[5.0, 5.0], [5.0, 5.0]]], false).is_err());
        assert!(dir.save_shape(" ", &[vec![[0.0, 0.0], [9.0, 9.0]]], false).is_err());
        assert!(!path.join("shapes/dot.toml").exists());

        // In use, then not.
        let used = dir.delete_shape("my_triangle").unwrap_err();
        assert_eq!(
            used,
            "my_triangle is still used by a gesture in config.toml; change that gesture first"
        );
        dir.save(&dir.load(None).unwrap().settings, &[], &[]).unwrap();
        dir.delete_shape("my_triangle").unwrap();
        assert!(!path.join("shapes/my_triangle.toml").exists());
        assert!(dir.delete_shape("circle").unwrap_err().contains("built in"));
    }

    #[test]
    fn open_settings_and_the_settings_window() {
        let (dir, path) = dir("open-settings");
        let state = dir.load(Some("io.github.tkutsu.mousewand")).unwrap();
        assert!(state.app.is_none(), "the settings window has its own tab");
        assert_eq!(
            state.settings.open_settings,
            Drawn {
                input: "shape".into(),
                value: "circle".into()
            }
        );
        let mut settings_app = state.settings_app.clone();
        assert_eq!(settings_app.classes, SETTINGS_CLASSES);

        // Nothing to save: no settings app file, and the default isn't written.
        dir.save(&state.settings, &[], std::slice::from_ref(&settings_app))
            .unwrap();
        assert!(!path.join("apps/mousewand-settings.toml").exists());
        let main = std::fs::read_to_string(path.join("config.toml")).unwrap();
        assert!(!main.contains("open_settings"), "{main}");

        let mut settings = state.settings.clone();
        settings.open_settings = Drawn {
            input: "stroke".into(),
            value: "U39D".into(),
        };
        settings_app.gestures.push(Gesture {
            name: Some("Save".into()),
            ..keys("39", "ctrl+s")
        });
        dir.save(&settings, &[], &[settings_app]).unwrap();
        let again = dir.load(None).unwrap();
        assert_eq!(again.settings.open_settings, settings.open_settings);
        assert_eq!(again.settings_app.gestures[0].steps[0].target, "ctrl+s");
        assert_eq!(again.settings_app.classes, SETTINGS_CLASSES);
    }

    #[test]
    fn invalid_saves_write_nothing() {
        let (dir, path) = dir("invalid");
        let state = dir.load(Some("kitty")).unwrap();
        let mut app = state.app.unwrap();
        assert_eq!(app.file, "kitty");
        app.gestures.push(keys("R", "ctrl+Tab"));
        let bad = dir.save(&state.settings, &[keys("L", "ctrl+nope")], std::slice::from_ref(&app));
        assert!(bad.unwrap_err().contains("nope"));
        assert!(!path.join("config.toml").exists());

        let mut settings = state.settings.clone();
        settings.theme = "missing".into();
        assert!(dir.save(&settings, &[], &[]).is_err());

        dir.save(&state.settings, &[keys("L", "alt+Left")], std::slice::from_ref(&app))
            .unwrap();
        let kitty = std::fs::read_to_string(path.join("apps/kitty.toml")).unwrap();
        assert!(
            !kitty.contains("match"),
            "a file named after its class needs no match: {kitty}"
        );
    }
}
