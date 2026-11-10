//! The wgpu side: one device for the daemon's lifetime, a surface per gesture.

use std::ptr::NonNull;

use anyhow::{Context, bail};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Proxy};
use wgpu::util::DeviceExt;

use crate::trail::Vertex;

/// Starting a GPU device takes 50–200 ms, so it starts once, with the
/// daemon, and gestures only pay for a surface.
pub struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    /// The pipeline for the last surface format seen.
    pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}

pub struct Target {
    surface: wgpu::Surface<'static>,
    config: Option<wgpu::SurfaceConfiguration>,
}

impl Gpu {
    pub fn new() -> anyhow::Result<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .context("no GPU adapter")?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).context("no GPU device")?;
        let shader = device.create_shader_module(wgpu::include_wgsl!("trail.wgsl"));
        log::info!("overlay renders on {}", adapter.get_info().name);
        Ok(Gpu {
            instance,
            adapter,
            device,
            queue,
            shader,
            pipeline: None,
        })
    }

    pub fn target(&self, conn: &Connection, surface: &WlSurface) -> anyhow::Result<Target> {
        let display = NonNull::new(conn.backend().display_ptr().cast()).context("no Wayland display")?;
        let window = NonNull::new(surface.id().as_ptr().cast()).context("no Wayland surface")?;
        // Safety: the layer surface outlives this wgpu surface; `Active`
        // declares `target` before `layer`, so it's dropped first.
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display))),
                    raw_window_handle: RawWindowHandle::Wayland(WaylandWindowHandle::new(window)),
                })
        }?;
        Ok(Target { surface, config: None })
    }

    /// Sizes the surface in physical pixels. Fails rather than fall back to
    /// an opaque surface, which would black out the monitor.
    pub fn configure(&self, target: &mut Target, width: u32, height: u32) -> anyhow::Result<()> {
        let caps = target.surface.get_capabilities(&self.adapter);
        if !caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            bail!("the GPU can't make transparent surfaces here ({:?})", caps.alpha_modes);
        }
        let mut config = target
            .surface
            .get_default_config(&self.adapter, width, height)
            .context("surface isn't supported by the adapter")?;
        config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        config.present_mode = wgpu::PresentMode::Fifo;
        target.surface.configure(&self.device, &config);
        target.config = Some(config);
        Ok(())
    }

    /// Draws one frame. Returns false if the frame was skipped.
    pub fn draw(&mut self, target: &Target, vertices: &[Vertex]) -> bool {
        let Some(config) = &target.config else { return false };
        let frame = match target.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                log::debug!("skipping a frame: {other:?}");
                return false;
            }
        };
        let pipeline = self.pipeline(config.format);
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("trail"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !vertices.is_empty() {
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("trail"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        true
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((cached, pipeline)) = &self.pipeline
            && *cached == format
        {
            return pipeline.clone();
        }
        let blend = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("trail"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipeline = Some((format, pipeline.clone()));
        pipeline
    }
}
