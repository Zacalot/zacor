use super::{
    Color, Frame, Point, PreparedDraw, PreparedFrame, PreparedRectVertex, PreparedTextRun,
    RenderCapabilities, RenderError, RenderResult, Renderer, Size, prepare_frame_with_text,
};
use crate::text::{GlyphImageFormat, GlyphKey, TextSystem};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

const RECT_SHADER: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

const TEXT_SHADER: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) mode: f32,
};

@group(0) @binding(0) var text_atlas: texture_2d<f32>;
@group(0) @binding(1) var text_sampler: sampler;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: f32,
) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    out.color = color;
    out.mode = mode;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(text_atlas, text_sampler, in.uv);
    let masked = vec4<f32>(in.color.rgb, in.color.a * sampled.a);
    return masked * (1.0 - in.mode) + sampled * in.mode;
}
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WgpuRendererConfig {
    pub format: wgpu::TextureFormat,
}

impl Default for WgpuRendererConfig {
    fn default() -> Self {
        Self {
            format: wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

pub struct WgpuRenderer {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    rect_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    text_atlas: WgpuTextAtlas,
    text_system: TextSystem,
    format: wgpu::TextureFormat,
}

impl WgpuRenderer {
    pub fn new() -> Result<Self, RenderError> {
        Self::with_config(WgpuRendererConfig::default())
    }

    pub fn with_config(config: WgpuRendererConfig) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| RenderError::new("failed to acquire wgpu adapter"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("zri-wgpu-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|error| RenderError::new(format!("failed to create wgpu device: {error}")))?;

        let rect_pipeline = create_rect_pipeline(&device, config.format);
        let text_bind_group_layout = create_text_bind_group_layout(&device);
        let text_sampler = create_text_sampler(&device);
        let text_atlas = WgpuTextAtlas::new(&device, &text_bind_group_layout, &text_sampler);
        let text_pipeline = create_text_pipeline(&device, config.format, &text_bind_group_layout);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            rect_pipeline,
            text_pipeline,
            text_atlas,
            text_system: TextSystem::new(),
            format: config.format,
        })
    }

    pub fn render_to_target(
        &mut self,
        target: &WgpuTarget,
        frame: &Frame,
    ) -> Result<RenderResult, RenderError> {
        if target.format != self.format {
            return Err(RenderError::new(
                "wgpu target format does not match renderer format",
            ));
        }

        let prepared = prepare_frame_with_text(frame, &mut self.text_system);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zri-offscreen-render"),
            });

        self.encode_prepared_frame(&mut encoder, &target.view, &prepared)?;

        self.queue.submit(std::iter::once(encoder.finish()));

        Ok(RenderResult {
            rendered_primitives: prepared.rendered_primitives,
            unsupported_primitives: prepared.unsupported_primitives,
        })
    }

