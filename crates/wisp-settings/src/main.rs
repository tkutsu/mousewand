//! wisp-settings: edits wisp's config files. The settings gesture starts it
//! with `WISP_CLASS` set to the window it was drawn over, so it opens on
//! that app. `wisp-settings --class <class>` does the same by hand.

mod files;

use std::path::PathBuf;

use files::{App, Dir, Gesture, Settings, State};

struct Context {
    dir: Dir,
    class: Option<String>,
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
    app: Option<App>,
) -> Result<(), String> {
    context.dir.save(&settings, &global, app.as_ref())
}

/// The same lookup as the daemon's.
fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WISP_CONFIG_DIR") {
        return dir.into();
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("wisp")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let class = match args.iter().position(|a| a == "--class") {
        Some(i) => args.get(i + 1).cloned(),
        None => std::env::var("WISP_CLASS").ok(),
    };
    tauri::Builder::default()
        .manage(Context {
            dir: Dir(config_dir()),
            class,
        })
        .invoke_handler(tauri::generate_handler![load, save])
        .run(tauri::generate_context!())
        .expect("wisp-settings failed to start");
}
