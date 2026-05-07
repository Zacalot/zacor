mod color;
mod geometry;
mod prepared;
mod raster;
mod renderer;
mod scene;
mod style;
mod test_renderer;
mod wgpu_backend;

pub use color::Color;
pub use geometry::{Coord, Insets, Point, Rect, Size};
pub use prepared::{PreparedFrame, PreparedRectVertex, prepare_frame};
pub use raster::{RasterImage, RasterRenderer};
pub use renderer::{RenderCapabilities, RenderError, RenderResult, Renderer};
pub use scene::{Frame, Primitive, Scene};
pub use style::{Stroke, TextStyle};
pub use test_renderer::TestRenderer;
pub use wgpu_backend::{
    Rgba8Pixel, WgpuReadbackImage, WgpuRenderer, WgpuRendererConfig, WgpuSurfaceTarget, WgpuTarget,
};
