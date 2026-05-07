use super::scene::{Color, FillRect, NativeScene, TextureSurface, label_quads};
use anyhow::{Context, Result, anyhow};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::{CompositeAlphaMode, SurfaceError, TextureFormat};
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

const TEXTURE_SHADER: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

pub struct Compositor {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    present_mode: wgpu::PresentMode,
    clear_color: wgpu::Color,
    rect_pipeline: wgpu::RenderPipeline,
    texture_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_sampler: wgpu::Sampler,
}

impl Compositor {
    pub fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .context("failed to acquire a wgpu adapter")?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("zred-native-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .context("failed to create a wgpu device")?;

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zred-rect-shader"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });
        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zred-rect-pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zred-rect-pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[RectVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zred-texture-bind-group-layout"),
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
            });
        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zred-texture-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let texture_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zred-texture-shader"),
            source: wgpu::ShaderSource::Wgsl(TEXTURE_SHADER.into()),
        });
        let texture_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("zred-texture-pipeline-layout"),
                bind_group_layouts: &[&texture_bind_group_layout],
                push_constant_ranges: &[],
            });
        let texture_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zred-texture-pipeline"),
            layout: Some(&texture_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &texture_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[TexturedVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &texture_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            present_mode: wgpu::PresentMode::AutoVsync,
            clear_color: wgpu::Color {
                r: 0.04,
                g: 0.05,
                b: 0.06,
                a: 1.0,
            },
            rect_pipeline,
            texture_pipeline,
            texture_bind_group_layout,
            texture_sampler,
        })
    }

    pub fn create_window_state(&self, window: Arc<Window>) -> Result<WindowCompositorState> {
        let surface = self
            .instance
            .create_surface(window.clone())
            .context("failed to create window surface")?;
        let size = window.inner_size();
        let capabilities = surface.get_capabilities(&self.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(TextureFormat::is_srgb)
            .ok_or_else(|| anyhow!("surface exposed no compatible texture format"))?;
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(CompositeAlphaMode::Auto);
        let present_mode = if capabilities.present_modes.contains(&self.present_mode) {
            self.present_mode
        } else {
            capabilities
                .present_modes
                .first()
                .copied()
                .unwrap_or(wgpu::PresentMode::AutoVsync)
        };

        let mut state = WindowCompositorState {
            window,
            surface,
            config: wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
            size,
        };
        state.configure(&self.device);
        Ok(state)
    }

    pub fn resize(&self, state: &mut WindowCompositorState, size: PhysicalSize<u32>) {
        state.resize(&self.device, size);
    }

    pub fn render(&self, state: &mut WindowCompositorState, scene: &NativeScene) -> Result<()> {
        let frame = match state.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(SurfaceError::Outdated | SurfaceError::Lost) => {
                state.configure(&self.device);
                return Ok(());
            }
            Err(SurfaceError::Timeout) => return Ok(()),
            Err(SurfaceError::OutOfMemory) => {
                return Err(anyhow!("wgpu surface ran out of memory"));
            }
            Err(SurfaceError::Other) => return Ok(()),
        };

        let mut all_fills = scene.fills.clone();
        all_fills.extend(label_quads(scene));
        let rect_vertices = build_rect_vertices(&all_fills, state.size());
        let rect_vertex_buffer = create_buffer_or_none(
            &self.device,
            "zred-rect-vertex-buffer",
            wgpu::BufferUsages::VERTEX,
            bytemuck::cast_slice(&rect_vertices),
        );
        let textured_quads = scene
            .textures
            .iter()
            .map(|surface| self.prepare_texture_quad(surface, state.size()))
            .collect::<Result<Vec<_>>>()?;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zred-native-scene-pass"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zred-native-scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            if let Some(vertex_buffer) = &rect_vertex_buffer {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }

            if !textured_quads.is_empty() {
                pass.set_pipeline(&self.texture_pipeline);
                for quad in &textured_quads {
                    pass.set_scissor_rect(
                        quad.scissor.x,
                        quad.scissor.y,
                        quad.scissor.width.max(1),
                        quad.scissor.height.max(1),
                    );
                    pass.set_bind_group(0, &quad.bind_group, &[]);
                    pass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                    pass.draw(0..6, 0..1);
                }
                pass.set_scissor_rect(0, 0, state.size().width.max(1), state.size().height.max(1));
            }

            for label in &scene.labels {
                pass.insert_debug_marker(&format!(
                    "label '{}' rgba({:.2},{:.2},{:.2},{:.2})",
                    label.text, label.color.r, label.color.g, label.color.b, label.color.a
                ));
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn prepare_texture_quad(
        &self,
        surface: &TextureSurface,
        size: PhysicalSize<u32>,
    ) -> Result<PreparedTextureQuad> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zred-pane-texture"),
            size: wgpu::Extent3d {
                width: surface.image.width,
                height: surface.image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
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
            &surface.image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(surface.image.width * 4),
                rows_per_image: Some(surface.image.height),
            },
            wgpu::Extent3d {
                width: surface.image.width,
                height: surface.image.height,
                depth_or_array_layers: 1,
            },
        );
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zred-pane-texture-bind-group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture_sampler),
                },
            ],
        });
        let vertices = build_textured_vertices(surface, size);
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("zred-texture-vertex-buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        Ok(PreparedTextureQuad {
            _texture: texture,
            _texture_view: texture_view,
            bind_group,
            vertex_buffer,
            scissor: surface.rect,
        })
    }
}

