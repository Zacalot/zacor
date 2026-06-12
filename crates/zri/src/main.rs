use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use zri::function::{FunctionName, FunctionRegistry, FunctionRouter};
use zri::host::{
    BufferId, BufferKind, HostRuntime, InterfaceHost, Pane, PaneChrome, PaneContent, PaneId,
    PaneNode, PaneTree, ViewCursor,
};
use zri::ingress::ZrIngress;
use zri::input::{Key, KeyLocation, ModifiersChangedEvent, PointerEvent, PointerEventKind};
use zri::keymap::{BindingChord, KeySequence, Keymap};
use zri::platform::winit::{convert_keyboard_event, convert_modifiers, convert_pointer_button};
use zri::render::{
    Color, Point, RenderError, Size, Stroke, WgpuRenderer, WgpuRendererConfig, WgpuSurfaceTarget,
};

const ROOT_PANE: PaneId = PaneId(1);

/// User event posted by the ingress waker to drain deferred events at the
/// turn boundary. The wake is coalesced inside `ZrIngress`.
#[derive(Debug)]
struct ZriWake;

fn main() {
    let event_loop = EventLoop::<ZriWake>::with_user_event()
        .build()
        .expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let ingress = ZrIngress::new(Arc::new(move || {
        let _ = proxy.send_event(ZriWake);
    }));
    let mut app = ZriApp::new(ingress);
    event_loop.run_app(&mut app).expect("zri app failed");
}

struct ZriApp {
    renderer: Option<WgpuRenderer>,
    native_window: Option<NativeWindow>,
    runtime: Option<HostRuntime>,
    ingress: ZrIngress,
    pointer_position: Point,
    modifiers: zri::input::Modifiers,
}

impl ZriApp {
    fn new(ingress: ZrIngress) -> Self {
        Self {
            renderer: None,
            native_window: None,
            runtime: None,
            ingress,
            pointer_position: Point::new(0.0, 0.0),
            modifiers: zri::input::Modifiers::default(),
        }
    }
}

struct NativeWindow {
    window: Arc<Window>,
    target: WgpuSurfaceTarget,
}

impl ApplicationHandler<ZriWake> for ZriApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native_window.is_some() {
            return;
        }

        if let Err(error) = self.create_native_window(event_loop) {
            eprintln!("failed to create native window: {error}");
            event_loop.exit();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ZriWake) {
        if let Some(runtime) = self.runtime.as_mut() {
            let outcome = runtime.drain_ingress(&mut self.ingress);
            if outcome.remaining {
                // Budget exhausted: yield back to the platform loop (input and
                // paint get a turn) and re-wake for the rest.
                self.ingress.request_wake();
            }
        }
        self.request_redraw_if_needed();
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
                }
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.resize(logical_size(size));
                }
                native_window.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_position = Point::new(position.x as f32, position.y as f32);
                self.handle_pointer(PointerEventKind::Move, None);
                self.request_redraw_if_needed();
            }
            WindowEvent::CursorLeft { .. } => {
                self.handle_pointer(PointerEventKind::Leave, None);
                self.request_redraw_if_needed();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = convert_modifiers(modifiers.state());
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.handle_keyboard(zri::input::KeyboardEvent::ModifiersChanged(
                        ModifiersChangedEvent {
                            modifiers: self.modifiers,
                        },
                    ));
                }
                self.request_redraw_if_needed();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event = convert_keyboard_event(event, self.modifiers);
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.handle_keyboard(event);
                }
                self.request_redraw_if_needed();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = convert_pointer_button(button) else {
                    return;
                };
                self.handle_pointer(
                    match state {
                        ElementState::Pressed => PointerEventKind::Down,
                        ElementState::Released => PointerEventKind::Up,
                    },
                    Some(button),
                );
                self.request_redraw_if_needed();
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(runtime)) =
                    (self.renderer.as_mut(), self.runtime.as_mut())
                {
                    runtime.rebuild_snapshot_if_dirty();
                    if let Err(error) = renderer
                        .render_to_surface_target(&mut native_window.target, runtime.frame())
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

impl ZriApp {
    fn create_native_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), RenderError> {
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("zri")
                        .with_inner_size(PhysicalSize::new(960, 540)),
                )
                .map_err(|error| RenderError::new(format!("failed to create window: {error}")))?,
        );

        let probe = WgpuRenderer::new()?;
        let format = probe.preferred_surface_format(window.clone())?;
        let renderer = WgpuRenderer::with_config(WgpuRendererConfig { format })?;
        let target = renderer.create_surface_target(window.clone())?;
        let size = logical_size(target.size());

        let (host, scratch) = initial_host();
        let mut runtime = HostRuntime::new(host, size);
        runtime.select_pane(ROOT_PANE);
        runtime.set_functions(demo_functions(scratch));
        runtime.set_keymaps(vec![("global".to_string(), demo_keymap())]);
        self.runtime = Some(runtime);
        self.renderer = Some(renderer);
        self.native_window = Some(NativeWindow { window, target });

        if let Some(native_window) = self.native_window.as_ref() {
            native_window.window.request_redraw();
        }

        Ok(())
    }

    fn handle_pointer(
        &mut self,
        kind: PointerEventKind,
        button: Option<zri::input::PointerButton>,
    ) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.handle_pointer(PointerEvent {
                kind,
                position: self.pointer_position,
                button,
                modifiers: self.modifiers,
            });
        }
    }

    fn request_redraw_if_needed(&mut self) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.take_redraw_requested() {
            if let Some(native_window) = self.native_window.as_ref() {
                native_window.window.request_redraw();
            }
        }
    }
}

