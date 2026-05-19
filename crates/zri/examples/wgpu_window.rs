use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton as WinitMouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{
    Key as WinitKey, KeyLocation as WinitKeyLocation, NamedKey as WinitNamedKey,
};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowAttributes, WindowId};
use zri::input::{
    FocusId, FocusState, HitBehavior, HitRegionId, Key, KeyDownEvent, KeyLocation, KeyUpEvent,
    KeyboardDispatchPlan, KeyboardDispatchResult, KeyboardEvent, KeyboardPropagation, Keystroke,
    ModifierKey, Modifiers, ModifiersChangedEvent, NamedKey, PointerButton, PointerDispatchPlan,
    PointerDispatchResult, PointerEvent, PointerEventKind, PointerState, PointerTransition,
    keyboard_dispatch_result, plan_keyboard_dispatch, plan_pointer_dispatch,
    pointer_dispatch_result,
};
use zri::render::{
    Color, Frame, Layer, PaintContext, Point, Rect, RenderError, Size, Stroke, WgpuRenderer,
    WgpuRendererConfig, WgpuSurfaceTarget,
};

const BLUE_RECT_HIT: HitRegionId = HitRegionId(1);
const WHITE_RECT_HIT: HitRegionId = HitRegionId(2);
const STROKE_RECT_HIT: HitRegionId = HitRegionId(3);
const BLUE_RECT_FOCUS: FocusId = FocusId(1);
const WHITE_RECT_FOCUS: FocusId = FocusId(2);

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = DemoApp::default();
    event_loop.run_app(&mut app).expect("window demo failed");
}

#[derive(Default)]
struct DemoApp {
    renderer: Option<WgpuRenderer>,
    native_window: Option<NativeWindow>,
    pointer_state: PointerState,
    focus_state: FocusState,
    pointer_position: Point,
    modifiers: Modifiers,
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
            WindowEvent::CursorMoved { position, .. } => {
                let frame = demo_frame(native_window.target.size());
                self.pointer_position = Point::new(position.x as f32, position.y as f32);
                let transition = self.pointer_state.process(
                    &frame,
                    PointerEvent {
                        kind: PointerEventKind::Move,
                        position: self.pointer_position,
                        button: None,
                        modifiers: self.modifiers,
                    },
                );
                self.log_dispatch(&frame, &transition);
            }
            WindowEvent::CursorLeft { .. } => {
                let frame = demo_frame(native_window.target.size());
                let transition = self.pointer_state.process(
                    &frame,
                    PointerEvent {
                        kind: PointerEventKind::Leave,
                        position: self.pointer_position,
                        button: None,
                        modifiers: self.modifiers,
                    },
                );
                self.log_dispatch(&frame, &transition);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = convert_modifiers(modifiers.state());
                let event = KeyboardEvent::ModifiersChanged(ModifiersChangedEvent {
                    modifiers: self.modifiers,
                });
                log_keyboard_event(&event);
                self.log_keyboard_dispatch(&event);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event = convert_keyboard_event(event, self.modifiers);
                log_keyboard_event(&event);
                self.log_keyboard_dispatch(&event);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = convert_pointer_button(button) else {
                    return;
                };
                let frame = demo_frame(native_window.target.size());
                let transition = self.pointer_state.process(
                    &frame,
                    PointerEvent {
                        kind: match state {
                            ElementState::Pressed => PointerEventKind::Down,
                            ElementState::Released => PointerEventKind::Up,
                        },
                        position: self.pointer_position,
                        button: Some(button),
                        modifiers: self.modifiers,
                    },
                );
                self.log_dispatch(&frame, &transition);
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

fn convert_modifiers(modifiers: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: modifiers.shift_key(),
        control: modifiers.control_key(),
        alt: modifiers.alt_key(),
        platform: modifiers.super_key(),
        function: false,
    }
}

fn convert_keyboard_event(event: winit::event::KeyEvent, modifiers: Modifiers) -> KeyboardEvent {
    let keystroke = build_keystroke(
        event.key_without_modifiers(),
        event.text.as_deref(),
        modifiers,
        event.location,
    );

    match event.state {
        ElementState::Pressed => KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke,
            repeat: event.repeat,
            prefer_text: false,
        }),
        ElementState::Released => KeyboardEvent::KeyUp(KeyUpEvent { keystroke }),
    }
}

