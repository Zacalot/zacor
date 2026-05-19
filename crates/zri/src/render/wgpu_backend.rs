use super::{
    Color, Frame, PreparedFrame, PreparedRectVertex, RenderCapabilities, RenderError, RenderResult,
    Renderer, Size, prepare_frame,
};
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

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            rect_pipeline,
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

        let prepared = prepare_frame(frame);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zri-offscreen-render"),
            });

        self.encode_prepared_frame(&mut encoder, &target.view, &prepared);

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

        let prepared = prepare_frame(frame);
        let view = surface_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zri-surface-render"),
            });
        self.encode_prepared_frame(&mut encoder, &view, &prepared);
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_frame.present();

        Ok(RenderResult {
            rendered_primitives: prepared.rendered_primitives,
            unsupported_primitives: prepared.unsupported_primitives,
        })
    }

    fn encode_prepared_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame,
    ) {
        let vertices = prepared_rect_vertices(prepared);
        let vertex_buffer = create_vertex_buffer_or_none(&self.device, &vertices);
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

        if let Some(vertex_buffer) = &vertex_buffer {
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
    }
}

impl Renderer for WgpuRenderer {
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

fn prepared_rect_vertices(prepared: &PreparedFrame) -> Vec<WgpuRectVertex> {
    prepared
        .rect_vertices
        .iter()
        .copied()
        .map(WgpuRectVertex::from)
        .collect()
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
    use crate::render::{Layer, PaintContext, Primitive, Rect, Scene, SceneItem, Stroke};

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
}
