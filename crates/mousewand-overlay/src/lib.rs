//! The gesture trail. A layer-shell surface exists only while a gesture is
//! drawn: plain clicks never create one, and a surface that isn't there
//! can't block fullscreen direct scanout or eat clicks if mousewand crashes.

mod gpu;
mod hint;
mod scene;
mod trail;
mod x11;

pub use x11::X11Overlay;

use std::sync::mpsc;
use std::thread;

use anyhow::Context;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{EventLoop, channel};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_pointer, wl_seat, wl_surface};
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::{delegate_registry, registry_handlers};

use gpu::{Gpu, Quad, Target};
use hint::{Image, Painter};
use mousewand_core::{Hint, HintStyle, Theme};
use scene::Scene;

/// The layer namespace, for compositors' layer rules.
pub const NAMESPACE: &str = "mousewand";

enum Command {
    Begin { monitor: Option<String> },
    Moved(f32, f32),
    Tick,
    End { matched: bool },
    Theme(Theme),
    HintStyle(HintStyle),
    Hint(Hint),
}

/// Controls the overlay thread.
pub struct Overlay {
    tx: channel::Sender<Command>,
    thread: thread::JoinHandle<()>,
}

impl Overlay {
    /// Connects to Wayland and starts the GPU. Fails if the compositor has
    /// no layer shell or there's no usable GPU.
    pub fn start() -> anyhow::Result<Overlay> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (tx, rx) = channel::channel();
        let thread = thread::Builder::new().name("overlay".into()).spawn(move || {
            match State::new(rx) {
                Ok((mut state, mut event_loop)) => {
                    let _ = ready_tx.send(Ok(()));
                    if let Err(e) = event_loop.run(None, &mut state, |_| {}) {
                        log::error!("overlay stopped: {e}");
                    }
                    // Surfaces go before the GPU device, on this thread.
                    state.active = None;
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            };
        })?;
        ready_rx.recv().context("overlay thread died")??;
        Ok(Overlay { tx, thread })
    }
}

impl mousewand_core::Trail for Overlay {
    /// Shows the trail on the named monitor, or wherever the compositor
    /// puts it if `None`.
    fn begin(&self, monitor: Option<String>) {
        let _ = self.tx.send(Command::Begin { monitor });
    }

    fn moved(&self, dx: f64, dy: f64) {
        let _ = self.tx.send(Command::Moved(dx as f32, dy as f32));
    }

    fn tick(&self) {
        let _ = self.tx.send(Command::Tick);
    }

    /// Lets clicks through and fades the trail out.
    fn end(&self, matched: bool) {
        let _ = self.tx.send(Command::End { matched });
    }

    fn set_theme(&self, theme: Theme) {
        let _ = self.tx.send(Command::Theme(theme));
    }

    fn set_hint_style(&self, style: HintStyle) {
        let _ = self.tx.send(Command::HintStyle(style));
    }

    fn hint(&self, hint: Hint) {
        let _ = self.tx.send(Command::Hint(hint));
    }

    /// Stops the overlay thread and frees the GPU before the process exits;
    /// tearing down a live Vulkan device at exit can crash the driver.
    fn shutdown(self: Box<Self>) {
        drop(self.tx);
        let _ = self.thread.join();
    }
}

struct State {
    conn: Connection,
    qh: QueueHandle<State>,
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    cursor_shapes: Option<CursorShapeManager>,
    pointer: Option<(wl_pointer::WlPointer, Option<WpCursorShapeDeviceV1>)>,
    gpu: Gpu,
    theme: Theme,
    hint_style: HintStyle,
    painter: Painter,
    active: Option<Active>,
}

/// The surface of the gesture being drawn.
struct Active {
    // Dropped before `layer`: the wgpu surface points into it.
    target: Target,
    layer: LayerSurface,
    /// Logical size, once configured.
    size: Option<(u32, u32)>,
    scale: i32,
    scene: Scene,
    hint: Option<Hint>,
    /// `hint`, painted at `scale`.
    hint_image: Option<Image>,
}