    pub fn read_target(&self, target: &WgpuTarget) -> Result<WgpuReadbackImage, RenderError> {
        if target.format != self.format {
            return Err(RenderError::new(
                "wgpu target format does not match renderer format",
            ));
        }
        if !is_rgba8_format(target.format) {
            return Err(RenderError::new(format!(
                "readback requires an RGBA8 target format, got {:?}",
                target.format
            )));
        }

        let width = target.width();
        let height = target.height();
        let layout = ReadbackLayout::new(width, height);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zri-readback-buffer"),
            size: layout.buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zri-readback-copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(layout.padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .map_err(|error| {
                RenderError::new(format!("failed to receive readback map result: {error}"))
            })?
            .map_err(|error| RenderError::new(format!("failed to map readback buffer: {error}")))?;

        let pixels = {
            let mapped = slice.get_mapped_range();
            read_pixels_from_padded_rows(
                &mapped,
                width,
                height,
                layout.unpadded_bytes_per_row,
                layout.padded_bytes_per_row,
            )
        };
        buffer.unmap();

        Ok(WgpuReadbackImage {
            width,
            height,
            pixels,
        })
    }

    pub fn preferred_surface_format(
        &self,
        window: Arc<Window>,
    ) -> Result<wgpu::TextureFormat, RenderError> {
        let surface = self
            .instance
            .create_surface(window)
            .map_err(|error| RenderError::new(format!("failed to create wgpu surface: {error}")))?;
        Ok(select_surface_format(
            &surface.get_capabilities(&self.adapter).formats,
        ))
    }

    pub fn create_surface_target(
        &self,
        window: Arc<Window>,
    ) -> Result<WgpuSurfaceTarget, RenderError> {
        let surface = self
            .instance
            .create_surface(window.clone())
            .map_err(|error| RenderError::new(format!("failed to create wgpu surface: {error}")))?;
        let capabilities = surface.get_capabilities(&self.adapter);
        if !capabilities.formats.contains(&self.format) {
            return Err(RenderError::new(format!(
                "renderer format {:?} is not supported by this surface",
                self.format
            )));
        }
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::AutoVsync)
        {
            wgpu::PresentMode::AutoVsync
        } else {
            capabilities
                .present_modes
                .first()
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo)
        };
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let size = window.inner_size();
        let mut target = WgpuSurfaceTarget {
            window,
            surface,
            config: wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
            size,
            format: self.format,
        };
        target.configure(&self.device);
        Ok(target)
    }

    pub fn resize_surface_target(&self, target: &mut WgpuSurfaceTarget, size: PhysicalSize<u32>) {
        target.resize(&self.device, size);
    }

    pub fn render_to_surface_target(
        &mut self,
        target: &mut WgpuSurfaceTarget,
        frame: &Frame,
    ) -> Result<RenderResult, RenderError> {
        if target.format != self.format {
            return Err(RenderError::new(
                "wgpu surface target format does not match renderer format",
            ));
        }

        let surface_frame = match target.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                target.configure(&self.device);
                return Ok(RenderResult::default());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(RenderResult::default()),
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err(RenderError::new("wgpu surface ran out of memory"));
            }
            Err(wgpu::SurfaceError::Other) => return Ok(RenderResult::default()),
        };

        let prepared = prepare_frame_with_text(frame, &mut self.text_system);
        let view = surface_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zri-surface-render"),
            });
        self.encode_prepared_frame(&mut encoder, &view, &prepared)?;
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_frame.present();

        Ok(RenderResult {
            rendered_primitives: prepared.rendered_primitives,
            unsupported_primitives: prepared.unsupported_primitives,
        })
    }

    fn encode_prepared_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame,
    ) -> Result<(), RenderError> {
        let vertices = prepared_rect_vertices(prepared);
        let vertex_buffer = create_vertex_buffer_or_none(&self.device, &vertices);
        let prepared_text = self.prepare_text_vertices(prepared)?;
        let text_vertex_buffer =
            create_text_vertex_buffer_or_none(&self.device, &prepared_text.vertices);
        let clear_color = prepared.clear_color.unwrap_or(Color::Transparent);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zri-prepared-frame-render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu_color(clear_color)),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        for draw in &prepared.draws {
            match *draw {
                PreparedDraw::Rects { start, count } => {
                    let Some(vertex_buffer) = &vertex_buffer else {
                        continue;
                    };
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.draw(start as u32..(start + count) as u32, 0..1);
                }
                PreparedDraw::Text { index } => {
                    let Some(vertex_buffer) = &text_vertex_buffer else {
                        continue;
                    };
                    let Some(range) = prepared_text.ranges.get(index).and_then(|range| *range)
                    else {
                        continue;
                    };
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, self.text_atlas.bind_group(), &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.draw(range.start..(range.start + range.count), 0..1);
                }
            }
        }

        Ok(())
    }

    fn prepare_text_vertices(
        &mut self,
        prepared: &PreparedFrame,
    ) -> Result<PreparedWgpuText, RenderError> {
        self.text_atlas.ensure_runs(
            &self.device,
            &self.queue,
            &mut self.text_system,
            &prepared.text_runs,
        )?;

        let mut vertices = Vec::new();
        let mut ranges = Vec::with_capacity(prepared.text_runs.len());

        for run in &prepared.text_runs {
            let start = vertices.len() as u32;
            for glyph in &run.glyphs {
                let Some(entry) = self.text_atlas.entry(glyph.glyph.key) else {
                    continue;
                };
                append_text_quad(&mut vertices, prepared.size, glyph.glyph, entry);
            }
            let count = vertices.len() as u32 - start;
            if count == 0 {
                ranges.push(None);
            } else {
                ranges.push(Some(PreparedTextRange { start, count }));
            }
        }

        Ok(PreparedWgpuText { vertices, ranges })
    }
}

