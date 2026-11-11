//! mousewand-settings: edits mousewand's config files. The settings gesture starts it
//! with `MOUSEWAND_CLASS` set to the window it was drawn over, so it opens on
//! that app. `mousewand-settings --class <class>` does the same by hand, and
//! `--notes` opens it on the daemon's setup notes. Only one runs: starting
//! another brings the open one to the front instead.

mod files;

use std::path::PathBuf;

use files::{App, Dir, Gesture, Settings, State};
use tauri::Manager;

struct Context {
    dir: Dir,
    class: Option<String>,
    open_notes: bool,
}

#[derive(serde::Serialize)]
struct Notes {
    notes: Vec<String>,
    /// Whether to open on them.
    open: bool,
}

#[tauri::command]
fn load(context: tauri::State<Context>) -> Result<State, String> {
    context.dir.load(context.class.as_deref())
}

#[tauri::command]
fn save(
    context: tauri::State<Context>,
    settings: Settings,
    global: Vec<Gesture>,
    apps: Vec<App>,
) -> Result<(), String> {
    context.dir.save(&settings, &global, &apps)
}

/// Saves a shape drawn in the window; returns the name it was saved under.
#[tauri::command]
fn save_shape(
    context: tauri::State<Context>,
    name: String,
    examples: Vec<Vec<[f64; 2]>>,
    redraw: bool,
) -> Result<String, String> {
    context.dir.save_shape(&name, &examples, redraw)
}

#[tauri::command]
fn delete_shape(context: tauri::State<Context>, name: String) -> Result<(), String> {
    context.dir.delete_shape(&name)
}

/// Whether a new drawing of a shape looks like the ones before it.
#[tauri::command]
fn alike(examples: Vec<Vec<(f64, f64)>>, drawing: Vec<(f64, f64)>) -> bool {
    mousewand_core::shape::alike(&examples, &drawing)
}

/// Whether mousewand can see which modifiers are held, which gestures
/// `with` them need.
#[tauri::command]
fn keyboard_access() -> bool {
    mousewand_input::keyboards_readable()
}

/// What the daemon's setup said on this compositor, in
/// `$XDG_STATE_HOME/mousewand/notes`, one per line.
#[tauri::command]
fn notes(context: tauri::State<Context>) -> Notes {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    let text = std::fs::read_to_string(base.join("mousewand/notes")).unwrap_or_default();
    Notes {
        notes: text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect(),
        open: context.open_notes,
    }
}

/// The same lookup as the daemon's.
fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MOUSEWAND_CONFIG_DIR") {
        return dir.into();
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("mousewand")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let class = match args.iter().position(|a| a == "--class") {
        Some(i) => args.get(i + 1).cloned(),
        None => std::env::var("MOUSEWAND_CLASS").ok(),
    };
    tauri::Builder::default()
        // First, so a second start exits before opening anything.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(Context {
            dir: Dir(config_dir()),
            class,
            open_notes: args.iter().any(|a| a == "--notes"),
        })
        .invoke_handler(tauri::generate_handler![
            load,
            save,
            save_shape,
            delete_shape,
            alike,
            keyboard_access,
            notes
        ])
        .run(tauri::generate_context!())
        .expect("mousewand-settings failed to start");
}
