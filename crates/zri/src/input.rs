use std::collections::HashMap;
use std::sync::Arc;

use crate::render::{Frame, Layer, Point, Rect};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HitRegionId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HitBehavior {
    #[default]
    Normal,
    BlockPointer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitRegion {
    pub id: HitRegionId,
    pub rect: Rect,
    pub clip: Option<Rect>,
    pub layer: Layer,
    pub behavior: HitBehavior,
}

impl HitRegion {
    pub fn new(id: HitRegionId, rect: Rect) -> Self {
        Self {
            id,
            rect,
            clip: None,
            layer: Layer::default(),
            behavior: HitBehavior::default(),
        }
    }

    pub fn clipped(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    pub fn layered(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }

    pub fn with_behavior(mut self, behavior: HitBehavior) -> Self {
        self.behavior = behavior;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FocusId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusRegion {
    pub id: FocusId,
    pub rect: Rect,
    pub clip: Option<Rect>,
    pub layer: Layer,
}

impl FocusRegion {
    pub fn new(id: FocusId, rect: Rect) -> Self {
        Self {
            id,
            rect,
            clip: None,
            layer: Layer::default(),
        }
    }

    pub fn clipped(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    pub fn layered(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FocusState {
    focused: Option<FocusId>,
}

impl FocusState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn focused(&self) -> Option<FocusId> {
        self.focused
    }

    pub fn focus(&mut self, id: FocusId) -> FocusTransition {
        let previous = self.focused;
        self.focused = Some(id);
        FocusTransition {
            previous,
            current: self.focused,
        }
    }

    pub fn blur(&mut self) -> FocusTransition {
        let previous = self.focused;
        self.focused = None;
        FocusTransition {
            previous,
            current: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusTransition {
    pub previous: Option<FocusId>,
    pub current: Option<FocusId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HitTest {
    pub regions: Vec<HitRegionId>,
}

impl HitTest {
    pub fn top(&self) -> Option<HitRegionId> {
        self.regions.first().copied()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Pointer(PointerEvent),
    Keyboard(KeyboardEvent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyboardEvent {
    KeyDown(KeyDownEvent),
    KeyUp(KeyUpEvent),
    ModifiersChanged(ModifiersChangedEvent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyDownEvent {
    pub keystroke: Keystroke,
    pub repeat: bool,
    pub prefer_text: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyUpEvent {
    pub keystroke: Keystroke,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModifiersChangedEvent {
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Keystroke {
    pub key: Key,
    pub text: Option<String>,
    pub modifiers: Modifiers,
    pub location: KeyLocation,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    Character(String),
    Named(NamedKey),
    Function(u8),
    Modifier(ModifierKey),
    Dead(Option<char>),
    Unknown(String),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NamedKey {
    Escape,
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModifierKey {
    Shift,
    Control,
    Alt,
    AltGraph,
    Platform,
    Function,
    CapsLock,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum KeyLocation {
    #[default]
    Standard,
    Left,
    Right,
    Numpad,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerEventKind,
    pub position: Point,
    pub button: Option<PointerButton>,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerEventKind {
    Move,
    Down,
    Up,
    Scroll,
    Enter,
    Leave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub platform: bool,
    pub function: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn any(&self) -> bool {
        self.shift || self.control || self.alt || self.platform || self.function
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PointerState {
    hovered: HitTest,
    captured: Option<HitRegionId>,
    pressed: Option<PointerPress>,
}

impl PointerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hovered(&self) -> &HitTest {
        &self.hovered
    }

    pub fn captured(&self) -> Option<HitRegionId> {
        self.captured
    }

    pub fn pressed(&self) -> Option<PointerPress> {
        self.pressed
    }

    pub fn capture(&mut self, id: HitRegionId) {
        self.captured = Some(id);
    }

    pub fn release_capture(&mut self) {
        self.captured = None;
    }

    pub fn process(&mut self, frame: &Frame, event: PointerEvent) -> PointerTransition {
        let previous_hovered = self.hovered.clone();
        let previous_pressed = self.pressed;
        let hit_test = match event.kind {
            PointerEventKind::Leave => HitTest::default(),
            PointerEventKind::Move
            | PointerEventKind::Down
            | PointerEventKind::Up
            | PointerEventKind::Scroll
            | PointerEventKind::Enter => hit_test(frame, event.position),
        };
        let entered = difference(&hit_test.regions, &previous_hovered.regions);
        let exited = difference(&previous_hovered.regions, &hit_test.regions);
        self.hovered = hit_test.clone();

        let target = self.captured.or_else(|| hit_test.top());
        let mut pressed = None;
        let mut released = None;

        match event.kind {
            PointerEventKind::Down => {
                if let Some(button) = event.button {
                    let press = PointerPress { button, target };
                    self.pressed = Some(press);
                    pressed = Some(press);
                }
            }
            PointerEventKind::Up => {
                released = previous_pressed;
                self.pressed = None;
                self.release_capture();
            }
            PointerEventKind::Leave => {
                self.pressed = None;
            }
            PointerEventKind::Move | PointerEventKind::Scroll | PointerEventKind::Enter => {}
        }

        PointerTransition {
            event_kind: event.kind,
            position: event.position,
            hit_test,
            target,
            captured: self.captured,
            entered,
            exited,
            pressed,
            released,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerPress {
    pub button: PointerButton,
    pub target: Option<HitRegionId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointerTransition {
    pub event_kind: PointerEventKind,
    pub position: Point,
    pub hit_test: HitTest,
    pub target: Option<HitRegionId>,
    pub captured: Option<HitRegionId>,
    pub entered: Vec<HitRegionId>,
    pub exited: Vec<HitRegionId>,
    pub pressed: Option<PointerPress>,
    pub released: Option<PointerPress>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchPhase {
    Capture,
    Bubble,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerDispatchStep {
    pub phase: DispatchPhase,
    pub region: HitRegionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerDispatchPlan {
    pub event_kind: PointerEventKind,
    pub physical_hit_test: HitTest,
    pub target: Option<HitRegionId>,
    pub captured: Option<HitRegionId>,
    pub capture_path: Vec<HitRegionId>,
    pub bubble_path: Vec<HitRegionId>,
    pub focus_request: Option<FocusId>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DispatchTarget {
    HitRegion(HitRegionId),
    FocusRegion(FocusId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchResult<Target> {
    pub handled: bool,
    pub propagation_stopped: bool,
    pub default_prevented: bool,
    pub consumer: Option<Target>,
}

impl<Target> DispatchResult<Target> {
    pub fn ignored() -> Self {
        Self {
            handled: false,
            propagation_stopped: false,
            default_prevented: false,
            consumer: None,
        }
    }

    pub fn handled_by(target: Target) -> Self {
        Self {
            handled: true,
            propagation_stopped: false,
            default_prevented: false,
            consumer: Some(target),
        }
    }

    pub fn with_handled(mut self, handled: bool) -> Self {
        self.handled = handled;
        self
    }

    pub fn stop_propagation(mut self) -> Self {
        self.propagation_stopped = true;
        self
    }

    pub fn prevent_default(mut self) -> Self {
        self.default_prevented = true;
        self
    }
}

pub type PointerDispatchResult = DispatchResult<HitRegionId>;
pub type KeyboardDispatchResult = DispatchResult<FocusId>;
pub type InputDispatchResult = DispatchResult<DispatchTarget>;

pub struct PointerDispatchContext<'a> {
    pub plan: &'a PointerDispatchPlan,
}

pub struct KeyboardDispatchContext<'a> {
    pub plan: &'a KeyboardDispatchPlan,
}

pub type PointerHandler =
    Arc<dyn Fn(&PointerDispatchContext<'_>) -> PointerDispatchResult + Send + Sync + 'static>;
pub type KeyboardHandler =
    Arc<dyn Fn(&KeyboardDispatchContext<'_>) -> KeyboardDispatchResult + Send + Sync + 'static>;

#[derive(Default)]
pub struct PointerListenerRegistry {
    handlers: HashMap<HitRegionId, Vec<PointerHandler>>,
}

impl PointerListenerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, target: HitRegionId, handler: PointerHandler) {
        self.handlers.entry(target).or_default().push(handler);
    }

    pub fn handlers_for(&self, target: HitRegionId) -> Option<&[PointerHandler]> {
        self.handlers.get(&target).map(Vec::as_slice)
    }
}

#[derive(Default)]
pub struct KeyboardListenerRegistry {
    handlers: HashMap<FocusId, Vec<KeyboardHandler>>,
}

impl KeyboardListenerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, target: FocusId, handler: KeyboardHandler) {
        self.handlers.entry(target).or_default().push(handler);
    }

    pub fn handlers_for(&self, target: FocusId) -> Option<&[KeyboardHandler]> {
        self.handlers.get(&target).map(Vec::as_slice)
    }
}

#[derive(Default)]
pub struct InputListenerRegistry {
    pub pointer: PointerListenerRegistry,
    pub keyboard: KeyboardListenerRegistry,
}

impl InputListenerRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardDispatchKind {
    KeyDown,
    KeyUp,
    ModifiersChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardPropagation {
    FocusOnly,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyboardDispatchPlan {
    pub event: KeyboardEvent,
    pub focused: Option<FocusId>,
    pub target: Option<FocusId>,
    pub kind: KeyboardDispatchKind,
    pub propagation: KeyboardPropagation,
}

impl KeyboardDispatchPlan {
    pub fn is_key_down(&self) -> bool {
        self.kind == KeyboardDispatchKind::KeyDown
    }

    pub fn is_key_up(&self) -> bool {
        self.kind == KeyboardDispatchKind::KeyUp
    }

    pub fn is_modifiers_changed(&self) -> bool {
        self.kind == KeyboardDispatchKind::ModifiersChanged
    }

    pub fn targets_focus(&self) -> bool {
        self.propagation == KeyboardPropagation::FocusOnly
    }

    pub fn is_dispatchable(&self) -> bool {
        self.target.is_some()
    }
}

pub fn hit_test(frame: &Frame, point: Point) -> HitTest {
    let frame_rect = Rect::from_xywh(0.0, 0.0, frame.size.width, frame.size.height);
    if !frame_rect.contains(point) {
        return HitTest::default();
    }

    let mut indexed_regions = frame.hit_regions.iter().enumerate().collect::<Vec<_>>();
    indexed_regions.sort_by_key(|(index, region)| (region.layer, *index));

    let mut hit_test = HitTest::default();
    for (_, region) in indexed_regions.into_iter().rev() {
        let effective_rect = effective_region_rect(region).intersect(&frame_rect);
        if !effective_rect.contains(point) {
            continue;
        }

        hit_test.regions.push(region.id);
        if region.behavior == HitBehavior::BlockPointer {
            break;
        }
    }
    hit_test
}

pub fn focus_test(frame: &Frame, point: Point) -> Option<FocusId> {
    let frame_rect = Rect::from_xywh(0.0, 0.0, frame.size.width, frame.size.height);
    if !frame_rect.contains(point) {
        return None;
    }

    let mut indexed_regions = frame.focus_regions.iter().enumerate().collect::<Vec<_>>();
    indexed_regions.sort_by_key(|(index, region)| (region.layer, *index));

    indexed_regions.into_iter().rev().find_map(|(_, region)| {
        let effective_rect = effective_focus_region_rect(region).intersect(&frame_rect);
        effective_rect.contains(point).then_some(region.id)
    })
}

pub fn plan_pointer_dispatch(frame: &Frame, transition: &PointerTransition) -> PointerDispatchPlan {
    let mut bubble_path = transition.hit_test.regions.clone();
    if let Some(captured) = transition.captured {
        if !bubble_path.contains(&captured) {
            bubble_path.insert(0, captured);
        }
    }

    let mut capture_path = bubble_path.clone();
    capture_path.reverse();

    let focus_request = match transition.event_kind {
        PointerEventKind::Down if transition.captured.is_none() => {
            focus_test(frame, transition.position)
        }
        PointerEventKind::Move
        | PointerEventKind::Up
        | PointerEventKind::Scroll
        | PointerEventKind::Enter
        | PointerEventKind::Leave
        | PointerEventKind::Down => None,
    };

    PointerDispatchPlan {
        event_kind: transition.event_kind,
        physical_hit_test: transition.hit_test.clone(),
        target: transition.target,
        captured: transition.captured,
        capture_path,
        bubble_path,
        focus_request,
    }
}

pub fn plan_keyboard_dispatch(
    focus_state: &FocusState,
    event: KeyboardEvent,
) -> KeyboardDispatchPlan {
    let focused = focus_state.focused();
    let kind = match &event {
        KeyboardEvent::KeyDown(_) => KeyboardDispatchKind::KeyDown,
        KeyboardEvent::KeyUp(_) => KeyboardDispatchKind::KeyUp,
        KeyboardEvent::ModifiersChanged(_) => KeyboardDispatchKind::ModifiersChanged,
    };
    KeyboardDispatchPlan {
        event,
        focused,
        target: focused,
        kind,
        propagation: match focused {
            Some(_) => KeyboardPropagation::FocusOnly,
            None => KeyboardPropagation::None,
        },
    }
}

pub fn pointer_dispatch_result(
    _plan: &PointerDispatchPlan,
    consumer: Option<HitRegionId>,
) -> PointerDispatchResult {
    match consumer {
        Some(consumer) => PointerDispatchResult::handled_by(consumer),
        None => PointerDispatchResult::ignored(),
    }
}

pub fn keyboard_dispatch_result(
    plan: &KeyboardDispatchPlan,
    consumer: Option<FocusId>,
) -> KeyboardDispatchResult {
    if !plan.is_dispatchable() {
        return KeyboardDispatchResult::ignored();
    }

    match consumer {
        Some(consumer) if Some(consumer) == plan.target => {
            KeyboardDispatchResult::handled_by(consumer)
        }
        None => KeyboardDispatchResult::ignored(),
        Some(_) => KeyboardDispatchResult::ignored(),
    }
}

impl From<PointerDispatchResult> for InputDispatchResult {
    fn from(value: PointerDispatchResult) -> Self {
        InputDispatchResult {
            handled: value.handled,
            propagation_stopped: value.propagation_stopped,
            default_prevented: value.default_prevented,
            consumer: value.consumer.map(DispatchTarget::HitRegion),
        }
    }
}

impl From<KeyboardDispatchResult> for InputDispatchResult {
    fn from(value: KeyboardDispatchResult) -> Self {
        InputDispatchResult {
            handled: value.handled,
            propagation_stopped: value.propagation_stopped,
            default_prevented: value.default_prevented,
            consumer: value.consumer.map(DispatchTarget::FocusRegion),
        }
    }
}

pub fn dispatch_pointer_plan(
    registry: &PointerListenerRegistry,
    plan: &PointerDispatchPlan,
) -> PointerDispatchResult {
    let context = PointerDispatchContext { plan };
    let mut result = PointerDispatchResult::ignored();

    for target in &plan.capture_path {
        let Some(handlers) = registry.handlers_for(*target) else {
            continue;
        };

        for handler in handlers {
            result = merge_pointer_results(result, handler(&context));
            if result.propagation_stopped {
                return result;
            }
        }
    }

    result
}

pub fn dispatch_keyboard_plan(
    registry: &KeyboardListenerRegistry,
    plan: &KeyboardDispatchPlan,
) -> KeyboardDispatchResult {
    if !plan.is_dispatchable() {
        return KeyboardDispatchResult::ignored();
    }

    let Some(target) = plan.target else {
        return KeyboardDispatchResult::ignored();
    };
    let Some(handlers) = registry.handlers_for(target) else {
        return KeyboardDispatchResult::ignored();
    };

    let context = KeyboardDispatchContext { plan };
    let mut result = KeyboardDispatchResult::ignored();
    for handler in handlers {
        result = merge_keyboard_results(result, handler(&context));
        if result.propagation_stopped {
            return result;
        }
    }

    result
}

pub fn dispatch_pointer(
    registry: &InputListenerRegistry,
    plan: &PointerDispatchPlan,
) -> PointerDispatchResult {
    dispatch_pointer_plan(&registry.pointer, plan)
}

pub fn dispatch_keyboard(
    registry: &InputListenerRegistry,
    plan: &KeyboardDispatchPlan,
) -> KeyboardDispatchResult {
    dispatch_keyboard_plan(&registry.keyboard, plan)
}

fn merge_pointer_results(
    current: PointerDispatchResult,
    next: PointerDispatchResult,
) -> PointerDispatchResult {
    PointerDispatchResult {
        handled: current.handled || next.handled,
        propagation_stopped: current.propagation_stopped || next.propagation_stopped,
        default_prevented: current.default_prevented || next.default_prevented,
        consumer: current.consumer.or(next.consumer),
    }
}

fn merge_keyboard_results(
    current: KeyboardDispatchResult,
    next: KeyboardDispatchResult,
) -> KeyboardDispatchResult {
    KeyboardDispatchResult {
        handled: current.handled || next.handled,
        propagation_stopped: current.propagation_stopped || next.propagation_stopped,
        default_prevented: current.default_prevented || next.default_prevented,
        consumer: current.consumer.or(next.consumer),
    }
}

fn effective_region_rect(region: &HitRegion) -> Rect {
    match region.clip {
        Some(clip) => region.rect.intersect(&clip),
        None => region.rect,
    }
}

fn effective_focus_region_rect(region: &FocusRegion) -> Rect {
    match region.clip {
        Some(clip) => region.rect.intersect(&clip),
        None => region.rect,
    }
}

fn difference(current: &[HitRegionId], previous: &[HitRegionId]) -> Vec<HitRegionId> {
    current
        .iter()
        .copied()
        .filter(|id| !previous.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Scene, Size};

    const BACK: HitRegionId = HitRegionId(1);
    const FRONT: HitRegionId = HitRegionId(2);
    const BLOCKER: HitRegionId = HitRegionId(3);
    const BACK_FOCUS: FocusId = FocusId(11);
    const FRONT_FOCUS: FocusId = FocusId(12);

    fn frame_with_regions(hit_regions: Vec<HitRegion>) -> Frame {
        Frame::with_hit_regions(Size::new(10.0, 10.0), Scene::new(), hit_regions)
    }

    fn frame_with_focus_regions(focus_regions: Vec<FocusRegion>) -> Frame {
        Frame::with_interaction_regions(
            Size::new(10.0, 10.0),
            Scene::new(),
            Vec::new(),
            focus_regions,
        )
    }

    #[test]
    fn hit_region_builder_sets_metadata() {
        let region = HitRegion::new(BACK, Rect::from_xywh(1.0, 2.0, 3.0, 4.0))
            .clipped(Rect::from_xywh(1.0, 2.0, 2.0, 2.0))
            .layered(Layer(5))
            .with_behavior(HitBehavior::BlockPointer);

        assert_eq!(region.id, BACK);
        assert_eq!(region.rect, Rect::from_xywh(1.0, 2.0, 3.0, 4.0));
        assert_eq!(region.clip, Some(Rect::from_xywh(1.0, 2.0, 2.0, 2.0)));
        assert_eq!(region.layer, Layer(5));
        assert_eq!(region.behavior, HitBehavior::BlockPointer);
    }

    #[test]
    fn pointer_event_preserves_backend_agnostic_data() {
        let event = InputEvent::Pointer(PointerEvent {
            kind: PointerEventKind::Down,
            position: Point::new(1.0, 2.0),
            button: Some(PointerButton::Primary),
            modifiers: Modifiers {
                shift: true,
                control: false,
                alt: true,
                platform: false,
                function: false,
            },
        });

        assert_eq!(
            event,
            InputEvent::Pointer(PointerEvent {
                kind: PointerEventKind::Down,
                position: Point::new(1.0, 2.0),
                button: Some(PointerButton::Primary),
                modifiers: Modifiers {
                    shift: true,
                    control: false,
                    alt: true,
                    platform: false,
                    function: false,
                },
            })
        );
    }

    #[test]
    fn keyboard_event_preserves_key_down_data() {
        let keystroke = Keystroke {
            key: Key::Character("a".to_string()),
            text: Some("a".to_string()),
            modifiers: Modifiers::default(),
            location: KeyLocation::Standard,
        };
        let event = InputEvent::Keyboard(KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: keystroke.clone(),
            repeat: false,
            prefer_text: false,
        }));

        assert_eq!(
            event,
            InputEvent::Keyboard(KeyboardEvent::KeyDown(KeyDownEvent {
                keystroke,
                repeat: false,
                prefer_text: false,
            }))
        );
    }

    #[test]
    fn keystroke_separates_key_identity_from_produced_text() {
        let keystroke = Keystroke {
            key: Key::Character("s".to_string()),
            text: Some("ß".to_string()),
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: true,
                platform: false,
                function: false,
            },
            location: KeyLocation::Standard,
        };

        assert_eq!(keystroke.key, Key::Character("s".to_string()));
        assert_eq!(keystroke.text, Some("ß".to_string()));
        assert!(keystroke.modifiers.alt);
    }

    #[test]
    fn key_down_preserves_repeat_and_prefer_text() {
        let event = KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Named(NamedKey::Enter),
                text: Some("\r".to_string()),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: true,
            prefer_text: true,
        };

        assert!(event.repeat);
        assert!(event.prefer_text);
    }

    #[test]
    fn key_up_is_separate_from_key_down() {
        let keystroke = Keystroke {
            key: Key::Named(NamedKey::Escape),
            text: None,
            modifiers: Modifiers::default(),
            location: KeyLocation::Standard,
        };

        assert_eq!(
            KeyboardEvent::KeyUp(KeyUpEvent {
                keystroke: keystroke.clone()
            }),
            KeyboardEvent::KeyUp(KeyUpEvent { keystroke })
        );
    }

    #[test]
    fn modifiers_changed_preserves_keyboard_modifier_state() {
        let modifiers = Modifiers {
            shift: true,
            control: true,
            alt: false,
            platform: true,
            function: true,
        };
        let event = KeyboardEvent::ModifiersChanged(ModifiersChangedEvent { modifiers });

        assert_eq!(
            event,
            KeyboardEvent::ModifiersChanged(ModifiersChangedEvent { modifiers })
        );
        assert!(modifiers.any());
        assert_eq!(Modifiers::none(), Modifiers::default());
    }

    #[test]
    fn keyboard_primitives_represent_locations_and_extended_keys() {
        assert_eq!(KeyLocation::default(), KeyLocation::Standard);
        assert_eq!(KeyLocation::Numpad, KeyLocation::Numpad);
        assert_eq!(Key::Function(12), Key::Function(12));
        assert_eq!(Key::Dead(Some('\'')), Key::Dead(Some('\'')));
        assert_eq!(
            Key::Modifier(ModifierKey::AltGraph),
            Key::Modifier(ModifierKey::AltGraph)
        );
        assert_eq!(
            Key::Unknown("LaunchApplication3".to_string()),
            Key::Unknown("LaunchApplication3".to_string())
        );
    }

    #[test]
    fn dispatch_result_ignored_is_unhandled() {
        let result = InputDispatchResult::ignored();

        assert!(!result.handled);
        assert!(!result.propagation_stopped);
        assert!(!result.default_prevented);
        assert_eq!(result.consumer, None);
    }

    #[test]
    fn dispatch_result_handled_by_sets_consumer() {
        let result = PointerDispatchResult::handled_by(BACK);

        assert!(result.handled);
        assert_eq!(result.consumer, Some(BACK));
    }

    #[test]
    fn dispatch_result_flags_can_be_set_explicitly() {
        let result = PointerDispatchResult::handled_by(BACK)
            .stop_propagation()
            .prevent_default();

        assert!(result.handled);
        assert!(result.propagation_stopped);
        assert!(result.default_prevented);
        assert_eq!(result.consumer, Some(BACK));
    }

    #[test]
    fn plan_keyboard_dispatch_targets_current_focus() {
        let mut focus_state = FocusState::new();
        focus_state.focus(BACK_FOCUS);
        let event = KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Character("a".to_string()),
                text: Some("a".to_string()),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        });

        let plan = plan_keyboard_dispatch(&focus_state, event.clone());

        assert_eq!(plan.event, event);
        assert_eq!(plan.focused, Some(BACK_FOCUS));
        assert_eq!(plan.target, Some(BACK_FOCUS));
        assert_eq!(plan.kind, KeyboardDispatchKind::KeyDown);
        assert_eq!(plan.propagation, KeyboardPropagation::FocusOnly);
        assert!(plan.is_key_down());
        assert!(plan.targets_focus());
        assert!(plan.is_dispatchable());
    }

    #[test]
    fn plan_keyboard_dispatch_is_untargeted_without_focus() {
        let focus_state = FocusState::new();
        let event = KeyboardEvent::ModifiersChanged(ModifiersChangedEvent {
            modifiers: Modifiers::default(),
        });

        let plan = plan_keyboard_dispatch(&focus_state, event.clone());

        assert_eq!(plan.event, event);
        assert_eq!(plan.focused, None);
        assert_eq!(plan.target, None);
        assert_eq!(plan.kind, KeyboardDispatchKind::ModifiersChanged);
        assert_eq!(plan.propagation, KeyboardPropagation::None);
        assert!(plan.is_modifiers_changed());
        assert!(!plan.targets_focus());
        assert!(!plan.is_dispatchable());
    }

    #[test]
    fn plan_keyboard_dispatch_sets_kind_for_key_up() {
        let mut focus_state = FocusState::new();
        focus_state.focus(BACK_FOCUS);
        let event = KeyboardEvent::KeyUp(KeyUpEvent {
            keystroke: Keystroke {
                key: Key::Named(NamedKey::Escape),
                text: None,
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
        });

        let plan = plan_keyboard_dispatch(&focus_state, event);

        assert_eq!(plan.kind, KeyboardDispatchKind::KeyUp);
        assert!(plan.is_key_up());
    }

    #[test]
    fn keyboard_dispatch_result_ignores_consumer_when_plan_has_no_target() {
        let focus_state = FocusState::new();
        let plan = plan_keyboard_dispatch(
            &focus_state,
            KeyboardEvent::ModifiersChanged(ModifiersChangedEvent {
                modifiers: Modifiers::default(),
            }),
        );

        let result = keyboard_dispatch_result(&plan, Some(BACK_FOCUS));

        assert!(!result.handled);
        assert_eq!(result.consumer, None);
    }

    #[test]
    fn keyboard_dispatch_result_only_accepts_plan_target() {
        let mut focus_state = FocusState::new();
        focus_state.focus(BACK_FOCUS);
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

        let wrong_result = keyboard_dispatch_result(&plan, Some(FRONT_FOCUS));
        let correct_result = keyboard_dispatch_result(&plan, Some(BACK_FOCUS));

        assert!(!wrong_result.handled);
        assert_eq!(wrong_result.consumer, None);
        assert!(correct_result.handled);
        assert_eq!(correct_result.consumer, Some(BACK_FOCUS));
    }

    #[test]
    fn typed_dispatch_results_convert_to_unified_result() {
        let pointer_result: InputDispatchResult = PointerDispatchResult::handled_by(BACK)
            .stop_propagation()
            .into();
        let keyboard_result: InputDispatchResult = KeyboardDispatchResult::handled_by(BACK_FOCUS)
            .prevent_default()
            .into();

        assert_eq!(
            pointer_result.consumer,
            Some(DispatchTarget::HitRegion(BACK))
        );
        assert!(pointer_result.propagation_stopped);
        assert_eq!(
            keyboard_result.consumer,
            Some(DispatchTarget::FocusRegion(BACK_FOCUS))
        );
        assert!(keyboard_result.default_prevented);
    }

    #[test]
    fn typed_dispatch_result_helpers_reflect_consumer_presence() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut pointer_state = PointerState::new();
        let pointer_transition = pointer_state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Down,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );
        let pointer_plan = plan_pointer_dispatch(&frame, &pointer_transition);
        let pointer_result = pointer_dispatch_result(&pointer_plan, pointer_plan.target);

        let mut focus_state = FocusState::new();
        focus_state.focus(BACK_FOCUS);
        let keyboard_plan = plan_keyboard_dispatch(
            &focus_state,
            KeyboardEvent::KeyUp(KeyUpEvent {
                keystroke: Keystroke {
                    key: Key::Named(NamedKey::Escape),
                    text: None,
                    modifiers: Modifiers::default(),
                    location: KeyLocation::Standard,
                },
            }),
        );
        let keyboard_result = keyboard_dispatch_result(&keyboard_plan, keyboard_plan.target);

        assert_eq!(pointer_result.consumer, Some(BACK));
        assert!(pointer_result.handled);
        assert_eq!(keyboard_result.consumer, Some(BACK_FOCUS));
        assert!(keyboard_result.handled);
    }

    #[test]
    fn pointer_listener_registry_stores_handlers_by_region() {
        let mut registry = PointerListenerRegistry::new();
        registry.register(BACK, Arc::new(|_| PointerDispatchResult::handled_by(BACK)));

        let handlers = registry.handlers_for(BACK).unwrap();
        assert_eq!(handlers.len(), 1);
        assert!(registry.handlers_for(FRONT).is_none());
    }

    #[test]
    fn keyboard_listener_registry_stores_handlers_by_focus() {
        let mut registry = KeyboardListenerRegistry::new();
        registry.register(
            BACK_FOCUS,
            Arc::new(|_| KeyboardDispatchResult::handled_by(BACK_FOCUS)),
        );

        let handlers = registry.handlers_for(BACK_FOCUS).unwrap();
        assert_eq!(handlers.len(), 1);
        assert!(registry.handlers_for(FRONT_FOCUS).is_none());
    }

    #[test]
    fn dispatch_pointer_plan_invokes_handlers_in_capture_order() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(FRONT, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        ]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = PointerListenerRegistry::new();
        registry.register(
            BACK,
            Arc::new({
                let events = events.clone();
                move |_| {
                    events.lock().unwrap().push(BACK);
                    PointerDispatchResult::ignored()
                }
            }),
        );
        registry.register(
            FRONT,
            Arc::new({
                let events = events.clone();
                move |_| {
                    events.lock().unwrap().push(FRONT);
                    PointerDispatchResult::ignored()
                }
            }),
        );

        dispatch_pointer_plan(&registry, &plan);

        assert_eq!(&*events.lock().unwrap(), &[BACK, FRONT]);
    }

    #[test]
    fn dispatch_pointer_plan_stops_when_propagation_stops() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(FRONT, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        ]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);
        let mut registry = PointerListenerRegistry::new();
        registry.register(
            BACK,
            Arc::new(|_| PointerDispatchResult::handled_by(BACK).stop_propagation()),
        );
        registry.register(
            FRONT,
            Arc::new(|_| PointerDispatchResult::handled_by(FRONT)),
        );

        let result = dispatch_pointer_plan(&registry, &plan);

        assert!(result.handled);
        assert!(result.propagation_stopped);
        assert_eq!(result.consumer, Some(BACK));
    }

    #[test]
    fn dispatch_pointer_plan_accumulates_default_prevented() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);
        let mut registry = PointerListenerRegistry::new();
        registry.register(
            BACK,
            Arc::new(|_| PointerDispatchResult::handled_by(BACK).prevent_default()),
        );

        let result = dispatch_pointer_plan(&registry, &plan);

        assert!(result.default_prevented);
        assert_eq!(result.consumer, Some(BACK));
    }

    #[test]
    fn dispatch_keyboard_plan_ignores_when_no_focus_target() {
        let registry = KeyboardListenerRegistry::new();
        let focus_state = FocusState::new();
        let plan = plan_keyboard_dispatch(
            &focus_state,
            KeyboardEvent::ModifiersChanged(ModifiersChangedEvent {
                modifiers: Modifiers::default(),
            }),
        );

        let result = dispatch_keyboard_plan(&registry, &plan);

        assert!(!result.handled);
        assert_eq!(result.consumer, None);
    }

    #[test]
    fn dispatch_keyboard_plan_invokes_focused_handlers() {
        let mut registry = KeyboardListenerRegistry::new();
        registry.register(
            BACK_FOCUS,
            Arc::new(|_| KeyboardDispatchResult::handled_by(BACK_FOCUS)),
        );
        let mut focus_state = FocusState::new();
        focus_state.focus(BACK_FOCUS);
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

        let result = dispatch_keyboard_plan(&registry, &plan);

        assert!(result.handled);
        assert_eq!(result.consumer, Some(BACK_FOCUS));
    }

    #[test]
    fn dispatch_keyboard_plan_stops_when_propagation_stops() {
        let mut registry = KeyboardListenerRegistry::new();
        registry.register(
            BACK_FOCUS,
            Arc::new(|_| KeyboardDispatchResult::handled_by(BACK_FOCUS).stop_propagation()),
        );
        registry.register(
            BACK_FOCUS,
            Arc::new(|_| KeyboardDispatchResult::handled_by(FRONT_FOCUS)),
        );
        let mut focus_state = FocusState::new();
        focus_state.focus(BACK_FOCUS);
        let plan = plan_keyboard_dispatch(
            &focus_state,
            KeyboardEvent::KeyUp(KeyUpEvent {
                keystroke: Keystroke {
                    key: Key::Named(NamedKey::Escape),
                    text: None,
                    modifiers: Modifiers::default(),
                    location: KeyLocation::Standard,
                },
            }),
        );

        let result = dispatch_keyboard_plan(&registry, &plan);

        assert!(result.propagation_stopped);
        assert_eq!(result.consumer, Some(BACK_FOCUS));
    }

    #[test]
    fn hit_test_returns_empty_outside_frame() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);

        assert_eq!(hit_test(&frame, Point::new(10.0, 5.0)), HitTest::default());
        assert_eq!(hit_test(&frame, Point::new(-1.0, 5.0)), HitTest::default());
    }

    #[test]
    fn hit_test_returns_region_containing_point() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(1.0, 1.0, 4.0, 4.0),
        )]);

        assert_eq!(
            hit_test(&frame, Point::new(2.0, 2.0)),
            HitTest {
                regions: vec![BACK]
            }
        );
    }

    #[test]
    fn hit_test_respects_region_clip() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0))
                .clipped(Rect::from_xywh(2.0, 2.0, 2.0, 2.0)),
        ]);

        assert_eq!(hit_test(&frame, Point::new(1.0, 1.0)), HitTest::default());
        assert_eq!(
            hit_test(&frame, Point::new(2.0, 2.0)),
            HitTest {
                regions: vec![BACK]
            }
        );
    }

    #[test]
    fn hit_test_ignores_empty_region() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(1.0, 1.0, 0.0, 4.0),
        )]);

        assert_eq!(hit_test(&frame, Point::new(1.0, 1.0)), HitTest::default());
    }

    #[test]
    fn hit_test_uses_right_and_bottom_exclusive_edges() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(1.0, 1.0, 4.0, 4.0),
        )]);

        assert_eq!(
            hit_test(&frame, Point::new(1.0, 1.0)),
            HitTest {
                regions: vec![BACK]
            }
        );
        assert_eq!(hit_test(&frame, Point::new(5.0, 1.0)), HitTest::default());
        assert_eq!(hit_test(&frame, Point::new(1.0, 5.0)), HitTest::default());
    }

    #[test]
    fn higher_layer_region_comes_first() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)).layered(Layer(0)),
            HitRegion::new(FRONT, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)).layered(Layer(5)),
        ]);

        assert_eq!(
            hit_test(&frame, Point::new(1.0, 1.0)),
            HitTest {
                regions: vec![FRONT, BACK],
            }
        );
    }

    #[test]
    fn same_layer_later_region_comes_first() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(FRONT, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        ]);

        assert_eq!(
            hit_test(&frame, Point::new(1.0, 1.0)),
            HitTest {
                regions: vec![FRONT, BACK],
            }
        );
    }

    #[test]
    fn block_pointer_stops_regions_behind_it() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(BLOCKER, Rect::from_xywh(0.0, 0.0, 10.0, 10.0))
                .with_behavior(HitBehavior::BlockPointer),
        ]);

        assert_eq!(
            hit_test(&frame, Point::new(1.0, 1.0)),
            HitTest {
                regions: vec![BLOCKER],
            }
        );
    }

    #[test]
    fn normal_regions_return_full_stack() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(FRONT, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        ]);

        assert_eq!(hit_test(&frame, Point::new(1.0, 1.0)).regions.len(), 2);
    }

    fn pointer_event(kind: PointerEventKind, position: Point) -> PointerEvent {
        PointerEvent {
            kind,
            position,
            button: None,
            modifiers: Modifiers::default(),
        }
    }

    fn pointer_button_event(
        kind: PointerEventKind,
        position: Point,
        button: PointerButton,
    ) -> PointerEvent {
        PointerEvent {
            kind,
            position,
            button: Some(button),
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn pointer_state_starts_empty() {
        let state = PointerState::new();

        assert_eq!(state.hovered(), &HitTest::default());
        assert_eq!(state.captured(), None);
        assert_eq!(state.pressed(), None);
    }

    #[test]
    fn focus_state_reports_focus_transitions() {
        let mut state = FocusState::new();

        assert_eq!(state.focused(), None);
        assert_eq!(
            state.focus(BACK_FOCUS),
            FocusTransition {
                previous: None,
                current: Some(BACK_FOCUS),
            }
        );
        assert_eq!(state.focused(), Some(BACK_FOCUS));
        assert_eq!(
            state.blur(),
            FocusTransition {
                previous: Some(BACK_FOCUS),
                current: None,
            }
        );
    }

    #[test]
    fn focus_test_uses_topmost_focus_region() {
        let frame = frame_with_focus_regions(vec![
            FocusRegion::new(BACK_FOCUS, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            FocusRegion::new(FRONT_FOCUS, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        ]);

        assert_eq!(focus_test(&frame, Point::new(1.0, 1.0)), Some(FRONT_FOCUS));
    }

    #[test]
    fn focus_test_respects_layer_and_clip() {
        let frame = frame_with_focus_regions(vec![
            FocusRegion::new(BACK_FOCUS, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)).layered(Layer(5)),
            FocusRegion::new(FRONT_FOCUS, Rect::from_xywh(0.0, 0.0, 10.0, 10.0))
                .clipped(Rect::from_xywh(0.0, 0.0, 1.0, 1.0)),
        ]);

        assert_eq!(focus_test(&frame, Point::new(0.5, 0.5)), Some(BACK_FOCUS));
        assert_eq!(focus_test(&frame, Point::new(9.0, 9.0)), Some(BACK_FOCUS));
        assert_eq!(focus_test(&frame, Point::new(11.0, 9.0)), None);
    }

    #[test]
    fn move_updates_hovered_regions_and_reports_target() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );

        assert_eq!(state.hovered().regions, vec![BACK]);
        assert_eq!(transition.target, Some(BACK));
        assert_eq!(transition.entered, vec![BACK]);
        assert!(transition.exited.is_empty());
    }

    #[test]
    fn move_reports_exited_regions() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
        )]);
        let mut state = PointerState::new();
        state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(5.0, 5.0)),
        );

        assert!(state.hovered().regions.is_empty());
        assert!(transition.entered.is_empty());
        assert_eq!(transition.exited, vec![BACK]);
    }

    #[test]
    fn leave_clears_hover_and_reports_exited() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut state = PointerState::new();
        state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Leave, Point::new(1.0, 1.0)),
        );

        assert!(state.hovered().regions.is_empty());
        assert_eq!(transition.target, None);
        assert_eq!(transition.exited, vec![BACK]);
    }

    #[test]
    fn down_records_pressed_target() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Down,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );

        let press = PointerPress {
            button: PointerButton::Primary,
            target: Some(BACK),
        };
        assert_eq!(state.pressed(), Some(press));
        assert_eq!(transition.pressed, Some(press));
        assert_eq!(transition.released, None);
    }

    #[test]
    fn up_reports_and_clears_pressed_target() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut state = PointerState::new();
        state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Down,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );
        let transition = state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Up,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );

        assert_eq!(state.pressed(), None);
        assert_eq!(
            transition.released,
            Some(PointerPress {
                button: PointerButton::Primary,
                target: Some(BACK),
            })
        );
    }

    #[test]
    fn capture_routes_target_to_captured_region() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 2.0, 2.0)),
            HitRegion::new(FRONT, Rect::from_xywh(8.0, 8.0, 2.0, 2.0)),
        ]);
        let mut state = PointerState::new();
        state.capture(BACK);
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(9.0, 9.0)),
        );

        assert_eq!(transition.hit_test.regions, vec![FRONT]);
        assert_eq!(transition.target, Some(BACK));
    }

    #[test]
    fn release_capture_restores_hit_test_target() {
        let frame = frame_with_regions(vec![HitRegion::new(
            FRONT,
            Rect::from_xywh(8.0, 8.0, 2.0, 2.0),
        )]);
        let mut state = PointerState::new();
        state.capture(BACK);
        state.release_capture();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(9.0, 9.0)),
        );

        assert_eq!(transition.target, Some(FRONT));
    }

    #[test]
    fn up_auto_releases_capture() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
        )]);
        let mut state = PointerState::new();
        state.capture(BACK);
        state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Up,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );

        assert_eq!(state.captured(), None);
    }

    #[test]
    fn captured_target_survives_pointer_outside_frame() {
        let frame = frame_with_regions(vec![HitRegion::new(
            BACK,
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
        )]);
        let mut state = PointerState::new();
        state.capture(BACK);
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(9.0, 9.0)),
        );

        assert!(transition.hit_test.regions.is_empty());
        assert_eq!(transition.target, Some(BACK));
    }

    #[test]
    fn pointer_dispatch_plan_routes_front_to_back_for_bubble() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(FRONT, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
        ]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);

        assert_eq!(plan.bubble_path, vec![FRONT, BACK]);
        assert_eq!(plan.capture_path, vec![BACK, FRONT]);
    }

    #[test]
    fn pointer_dispatch_plan_preserves_blocked_hit_stack() {
        let frame = frame_with_regions(vec![
            HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0)),
            HitRegion::new(BLOCKER, Rect::from_xywh(0.0, 0.0, 10.0, 10.0))
                .with_behavior(HitBehavior::BlockPointer),
        ]);
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);

        assert_eq!(plan.physical_hit_test.regions, vec![BLOCKER]);
        assert_eq!(plan.bubble_path, vec![BLOCKER]);
        assert_eq!(plan.capture_path, vec![BLOCKER]);
    }

    #[test]
    fn pointer_dispatch_plan_routes_to_captured_region_outside_physical_hit_stack() {
        let frame = frame_with_regions(vec![HitRegion::new(
            FRONT,
            Rect::from_xywh(8.0, 8.0, 2.0, 2.0),
        )]);
        let mut state = PointerState::new();
        state.capture(BACK);
        let transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(9.0, 9.0)),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);

        assert_eq!(plan.physical_hit_test.regions, vec![FRONT]);
        assert_eq!(plan.target, Some(BACK));
        assert_eq!(plan.captured, Some(BACK));
        assert_eq!(plan.bubble_path, vec![BACK, FRONT]);
        assert_eq!(plan.capture_path, vec![FRONT, BACK]);
    }

    #[test]
    fn pointer_dispatch_plan_requests_focus_on_uncaptured_down() {
        let frame = Frame::with_interaction_regions(
            Size::new(10.0, 10.0),
            Scene::new(),
            vec![HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0))],
            vec![FocusRegion::new(
                BACK_FOCUS,
                Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
            )],
        );
        let mut state = PointerState::new();
        let transition = state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Down,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );
        let plan = plan_pointer_dispatch(&frame, &transition);

        assert_eq!(plan.focus_request, Some(BACK_FOCUS));
    }

    #[test]
    fn pointer_dispatch_plan_does_not_request_focus_for_move_or_capture() {
        let frame = Frame::with_interaction_regions(
            Size::new(10.0, 10.0),
            Scene::new(),
            vec![HitRegion::new(BACK, Rect::from_xywh(0.0, 0.0, 10.0, 10.0))],
            vec![FocusRegion::new(
                BACK_FOCUS,
                Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
            )],
        );
        let mut state = PointerState::new();

        let move_transition = state.process(
            &frame,
            pointer_event(PointerEventKind::Move, Point::new(1.0, 1.0)),
        );
        assert_eq!(
            plan_pointer_dispatch(&frame, &move_transition).focus_request,
            None
        );

        state.capture(BACK);
        let down_transition = state.process(
            &frame,
            pointer_button_event(
                PointerEventKind::Down,
                Point::new(1.0, 1.0),
                PointerButton::Primary,
            ),
        );
        assert_eq!(
            plan_pointer_dispatch(&frame, &down_transition).focus_request,
            None
        );
    }
}
