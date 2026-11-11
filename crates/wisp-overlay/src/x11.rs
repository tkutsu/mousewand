//! The trail on X11: a click-through, override-redirect window with a
//! 32-bit ARGB visual over the whole screen, mapped only during a gesture.
//! It needs a compositing manager to be transparent; without one wisp draws
//! no trail rather than a black box.

use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XcbDisplayHandle, XcbWindowHandle};
use wisp_core::Theme;
use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    ClipOrdering, ColormapAlloc, ConnectionExt as _, CreateWindowAux, VisualClass, WindowClass,
};
use x11rb::xcb_ffi::XCBConnection;

use crate::gpu::{Gpu, Target};
use crate::scene::Scene;

/// About 60 frames a second.
const FRAME: Duration = Duration::from_micros(16_667);

enum Command {
    Begin,
    Tick,
    End { matched: bool },
    Theme(Theme),
}

pub struct X11Overlay {
    tx: Sender<Command>,
    thread: thread::JoinHandle<()>,
}

impl X11Overlay {
    /// Fails without an X display, a compositing manager, or a usable GPU.
    pub fn start() -> anyhow::Result<X11Overlay> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (tx, rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("x11 overlay".into())
            .spawn(move || match Window::new() {
                Ok(mut window) => {
                    let _ = ready_tx.send(Ok(()));
                    window.run(rx);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })?;
        ready_rx.recv().context("overlay thread died")??;
        Ok(X11Overlay { tx, thread })
    }
}

impl wisp_core::Trail for X11Overlay {
    fn begin(&self, _: Option<String>) {
        let _ = self.tx.send(Command::Begin);
    }

    fn tick(&self) {
        let _ = self.tx.send(Command::Tick);
    }

    fn end(&self, matched: bool) {
        let _ = self.tx.send(Command::End { matched });
    }

    fn set_theme(&self, theme: Theme) {
        let _ = self.tx.send(Command::Theme(theme));
    }

    fn shutdown(self: Box<Self>) {
        drop(self.tx);
        let _ = self.thread.join();
    }
}

struct Window {
    // Dropped before `conn`: the wgpu surface points into it.
    target: Target,
    gpu: Gpu,
    conn: XCBConnection,
    root: u32,
    window: u32,
    size: (u16, u16),
    theme: Theme,
    scene: Option<Scene>,
    last_frame: Instant,
}

impl Window {
    fn new() -> anyhow::Result<Self> {
        let (conn, screen_num) = XCBConnection::connect(None).context("no X display")?;
        let screen = conn.setup().roots[screen_num].clone();
        let cm = conn
            .intern_atom(false, format!("_NET_WM_CM_S{screen_num}").as_bytes())?
            .reply()?
            .atom;
        if conn.get_selection_owner(cm)?.reply()?.owner == x11rb::NONE {
            bail!("no compositing manager, so no transparent windows");
        }
        let visual = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| &d.visuals)
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .context("no 32-bit visual")?
            .visual_id;
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual)?;
        let window = conn.generate_id()?;
        let size = (screen.width_in_pixels, screen.height_in_pixels);
        conn.create_window(
            32,
            window,
            screen.root,
            0,
            0,
            size.0,
            size.1,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .override_redirect(1)
                .border_pixel(0)
                .background_pixel(0)
                .colormap(colormap),
        )?;
        // An empty input region: every click goes to the windows below.
        conn.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )?;
        conn.flush()?;

        let gpu = Gpu::new()?;
        let display = NonNull::new(conn.get_raw_xcb_connection());
        let handle = NonZeroU32::new(window).context("bad window id")?;
        // Safety: `Window` drops `target` before `conn`, and never destroys
        // the X window while the surface lives.
        let mut target = unsafe {
            gpu.raw_target(
                RawDisplayHandle::Xcb(XcbDisplayHandle::new(display, screen_num as i32)),
                RawWindowHandle::Xcb(XcbWindowHandle::new(handle)),
            )
        }?;
        gpu.configure(&mut target, size.0.into(), size.1.into(), true)?;
        log::info!("X11 trail ready");
        Ok(Window {
            target,
            gpu,
            conn,
            root: screen.root,
            window,
            size,
            theme: Theme::default(),
            scene: None,
            last_frame: Instant::now(),
        })
    }

    /// Waits for commands while idle; while a gesture runs, samples the
    /// pointer and draws every frame (presenting paces the loop).
    fn run(&mut self, rx: Receiver<Command>) {
        loop {
            let command = if self.scene.is_some() {
                match rx.recv_timeout(Duration::ZERO) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                match rx.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                }
            };
            let result = match command {
                Some(command) => self.command(command),
                None => {
                    // Present doesn't always throttle on X11, and flooding
                    // the server with frames delays its input handling, so
                    // pace frames ourselves.
                    let next = self.last_frame + FRAME;
                    if let Some(wait) = next.checked_duration_since(Instant::now()) {
                        thread::sleep(wait);
                    }
                    self.last_frame = Instant::now();
                    self.frame()
                }
            };
            if let Err(e) = result {
                log::error!("X11 trail: {e:#}");
                self.scene = None;
                let _ = self.hide();
            }
        }
    }

    fn command(&mut self, command: Command) -> anyhow::Result<()> {
        match command {
            Command::Begin => {
                self.scene = Some(Scene::new());
                self.conn.map_window(self.window)?;
                self.conn.flush()?;
            }
            Command::Tick => {
                if let Some(scene) = &mut self.scene {
                    scene.tick();
                }
            }
            Command::End { matched } => {
                if let Some(scene) = &mut self.scene {
                    scene.end(matched);
                }
            }
            Command::Theme(theme) => self.theme = theme,
        }
        Ok(())
    }

    fn frame(&mut self) -> anyhow::Result<()> {
        let Some(scene) = &mut self.scene else { return Ok(()) };
        let pointer = self.conn.query_pointer(self.root)?.reply()?;
        scene.push(pointer.root_x.into(), pointer.root_y.into());
        let size = (self.size.0.into(), self.size.1.into());
        let target = &self.target;
        match scene.strips(size, &self.theme, |c| target.color(c)) {
            Some(strips) => {
                self.gpu.draw(&self.target, &strips);
                Ok(())
            }
            None => {
                self.scene = None;
                self.hide()
            }
        }
    }

    /// Clears the last frame, so the next gesture doesn't flash it, and
    /// unmaps the window.
    fn hide(&mut self) -> anyhow::Result<()> {
        self.gpu.draw(&self.target, &[]);
        self.conn.unmap_window(self.window)?;
        self.conn.flush()?;
        Ok(())
    }
}
