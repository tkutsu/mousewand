//! GNOME Shell, through wisp's Shell extension (`dist/gnome-extension`),
//! which answers over D-Bus and draws the trail, since GNOME has no
//! layer-shell. GNOME has no dispatchers to run.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, bail};
use serde::Deserialize;
use wisp_core::{Theme, Trail};
use zbus::blocking::Connection;

use crate::{Backend, Target, Window};

pub const NAME: &str = "io.github.tkutsu.Wisp.Shell";
const PATH: &str = "/io/github/tkutsu/Wisp/Shell";
const UUID: &str = "wisp@tkutsu.github.io";

/// The extension, built in so wisp can install it wherever it was installed
/// from. Bump `version` in its metadata with every change, so users hear
/// about the update.
const FILES: [(&str, &str); 3] = [
    (
        "metadata.json",
        include_str!("../../../dist/gnome-extension/wisp@tkutsu.github.io/metadata.json"),
    ),
    (
        "extension.js",
        include_str!("../../../dist/gnome-extension/wisp@tkutsu.github.io/extension.js"),
    ),
    (
        "trail.js",
        include_str!("../../../dist/gnome-extension/wisp@tkutsu.github.io/trail.js"),
    ),
];

#[derive(PartialEq)]
enum Install {
    Current,
    Installed,
    Updated,
}

/// Writes the extension's files where they differ from wisp's.
fn install(dir: &Path) -> std::io::Result<Install> {
    let new = !dir.exists();
    let mut changed = false;
    std::fs::create_dir_all(dir)?;
    for (name, text) in FILES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
            std::fs::write(&path, text)?;
            changed = true;
        }
    }
    Ok(match (new, changed) {
        (true, _) => Install::Installed,
        (false, true) => Install::Updated,
        (false, false) => Install::Current,
    })
}

fn version() -> u64 {
    serde_json::from_str::<serde_json::Value>(FILES[0].1)
        .ok()
        .and_then(|m| m["version"].as_u64())
        .unwrap_or_default()
}

