//! X11 desktops (Xfce, Cinnamon, MATE, i3, …), through the X server and
//! the window manager's EWMH support.

use anyhow::{Context, bail};
use x11rb::connection::Connection;
use x11rb::properties::WmClass;
use x11rb::protocol::xproto::{Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, Window as XWindow};
use x11rb::rust_connection::RustConnection;

use crate::{Backend, Target, Window};

pub struct X11 {
    conn: RustConnection,
    root: XWindow,
    wm_state: Atom,
    net_wm_name: Atom,
    utf8_string: Atom,
    net_active_window: Atom,
}

impl X11 {
    pub fn connect() -> anyhow::Result<Self> {
        let (conn, screen) = x11rb::connect(None).context("no X display")?;
        let root = conn.setup().roots[screen].root;
        let atom = |name: &str| -> anyhow::Result<Atom> { Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom) };
        let (wm_state, net_wm_name, utf8_string, net_active_window) = (
            atom("WM_STATE")?,
            atom("_NET_WM_NAME")?,
            atom("UTF8_STRING")?,
            atom("_NET_ACTIVE_WINDOW")?,
        );
        log::info!("using X11");
        Ok(X11 {
            conn,
            root,
            wm_state,
            net_wm_name,
            utf8_string,
            net_active_window,
        })
    }

    /// The application window inside a window manager's frame: the one
    /// with `WM_STATE`, at most a few levels down.
    fn client(&self, window: XWindow, depth: u8) -> anyhow::Result<Option<XWindow>> {
        let state = self
            .conn
            .get_property(false, window, self.wm_state, AtomEnum::ANY, 0, 0)?
            .reply()?;
        if state.type_ != x11rb::NONE {
            return Ok(Some(window));
        }
        if depth == 0 {
            return Ok(None);
        }
        // Children come bottom first; the topmost is likeliest.
        for child in self.conn.query_tree(window)?.reply()?.children.into_iter().rev() {
            if let Some(client) = self.client(child, depth - 1)? {
                return Ok(Some(client));
            }
        }
        Ok(None)
    }

    /// The window the window manager reports as active.
    fn active(&self) -> anyhow::Result<Option<XWindow>> {
        let reply = self
            .conn
            .get_property(false, self.root, self.net_active_window, AtomEnum::WINDOW, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut v| v.next()).filter(|&w| w != x11rb::NONE))
    }

    fn title(&self, window: XWindow) -> anyhow::Result<String> {
        let utf8 = self
            .conn
            .get_property(false, window, self.net_wm_name, self.utf8_string, 0, 1024)?
            .reply()?;
        let bytes = if utf8.value.is_empty() {
            self.conn
                .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 1024)?
                .reply()?
                .value
        } else {
            utf8.value
        };
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Backend for X11 {
    fn pointer_target(&self) -> anyhow::Result<Target> {
        let pointer = self.conn.query_pointer(self.root)?.reply()?;
        // `child` is the top-level window under the pointer, usually a frame.
        let client = match pointer.child {
            x11rb::NONE => None,
            child => self.client(child, 3)?,
        };
        let window = match client {
            Some(client) => {
                // WM_CLASS is "instance\0class\0"; the class is what users
                // know, e.g. "firefox", "XTerm".
                let class = WmClass::get(&self.conn, client)?
                    .reply()?
                    .map(|c| String::from_utf8_lossy(c.class()).into_owned())
                    .unwrap_or_default();
                Some(Window {
                    id: client.to_string(),
                    class,
                    title: self.title(client)?,
                })
            }
            None => None,
        };
        Ok(Target { window, monitor: None })
    }

    /// Asks the window manager to activate it, the EWMH way, and waits until
    /// it has: the request is asynchronous, and keys sent before the switch
    /// would reach the window that had focus.
    fn focus(&self, window: &Window) -> anyhow::Result<()> {
        let id: XWindow = window.id.parse()?;
        if self.active()? == Some(id) {
            return Ok(());
        }
        // Source 2: a pager or similar tool acting for the user.
        let event = ClientMessageEvent::new(32, id, self.net_active_window, [2, x11rb::CURRENT_TIME, 0, 0, 0]);
        self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )?;
        self.conn.flush()?;
        for _ in 0..50 {
            if self.active()? == Some(id) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        log::warn!("the window manager didn't focus {:?} in time", window.class);
        Ok(())
    }

    fn dispatch(&self, _: &str) -> anyhow::Result<()> {
        bail!("X11 has no dispatchers; use a keys or command action")
    }
}