fn build_keystroke(
    binding_key: WinitKey,
    text: Option<&str>,
    modifiers: Modifiers,
    location: WinitKeyLocation,
) -> Keystroke {
    let key = convert_key(binding_key);
    Keystroke {
        text: normalize_text(text, modifiers, &key),
        key,
        modifiers,
        location: convert_key_location(location),
    }
}

fn normalize_text(text: Option<&str>, modifiers: Modifiers, key: &Key) -> Option<String> {
    if modifiers.control || modifiers.platform {
        return None;
    }

    match key {
        Key::Character(_) | Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Tab) => {
            text.map(ToOwned::to_owned)
        }
        Key::Named(NamedKey::Space) => text.map(ToOwned::to_owned),
        Key::Named(_) | Key::Function(_) | Key::Modifier(_) | Key::Dead(_) | Key::Unknown(_) => {
            None
        }
    }
}

fn convert_key(key: WinitKey) -> Key {
    match key {
        WinitKey::Character(text) => Key::Character(text.to_string()),
        WinitKey::Named(key) => convert_named_key(key),
        WinitKey::Dead(dead) => Key::Dead(dead),
        WinitKey::Unidentified(native) => Key::Unknown(format!("{native:?}")),
    }
}

fn convert_named_key(key: WinitNamedKey) -> Key {
    match key {
        WinitNamedKey::Escape => Key::Named(NamedKey::Escape),
        WinitNamedKey::Enter => Key::Named(NamedKey::Enter),
        WinitNamedKey::Tab => Key::Named(NamedKey::Tab),
        WinitNamedKey::Space => Key::Named(NamedKey::Space),
        WinitNamedKey::Backspace => Key::Named(NamedKey::Backspace),
        WinitNamedKey::Delete => Key::Named(NamedKey::Delete),
        WinitNamedKey::Insert => Key::Named(NamedKey::Insert),
        WinitNamedKey::ArrowUp => Key::Named(NamedKey::ArrowUp),
        WinitNamedKey::ArrowDown => Key::Named(NamedKey::ArrowDown),
        WinitNamedKey::ArrowLeft => Key::Named(NamedKey::ArrowLeft),
        WinitNamedKey::ArrowRight => Key::Named(NamedKey::ArrowRight),
        WinitNamedKey::Home => Key::Named(NamedKey::Home),
        WinitNamedKey::End => Key::Named(NamedKey::End),
        WinitNamedKey::PageUp => Key::Named(NamedKey::PageUp),
        WinitNamedKey::PageDown => Key::Named(NamedKey::PageDown),
        WinitNamedKey::Shift => Key::Modifier(ModifierKey::Shift),
        WinitNamedKey::Control => Key::Modifier(ModifierKey::Control),
        WinitNamedKey::Alt => Key::Modifier(ModifierKey::Alt),
        WinitNamedKey::AltGraph => Key::Modifier(ModifierKey::AltGraph),
        WinitNamedKey::Super => Key::Modifier(ModifierKey::Platform),
        WinitNamedKey::Fn => Key::Modifier(ModifierKey::Function),
        WinitNamedKey::CapsLock => Key::Modifier(ModifierKey::CapsLock),
        WinitNamedKey::F1 => Key::Function(1),
        WinitNamedKey::F2 => Key::Function(2),
        WinitNamedKey::F3 => Key::Function(3),
        WinitNamedKey::F4 => Key::Function(4),
        WinitNamedKey::F5 => Key::Function(5),
        WinitNamedKey::F6 => Key::Function(6),
        WinitNamedKey::F7 => Key::Function(7),
        WinitNamedKey::F8 => Key::Function(8),
        WinitNamedKey::F9 => Key::Function(9),
        WinitNamedKey::F10 => Key::Function(10),
        WinitNamedKey::F11 => Key::Function(11),
        WinitNamedKey::F12 => Key::Function(12),
        key => Key::Unknown(format!("{key:?}")),
    }
}

fn convert_key_location(location: WinitKeyLocation) -> KeyLocation {
    match location {
        WinitKeyLocation::Standard => KeyLocation::Standard,
        WinitKeyLocation::Left => KeyLocation::Left,
        WinitKeyLocation::Right => KeyLocation::Right,
        WinitKeyLocation::Numpad => KeyLocation::Numpad,
    }
}

