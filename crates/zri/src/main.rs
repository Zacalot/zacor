use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use zri::function::{FunctionName, FunctionRegistry, FunctionRouter};
use zri::host::{
    Axis, BufferId, BufferKind, HostRuntime, InterfaceHost, Pane, PaneChrome, PaneContent, PaneId,
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

/// Cursor blink half-period (the Emacs `blink-cursor-interval` default).
const BLINK_INTERVAL: Duration = Duration::from_millis(500);

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
    /// Next cursor blink-phase flip; pushed forward by keyboard activity so
    /// the cursor stays solid while typing.
    next_blink: Instant,
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
            next_blink: Instant::now() + BLINK_INTERVAL,
        }
    }
}

struct NativeWindow {
    window: Arc<Window>,
    target: WgpuSurfaceTarget,
}

impl ApplicationHandler<ZriWake> for ZriApp {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.toggle_cursor_blink();
            }
            self.next_blink = Instant::now() + BLINK_INTERVAL;
            self.request_redraw_if_needed();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The blink timer runs only while a cursor would paint; otherwise the
        // loop stays fully event-driven.
        let blinking = self
            .runtime
            .as_ref()
            .is_some_and(HostRuntime::has_active_cursor);
        event_loop.set_control_flow(if blinking {
            ControlFlow::WaitUntil(self.next_blink)
        } else {
            ControlFlow::Wait
        });
    }

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
                // The runtime resets the blink phase to visible; restart the
                // half-period so the cursor stays solid while typing.
                self.next_blink = Instant::now() + BLINK_INTERVAL;
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
        runtime.set_functions(default_functions(scratch));
        runtime.set_keymaps(vec![("global".to_string(), default_keymap())]);
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

/// Built-in interface functions: the F1 demo stamp plus the pane/buffer
/// verbs behind the default keymap. Handlers only push effects; the runtime
/// applies them with target revalidation.
fn default_functions(scratch: BufferId) -> FunctionRouter {
    let mut registry = FunctionRegistry::new();
    registry.register_rust(
        FunctionName::new("demo.stamp").expect("valid function name"),
        Arc::new(move |context| {
            context.buf_append(scratch, "[zri] demo.stamp\n");
        }),
    );
    registry.register_rust(
        FunctionName::new("pane.split-below").expect("valid function name"),
        Arc::new(|context| context.split_active_pane(Axis::Vertical)),
    );
    registry.register_rust(
        FunctionName::new("pane.split-right").expect("valid function name"),
        Arc::new(|context| context.split_active_pane(Axis::Horizontal)),
    );
    registry.register_rust(
        FunctionName::new("pane.other").expect("valid function name"),
        Arc::new(|context| context.focus_next_pane()),
    );
    registry.register_rust(
        FunctionName::new("buffer.new").expect("valid function name"),
        Arc::new(|context| context.open_scratch_buffer()),
    );
    FunctionRouter::new(registry)
}

/// Default Emacs-style bindings: `C-x 2` / `C-x 3` split below/right,
/// `C-x o` cycles panes, `C-x b` opens a fresh scratch buffer (placeholder
/// for real buffer switching until the prompt exists), F1 demo stamp.
fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new();
    bind(
        &mut keymap,
        vec![BindingChord {
            key: Key::Function(1),
            modifiers: zri::input::Modifiers::default(),
            location: KeyLocation::Standard,
        }],
        "demo.stamp",
    );
    bind(
        &mut keymap,
        vec![ctrl_chord("x"), char_chord("2")],
        "pane.split-below",
    );
    bind(
        &mut keymap,
        vec![ctrl_chord("x"), char_chord("3")],
        "pane.split-right",
    );
    bind(
        &mut keymap,
        vec![ctrl_chord("x"), char_chord("o")],
        "pane.other",
    );
    bind(
        &mut keymap,
        vec![ctrl_chord("x"), char_chord("b")],
        "buffer.new",
    );
    keymap
}

fn bind(keymap: &mut Keymap, chords: Vec<BindingChord>, function: &str) {
    keymap
        .bind(
            KeySequence::new(chords).expect("non-empty sequence"),
            FunctionName::new(function).expect("valid function name"),
        )
        .expect("conflict-free default binding");
}

fn char_chord(ch: &str) -> BindingChord {
    BindingChord {
        key: Key::Character(ch.to_string()),
        modifiers: zri::input::Modifiers::default(),
        location: KeyLocation::Standard,
    }
}

fn ctrl_chord(ch: &str) -> BindingChord {
    BindingChord {
        modifiers: zri::input::Modifiers {
            control: true,
            ..Default::default()
        },
        ..char_chord(ch)
    }
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

    fn key_down(key: Key, text: Option<&str>, control: bool) -> zri::input::KeyboardEvent {
        zri::input::KeyboardEvent::KeyDown(zri::input::KeyDownEvent {
            keystroke: zri::input::Keystroke {
                key,
                text: text.map(ToOwned::to_owned),
                modifiers: zri::input::Modifiers {
                    control,
                    ..Default::default()
                },
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        })
    }

    #[test]
    fn demo_binding_stamps_scratch_buffer_through_function_path() {
        let (host, scratch) = initial_host();
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 50.0));
        runtime.select_pane(ROOT_PANE);
        runtime.set_functions(default_functions(scratch));
        runtime.set_keymaps(vec![("global".to_string(), default_keymap())]);

        let result = runtime.handle_keyboard(key_down(Key::Function(1), None, false));

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

    #[test]
    fn default_keymap_binds_pane_and_buffer_commands() {
        let keymap = default_keymap();
        let active = [zri::keymap::ActiveKeymap {
            name: "global",
            keymap: &keymap,
        }];
        let mut resolver = zri::keymap::KeymapResolver::new();

        let pending = resolver.resolve(&key_down(Key::Character("x".into()), None, true), &active);
        assert!(matches!(
            pending,
            zri::keymap::KeymapResolution::Pending { .. }
        ));

        let matched = resolver.resolve(
            &key_down(Key::Character("2".into()), Some("2"), false),
            &active,
        );
        assert_eq!(
            matched.matched_function(),
            Some(&FunctionName::new("pane.split-below").unwrap())
        );
    }

    #[test]
    fn default_bindings_split_scratch_pane_end_to_end() {
        let (host, scratch) = initial_host();
        let mut runtime = HostRuntime::new(host, Size::new(200.0, 100.0));
        runtime.select_pane(ROOT_PANE);
        runtime.set_functions(default_functions(scratch));
        runtime.set_keymaps(vec![("global".to_string(), default_keymap())]);

        runtime.handle_keyboard(key_down(Key::Character("x".into()), None, true));
        runtime.handle_keyboard(key_down(Key::Character("2".into()), Some("2"), false));

        // The split landed and focus stayed on the original pane.
        assert_eq!(runtime.host().pane_order().len(), 2);
        assert_eq!(runtime.host().active_pane(), Some(ROOT_PANE));
    }
}
