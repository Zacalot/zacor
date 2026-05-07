use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};
use zri::render::{
    Color, Frame, Rect, RenderError, Scene, Size, Stroke, WgpuRenderer, WgpuRendererConfig,
    WgpuSurfaceTarget,
};

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = DemoApp::default();
    event_loop.run_app(&mut app).expect("window demo failed");
}

#[derive(Default)]
struct DemoApp {
    renderer: Option<WgpuRenderer>,
    native_window: Option<NativeWindow>,
}

struct NativeWindow {
    window: Arc<Window>,
    target: WgpuSurfaceTarget,
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native_window.is_some() {
            return;
        }

        if let Err(error) = self.create_native_window(event_loop) {
            eprintln!("failed to create native window: {error}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(native_window) = self.native_window.as_mut() else {
            return;
        };
        if native_window.window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_ref() {
                    renderer.resize_surface_target(&mut native_window.target, size);
                    native_window.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    let frame = demo_frame(native_window.target.size());
                    if let Err(error) =
                        renderer.render_to_surface_target(&mut native_window.target, &frame)
                    {
                        eprintln!("render failed: {error}");
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }
}

impl DemoApp {
    fn create_native_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), RenderError> {
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("zri wgpu window")
                        .with_inner_size(PhysicalSize::new(960, 540)),
                )
                .map_err(|error| RenderError::new(format!("failed to create window: {error}")))?,
        );

        let probe = WgpuRenderer::new()?;
        let format = probe.preferred_surface_format(window.clone())?;
        let renderer = WgpuRenderer::with_config(WgpuRendererConfig { format })?;
        let target = renderer.create_surface_target(window.clone())?;

        self.renderer = Some(renderer);
        self.native_window = Some(NativeWindow { window, target });

        if let Some(native_window) = self.native_window.as_ref() {
            native_window.window.request_redraw();
        }

        Ok(())
    }
}

fn demo_frame(size: PhysicalSize<u32>) -> Frame {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let mut scene = Scene::new();
    scene.clear_color(Color::rgb(10, 13, 18));
    scene.fill_rect(
        Rect::from_xywh(width * 0.08, height * 0.12, width * 0.48, height * 0.36),
        Color::rgb(58, 105, 210),
    );
    scene.fill_rect(
        Rect::from_xywh(width * 0.44, height * 0.34, width * 0.40, height * 0.42),
        Color::rgb(235, 238, 245),
    );
    scene.stroke_rect(
        Rect::from_xywh(width * 0.18, height * 0.20, width * 0.64, height * 0.58),
        Stroke::new(Color::rgb(255, 88, 88), 4.0),
    );
    Frame::new(Size::new(width, height), scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_frame_contains_supported_wgpu_primitives_only() {
        let frame = demo_frame(PhysicalSize::new(800, 600));
        assert_eq!(frame.scene.primitives().len(), 4);
    }
}
