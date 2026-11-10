//! The gesture trail. A layer-shell surface exists only while a gesture is
//! drawn: plain clicks never create one, and a surface that isn't there
//! can't block fullscreen direct scanout or eat clicks if wisp crashes.

mod gpu;
mod trail;

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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

use gpu::{Gpu, Target};
use trail::Trail;

/// The layer namespace, so users can write `layerrule = noanim, wisp`.
pub const NAMESPACE: &str = "wisp";
/// How long the trail fades after release.
const END_FADE: Duration = Duration::from_millis(300);
const COLOR: [f32; 3] = [0.45, 0.75, 1.0];
const REJECTED: [f32; 3] = [1.0, 0.35, 0.35];

enum Command {
    Begin { monitor: Option<String> },
    End { matched: bool },
}

/// Controls the overlay thread.
pub struct Overlay {
    tx: channel::Sender<Command>,
}

impl Overlay {
    /// Connects to Wayland and starts the GPU. Fails if the compositor has
    /// no layer shell or there's no usable GPU.
    pub fn start() -> anyhow::Result<Overlay> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (tx, rx) = channel::channel();
        thread::Builder::new().name("overlay".into()).spawn(move || {
            match State::new(rx) {
                Ok((mut state, mut event_loop)) => {
                    let _ = ready_tx.send(Ok(()));
                    if let Err(e) = event_loop.run(None, &mut state, |_| {}) {
                        log::error!("overlay stopped: {e}");
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            };
        })?;
        ready_rx.recv().context("overlay thread died")??;
        Ok(Overlay { tx })
    }

    /// Shows the trail on the named monitor, or wherever the compositor
    /// puts it if `None`.
    pub fn begin(&self, monitor: Option<String>) {
        let _ = self.tx.send(Command::Begin { monitor });
    }

    /// Lets clicks through and fades the trail out.
    pub fn end(&self, matched: bool) {
        let _ = self.tx.send(Command::End { matched });
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
    trail: Trail,
    start: Instant,
    ended: Option<(Instant, bool)>,
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
        event_loop
            .handle()
            .insert_source(rx, |event, _, state| {
                if let channel::Event::Msg(command) = event {
                    state.command(command);
                }
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
            Command::End { matched } => {
                let Some(active) = &mut self.active else { return };
                active.ended = Some((Instant::now(), matched));
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
            trail: Trail::default(),
            start: Instant::now(),
            ended: None,
        });
        Ok(())
    }

    fn configure_surface(&mut self) {
        let Some(active) = &mut self.active else { return };
        let Some((w, h)) = active.size else { return };
        let scale = active.scale.max(1);
        active.layer.wl_surface().set_buffer_scale(scale);
        if let Err(e) = self
            .gpu
            .configure(&mut active.target, w * scale as u32, h * scale as u32)
        {
            log::error!("overlay disabled for this gesture: {e:#}");
            self.active = None;
        }
    }

    fn draw(&mut self) {
        let Some(active) = &mut self.active else { return };
        let Some((w, h)) = active.size else { return };
        let fade = match active.ended {
            Some((at, _)) => 1.0 - at.elapsed().as_secs_f32() / END_FADE.as_secs_f32(),
            None => 1.0,
        };
        if fade <= 0.0 {
            self.active = None;
            return;
        }
        let color = match active.ended {
            Some((_, false)) => REJECTED,
            _ => COLOR,
        };
        let now = active.start.elapsed().as_secs_f32();
        let mesh = active.trail.mesh(now, (w as f32, h as f32), fade, color);
        let surface = active.layer.wl_surface();
        // Ask for the next frame before presenting, which commits.
        surface.frame(&self.qh, FrameCallbackData(surface.clone()));
        if !self.gpu.draw(&active.target, &mesh) {
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
            if active.ended.is_none() {
                let (x, y) = event.position;
                active
                    .trail
                    .push(x as f32, y as f32, active.start.elapsed().as_secs_f32());
            }
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