pub struct WindowCompositorState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
}

impl WindowCompositorState {
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

struct PreparedTextureQuad {
    _texture: wgpu::Texture,
    _texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    scissor: super::scene::Rect,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl RectVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RectVertex>() as u64,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TexturedVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl TexturedVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TexturedVertex>() as u64,
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
            ],
        }
    }
}

fn create_buffer_or_none(
    device: &wgpu::Device,
    label: &str,
    usage: wgpu::BufferUsages,
    contents: &[u8],
) -> Option<wgpu::Buffer> {
    if contents.is_empty() {
        return None;
    }

    Some(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage,
        }),
    )
}

fn build_rect_vertices(fills: &[FillRect], size: PhysicalSize<u32>) -> Vec<RectVertex> {
    let mut vertices = Vec::with_capacity(fills.len() * 6);
    for fill in fills {
        let [left, right, top, bottom] = fill.rect.normalized_bounds(size);
        let color = [fill.color.r, fill.color.g, fill.color.b, fill.color.a];
        vertices.extend_from_slice(&[
            RectVertex {
                position: [left, top],
                color,
            },
            RectVertex {
                position: [right, top],
                color,
            },
            RectVertex {
                position: [right, bottom],
                color,
            },
            RectVertex {
                position: [left, top],
                color,
            },
            RectVertex {
                position: [right, bottom],
                color,
            },
            RectVertex {
                position: [left, bottom],
                color,
            },
        ]);
    }
    vertices
}

fn build_textured_vertices(
    surface: &TextureSurface,
    size: PhysicalSize<u32>,
) -> [TexturedVertex; 6] {
    let [left, right, top, bottom] = surface.rect.normalized_bounds(size);
    [
        TexturedVertex {
            position: [left, top],
            uv: [0.0, 0.0],
        },
        TexturedVertex {
            position: [right, top],
            uv: [1.0, 0.0],
        },
        TexturedVertex {
            position: [right, bottom],
            uv: [1.0, 1.0],
        },
        TexturedVertex {
            position: [left, top],
            uv: [0.0, 0.0],
        },
        TexturedVertex {
            position: [right, bottom],
            uv: [1.0, 1.0],
        },
        TexturedVertex {
            position: [left, bottom],
            uv: [0.0, 1.0],
        },
    ]
}

impl From<Color> for wgpu::Color {
    fn from(value: Color) -> Self {
        Self {
            r: value.r as f64,
            g: value.g as f64,
            b: value.b as f64,
            a: value.a as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontends::native::scene::{Rect, TextureImage, TextureSurface};

    #[test]
    fn textured_vertices_cover_full_rect_with_uvs() {
        let surface = TextureSurface {
            rect: Rect {
                x: 10,
                y: 20,
                width: 40,
                height: 30,
            },
            image: TextureImage {
                width: 2,
                height: 2,
                pixels: vec![255; 16],
            },
        };

        let vertices = build_textured_vertices(&surface, PhysicalSize::new(100, 100));

        assert_eq!(vertices[0].uv, [0.0, 0.0]);
        assert_eq!(vertices[2].uv, [1.0, 1.0]);
        assert_eq!(vertices[5].uv, [0.0, 1.0]);
    }
}