fn convert_pointer_button(button: WinitMouseButton) -> Option<PointerButton> {
    match button {
        WinitMouseButton::Left => Some(PointerButton::Primary),
        WinitMouseButton::Right => Some(PointerButton::Secondary),
        WinitMouseButton::Middle => Some(PointerButton::Middle),
        WinitMouseButton::Back | WinitMouseButton::Forward => None,
        WinitMouseButton::Other(button) => Some(PointerButton::Other(button)),
    }
}

fn log_pointer_transition(transition: &PointerTransition, plan: &PointerDispatchPlan) {
    if transition.entered.is_empty()
        && transition.exited.is_empty()
        && transition.pressed.is_none()
        && transition.released.is_none()
        && plan.focus_request.is_none()
    {
        return;
    }

    println!("pointer target: {:?}", transition.target);
    println!("dispatch capture path: {:?}", plan.capture_path);
    println!("dispatch bubble path: {:?}", plan.bubble_path);
    if !transition.entered.is_empty() {
        println!("entered: {:?}", transition.entered);
    }
    if !transition.exited.is_empty() {
        println!("exited: {:?}", transition.exited);
    }
    if let Some(press) = transition.pressed {
        println!("pressed: {:?}", press);
    }
    if let Some(press) = transition.released {
        println!("released: {:?}", press);
    }
    if let Some(focus_request) = plan.focus_request {
        println!("focus request: {:?}", focus_request);
    }
}

fn log_pointer_dispatch_result(result: &PointerDispatchResult) {
    println!(
        "pointer result: handled={}, propagation_stopped={}, default_prevented={}, consumer={:?}",
        result.handled, result.propagation_stopped, result.default_prevented, result.consumer
    );
}

fn log_keyboard_event(event: &KeyboardEvent) {
    match event {
        KeyboardEvent::KeyDown(event) => println!(
            "keyboard down: key={:?}, text={:?}, modifiers={:?}, location={:?}, repeat={}, prefer_text={}",
            event.keystroke.key,
            event.keystroke.text,
            event.keystroke.modifiers,
            event.keystroke.location,
            event.repeat,
            event.prefer_text
        ),
        KeyboardEvent::KeyUp(event) => println!(
            "keyboard up: key={:?}, modifiers={:?}, location={:?}",
            event.keystroke.key, event.keystroke.modifiers, event.keystroke.location
        ),
        KeyboardEvent::ModifiersChanged(event) => {
            println!("modifiers changed: {:?}", event.modifiers)
        }
    }
}

fn log_keyboard_dispatch_result(result: &KeyboardDispatchResult) {
    println!(
        "keyboard result: handled={}, propagation_stopped={}, default_prevented={}, consumer={:?}",
        result.handled, result.propagation_stopped, result.default_prevented, result.consumer
    );
}

fn log_keyboard_dispatch_plan(plan: &KeyboardDispatchPlan) {
    println!(
        "keyboard plan: kind={:?}, focused={:?}, target={:?}, propagation={:?}",
        plan.kind, plan.focused, plan.target, plan.propagation
    );
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

    fn log_dispatch(&mut self, frame: &Frame, transition: &PointerTransition) {
        let plan = plan_pointer_dispatch(frame, transition);
        log_pointer_transition(transition, &plan);
        if let Some(focus_request) = plan.focus_request {
            let focus_transition = self.focus_state.focus(focus_request);
            println!("focus transition: {:?}", focus_transition);
        }
        let result = simulate_pointer_dispatch_result(&plan);
        log_pointer_dispatch_result(&result);
    }

    fn log_keyboard_dispatch(&self, event: &KeyboardEvent) {
        let plan = plan_keyboard_dispatch(&self.focus_state, event.clone());
        log_keyboard_dispatch_plan(&plan);
        let result = simulate_keyboard_dispatch_result(&plan);
        log_keyboard_dispatch_result(&result);
    }
}

fn simulate_pointer_dispatch_result(plan: &PointerDispatchPlan) -> PointerDispatchResult {
    let mut result = pointer_dispatch_result(plan, plan.target);
    if matches!(plan.event_kind, PointerEventKind::Down) && result.handled {
        result = result.prevent_default();
    }
    if plan.captured.is_some() {
        result = result.stop_propagation();
    }
    result
}

