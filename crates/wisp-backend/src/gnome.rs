//! GNOME Shell, through wisp's Shell extension (`dist/gnome-extension`),
//! which answers over D-Bus and draws the trail, since GNOME has no
//! layer-shell. GNOME has no dispatchers to run.

use anyhow::{Context, bail};
use serde::Deserialize;
use wisp_core::{Theme, Trail};
use zbus::blocking::Connection;

use crate::{Backend, Target, Window};

pub const NAME: &str = "io.github.tkutsu.Wisp.Shell";
const PATH: &str = "/io/github/tkutsu/Wisp/Shell";

pub struct Gnome {
    bus: Connection,
}

#[derive(Deserialize)]
struct Report {
    id: String,
    class: String,
    title: String,
}

impl Gnome {
    pub fn connect() -> anyhow::Result<Self> {
        let bus = Connection::session().context("connecting to the session bus")?;
        if !crate::has_owner(&bus, NAME) {
            bail!(
                "GNOME needs wisp's Shell extension: copy dist/gnome-extension/wisp@tkutsu.github.io \
                 to ~/.local/share/gnome-shell/extensions/, log out and in, then \
                 gnome-extensions enable wisp@tkutsu.github.io"
            );
        }
        log::info!("using wisp's GNOME Shell extension");
        Ok(Gnome { bus })
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
    fn pointer_target(&self) -> anyhow::Result<Target> {
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
        Some(Box::new(GnomeTrail { bus: self.bus.clone() }))
    }
}

struct GnomeTrail {
    bus: Connection,
}

impl GnomeTrail {
    fn send<B>(&self, method: &str, body: &B)
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
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