fn gsettings_get(key: &str) -> anyhow::Result<String> {
    let out = Command::new("gsettings")
        .args(["get", "org.gnome.shell", key])
        .output()?;
    if !out.status.success() {
        bail!("gsettings get {key}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Adds the extension to the Shell's list. `gnome-extensions enable` can't:
/// the Shell only knows extensions that were there when it started.
fn enable() -> anyhow::Result<()> {
    // A GVariant string array: `['a@b', 'c@d']`, or `@as []` when empty.
    let current = gsettings_get("enabled-extensions")?;
    let mut uuids: Vec<&str> = current.split('\'').skip(1).step_by(2).collect();
    if uuids.contains(&UUID) {
        return Ok(());
    }
    uuids.push(UUID);
    let list = format!(
        "[{}]",
        uuids.iter().map(|u| format!("'{u}'")).collect::<Vec<_>>().join(", ")
    );
    let status = Command::new("gsettings")
        .args(["set", "org.gnome.shell", "enabled-extensions", &list])
        .status()?;
    if !status.success() {
        bail!("gsettings set enabled-extensions failed");
    }
    Ok(())
}

pub struct Gnome {
    bus: Connection,
    /// Whether the extension answers, as of the last gesture: until the
    /// login after installing, it doesn't, and wisp quietly does without.
    live: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct Report {
    id: String,
    class: String,
    title: String,
}

impl Gnome {
    /// Works before the extension runs, too: `setup` installs it, and until
    /// the next login gestures get the global set, without a trail.
    pub fn connect() -> anyhow::Result<Self> {
        let bus = Connection::session().context("connecting to the session bus")?;
        log::info!("using GNOME Shell");
        let live = Arc::new(AtomicBool::new(crate::has_owner(&bus, NAME)));
        Ok(Gnome { bus, live })
    }

    fn call<B, R>(&self, method: &str, body: &B) -> anyhow::Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'de> Deserialize<'de> + zbus::zvariant::Type,
    {
        let reply = self
            .bus
            .call_method(Some(NAME), PATH, Some(NAME), method, body)
            .with_context(|| format!("GNOME Shell {method}"))?;
        Ok(reply.body().deserialize()?)
    }
}

impl Backend for Gnome {
    /// Installs or updates the extension, and enables it the first time.
    /// GNOME loads new extension code only at login.
    fn setup(&mut self) -> Vec<String> {
        let Some(dir) = crate::xdg_home("DATA", ".local/share").map(|d| d.join("gnome-shell/extensions").join(UUID))
        else {
            return Vec::new();
        };
        let running = self.live.load(Ordering::Relaxed);
        let install = match install(&dir) {
            Ok(install) => install,
            Err(e) => {
                return vec![format!(
                    "wisp couldn't install its GNOME Shell extension in `{}`: {e}. Until it's there, gestures \
                     work, but only the ones for every app, and without the trail.",
                    dir.display()
                )];
            }
        };
        let mut notes = Vec::new();
        if install == Install::Installed {
            log::info!("installed the GNOME Shell extension in {}", dir.display());
            if let Err(e) = enable() {
                log::warn!("can't enable the extension: {e:#}");
            }
        }
        if !running {
            notes.push(
                "wisp installed its GNOME Shell extension, and GNOME starts new extensions only at login: log \
                 out and back in once. Until then, gestures work, but only the ones for every app, and \
                 without the trail. If it still isn't running after that, turn on wisp in the Extensions app."
                    .to_owned(),
            );
        } else if install == Install::Updated {
            notes.push(format!(
                "wisp updated its GNOME Shell extension to version {}. Log out and back in to load it.",
                version()
            ));
        }
        if gsettings_get("disable-user-extensions").is_ok_and(|v| v == "true") {
            notes.push(
                "GNOME has user extensions turned off, wisp's included. Turn them on with the switch at the \
                 top of the Extensions app."
                    .to_owned(),
            );
        }
        notes
    }

    fn pointer_target(&self) -> anyhow::Result<Target> {
        let live = crate::has_owner(&self.bus, NAME);
        self.live.store(live, Ordering::Relaxed);
        if !live {
            return Ok(Target::default());
        }
        let json: String = self.call("PointerTarget", &())?;
        let report: Report = serde_json::from_str(&json)?;
        Ok(Target {
            window: (!report.id.is_empty()).then_some(Window {
                id: report.id,
                class: report.class,
                title: report.title,
            }),
            monitor: None,
        })
    }

    fn focus(&self, window: &Window) -> anyhow::Result<()> {
        let done: bool = self.call("Focus", &(window.id.as_str(),))?;
        if !done {
            bail!("the window is gone");
        }
        Ok(())
    }

    fn dispatch(&self, _: &str) -> anyhow::Result<()> {
        bail!("GNOME has no dispatchers; use a keys or command action")
    }

    /// GNOME has no layer-shell, so the extension draws the trail inside
    /// the Shell.
    fn trail(&self) -> Option<Box<dyn Trail>> {
        Some(Box::new(GnomeTrail {
            bus: self.bus.clone(),
            live: self.live.clone(),
        }))
    }
}

struct GnomeTrail {
    bus: Connection,
    live: Arc<AtomicBool>,
}

impl GnomeTrail {
    fn send<B>(&self, method: &str, body: &B)
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        if !self.live.load(Ordering::Relaxed) {
            return;
        }
        if let Err(e) = self.bus.call_method(Some(NAME), PATH, Some(NAME), method, body) {
            log::warn!("GNOME trail {method}: {e}");
        }
    }
}

impl Trail for GnomeTrail {
    /// The extension draws across all monitors, so `monitor` isn't needed.
    fn begin(&self, _: Option<String>) {
        self.send("Begin", &());
    }

    fn tick(&self) {
        self.send("Tick", &());
    }

    fn end(&self, matched: bool) {
        self.send("End", &(matched,));
    }

    fn set_theme(&self, theme: Theme) {
        let json = serde_json::json!({
            "color": theme.color.0,
            "rejected": theme.rejected.0,
            "width": theme.width,
            "lifeMs": theme.life_ms,
            "fadeMs": theme.fade_ms,
            "glow": theme.glow,
        });
        self.send("SetTheme", &(json.to_string(),));
    }
}