fn simulate_keyboard_dispatch_result(plan: &KeyboardDispatchPlan) -> KeyboardDispatchResult {
    if plan.propagation == KeyboardPropagation::None {
        return keyboard_dispatch_result(plan, None);
    }

    match plan.target {
        Some(target) => keyboard_dispatch_result(plan, Some(target)).stop_propagation(),
        None => keyboard_dispatch_result(plan, None),
    }
}

fn demo_frame(size: PhysicalSize<u32>) -> Frame {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let blue_rect = Rect::from_xywh(width * 0.08, height * 0.12, width * 0.48, height * 0.36);
    let white_rect = Rect::from_xywh(width * 0.44, height * 0.34, width * 0.40, height * 0.42);
    let stroke_rect = Rect::from_xywh(width * 0.18, height * 0.20, width * 0.64, height * 0.58);

    let mut paint = PaintContext::new();
    paint.clear_color(Color::rgb(10, 13, 18));
    paint.fill_rect(blue_rect, Color::rgb(58, 105, 210));
    paint.hit_region(BLUE_RECT_HIT, blue_rect);
    paint.focus_region(BLUE_RECT_FOCUS, blue_rect);

    paint.with_layer(Layer(1), |paint| {
        paint.fill_rect(white_rect, Color::rgb(235, 238, 245));
        paint.hit_region(WHITE_RECT_HIT, white_rect);
        paint.focus_region(WHITE_RECT_FOCUS, white_rect);
    });

    paint.with_layer(Layer(2), |paint| {
        paint.stroke_rect(stroke_rect, Stroke::new(Color::rgb(255, 88, 88), 4.0));
        paint.hit_region_with_behavior(STROKE_RECT_HIT, stroke_rect, HitBehavior::BlockPointer);
    });

    paint.finish_frame(Size::new(width, height))
}