impl Renderer for WgpuRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities {
            fills: true,
            strokes: true,
            lines: false,
            text: true,
        }
    }

    fn render(&mut self, _frame: &Frame) -> Result<RenderResult, RenderError> {
        Err(RenderError::new("WgpuRenderer requires render_to_target"))
    }
}

pub struct WgpuTarget {
    size: Size,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
}

impl WgpuTarget {
    pub fn new(renderer: &WgpuRenderer, size: Size) -> Self {
        let width = texture_dimension(size.width);
        let height = texture_dimension(size.height);
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zri-offscreen-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            size: Size::new(width as f32, height as f32),
            texture,
            view,
            format: renderer.format,
        }
    }

    pub fn size(&self) -> Size {
        self.size
    }

    fn width(&self) -> u32 {
        self.size.width as u32
    }

    fn height(&self) -> u32 {
        self.size.height as u32
    }
}

pub struct WgpuSurfaceTarget {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    format: wgpu::TextureFormat,
}

impl WgpuSurfaceTarget {
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    fn resize(&mut self, device: &wgpu::Device, size: PhysicalSize<u32>) {
        self.size = size;
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.configure(device);
    }

    fn configure(&mut self, device: &wgpu::Device) {
        self.surface.configure(device, &self.config);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rgba8Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8Pixel {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WgpuReadbackImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgba8Pixel>,
}

impl WgpuReadbackImage {
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8Pixel> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels.get((y * self.width + x) as usize).copied()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
struct WgpuRectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
struct WgpuTextVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    mode: f32,
}

impl WgpuTextVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WgpuTextVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as u64,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2) as u64,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>())
                        as u64,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

#[derive(Clone, Copy)]
struct AtlasEntry {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    mode: f32,
}

struct WgpuTextAtlas {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    entries: std::collections::HashMap<GlyphKey, AtlasEntry>,
}

struct PreparedWgpuText {
    vertices: Vec<WgpuTextVertex>,
    ranges: Vec<Option<PreparedTextRange>>,
}

#[derive(Clone, Copy)]
struct PreparedTextRange {
    start: u32,
    count: u32,
}

impl From<PreparedRectVertex> for WgpuRectVertex {
    fn from(value: PreparedRectVertex) -> Self {
        Self {
            position: value.position,
            color: value.color,
        }
    }
}

impl WgpuRectVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WgpuRectVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as u64,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