fn initial_host() -> (InterfaceHost, BufferId) {
    let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(ROOT_PANE)))
        .with_background(Color::rgb(10, 13, 18));
    let buffer =
        host.create_buffer_with_text(BufferKind::Text, "*scratch*", "zri scratch buffer\n\n");
    let view = host.create_view(buffer);
    host.view_mut(view)
        .expect("scratch view should exist")
        .set_cursor(ViewCursor { line: 2, column: 0 });
    host.insert_pane(
        Pane::new(ROOT_PANE, PaneContent::empty())
            .with_view(view)
            .with_focusable(true)
            .with_chrome(
                PaneChrome::new()
                    .with_background(Color::rgb(15, 19, 28))
                    .with_border(Stroke::new(Color::rgb(48, 58, 78), 1.0)),
            ),
    );
    (host, buffer)
}

/// Demo function proving the keymap -> function -> effect path natively:
/// F1 stamps a line into the scratch buffer through the effect chokepoint.
fn demo_functions(scratch: BufferId) -> FunctionRouter {
    let mut registry = FunctionRegistry::new();
    registry.register_rust(
        FunctionName::new("demo.stamp").expect("valid function name"),
        std::sync::Arc::new(move |context| {
            context.buf_append(scratch, "[zri] demo.stamp\n");
        }),
    );
    FunctionRouter::new(registry)
}

fn demo_keymap() -> Keymap {
    let mut keymap = Keymap::new();
    keymap
        .bind(
            KeySequence::new(vec![BindingChord {
                key: Key::Function(1),
                modifiers: zri::input::Modifiers::default(),
                location: KeyLocation::Standard,
            }])
            .expect("non-empty sequence"),
            FunctionName::new("demo.stamp").expect("valid function name"),
        )
        .expect("conflict-free demo binding");
    keymap
}

fn logical_size(size: PhysicalSize<u32>) -> Size {
    Size::new(size.width.max(1) as f32, size.height.max(1) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zri::render::Size;

    #[test]
    fn runtime_select_exposes_scratch_buffer_view() {
        let (host, _) = initial_host();
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 50.0));
        runtime.select_pane(ROOT_PANE);

        let host = runtime.host();
        let view = host.selected_view().unwrap();
        let buffer = host.view(view).unwrap().buffer();

        assert_eq!(host.active_pane(), Some(ROOT_PANE));
        assert_eq!(host.buffer(buffer).unwrap().kind(), BufferKind::Text);
        assert_eq!(host.buffer(buffer).unwrap().name(), "*scratch*");
    }

    #[test]
    fn demo_binding_stamps_scratch_buffer_through_function_path() {
        let (host, scratch) = initial_host();
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 50.0));
        runtime.select_pane(ROOT_PANE);
        runtime.set_functions(demo_functions(scratch));
        runtime.set_keymaps(vec![("global".to_string(), demo_keymap())]);

        let result = runtime.handle_keyboard(zri::input::KeyboardEvent::KeyDown(
            zri::input::KeyDownEvent {
                keystroke: zri::input::Keystroke {
                    key: Key::Function(1),
                    text: None,
                    modifiers: zri::input::Modifiers::default(),
                    location: KeyLocation::Standard,
                },
                repeat: false,
                prefer_text: false,
            },
        ));

        assert!(result.functions[0].result.is_ok());
        assert!(
            runtime
                .host()
                .buffer(scratch)
                .unwrap()
                .text()
                .ends_with("[zri] demo.stamp\n")
        );
    }
}
