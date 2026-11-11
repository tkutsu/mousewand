//! KWin (Plasma 6), through short KWin scripts loaded over D-Bus.
//!
//! KWin has no D-Bus call for "the window under the cursor", but its
//! scripts can see the cursor and the stacking order, and can call D-Bus.
//! So each query loads a one-shot script that reports back to a tiny D-Bus
//! object mousewand serves, then unloads it. Scripts use only KWin's documented
//! scripting API, so Plasma updates don't break them.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::{Context, bail};
use serde::Deserialize;
use zbus::blocking::Connection;

use crate::{Backend, Target, Window};

const NAME: &str = "io.github.tkutsu.Mousewand";
const PATH: &str = "/io/github/tkutsu/Mousewand";

pub struct KWin {
    bus: Connection,
    replies: Mutex<Receiver<String>>,
}

/// Receives what scripts report.
struct Reporter {
    tx: Mutex<Sender<String>>,
}

#[zbus::interface(name = "io.github.tkutsu.Mousewand")]
impl Reporter {
    fn report(&self, json: String) {
        let _ = self.tx.lock().unwrap().send(json);
    }
}

/// Finds the topmost normal window under the cursor on the current desktop.
const TARGET: &str = r#"
const p = workspace.cursorPos;
const order = workspace.stackingOrder;
let found = null;
for (let i = order.length - 1; i >= 0; i--) {
    const w = order[i];
    if (!(w.normalWindow || w.dialog) || w.minimized) continue;
    if (!w.onAllDesktops && !w.desktops.includes(workspace.currentDesktop)) continue;
    const g = w.frameGeometry;
    if (p.x >= g.x && p.x < g.x + g.width && p.y >= g.y && p.y < g.y + g.height) {
        found = w;
        break;
    }
}
const screen = workspace.screenAt(p);
REPORT(JSON.stringify({
    id: found ? found.internalId.toString() : "",
    class: found ? found.resourceClass : "",
    title: found ? found.caption : "",
    monitor: screen ? screen.name : "",
}));
"#;

const FOCUS: &str = r#"
const id = ID;
let done = false;
for (const w of workspace.stackingOrder) {
    if (w.internalId.toString() === id) {
        workspace.activeWindow = w;
        done = true;
        break;
    }
}
REPORT(done ? "ok" : "gone");
"#;

#[derive(Deserialize)]
struct Report {
    id: String,
    class: String,
    title: String,
    monitor: String,
}

impl KWin {
    pub fn connect() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let bus = zbus::blocking::connection::Builder::session()?
            .name(NAME)?
            .serve_at(PATH, Reporter { tx: Mutex::new(tx) })?
            .build()
            .context("connecting to the session bus")?;
        log::info!("using KWin scripting over D-Bus");
        Ok(KWin {
            bus,
            replies: Mutex::new(rx),
        })
    }

    /// Runs a script and returns what it reported.
    fn run(&self, script: &str) -> anyhow::Result<String> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let plugin = format!(
            "mousewand-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let report = format!("(text => callDBus(\"{NAME}\", \"{PATH}\", \"{NAME}\", \"Report\", text))");
        let source = script.replace("REPORT", &report);
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
        let file = std::path::Path::new(&runtime).join(format!("{plugin}.js"));
        std::fs::write(&file, source)?;

        let replies = self.replies.lock().unwrap();
        while replies.try_recv().is_ok() {}
        let result = self.load_and_run(&file, &plugin).and_then(|()| {
            replies
                .recv_timeout(Duration::from_secs(1))
                .context("KWin script didn't report back")
        });
        let _ = self.call(
            "/Scripting",
            "org.kde.kwin.Scripting",
            "unloadScript",
            &(plugin.as_str(),),
        );
        let _ = std::fs::remove_file(&file);
        result
    }

    fn load_and_run(&self, file: &std::path::Path, plugin: &str) -> anyhow::Result<()> {
        let path = file.to_str().context("non-UTF-8 runtime dir")?;
        let id: i32 = self
            .call("/Scripting", "org.kde.kwin.Scripting", "loadScript", &(path, plugin))?
            .body()
            .deserialize()?;
        if id < 0 {
            bail!("KWin refused the script");
        }
        self.call(&format!("/Scripting/Script{id}"), "org.kde.kwin.Script", "run", &())?;
        Ok(())
    }

    fn call<B>(&self, path: &str, iface: &str, method: &str, body: &B) -> anyhow::Result<zbus::Message>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.bus
            .call_method(Some("org.kde.KWin"), path, Some(iface), method, body)
            .with_context(|| format!("KWin {method}"))
    }
}

impl Backend for KWin {
    fn pointer_target(&self) -> anyhow::Result<Target> {
        let report: Report = serde_json::from_str(&self.run(TARGET)?)?;
        let nonempty = |s: String| (!s.is_empty()).then_some(s);
        Ok(Target {
            window: nonempty(report.id).map(|id| Window {
                id,
                class: report.class,
                title: report.title,
            }),
            monitor: nonempty(report.monitor),
        })
    }

    fn focus(&self, window: &Window) -> anyhow::Result<()> {
        let id = serde_json::to_string(&window.id)?;
        match self.run(&FOCUS.replace("ID", &id))?.as_str() {
            "ok" => Ok(()),
            _ => bail!("the window is gone"),
        }
    }

    /// Invokes a KWin global shortcut by its name, e.g. "Switch to Next Desktop".
    fn dispatch(&self, shortcut: &str) -> anyhow::Result<()> {
        self.bus
            .call_method(
                Some("org.kde.kglobalaccel"),
                "/component/kwin",
                Some("org.kde.kglobalaccel.Component"),
                "invokeShortcut",
                &(shortcut,),
            )
            .with_context(|| format!("KWin shortcut {shortcut:?}"))?;
        Ok(())
    }
}
