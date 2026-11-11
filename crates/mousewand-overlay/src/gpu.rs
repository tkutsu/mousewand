//! The wgpu side: one device for the daemon's lifetime, a surface per gesture.

use std::ptr::NonNull;

use anyhow::{Context, bail};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Proxy};
use wgpu::util::DeviceExt;

use crate::hint::Image;
use crate::trail::Vertex;

/// The hint image, placed in physical pixels on the surface.
pub struct Quad<'a> {
    pub image: &'a Image,
    pub x: f32,
    pub y: f32,
    pub alpha: f32,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct QuadVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    alpha: f32,
}

/// The hint's pipeline for one surface format, and the image last uploaded.
struct HintGpu {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    texture: Option<(u64, wgpu::BindGroup)>,
}

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
    hint_shader: wgpu::ShaderModule,
    hint: Option<HintGpu>,
}

pub struct Target {
    surface: wgpu::Surface<'static>,
    config: Option<wgpu::SurfaceConfiguration>,
}

impl Target {
    /// A theme's sRGB color as the shader should output it: sRGB surfaces
    /// encode on write, so they take linear values.
    pub fn color(&self, srgb: [f32; 3]) -> [f32; 3] {
        if !self.config.as_ref().is_some_and(|c| c.format.is_srgb()) {
            return srgb;
        }
        srgb.map(|c| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    }
}

impl Gpu {
    pub fn new() -> anyhow::Result<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .context("no GPU adapter")?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).context("no GPU device")?;
        let shader = device.create_shader_module(wgpu::include_wgsl!("trail.wgsl"));
        let hint_shader = device.create_shader_module(wgpu::include_wgsl!("hint.wgsl"));
        log::info!("overlay renders on {}", adapter.get_info().name);
        Ok(Gpu {
            instance,
            adapter,
            device,
            queue,
            shader,
            pipeline: None,
            hint_shader,
            hint: None,
        })
    }

    pub fn target(&self, conn: &Connection, surface: &WlSurface) -> anyhow::Result<Target> {
        let display = NonNull::new(conn.backend().display_ptr().cast()).context("no Wayland display")?;
        let window = NonNull::new(surface.id().as_ptr().cast()).context("no Wayland surface")?;
        // Safety: the layer surface outlives this wgpu surface; `Active`
        // declares `target` before `layer`, so it's dropped first.
        unsafe {
            self.raw_target(
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
                RawWindowHandle::Wayland(WaylandWindowHandle::new(window)),
            )
        }
    }

    /// # Safety
    /// The display and window must outlive the returned target.
    pub unsafe fn raw_target(&self, display: RawDisplayHandle, window: RawWindowHandle) -> anyhow::Result<Target> {
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(display),
                    raw_window_handle: window,
                })
        }?;
        Ok(Target { surface, config: None })
    }

    /// Sizes the surface in physical pixels. Fails rather than fall back to
    /// an opaque surface, which would black out the monitor. `inherit` also
    /// accepts the window's own alpha, which on X11 means the compositor
    /// reads a 32-bit window's pixels as premultiplied.
    pub fn configure(&self, target: &mut Target, width: u32, height: u32, inherit: bool) -> anyhow::Result<()> {
        let caps = target.surface.get_capabilities(&self.adapter);
        let mut accepted = vec![wgpu::CompositeAlphaMode::PreMultiplied];
        if inherit {
            accepted.push(wgpu::CompositeAlphaMode::Inherit);
        }
        let Some(&alpha_mode) = accepted.iter().find(|mode| caps.alpha_modes.contains(mode)) else {
            bail!("the GPU can't make transparent surfaces here ({:?})", caps.alpha_modes);
        };
        let mut config = target
            .surface
            .get_default_config(&self.adapter, width, height)
            .context("surface isn't supported by the adapter")?;
        // Compositors blend buffers as premultiplied sRGB. On an sRGB format
        // the GPU would encode the premultiplied values, which brightens and
        // whitens everything partly transparent, like the glow.
        let plain = config.format.remove_srgb_suffix();
        if caps.formats.contains(&plain) {
            config.format = plain;
        }
        config.alpha_mode = alpha_mode;
        config.present_mode = wgpu::PresentMode::Fifo;
        target.surface.configure(&self.device, &config);
        target.config = Some(config);
        Ok(())
    }

    /// Draws one frame of triangle strips, and the hint over them. Returns
    /// false if the frame was skipped.
    pub fn draw(&mut self, target: &Target, strips: &[Vec<Vertex>], hint: Option<Quad>) -> bool {
        let Some(config) = &target.config else { return false };
        let frame = match target.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                log::debug!("skipping a frame: {other:?}");
                return false;
            }
        };
        let pipeline = self.pipeline(config.format);
        let hint = hint.map(|quad| (self.hint_bind_group(config.format, quad.image), quad));
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
            let vertices = strips.concat();
            if !vertices.is_empty() {
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("trail"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                let mut start = 0;
                for strip in strips {
                    let end = start + strip.len() as u32;
                    pass.draw(start..end, 0..1);
                    start = end;
                }
            }
            if let (Some((bind_group, quad)), Some(gpu)) = (hint, &self.hint) {
                let (w, h) = (config.width as f32, config.height as f32);
                let clip = |x: f32, y: f32| [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0];
                let (x0, y0) = (quad.x, quad.y);
                let (x1, y1) = (x0 + quad.image.width as f32, y0 + quad.image.height as f32);
                let vertex = |x, y, u, v| QuadVertex {
                    pos: clip(x, y),
                    uv: [u, v],
                    alpha: quad.alpha,
                };
                let vertices = [
                    vertex(x0, y0, 0.0, 0.0),
                    vertex(x0, y1, 0.0, 1.0),
                    vertex(x1, y0, 1.0, 0.0),
                    vertex(x1, y1, 1.0, 1.0),
                ];
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("hint"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&gpu.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..4, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        true
    }

    /// The hint image as a bind group, uploaded only when it changed.
    fn hint_bind_group(&mut self, format: wgpu::TextureFormat, image: &Image) -> wgpu::BindGroup {
        if self.hint.as_ref().is_none_or(|h| h.format != format) {
            self.hint = Some(self.hint_pipeline(format));
        }
        let gpu = self.hint.as_mut().expect("just made");
        if let Some((id, bind_group)) = &gpu.texture
            && *id == image.id
        {
            return bind_group.clone();
        }
        let size = wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        };
        // On an sRGB surface the GPU encodes on write, so it must decode here.
        let texture_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hint"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hint"),
            layout: &gpu.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                },
            ],
        });
        gpu.texture = Some((image.id, bind_group.clone()));
        bind_group
    }

    fn hint_pipeline(&self, format: wgpu::TextureFormat) -> HintGpu {
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hint"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &self.hint_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<QuadVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.hint_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Pixel-aligned and drawn at its own size: nearest keeps it sharp.
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hint"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        HintGpu {
            format,
            pipeline,
            sampler,
            texture: None,
        }
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((cached, pipeline)) = &self.pipeline
            && *cached == format
        {
            return pipeline.clone();
        }
        // Max, not over: where the ribbon crosses or folds over itself, as it
        // does at sharp turns, it stays as bright as one layer.
        let max = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let blend = wgpu::BlendState { color: max, alpha: max };
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
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x3, 4 => Float32],
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