fn create_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zri-rect-shader"),
        source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zri-rect-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zri-rect-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[WgpuRectVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn create_text_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("zri-text-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_text_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("zri-text-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

fn create_text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zri-text-shader"),
        source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zri-text-pipeline-layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zri-text-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[WgpuTextVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn prepared_rect_vertices(prepared: &PreparedFrame) -> Vec<WgpuRectVertex> {
    prepared
        .rect_vertices
        .iter()
        .copied()
        .map(WgpuRectVertex::from)
        .collect()
}

fn create_text_vertex_buffer_or_none(
    device: &wgpu::Device,
    vertices: &[WgpuTextVertex],
) -> Option<wgpu::Buffer> {
    if vertices.is_empty() {
        return None;
    }

    Some(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("zri-text-vertex-buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    )
}

fn append_text_quad(
    vertices: &mut Vec<WgpuTextVertex>,
    frame_size: Size,
    glyph: crate::text::LaidOutGlyph,
    entry: AtlasEntry,
) {
    if entry.width == 0 || entry.height == 0 {
        return;
    }

    let left = glyph.x + entry.left;
    let top = glyph.y - entry.top;
    let right = left + entry.width as i32;
    let bottom = top + entry.height as i32;
    let color = glyph.color.to_f32_rgba();

    let top_left = logical_to_ndc(Point::new(left as f32, top as f32), frame_size);
    let top_right = logical_to_ndc(Point::new(right as f32, top as f32), frame_size);
    let bottom_right = logical_to_ndc(Point::new(right as f32, bottom as f32), frame_size);
    let bottom_left = logical_to_ndc(Point::new(left as f32, bottom as f32), frame_size);
    let mode = entry.mode;
    let uv_min = entry.uv_min;
    let uv_max = entry.uv_max;

    vertices.extend_from_slice(&[
        WgpuTextVertex {
            position: top_left,
            uv: [uv_min[0], uv_min[1]],
            color,
            mode,
        },
        WgpuTextVertex {
            position: top_right,
            uv: [uv_max[0], uv_min[1]],
            color,
            mode,
        },
        WgpuTextVertex {
            position: bottom_right,
            uv: [uv_max[0], uv_max[1]],
            color,
            mode,
        },
        WgpuTextVertex {
            position: top_left,
            uv: [uv_min[0], uv_min[1]],
            color,
            mode,
        },
        WgpuTextVertex {
            position: bottom_right,
            uv: [uv_max[0], uv_max[1]],
            color,
            mode,
        },
        WgpuTextVertex {
            position: bottom_left,
            uv: [uv_min[0], uv_max[1]],
            color,
            mode,
        },
    ]);
}

fn logical_to_ndc(point: Point, size: Size) -> [f32; 2] {
    [
        (point.x / size.width) * 2.0 - 1.0,
        1.0 - (point.y / size.height) * 2.0,
    ]
}

impl WgpuTextAtlas {
    fn new(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let width = 2048;
        let height = 2048;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zri-text-atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zri-text-bind-group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            bind_group,
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            entries: std::collections::HashMap::new(),
        }
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn entry(&self, key: GlyphKey) -> Option<AtlasEntry> {
        self.entries.get(&key).copied()
    }

    fn ensure_runs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text_system: &mut TextSystem,
        runs: &[PreparedTextRun],
    ) -> Result<(), RenderError> {
        for run in runs {
            for glyph in &run.glyphs {
                self.ensure_glyph(device, queue, text_system, glyph.glyph.key)?;
            }
        }
        Ok(())
    }

    fn ensure_glyph(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        text_system: &mut TextSystem,
        key: GlyphKey,
    ) -> Result<(), RenderError> {
        if self.entries.contains_key(&key) {
            return Ok(());
        }

        let Some(image) = text_system.rasterize_glyph(key) else {
            return Ok(());
        };
        if image.width == 0 || image.height == 0 {
            return Ok(());
        }

        let (x, y) = self.allocate(image.width, image.height)?;
        let rgba = rgba_pixels_for_glyph(&image);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );

        self.entries.insert(
            key,
            AtlasEntry {
                left: image.left,
                top: image.top,
                width: image.width,
                height: image.height,
                uv_min: [x as f32 / self.width as f32, y as f32 / self.height as f32],
                uv_max: [
                    (x + image.width) as f32 / self.width as f32,
                    (y + image.height) as f32 / self.height as f32,
                ],
                mode: match image.format {
                    GlyphImageFormat::Mask => 0.0,
                    GlyphImageFormat::Color => 1.0,
                },
            },
        );

        Ok(())
    }

    fn allocate(&mut self, width: u32, height: u32) -> Result<(u32, u32), RenderError> {
        if width > self.width || height > self.height {
            return Err(RenderError::new(
                "glyph does not fit within text atlas dimensions",
            ));
        }

        if self.cursor_x + width > self.width {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }

        if self.cursor_y + height > self.height {
            return Err(RenderError::new("text atlas is full"));
        }

        let origin = (self.cursor_x, self.cursor_y);
        self.cursor_x += width;
        self.row_height = self.row_height.max(height);
        Ok(origin)
    }
}

fn rgba_pixels_for_glyph(image: &crate::text::GlyphImage) -> Vec<u8> {
    match image.format {
        GlyphImageFormat::Mask => image
            .data
            .iter()
            .flat_map(|alpha| [255, 255, 255, *alpha])
            .collect(),
        GlyphImageFormat::Color => image.data.clone(),
    }
}

fn create_vertex_buffer_or_none(
    device: &wgpu::Device,
    vertices: &[WgpuRectVertex],
) -> Option<wgpu::Buffer> {
    if vertices.is_empty() {
        return None;
    }

    Some(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("zri-rect-vertex-buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    )
}

fn texture_dimension(value: f32) -> u32 {
    if !value.is_finite() {
        return 1;
    }
    value.ceil().max(1.0) as u32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReadbackLayout {
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    buffer_size: u64,
}

impl ReadbackLayout {
    fn new(width: u32, height: u32) -> Self {
        let unpadded_bytes_per_row = width * 4;
        let padded_bytes_per_row =
            align_to(unpadded_bytes_per_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        Self {
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            buffer_size: padded_bytes_per_row as u64 * height as u64,
        }
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    value.div_ceil(alignment) * alignment
}

fn read_pixels_from_padded_rows(
    bytes: &[u8],
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
) -> Vec<Rgba8Pixel> {
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in 0..height as usize {
        let start = row * padded_bytes_per_row as usize;
        let end = start + unpadded_bytes_per_row as usize;
        for pixel in bytes[start..end].chunks_exact(4) {
            pixels.push(Rgba8Pixel::new(pixel[0], pixel[1], pixel[2], pixel[3]));
        }
    }
    pixels
}

fn is_rgba8_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
    )
}

fn select_surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| formats.first().copied())
        .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb)
}

fn wgpu_color(color: Color) -> wgpu::Color {
    let [r, g, b, a] = color.to_f32_rgba();
    wgpu::Color {
        r: r as f64,
        g: g as f64,
        b: b as f64,
        a: a as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{
        Layer, PaintContext, Primitive, Rect, Scene, SceneItem, Stroke, prepare_frame,
    };

    #[test]
    fn converts_prepared_vertex_to_wgpu_vertex() {
        let prepared = PreparedRectVertex {
            position: [-0.5, 0.25],
            color: [1.0, 0.5, 0.0, 1.0],
        };

        assert_eq!(
            WgpuRectVertex::from(prepared),
            WgpuRectVertex {
                position: [-0.5, 0.25],
                color: [1.0, 0.5, 0.0, 1.0],
            }
        );
    }

    #[test]
    fn texture_dimensions_are_clamped_and_rounded_up() {
        assert_eq!(texture_dimension(-10.0), 1);
        assert_eq!(texture_dimension(0.0), 1);
        assert_eq!(texture_dimension(1.1), 2);
        assert_eq!(texture_dimension(f32::NAN), 1);
    }

    #[test]
    fn readback_layout_aligns_rows_for_wgpu_copy() {
        let layout = ReadbackLayout::new(3, 2);
        assert_eq!(layout.unpadded_bytes_per_row, 12);
        assert_eq!(
            layout.padded_bytes_per_row,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
        );
        assert_eq!(
            layout.buffer_size,
            (wgpu::COPY_BYTES_PER_ROW_ALIGNMENT * 2) as u64
        );
    }

    #[test]
    fn readback_image_returns_pixels_by_coordinate() {
        let image = WgpuReadbackImage {
            width: 2,
            height: 1,
            pixels: vec![Rgba8Pixel::new(1, 2, 3, 4), Rgba8Pixel::new(5, 6, 7, 8)],
        };

        assert_eq!(image.pixel(0, 0), Some(Rgba8Pixel::new(1, 2, 3, 4)));
        assert_eq!(image.pixel(1, 0), Some(Rgba8Pixel::new(5, 6, 7, 8)));
        assert_eq!(image.pixel(2, 0), None);
    }

    #[test]
    fn padded_rows_are_stripped_to_tight_pixels() {
        let mut bytes = vec![0; 16];
        bytes[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        bytes[8..16].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);

        let pixels = read_pixels_from_padded_rows(&bytes, 1, 2, 4, 8);

        assert_eq!(
            pixels,
            vec![Rgba8Pixel::new(1, 2, 3, 4), Rgba8Pixel::new(9, 10, 11, 12)]
        );
    }

    #[test]
    fn prepared_frame_tracks_rendered_primitive_count() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::WHITE);
        scene.stroke_rect(
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
            Stroke::new(Color::WHITE, 1.0),
        );
        let frame = Frame::new(Size::new(100.0, 100.0), scene);

        let prepared = prepare_frame(&frame);

        assert_eq!(prepared.rendered_primitives, 3);
        assert_eq!(prepared.unsupported_primitives, 0);
    }

    #[test]
    fn renderer_trait_requires_explicit_target() {
        let mut renderer = WgpuRendererStub;
        let frame = Frame::new(Size::new(1.0, 1.0), Scene::new());
        let error = renderer.render(&frame).unwrap_err();
        assert_eq!(error.message(), "WgpuRenderer requires render_to_target");
    }

    struct WgpuRendererStub;

    impl Renderer for WgpuRendererStub {
        fn capabilities(&self) -> RenderCapabilities {
            RenderCapabilities {
                fills: true,
                strokes: true,
                lines: false,
                text: false,
            }
        }

        fn render(&mut self, _frame: &Frame) -> Result<RenderResult, RenderError> {
            Err(RenderError::new("WgpuRenderer requires render_to_target"))
        }
    }

    #[test]
    #[ignore]
    fn creates_wgpu_renderer_and_offscreen_target() {
        let renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(64.0, 32.0));
        assert_eq!(target.size(), Size::new(64.0, 32.0));
    }

    #[test]
    #[ignore]
    fn renders_clear_and_rects_to_offscreen_target() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(64.0, 64.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.fill_rect(Rect::from_xywh(8.0, 8.0, 24.0, 24.0), Color::WHITE);
        let frame = Frame::new(Size::new(64.0, 64.0), scene);

        let result = renderer.render_to_target(&target, &frame).unwrap();

        assert_eq!(
            result,
            RenderResult {
                rendered_primitives: 2,
                unsupported_primitives: 0,
            }
        );
    }

    #[test]
    #[ignore]
    fn reads_back_clear_color_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(8.0, 8.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::rgb(12, 34, 56));
        let frame = Frame::new(Size::new(8.0, 8.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(0, 0), Some(Rgba8Pixel::new(12, 34, 56, 255)));
        assert_eq!(image.pixel(7, 7), Some(Rgba8Pixel::new(12, 34, 56, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_filled_rect_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(16.0, 16.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.fill_rect(Rect::from_xywh(4.0, 4.0, 4.0, 4.0), Color::WHITE);
        let frame = Frame::new(Size::new(16.0, 16.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(0, 0), Some(Rgba8Pixel::new(0, 0, 0, 255)));
        assert_eq!(image.pixel(5, 5), Some(Rgba8Pixel::new(255, 255, 255, 255)));
        assert_eq!(image.pixel(9, 9), Some(Rgba8Pixel::new(0, 0, 0, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_clipped_rect_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(8.0, 8.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.fill_rect(Rect::from_xywh(-2.0, -2.0, 4.0, 4.0), Color::WHITE);
        let frame = Frame::new(Size::new(8.0, 8.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(0, 0), Some(Rgba8Pixel::new(255, 255, 255, 255)));
        assert_eq!(image.pixel(3, 3), Some(Rgba8Pixel::new(0, 0, 0, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_item_clip_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(8.0, 8.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 6.0, 6.0),
                color: Color::WHITE,
            })
            .clipped(Rect::from_xywh(2.0, 2.0, 2.0, 2.0)),
        );
        let frame = Frame::new(Size::new(8.0, 8.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(1, 1), Some(Rgba8Pixel::new(0, 0, 0, 255)));
        assert_eq!(image.pixel(2, 2), Some(Rgba8Pixel::new(255, 255, 255, 255)));
        assert_eq!(image.pixel(4, 4), Some(Rgba8Pixel::new(0, 0, 0, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_stroke_rect_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(12.0, 12.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.stroke_rect(
            Rect::from_xywh(3.0, 3.0, 6.0, 6.0),
            Stroke::new(Color::rgb(255, 0, 0), 1.0),
        );
        let frame = Frame::new(Size::new(12.0, 12.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(3, 3), Some(Rgba8Pixel::new(255, 0, 0, 255)));
        assert_eq!(image.pixel(5, 5), Some(Rgba8Pixel::new(0, 0, 0, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_layered_overlap_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(10.0, 10.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
                color: Color::rgb(0, 0, 255),
            })
            .layered(Layer(0)),
        );
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(2.0, 2.0, 6.0, 6.0),
                color: Color::rgb(0, 255, 0),
            })
            .layered(Layer(5)),
        );
        let frame = Frame::new(Size::new(10.0, 10.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(1, 1), Some(Rgba8Pixel::new(0, 0, 255, 255)));
        assert_eq!(image.pixel(3, 3), Some(Rgba8Pixel::new(0, 255, 0, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_nested_paint_context_clip_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(10.0, 10.0));
        let mut paint = PaintContext::new();
        paint.clear_color(Color::BLACK);
        paint.with_clip(Rect::from_xywh(1.0, 1.0, 6.0, 6.0), |paint| {
            paint.with_clip(Rect::from_xywh(3.0, 3.0, 4.0, 4.0), |paint| {
                paint.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::WHITE);
            });
        });
        let frame = Frame::new(Size::new(10.0, 10.0), paint.finish());

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        assert_eq!(image.pixel(2, 2), Some(Rgba8Pixel::new(0, 0, 0, 255)));
        assert_eq!(image.pixel(3, 3), Some(Rgba8Pixel::new(255, 255, 255, 255)));
        assert_eq!(image.pixel(7, 7), Some(Rgba8Pixel::new(0, 0, 0, 255)));
    }

    #[test]
    #[ignore]
    fn reads_back_text_pixels() {
        let mut renderer = WgpuRenderer::new().unwrap();
        let target = WgpuTarget::new(&renderer, Size::new(64.0, 32.0));
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.text(
            Point::new(4.0, 8.0),
            "x",
            crate::render::TextStyle::new(Color::WHITE, 18.0),
        );
        let frame = Frame::new(Size::new(64.0, 32.0), scene);

        renderer.render_to_target(&target, &frame).unwrap();
        let image = renderer.read_target(&target).unwrap();

        let mut found_non_black = false;
        for y in 0..image.height {
            for x in 0..image.width {
                if let Some(pixel) = image.pixel(x, y)
                    && pixel != Rgba8Pixel::new(0, 0, 0, 255)
                {
                    found_non_black = true;
                    break;
                }
            }
            if found_non_black {
                break;
            }
        }

        assert!(found_non_black);
    }
}
