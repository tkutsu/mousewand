//! GNOME Shell, through wisp's Shell extension (`dist/gnome-extension`),
//! which answers over D-Bus. GNOME has no dispatchers to run, and no
//! layer-shell for the trail.

use anyhow::{Context, bail};
use serde::Deserialize;
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
}
