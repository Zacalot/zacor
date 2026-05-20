mod color;
mod geometry;
mod paint;
mod prepared;
mod raster;
mod renderer;
mod scene;
mod style;
mod terminal;
mod test_renderer;
mod wgpu_backend;

pub use color::Color;
pub use geometry::{Coord, Insets, Point, Rect, Size};
pub use paint::PaintContext;
pub use prepared::{
    PreparedDraw, PreparedFrame, PreparedRectVertex, PreparedTextGlyph, PreparedTextRun,
    prepare_frame, prepare_frame_with_text,
};
pub use raster::{RasterImage, RasterRenderer};
pub use renderer::{RenderCapabilities, RenderError, RenderResult, Renderer};
pub use scene::{
    Frame, Layer, Primitive, Scene, SceneItem, SurfaceFallback, SurfaceKind, SurfaceSlotId,
};
pub use style::{Stroke, TextStyle};
pub use terminal::{TerminalCell, TerminalGrid, TerminalRenderer};
pub use test_renderer::TestRenderer;
pub use wgpu_backend::{
    Rgba8Pixel, WgpuReadbackImage, WgpuRenderer, WgpuRendererConfig, WgpuSurfaceTarget, WgpuTarget,
};