#[cfg(test)]
fn top_hit(frame: &Frame, point: Point) -> Option<HitRegionId> {
    zri::input::hit_test(frame, point).regions.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_frame_contains_supported_wgpu_primitives_only() {
        let frame = demo_frame(PhysicalSize::new(800, 600));
        assert_eq!(frame.scene.items().len(), 4);
    }

    #[test]
    fn demo_frame_contains_hit_regions() {
        let frame = demo_frame(PhysicalSize::new(800, 600));
        assert_eq!(frame.hit_regions.len(), 3);
    }

    #[test]
    fn demo_frame_contains_focus_regions() {
        let frame = demo_frame(PhysicalSize::new(800, 600));
        assert_eq!(frame.focus_regions.len(), 2);
    }

    #[test]
    fn convert_modifiers_sets_keyboard_modifier_state() {
        let modifiers = convert_modifiers(winit::keyboard::ModifiersState::SHIFT);

        assert!(modifiers.shift);
        assert!(!modifiers.control);
        assert!(!modifiers.alt);
        assert!(!modifiers.platform);
        assert!(!modifiers.function);
    }

    #[test]
    fn convert_named_key_maps_common_keys() {
        assert_eq!(
            convert_named_key(WinitNamedKey::Escape),
            Key::Named(NamedKey::Escape)
        );
        assert_eq!(convert_named_key(WinitNamedKey::F1), Key::Function(1));
        assert_eq!(
            convert_named_key(WinitNamedKey::Shift),
            Key::Modifier(ModifierKey::Shift)
        );
    }

    #[test]
    fn convert_key_preserves_character_and_dead_keys() {
        assert_eq!(
            convert_key(WinitKey::Character("a".into())),
            Key::Character("a".to_string())
        );
        assert_eq!(
            convert_key(WinitKey::Dead(Some('\''))),
            Key::Dead(Some('\''))
        );
    }

    #[test]
    fn convert_key_location_maps_numpad() {
        assert_eq!(
            convert_key_location(WinitKeyLocation::Numpad),
            KeyLocation::Numpad
        );
    }

    #[test]
    fn build_keystroke_keeps_shifted_alpha_as_base_key_identity() {
        let keystroke = build_keystroke(
            WinitKey::Character("a".into()),
            Some("A"),
            Modifiers {
                shift: true,
                control: false,
                alt: false,
                platform: false,
                function: false,
            },
            WinitKeyLocation::Standard,
        );

        assert_eq!(keystroke.key, Key::Character("a".to_string()));
        assert_eq!(keystroke.text, Some("A".to_string()));
        assert!(keystroke.modifiers.shift);
    }

    #[test]
    fn build_keystroke_keeps_shifted_symbol_as_base_key_identity() {
        let keystroke = build_keystroke(
            WinitKey::Character("2".into()),
            Some("@"),
            Modifiers {
                shift: true,
                control: false,
                alt: false,
                platform: false,
                function: false,
            },
            WinitKeyLocation::Standard,
        );

        assert_eq!(keystroke.key, Key::Character("2".to_string()));
        assert_eq!(keystroke.text, Some("@".to_string()));
        assert!(keystroke.modifiers.shift);
    }

    #[test]
    fn build_keystroke_suppresses_text_for_control_modified_printable_keys() {
        let keystroke = build_keystroke(
            WinitKey::Character("a".into()),
            Some("a"),
            Modifiers {
                shift: false,
                control: true,
                alt: false,
                platform: false,
                function: false,
            },
            WinitKeyLocation::Standard,
        );

        assert_eq!(keystroke.key, Key::Character("a".to_string()));
        assert_eq!(keystroke.text, None);
    }

    #[test]
    fn build_keystroke_suppresses_text_for_platform_modified_printable_keys() {
        let keystroke = build_keystroke(
            WinitKey::Character("a".into()),
            Some("a"),
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                platform: true,
                function: false,
            },
            WinitKeyLocation::Standard,
        );

        assert_eq!(keystroke.key, Key::Character("a".to_string()));
        assert_eq!(keystroke.text, None);
    }

    #[test]
    fn simulate_pointer_dispatch_result_handles_targets() {
        let frame = demo_frame(PhysicalSize::new(800, 600));
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            PointerEvent {
                kind: PointerEventKind::Down,
                position: Point::new(80.0, 90.0),
                button: Some(PointerButton::Primary),
                modifiers: Modifiers::default(),
            },
        );
        let plan = plan_pointer_dispatch(&frame, &transition);
        let result = simulate_pointer_dispatch_result(&plan);

        assert!(result.handled);
        assert!(result.default_prevented);
        assert_eq!(result.consumer, Some(BLUE_RECT_HIT));
    }

    #[test]
    fn simulate_keyboard_dispatch_result_uses_focus_target() {
        let mut focus_state = FocusState::new();
        focus_state.focus(BLUE_RECT_FOCUS);
        let plan = plan_keyboard_dispatch(
            &focus_state,
            KeyboardEvent::KeyDown(KeyDownEvent {
                keystroke: Keystroke {
                    key: Key::Character("a".to_string()),
                    text: Some("a".to_string()),
                    modifiers: Modifiers::default(),
                    location: KeyLocation::Standard,
                },
                repeat: false,
                prefer_text: false,
            }),
        );
        let result = simulate_keyboard_dispatch_result(&plan);

        assert!(result.handled);
        assert!(result.propagation_stopped);
        assert_eq!(result.consumer, Some(BLUE_RECT_FOCUS));
        assert_eq!(plan.propagation, KeyboardPropagation::FocusOnly);
    }

    #[test]
    fn simulate_keyboard_dispatch_result_ignores_untargeted_plan() {
        let focus_state = FocusState::new();
        let plan = plan_keyboard_dispatch(
            &focus_state,
            KeyboardEvent::ModifiersChanged(ModifiersChangedEvent {
                modifiers: Modifiers::default(),
            }),
        );
        let result = simulate_keyboard_dispatch_result(&plan);

        assert!(!result.handled);
        assert_eq!(result.consumer, None);
        assert_eq!(plan.propagation, KeyboardPropagation::None);
    }

    #[test]
    fn demo_frame_hit_test_returns_topmost_region() {
        let frame = demo_frame(PhysicalSize::new(800, 600));

        assert_eq!(top_hit(&frame, Point::new(80.0, 90.0)), Some(BLUE_RECT_HIT));
        assert_eq!(
            top_hit(&frame, Point::new(400.0, 260.0)),
            Some(STROKE_RECT_HIT)
        );
        assert_eq!(top_hit(&frame, Point::new(760.0, 560.0)), None);
    }
}
