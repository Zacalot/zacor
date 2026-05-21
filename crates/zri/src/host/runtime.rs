use crate::input::{
    FocusState, FocusTransition, InputEvent, KeyboardDispatchPlan, KeyboardDispatchResult,
    KeyboardEvent, PointerDispatchPlan, PointerDispatchResult, PointerEvent, PointerState,
    PointerTransition, dispatch_keyboard, dispatch_pointer, plan_keyboard_dispatch,
    plan_pointer_dispatch,
};
use crate::render::{Frame, Size};

use super::{HostSnapshot, InterfaceHost};

pub struct HostRuntime {
    host: InterfaceHost,
    size: Size,
    snapshot: HostSnapshot,
    pointer_state: PointerState,
    focus_state: FocusState,
    dirty: bool,
    redraw_requested: bool,
}

impl HostRuntime {
    pub fn new(host: InterfaceHost, size: Size) -> Self {
        let snapshot = host.build_snapshot(size);
        Self {
            host,
            size,
            snapshot,
            pointer_state: PointerState::new(),
            focus_state: FocusState::new(),
            dirty: false,
            redraw_requested: false,
        }
    }

    pub fn host(&self) -> &InterfaceHost {
        &self.host
    }

    pub fn update_host<R>(&mut self, update: impl FnOnce(&mut InterfaceHost) -> R) -> R {
        let result = update(&mut self.host);
        self.invalidate();
        self.request_redraw();
        result
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn snapshot(&self) -> &HostSnapshot {
        &self.snapshot
    }

    pub fn frame(&self) -> &Frame {
        &self.snapshot.frame
    }

    pub fn pointer_state(&self) -> &PointerState {
        &self.pointer_state
    }

    pub fn focus_state(&self) -> &FocusState {
        &self.focus_state
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub fn take_redraw_requested(&mut self) -> bool {
        std::mem::take(&mut self.redraw_requested)
    }

    pub fn rebuild_snapshot_if_dirty(&mut self) -> bool {
        if !self.dirty {
            return false;
        }

        self.snapshot = self.host.build_snapshot(self.size);
        self.dirty = false;
        true
    }

    pub fn resize(&mut self, size: Size) -> HostTurnResult {
        if self.size == size {
            return HostTurnResult::default();
        }

        self.size = size;
        self.invalidate();
        self.request_redraw();
        HostTurnResult {
            snapshot_rebuilt: false,
            redraw_requested: true,
        }
    }

    pub fn handle_input(&mut self, event: InputEvent) -> InputTurnResult {
        match event {
            InputEvent::Pointer(event) => InputTurnResult::Pointer(self.handle_pointer(event)),
            InputEvent::Keyboard(event) => InputTurnResult::Keyboard(self.handle_keyboard(event)),
        }
    }

    pub fn handle_pointer(&mut self, event: PointerEvent) -> PointerTurnResult {
        let snapshot_rebuilt = self.rebuild_snapshot_if_dirty();
        let transition = self.pointer_state.process(&self.snapshot.frame, event);
        let plan = plan_pointer_dispatch(&self.snapshot.frame, &transition);
        let focus = plan.focus_request.map(|focus_id| {
            let focus_transition = self.focus_state.focus(focus_id);
            if let Some(pane_id) = self.snapshot.targets.pane_for_focus_region(focus_id) {
                self.host.select_pane(pane_id);
            }
            focus_transition
        });
        let dispatch = dispatch_pointer(&self.snapshot.listeners, &plan);

        PointerTurnResult {
            transition,
            plan,
            dispatch,
            focus,
            turn: HostTurnResult {
                snapshot_rebuilt,
                redraw_requested: self.redraw_requested,
            },
        }
    }

    pub fn handle_keyboard(&mut self, event: KeyboardEvent) -> KeyboardTurnResult {
        let snapshot_rebuilt = self.rebuild_snapshot_if_dirty();
        let plan = plan_keyboard_dispatch(&self.focus_state, event);
        let dispatch = dispatch_keyboard(&self.snapshot.listeners, &plan);

        KeyboardTurnResult {
            plan,
            dispatch,
            turn: HostTurnResult {
                snapshot_rebuilt,
                redraw_requested: self.redraw_requested,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostTurnResult {
    pub snapshot_rebuilt: bool,
    pub redraw_requested: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputTurnResult {
    Pointer(PointerTurnResult),
    Keyboard(KeyboardTurnResult),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointerTurnResult {
    pub transition: PointerTransition,
    pub plan: PointerDispatchPlan,
    pub dispatch: PointerDispatchResult,
    pub focus: Option<FocusTransition>,
    pub turn: HostTurnResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyboardTurnResult {
    pub plan: KeyboardDispatchPlan,
    pub dispatch: KeyboardDispatchResult,
    pub turn: HostTurnResult,
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::host::{Axis, Pane, PaneContent, PaneId, PaneNode, PaneTree, SplitId, SplitNode};
    use crate::input::{
        FocusId, HitRegionId, Key, KeyDownEvent, KeyLocation, KeyboardDispatchResult,
        KeyboardEvent, Keystroke, Modifiers, PointerDispatchResult,
    };

    use super::*;

    const LEFT: PaneId = PaneId(1);
    const RIGHT: PaneId = PaneId(2);

    fn horizontal_tree() -> PaneTree {
        PaneTree::new(PaneNode::split(
            SplitNode::new(
                SplitId(1),
                Axis::Horizontal,
                vec![PaneNode::pane(LEFT), PaneNode::pane(RIGHT)],
            )
            .unwrap(),
        ))
    }

    fn pointer_event(x: f32, y: f32, kind: crate::input::PointerEventKind) -> PointerEvent {
        PointerEvent {
            kind,
            position: crate::render::Point::new(x, y),
            button: Some(crate::input::PointerButton::Primary),
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn runtime_builds_initial_snapshot() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));

        let runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        assert_eq!(runtime.frame().size, Size::new(20.0, 10.0));
        assert!(!runtime.is_dirty());
    }

    #[test]
    fn invalidate_rebuilds_snapshot_once() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        runtime.invalidate();

        assert!(runtime.rebuild_snapshot_if_dirty());
        assert!(!runtime.rebuild_snapshot_if_dirty());
    }

    #[test]
    fn update_host_invalidates_and_requests_redraw() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        runtime.update_host(|host| {
            host.insert_pane(Pane::new(RIGHT, PaneContent::empty()));
        });

        assert!(runtime.is_dirty());
        assert!(runtime.take_redraw_requested());
    }

    #[test]
    fn resize_invalidates_and_requests_redraw() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        let result = runtime.resize(Size::new(30.0, 15.0));

        assert_eq!(
            result,
            HostTurnResult {
                snapshot_rebuilt: false,
                redraw_requested: true,
            }
        );
        assert!(runtime.is_dirty());
        assert!(runtime.take_redraw_requested());
    }

    #[test]
    fn pointer_turn_updates_focus_and_active_pane() {
        let mut host = InterfaceHost::new(horizontal_tree());
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 20.0));

        let result = runtime.handle_pointer(pointer_event(
            10.0,
            5.0,
            crate::input::PointerEventKind::Down,
        ));

        assert_eq!(result.plan.focus_request, Some(FocusId(LEFT.0)));
        assert_eq!(runtime.focus_state().focused(), Some(FocusId(LEFT.0)));
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
    }

    #[test]
    fn pointer_turn_dispatches_snapshot_listener() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut content = PaneContent::empty();
        content.on_pointer(Arc::new({
            let events = events.clone();
            move |_| {
                events.lock().unwrap().push("pointer");
                PointerDispatchResult::handled_by(HitRegionId(LEFT.0))
            }
        }));
        host.insert_pane(Pane::new(LEFT, content));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        let result = runtime.handle_pointer(pointer_event(
            5.0,
            5.0,
            crate::input::PointerEventKind::Down,
        ));

        assert_eq!(result.dispatch.consumer, Some(HitRegionId(LEFT.0)));
        assert_eq!(events.lock().unwrap().as_slice(), &["pointer"]);
    }

    #[test]
    fn keyboard_turn_dispatches_to_current_focus() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut content = PaneContent::empty();
        content.on_keyboard(Arc::new({
            let events = events.clone();
            move |_| {
                events.lock().unwrap().push("keyboard");
                KeyboardDispatchResult::handled_by(FocusId(LEFT.0))
            }
        }));
        host.insert_pane(Pane::new(LEFT, content).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let _ = runtime.handle_pointer(pointer_event(
            5.0,
            5.0,
            crate::input::PointerEventKind::Down,
        ));

        let result = runtime.handle_keyboard(KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Character("a".to_string()),
                text: Some("a".to_string()),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        }));

        assert_eq!(result.plan.target, Some(FocusId(LEFT.0)));
        assert_eq!(result.dispatch.consumer, Some(FocusId(LEFT.0)));
        assert_eq!(events.lock().unwrap().as_slice(), &["keyboard"]);
    }
}
