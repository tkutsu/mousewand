//! Reading the config directory and watching it for changes.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use mousewand_core::{Config, Theme};
use notify::{RecursiveMode, Watcher};

/// `$MOUSEWAND_CONFIG_DIR`, else `$XDG_CONFIG_HOME/mousewand`, else `~/.config/mousewand`.
pub fn dir() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("MOUSEWAND_CONFIG_DIR") {
        return Ok(dir.into());
    }
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(base) => PathBuf::from(base),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".config"),
    };
    Ok(base.join("mousewand"))
}

/// Loads `config.toml`, `apps/*.toml` and `shapes/*.toml`. Missing files
/// mean defaults.
pub fn load(dir: &Path) -> anyhow::Result<Config> {
    let main = read_optional(&dir.join("config.toml"))?;
    let apps = toml_files(&dir.join("apps"))?;
    let shapes = toml_files(&dir.join("shapes"))?;
    let mut config = Config::parse(&main, &apps, &shapes).map_err(anyhow::Error::msg)?;
    config.theme = theme(dir, &config.settings.theme)?;
    Ok(config)
}

/// Each `*.toml` in `dir` as `(file stem, text)`, sorted. No directory is
/// no files.
fn toml_files(dir: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for path in entries.filter_map(|e| Some(e.ok()?.path())) {
            if path.extension().is_some_and(|ext| ext == "toml") {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
                files.push((stem, text));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// `themes/<name>.toml`. "default" is built in, unless that file exists.
fn theme(dir: &Path, name: &str) -> anyhow::Result<Theme> {
    let path = dir.join("themes").join(format!("{name}.toml"));
    let text = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && name == "default" => return Ok(Theme::default()),
        other => other.with_context(|| format!("reading theme {}", path.display()))?,
    };
    Theme::parse(name, &text).map_err(anyhow::Error::msg)
}

fn read_optional(path: &Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        other => other.with_context(|| format!("reading {}", path.display())),
    }
}

/// Calls `changed` after the directory settles following a change. The
/// whole directory is watched: editors that save by renaming replace files.
pub fn watch(dir: &Path, changed: impl Fn() + Send + 'static) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|e| !e.kind.is_access()) {
            let _ = Sender::send(&tx, ());
        }
    })?;
    watcher.watch(dir, RecursiveMode::Recursive)?;
    thread::Builder::new().name("config watch".into()).spawn(move || {
        let _watcher = watcher;
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(150)).is_ok() {}
            changed();
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn examples_parse() {
        let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let config = super::load(&examples).unwrap();
        assert_eq!(config.apps.len(), 2);
        let l = mousewand_core::Input::Stroke("L".parse().unwrap());
        assert!(config.gestures_for("firefox").iter().any(|g| g.input == l));
        assert_eq!(config.global.len(), 3);
        assert!(config.hint.enabled);
    }
}