impl State {
    fn new(rx: channel::Channel<Command>) -> anyhow::Result<(State, EventLoop<'static, State>)> {
        let conn = Connection::connect_to_env().context("no Wayland display")?;
        let (globals, queue) = registry_queue_init(&conn)?;
        let qh = queue.handle();
        let event_loop = EventLoop::<State>::try_new()?;
        WaylandSource::new(conn.clone(), queue)
            .insert(event_loop.handle())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let stop = event_loop.get_signal();
        event_loop
            .handle()
            .insert_source(rx, move |event, _, state| match event {
                channel::Event::Msg(command) => state.command(command),
                channel::Event::Closed => stop.stop(),
            })
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let state = State {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
            compositor: CompositorState::bind(&globals, &qh).context("no wl_compositor")?,
            layer_shell: LayerShell::bind(&globals, &qh).context("the compositor has no layer shell")?,
            cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
            pointer: None,
            gpu: Gpu::new()?,
            theme: Theme::default(),
            hint_style: HintStyle::default(),
            painter: Painter::default(),
            active: None,
            conn,
            qh,
        };
        Ok((state, event_loop))
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Begin { monitor } => {
                // A new gesture replaces one that's still fading.
                self.active = None;
                if let Err(e) = self.begin(monitor) {
                    log::error!("can't show the trail: {e:#}");
                }
            }
            Command::Theme(theme) => self.theme = theme,
            Command::HintStyle(style) => self.hint_style = style,
            Command::Hint(hint) => {
                if let Some(active) = &mut self.active {
                    active.hint = Some(hint);
                    self.paint_hint();
                }
            }
            Command::Moved(dx, dy) => {
                if let Some(active) = &mut self.active {
                    active.scene.moved(dx, dy);
                }
            }
            Command::Tick => {
                if let Some(active) = &mut self.active {
                    active.scene.tick();
                }
            }
            Command::End { matched } => {
                let Some(active) = &mut self.active else { return };
                active.scene.end(matched);
                // From here on clicks go to the windows below.
                if let Ok(region) = Region::new(&self.compositor) {
                    active.layer.set_input_region(Some(region.wl_region()));
                    active.layer.commit();
                }
            }
        }
    }

    fn begin(&mut self, monitor: Option<String>) -> anyhow::Result<()> {
        let output = monitor.and_then(|name| {
            self.outputs
                .outputs()
                .find(|o| self.outputs.info(o).and_then(|i| i.name).as_ref() == Some(&name))
        });
        let surface = self.compositor.create_surface(&self.qh);
        let target = self.gpu.target(&self.conn, &surface)?;
        let layer =
            self.layer_shell
                .create_layer_surface(&self.qh, surface, Layer::Overlay, Some(NAMESPACE), output.as_ref());
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        layer.commit();
        self.active = Some(Active {
            target,
            layer,
            size: None,
            scale: 1,
            scene: Scene::new(),
            hint: None,
            hint_image: None,
        });
        Ok(())
    }

    /// Paints the hint at the surface's scale.
    fn paint_hint(&mut self) {
        let Some(active) = &mut self.active else { return };
        active.hint_image = match (&active.hint, self.hint_style.enabled) {
            (Some(hint), true) => self.painter.paint(hint, &self.hint_style, active.scale.max(1) as f32),
            _ => None,
        };
    }

    fn configure_surface(&mut self) {
        let Some(active) = &mut self.active else { return };
        let Some((w, h)) = active.size else { return };
        let scale = active.scale.max(1);
        active.layer.wl_surface().set_buffer_scale(scale);
        if let Err(e) = self
            .gpu
            .configure(&mut active.target, w * scale as u32, h * scale as u32, false)
        {
            log::error!("overlay disabled for this gesture: {e:#}");
            self.active = None;
        }
    }

    fn draw(&mut self) {
        let Some(active) = &mut self.active else { return };
        let Some((w, h)) = active.size else { return };
        let target = &active.target;
        let Some(strips) = active
            .scene
            .strips((w as f32, h as f32), &self.theme, |c| target.color(c))
        else {
            self.active = None;
            return;
        };
        let scale = active.scale.max(1) as u32;
        let hint = active.hint_image.as_ref().map(|image| {
            let (x, y) = hint::place(&self.hint_style, image, (w * scale, h * scale), scale as f32);
            Quad {
                image,
                x,
                y,
                alpha: self.hint_style.opacity * active.scene.fade(&self.theme).max(0.0),
            }
        });
        let surface = active.layer.wl_surface();
        // Ask for the next frame before presenting, which commits.
        surface.frame(&self.qh, FrameCallbackData(surface.clone()));
        if !self.gpu.draw(&active.target, &strips, hint) {
            surface.commit();
        }
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if let Some(active) = &mut self.active
            && active.layer.wl_surface() == surface
            && active.scale != factor
        {
            active.scale = factor;
            self.configure_surface();
            self.paint_hint();
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if self.active.as_ref().is_some_and(|a| a.layer.wl_surface() == surface) {
            self.draw();
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.active.as_ref().is_some_and(|a| &a.layer == layer) {
            self.active = None;
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(active) = &mut self.active else { return };
        if &active.layer != layer {
            return;
        }
        let first = active.size.is_none();
        active.size = Some(configure.new_size);
        self.configure_surface();
        if first {
            self.draw();
        }
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(active) = &mut self.active else { return };
        for event in events.iter().filter(|e| &e.surface == active.layer.wl_surface()) {
            match event.kind {
                PointerEventKind::Enter { serial } => {
                    // Keep the normal arrow while drawing.
                    if let Some((_, Some(device))) = &self.pointer {
                        device.set_shape(serial, Shape::Default);
                    }
                }
                PointerEventKind::Motion { .. } => {}
                _ => continue,
            }
            let (x, y) = event.position;
            active.scene.push(x as f32, y as f32);
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => {
                    let device = self.cursor_shapes.as_ref().map(|m| m.get_shape_device(&pointer, qh));
                    self.pointer = Some((pointer, device));
                }
                Err(e) => log::warn!("no pointer: {e}"),
            }
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer
            && let Some((pointer, device)) = self.pointer.take()
        {
            if let Some(device) = device {
                device.destroy();
            }
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(State);
smithay_client_toolkit::delegate_dispatch2!(State);
