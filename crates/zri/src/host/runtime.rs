use std::collections::HashMap;

use zacor_protocol::daemon_invoke::{InvocationEvent, InvocationMessageLevel};

use crate::function::{
    FunctionEffect, FunctionError, FunctionInvocation, FunctionName, FunctionRouter,
};
use crate::ingress::{DrainOutcome, IngressEvent, InvocationId, ZrIngress};
use crate::input::{
    FocusState, FocusTransition, InputEvent, KeyDownEvent, KeyboardDispatchPlan,
    KeyboardDispatchResult, KeyboardEvent, PointerDispatchPlan, PointerDispatchResult,
    PointerEvent, PointerState, PointerTransition, dispatch_keyboard, dispatch_pointer,
    plan_keyboard_dispatch, plan_pointer_dispatch,
};
use crate::keymap::{ActiveKeymap, BindingChord, Keymap, KeymapResolution, KeymapResolver};
use crate::render::{Frame, Size};
use crate::text::TextSystem;

use super::{
    BufferKind, HostSnapshot, InterfaceHost, PaneId, TextInputResult, apply_keyboard_event,
};

/// How many deferred ingress events one turn applies before yielding back to
/// the platform loop (Neovim breaks its event drain on pending input; Zed
/// budgets foreground task drains at 10ms — a count budget is the simplest
/// testable equivalent).
pub const INGRESS_DRAIN_BUDGET: usize = 64;

pub struct HostRuntime {
    host: InterfaceHost,
    size: Size,
    snapshot: HostSnapshot,
    /// The text kernel handle painting measures through; the runtime owns the
    /// process's one instance and hands it to snapshot builds (the renderer
    /// keeps its own for glyph rasterization).
    text: TextSystem,
    pointer_state: PointerState,
    focus_state: FocusState,
    /// Caller-ordered active keymaps; the first layer that matches or goes
    /// pending decides the resolution.
    keymaps: Vec<(String, Keymap)>,
    resolver: KeymapResolver,
    functions: Option<FunctionRouter>,
    /// Where streamed invocation output lands. Bound by the app when it
    /// starts an invocation; unbound when the stream closes. Scaffolding:
    /// the prompt phase will likely move this routing onto a session object.
    invocation_buffers: HashMap<InvocationId, crate::host::BufferId>,
    dirty: bool,
    redraw_requested: bool,
}

impl HostRuntime {
    pub fn new(host: InterfaceHost, size: Size) -> Self {
        let mut text = TextSystem::new();
        let snapshot = host.build_snapshot(size, &mut text);
        Self {
            host,
            size,
            snapshot,
            text,
            pointer_state: PointerState::new(),
            focus_state: FocusState::new(),
            keymaps: Vec::new(),
            resolver: KeymapResolver::new(),
            functions: None,
            invocation_buffers: HashMap::new(),
            dirty: false,
            redraw_requested: false,
        }
    }

    /// Route a daemon invocation's streamed output into a local buffer.
    /// Routing ends when the stream closes; a vanished buffer is skipped at
    /// apply time (buffered-then-committed with target revalidation).
    pub fn bind_invocation_buffer(&mut self, id: InvocationId, buffer: crate::host::BufferId) {
        self.invocation_buffers.insert(id, buffer);
    }

    /// Replace the active keymap stack (caller-ordered precedence). Any
    /// pending key sequence is dropped: it was typed against the old stack.
    pub fn set_keymaps(&mut self, keymaps: Vec<(String, Keymap)>) {
        self.keymaps = keymaps;
        self.resolver.clear_pending();
    }

    pub fn set_functions(&mut self, functions: FunctionRouter) {
        self.functions = Some(functions);
    }

    /// Cursor blink phase, driven by the app shell's timer. The hidden phase
    /// skips cursor painting; key-down turns and selection changes force the
    /// cursor visible again (Emacs pre-command behavior).
    pub fn set_cursor_blink_visible(&mut self, visible: bool) {
        if self.host.cursor_visible() == visible {
            return;
        }
        self.host.set_cursor_visible(visible);
        self.invalidate();
        self.request_redraw();
    }

    pub fn toggle_cursor_blink(&mut self) {
        self.set_cursor_blink_visible(!self.host.cursor_visible());
    }

    /// Whether a cursor would currently paint (some pane is selected with a
    /// hosted view); the app shell gates its blink timer on this.
    pub fn has_active_cursor(&self) -> bool {
        self.host.selected_view().is_some()
    }

    /// The single transactional pane-selection entry point. Focus state is the
    /// selection authority; the host's active pane is a follower updated here
    /// and in `apply_focus` only, never independently.
    ///
    /// Ordering follows the Helix `Editor::focus` precedent: early-out on
    /// no-op, commit pending state of the outgoing context, swap, then notify
    /// (the returned transition) only after the invariant holds.
    pub fn select_pane(&mut self, id: PaneId) -> Option<FocusTransition> {
        if self.host.active_pane() == Some(id) {
            return None;
        }
        let focus_id = match self.host.pane(id) {
            Some(pane) => pane.focus_id(),
            None => return None,
        };

        // Commit-old hook: when pending edit state exists (future editing
        // transactions), commit it for the outgoing context here, before the
        // swap, so side effects never observe a half-updated selection.

        let transition = focus_id.map(|focus_id| self.focus_state.focus(focus_id));
        self.host.set_active_pane(Some(id));
        // A pending key sequence was typed against the previous focus context
        // and must not resolve in the new one (Zed precedent).
        self.resolver.clear_pending();
        self.host.set_cursor_visible(true);
        self.invalidate();
        self.request_redraw();
        transition
    }

    /// Apply a focus change (from pointer dispatch or programmatic selection)
    /// and sync the derived active pane from the interaction target map.
    fn apply_focus(&mut self, focus_id: crate::input::FocusId) -> FocusTransition {
        let transition = self.focus_state.focus(focus_id);
        if let Some(pane_id) = self.snapshot.targets.pane_for_focus_region(focus_id) {
            self.host.set_active_pane(Some(pane_id));
        }
        self.resolver.clear_pending();
        self.set_cursor_blink_visible(true);
        transition
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

        self.snapshot = self.host.build_snapshot(self.size, &mut self.text);
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

    /// Apply deferred ingress events at the turn boundary, with a budget.
    /// Targets are revalidated at apply time (a vanished buffer means the
    /// event is skipped); when the outcome reports `remaining`, the caller
    /// should `ingress.request_wake()` and yield to the platform loop.
    pub fn drain_ingress(&mut self, ingress: &mut ZrIngress) -> DrainOutcome {
        let host = &mut self.host;
        let invocation_buffers = &mut self.invocation_buffers;
        let mut changed = false;
        let outcome = ingress.drain(INGRESS_DRAIN_BUDGET, |event| match event {
            IngressEvent::BufferAppend { buffer, text } => {
                changed |= host.append_to_buffer(buffer, text);
            }
            IngressEvent::Invocation { id, event } => {
                let Some(buffer) = invocation_buffers.get(&id).copied() else {
                    return;
                };
                if let Some(line) = render_invocation_event(&event) {
                    changed |= host.append_to_buffer(buffer, line);
                }
            }
            IngressEvent::InvocationClosed { id } => {
                invocation_buffers.remove(&id);
            }
            IngressEvent::PlaneError { message } => {
                eprintln!("zri: service plane error: {message}");
            }
        });
        if changed {
            self.invalidate();
            self.request_redraw();
        }
        outcome
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
        let focus = plan
            .focus_request
            .map(|focus_id| self.apply_focus(focus_id));
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

    /// One keyboard turn. Ordering is the contract from the design rules:
    ///
    /// 1. listener dispatch (a `prevent_default` consumes the key outright)
    /// 2. keymap resolution — `Matched` invokes the bound function and
    ///    `Pending` swallows the key; both suppress text insertion
    /// 3. failed sequences replay their swallowed keys: single-chord binding
    ///    lookup first, then literal insertion for text-bearing keys, then
    ///    drop — never silent discard
    ///
    /// Matched functions run at the end of the turn through the configured
    /// `FunctionRouter`; their effects are applied buffered-then-committed
    /// with target revalidation.
    pub fn handle_keyboard(&mut self, event: KeyboardEvent) -> KeyboardTurnResult {
        // Typing holds the cursor solid: reset the blink phase before the
        // snapshot for this turn is (re)built.
        if matches!(event, KeyboardEvent::KeyDown(_)) {
            self.set_cursor_blink_visible(true);
        }
        let snapshot_rebuilt = self.rebuild_snapshot_if_dirty();
        let plan = plan_keyboard_dispatch(&self.focus_state, event.clone());
        let dispatch = dispatch_keyboard(&self.snapshot.listeners, &plan);
        let mut functions = Vec::new();
        let (keymap, text_input) = if dispatch.default_prevented {
            (KeymapResolution::Ignored, TextInputResult::default())
        } else {
            self.keymap_then_text(&event, &mut functions)
        };

        KeyboardTurnResult {
            plan,
            dispatch,
            keymap,
            functions,
            text_input,
            turn: HostTurnResult {
                snapshot_rebuilt,
                redraw_requested: self.redraw_requested,
            },
        }
    }

    fn keymap_then_text(
        &mut self,
        event: &KeyboardEvent,
        invocations: &mut Vec<FunctionInvocation>,
    ) -> (KeymapResolution, TextInputResult) {
        let resolution = {
            let active: Vec<ActiveKeymap<'_>> = self
                .keymaps
                .iter()
                .map(|(name, keymap)| ActiveKeymap {
                    name: name.as_str(),
                    keymap,
                })
                .collect();
            self.resolver.resolve(event, &active)
        };

        let text_input = match &resolution {
            KeymapResolution::Ignored => self.apply_text_event(event),
            KeymapResolution::Pending { .. } => TextInputResult::default(),
            KeymapResolution::Matched { function, .. } => {
                let function = function.clone();
                self.invoke_function(&function, invocations);
                TextInputResult::default()
            }
            KeymapResolution::NotFound { events, .. }
            | KeymapResolution::Cancelled { events, .. } => {
                let events = events.clone();
                self.replay_events(&events, invocations)
            }
        };
        (resolution, text_input)
    }

    fn apply_text_event(&mut self, event: &KeyboardEvent) -> TextInputResult {
        let result = apply_keyboard_event(&mut self.host, event);
        if result.changed {
            self.invalidate();
            self.request_redraw();
        }
        result
    }

    /// Replay swallowed keys after a failed/cancelled sequence (Helix
    /// insert-mode replay order): exact single-chord binding, then literal
    /// insertion for text-bearing keys, then drop. Single-chord lookup never
    /// re-enters pending state.
    fn replay_events(
        &mut self,
        events: &[KeyDownEvent],
        invocations: &mut Vec<FunctionInvocation>,
    ) -> TextInputResult {
        let mut text_input = TextInputResult::default();
        for replay in events {
            if let Some(function) = self.single_chord_binding(replay) {
                self.invoke_function(&function, invocations);
                continue;
            }
            let result = self.apply_text_event(&KeyboardEvent::KeyDown(replay.clone()));
            if result.changed {
                text_input = result;
            }
        }
        text_input
    }

    fn single_chord_binding(&self, event: &KeyDownEvent) -> Option<FunctionName> {
        let chord = BindingChord::from_key_down(event);
        if chord.is_modifier_only() {
            return None;
        }
        self.keymaps
            .iter()
            .find_map(|(_, keymap)| keymap.binding_for_chord(&chord).cloned())
    }

    /// Select the next focusable pane in tree order, wrapping (Emacs
    /// `other-window`). Rides the transactional `select_pane` path, so a
    /// single-pane cycle early-outs as a no-op. Returns whether the
    /// selection changed.
    fn focus_to_next_pane(&mut self) -> bool {
        let order: Vec<PaneId> = self
            .host
            .pane_order()
            .into_iter()
            .filter(|id| self.host.pane(*id).is_some_and(|pane| pane.focusable()))
            .collect();
        if order.is_empty() {
            return false;
        }
        let next = match self
            .host
            .active_pane()
            .and_then(|active| order.iter().position(|id| *id == active))
        {
            Some(index) => order[(index + 1) % order.len()],
            None => order[0],
        };
        self.select_pane(next).is_some()
    }

    /// Invoke a bound function and apply its effects through host mutation
    /// paths (buffered-then-committed; vanished targets are skipped).
    fn invoke_function(&mut self, name: &FunctionName, invocations: &mut Vec<FunctionInvocation>) {
        let result = match &self.functions {
            Some(functions) => functions.invoke(name),
            None => Err(FunctionError::new(format!(
                "no function router configured for: {}",
                name.as_str()
            ))),
        };
        if let Ok(outcome) = &result {
            let mut changed = false;
            for effect in outcome.effects() {
                match effect {
                    FunctionEffect::BufferAppend { buffer, text } => {
                        changed |= self.host.append_to_buffer(*buffer, text);
                    }
                    FunctionEffect::SplitActivePane { axis } => {
                        // Focus stays on the original pane (Emacs split
                        // behavior); no active pane means nothing to split.
                        if let Some(active) = self.host.active_pane() {
                            changed |= self.host.split_pane(active, *axis).is_ok();
                        }
                    }
                    FunctionEffect::FocusNextPane => {
                        changed |= self.focus_to_next_pane();
                    }
                    FunctionEffect::OpenScratchBuffer => {
                        if let Some(active) = self.host.active_pane() {
                            let buffer = self.host.create_buffer(BufferKind::Text, "*scratch*");
                            let view = self.host.create_view(buffer);
                            changed |= self.host.set_pane_view(active, Some(view));
                        }
                    }
                }
            }
            if changed {
                self.invalidate();
                self.request_redraw();
            }
        }
        invocations.push(FunctionInvocation {
            function: name.clone(),
            result,
        });
    }
}

/// Render one streamed invocation event as an output-buffer line. Output
/// records keep their JSON shape (one record per line); progress is elided.
fn render_invocation_event(event: &InvocationEvent) -> Option<String> {
    match event {
        InvocationEvent::Output { record } => Some(format!("{record}\n")),
        InvocationEvent::Progress { .. } => None,
        InvocationEvent::Message { level, text } => {
            let level = match level {
                InvocationMessageLevel::Info => "info",
                InvocationMessageLevel::Warning => "warning",
                InvocationMessageLevel::Error => "error",
            };
            Some(format!("[{level}] {text}\n"))
        }
        InvocationEvent::Done { exit_code, error } => match error {
            Some(error) => Some(format!("[error] {error} (exit {exit_code})\n")),
            None if *exit_code != 0 => Some(format!("[done] exit {exit_code}\n")),
            None => None,
        },
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
    /// How the keymap layer resolved this key (Ignored when listeners
    /// prevented default before the keymap ran).
    pub keymap: KeymapResolution,
    /// Functions invoked this turn (a match, or single-chord replays), with
    /// their outcomes or errors.
    pub functions: Vec<FunctionInvocation>,
    pub text_input: TextInputResult,
    pub turn: HostTurnResult,
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::host::{
        Axis, BufferKind, Pane, PaneContent, PaneId, PaneNode, PaneTree, SplitId, SplitNode,
    };
    use crate::input::{
        Key, KeyDownEvent, KeyLocation, KeyboardDispatchResult, KeyboardEvent, Keystroke,
        Modifiers, PointerDispatchResult,
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
        let left_focus = runtime.host().pane_focus_id(LEFT).unwrap();

        let result = runtime.handle_pointer(pointer_event(
            10.0,
            5.0,
            crate::input::PointerEventKind::Down,
        ));

        assert_eq!(result.plan.focus_request, Some(left_focus));
        assert_eq!(runtime.focus_state().focused(), Some(left_focus));
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
    }

    #[test]
    fn pointer_turn_selects_hosted_view_for_focused_pane() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let left_focus = runtime.host().pane_focus_id(LEFT).unwrap();

        let result = runtime.handle_pointer(pointer_event(
            5.0,
            5.0,
            crate::input::PointerEventKind::Down,
        ));

        assert_eq!(result.plan.focus_request, Some(left_focus));
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
        assert_eq!(runtime.host().selected_view(), Some(view));
    }

    #[test]
    fn select_pane_initializes_focus_and_active_pane() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let left_focus = runtime.host().pane_focus_id(LEFT).unwrap();

        let transition = runtime.select_pane(LEFT);

        assert_eq!(
            transition,
            Some(crate::input::FocusTransition {
                previous: None,
                current: Some(left_focus),
            })
        );
        assert_eq!(runtime.focus_state().focused(), Some(left_focus));
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
        assert!(runtime.is_dirty());
        assert!(runtime.take_redraw_requested());
    }

    #[test]
    fn select_pane_updates_focus_and_derived_view_together() {
        let mut host = InterfaceHost::new(horizontal_tree());
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        host.insert_pane(
            Pane::new(RIGHT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 20.0));
        let right_focus = runtime.host().pane_focus_id(RIGHT).unwrap();
        runtime.select_pane(LEFT);

        runtime.select_pane(RIGHT);

        assert_eq!(runtime.focus_state().focused(), Some(right_focus));
        assert_eq!(runtime.host().active_pane(), Some(RIGHT));
        assert_eq!(runtime.host().selected_view(), Some(view));
    }

    #[test]
    fn select_pane_returns_none_for_unknown_pane_and_noop_reselect() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        let left_focus = runtime.host().pane_focus_id(LEFT).unwrap();
        assert_eq!(runtime.select_pane(PaneId(99)), None);
        assert_eq!(runtime.host().active_pane(), None);

        assert!(runtime.select_pane(LEFT).is_some());
        assert_eq!(runtime.select_pane(LEFT), None);
        assert_eq!(runtime.focus_state().focused(), Some(left_focus));
    }

    #[test]
    fn keyboard_turn_after_select_pane_dispatches_and_inserts_into_same_pane() {
        let mut host = InterfaceHost::new(horizontal_tree());
        let left_buffer = host.create_buffer(BufferKind::Text, "left");
        let left_view = host.create_view(left_buffer);
        let right_buffer = host.create_buffer(BufferKind::Text, "right");
        let right_view = host.create_view(right_buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(left_view),
        );
        host.insert_pane(
            Pane::new(RIGHT, PaneContent::empty())
                .with_focusable(true)
                .with_view(right_view),
        );
        let left_focus = host.pane_focus_id(LEFT).unwrap();
        let right_focus = host.pane_focus_id(RIGHT).unwrap();
        host.pane_mut(LEFT)
            .unwrap()
            .content_mut()
            .on_keyboard(Arc::new(move |_| {
                KeyboardDispatchResult::handled_by(left_focus)
            }));
        host.pane_mut(RIGHT)
            .unwrap()
            .content_mut()
            .on_keyboard(Arc::new(move |_| {
                KeyboardDispatchResult::handled_by(right_focus)
            }));
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 20.0));
        runtime.select_pane(LEFT);
        runtime.select_pane(RIGHT);

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

        // Dispatch target and insertion target must agree: this is the drift
        // regression the single selection authority exists to prevent.
        assert_eq!(result.dispatch.consumer, Some(right_focus));
        assert_eq!(runtime.host().buffer(right_buffer).unwrap().text(), "a");
        assert_eq!(runtime.host().buffer(left_buffer).unwrap().text(), "");
    }

    #[test]
    fn keyboard_turn_applies_text_input_to_selected_text_buffer() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        runtime.select_pane(LEFT);
        runtime.take_redraw_requested();

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

        assert!(result.text_input.changed);
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "a");
        assert!(runtime.is_dirty());
        assert!(runtime.take_redraw_requested());
    }

    #[test]
    fn keyboard_turn_does_not_apply_text_input_when_default_prevented() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let left_focus = host.pane_focus_id(LEFT).unwrap();
        host.pane_mut(LEFT)
            .unwrap()
            .content_mut()
            .on_keyboard(Arc::new(move |_| {
                KeyboardDispatchResult::handled_by(left_focus).prevent_default()
            }));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        runtime.select_pane(LEFT);

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

        assert!(result.dispatch.default_prevented);
        assert!(!result.text_input.changed);
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "");
    }

    #[test]
    fn pointer_turn_dispatches_snapshot_listener() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let events = Arc::new(Mutex::new(Vec::new()));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let left_hit = host.pane_hit_region_id(LEFT).unwrap();
        host.pane_mut(LEFT)
            .unwrap()
            .content_mut()
            .on_pointer(Arc::new({
                let events = events.clone();
                move |_| {
                    events.lock().unwrap().push("pointer");
                    PointerDispatchResult::handled_by(left_hit)
                }
            }));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));

        let result = runtime.handle_pointer(pointer_event(
            5.0,
            5.0,
            crate::input::PointerEventKind::Down,
        ));

        assert_eq!(result.dispatch.consumer, Some(left_hit));
        assert_eq!(events.lock().unwrap().as_slice(), &["pointer"]);
    }

    #[test]
    fn keyboard_turn_dispatches_to_current_focus() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let events = Arc::new(Mutex::new(Vec::new()));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let left_focus = host.pane_focus_id(LEFT).unwrap();
        host.pane_mut(LEFT)
            .unwrap()
            .content_mut()
            .on_keyboard(Arc::new({
                let events = events.clone();
                move |_| {
                    events.lock().unwrap().push("keyboard");
                    KeyboardDispatchResult::handled_by(left_focus)
                }
            }));
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

        assert_eq!(result.plan.target, Some(left_focus));
        assert_eq!(result.dispatch.consumer, Some(left_focus));
        assert_eq!(events.lock().unwrap().as_slice(), &["keyboard"]);
    }

    fn test_ingress() -> ZrIngress {
        ZrIngress::new(Arc::new(|| {}))
    }

    fn char_key(ch: &str) -> KeyboardEvent {
        KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Character(ch.to_string()),
                text: Some(ch.to_string()),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        })
    }

    fn chord(ch: &str) -> BindingChord {
        BindingChord {
            key: Key::Character(ch.to_string()),
            modifiers: Modifiers::default(),
            location: KeyLocation::Standard,
        }
    }

    fn text_runtime() -> (HostRuntime, crate::host::BufferId) {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        runtime.select_pane(LEFT);
        (runtime, buffer)
    }

    fn stamp_router(target: crate::host::BufferId) -> FunctionRouter {
        let mut registry = crate::function::FunctionRegistry::new();
        registry.register_rust(
            FunctionName::new("demo.stamp").unwrap(),
            Arc::new(move |context| context.buf_append(target, "[stamp]")),
        );
        FunctionRouter::new(registry)
    }

    fn goto_keymap() -> Keymap {
        let mut keymap = Keymap::new();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![chord("g"), chord("d")]).unwrap(),
                FunctionName::new("demo.stamp").unwrap(),
            )
            .unwrap();
        keymap
    }

    #[test]
    fn matched_sequence_invokes_function_and_suppresses_insertion() {
        let (mut runtime, buffer) = text_runtime();
        runtime.set_functions(stamp_router(buffer));
        runtime.set_keymaps(vec![("global".into(), goto_keymap())]);

        let pending = runtime.handle_keyboard(char_key("g"));
        let matched = runtime.handle_keyboard(char_key("d"));

        assert!(matches!(pending.keymap, KeymapResolution::Pending { .. }));
        assert!(matches!(matched.keymap, KeymapResolution::Matched { .. }));
        assert_eq!(matched.functions.len(), 1);
        assert!(matched.functions[0].result.is_ok());
        // The bound function's effect landed; the prefix keys never did.
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "[stamp]");
    }

    #[test]
    fn failed_sequence_replays_swallowed_keys_as_text() {
        let (mut runtime, buffer) = text_runtime();
        runtime.set_functions(stamp_router(buffer));
        runtime.set_keymaps(vec![("global".into(), goto_keymap())]);

        let pending = runtime.handle_keyboard(char_key("g"));
        assert!(matches!(pending.keymap, KeymapResolution::Pending { .. }));
        assert_eq!(
            runtime.host().buffer(buffer).unwrap().text(),
            "",
            "prefix keys must not leak into the buffer while pending"
        );

        let failed = runtime.handle_keyboard(char_key("x"));

        assert!(matches!(failed.keymap, KeymapResolution::NotFound { .. }));
        assert!(failed.text_input.changed);
        assert_eq!(
            runtime.host().buffer(buffer).unwrap().text(),
            "gx",
            "both swallowed keys replay into the buffer in order"
        );
    }

    #[test]
    fn replayed_key_with_single_chord_binding_invokes_instead_of_inserting() {
        let (mut runtime, buffer) = text_runtime();
        let mut registry = crate::function::FunctionRegistry::new();
        registry.register_rust(
            FunctionName::new("demo.stamp").unwrap(),
            Arc::new(move |context| context.buf_append(buffer, "[stamp]")),
        );
        registry.register_rust(
            FunctionName::new("demo.exact").unwrap(),
            Arc::new(move |context| context.buf_append(buffer, "[exact]")),
        );
        runtime.set_functions(FunctionRouter::new(registry));
        let mut keymap = goto_keymap();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![chord("x")]).unwrap(),
                FunctionName::new("demo.exact").unwrap(),
            )
            .unwrap();
        runtime.set_keymaps(vec![("global".into(), keymap)]);

        let _ = runtime.handle_keyboard(char_key("g"));
        let failed = runtime.handle_keyboard(char_key("x"));

        // "g x" fails as a sequence; replay re-resolves "x" as its exact
        // single-chord binding while "g" falls through to text.
        assert!(matches!(failed.keymap, KeymapResolution::NotFound { .. }));
        assert_eq!(failed.functions.len(), 1);
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "g[exact]");
    }

    #[test]
    fn select_pane_clears_pending_sequence() {
        let mut host = InterfaceHost::new(horizontal_tree());
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 20.0));
        runtime.select_pane(LEFT);
        runtime.set_functions(stamp_router(buffer));
        runtime.set_keymaps(vec![("global".into(), goto_keymap())]);

        let _ = runtime.handle_keyboard(char_key("g"));
        runtime.select_pane(RIGHT);
        runtime.select_pane(LEFT);
        let after = runtime.handle_keyboard(char_key("d"));

        // The pending "g" died with the focus change: "d" alone is unbound
        // and replays as text instead of completing "g d".
        assert!(matches!(after.keymap, KeymapResolution::NotFound { .. }));
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "d");
    }

    #[test]
    fn listener_prevent_default_beats_keymap() {
        let (mut runtime, buffer) = text_runtime();
        runtime.set_functions(stamp_router(buffer));
        let mut keymap = Keymap::new();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![chord("g")]).unwrap(),
                FunctionName::new("demo.stamp").unwrap(),
            )
            .unwrap();
        runtime.set_keymaps(vec![("global".into(), keymap)]);
        let left_focus = runtime.host().pane_focus_id(LEFT).unwrap();
        runtime.update_host(|host| {
            host.pane_mut(LEFT)
                .unwrap()
                .content_mut()
                .on_keyboard(Arc::new(move |_| {
                    KeyboardDispatchResult::handled_by(left_focus).prevent_default()
                }));
        });

        let result = runtime.handle_keyboard(char_key("g"));

        assert!(result.dispatch.default_prevented);
        assert_eq!(result.keymap, KeymapResolution::Ignored);
        assert!(result.functions.is_empty());
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "");
    }

    #[test]
    fn matched_function_error_is_reported_in_turn_result() {
        let (mut runtime, buffer) = text_runtime();
        // No router configured at all.
        let mut keymap = Keymap::new();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![chord("g")]).unwrap(),
                FunctionName::new("demo.stamp").unwrap(),
            )
            .unwrap();
        runtime.set_keymaps(vec![("global".into(), keymap)]);

        let result = runtime.handle_keyboard(char_key("g"));

        assert_eq!(result.functions.len(), 1);
        assert!(result.functions[0].result.is_err());
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "");
    }

    #[test]
    fn lua_functions_route_through_delegate_and_apply_effects() {
        let (mut runtime, buffer) = text_runtime();
        let lua = crate::lua::LuaHost::new().unwrap();
        lua.load_chunk(
            "config.lua",
            &format!(
                r#"
                    zri.register_function("user.stamp", function(ctx)
                        ctx.buf_append({}, "[lua]")
                    end)
                "#,
                buffer.0
            ),
        )
        .unwrap();
        runtime.set_functions(
            FunctionRouter::new(crate::function::FunctionRegistry::new()).with_delegate(lua),
        );
        let mut keymap = Keymap::new();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![chord("g")]).unwrap(),
                FunctionName::new("user.stamp").unwrap(),
            )
            .unwrap();
        runtime.set_keymaps(vec![("global".into(), keymap)]);

        let result = runtime.handle_keyboard(char_key("g"));

        assert!(result.functions[0].result.is_ok());
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "[lua]");
    }

    #[test]
    fn drain_ingress_appends_to_buffer_and_invalidates() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Log, "log");
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let mut ingress = test_ingress();
        let sender = ingress.sender();
        sender.send(IngressEvent::BufferAppend {
            buffer,
            text: "line\n".into(),
        });

        let outcome = runtime.drain_ingress(&mut ingress);

        assert_eq!(outcome.applied, 1);
        assert!(!outcome.remaining);
        assert_eq!(runtime.host().buffer(buffer).unwrap().text(), "line\n");
        assert!(runtime.is_dirty());
        assert!(runtime.take_redraw_requested());
    }

    #[test]
    fn drain_ingress_revalidates_missing_buffer_target() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let mut ingress = test_ingress();
        ingress.sender().send(IngressEvent::BufferAppend {
            buffer: crate::host::BufferId(999),
            text: "lost".into(),
        });

        let outcome = runtime.drain_ingress(&mut ingress);

        // The event is consumed (it counts against the budget) but a vanished
        // target is skipped without invalidating anything.
        assert_eq!(outcome.applied, 1);
        assert!(!runtime.is_dirty());
        assert!(!runtime.take_redraw_requested());
    }

    #[test]
    fn drain_ingress_routes_invocation_events_to_bound_buffer() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Output, "*output*");
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let mut ingress = test_ingress();
        let sender = ingress.sender();
        let id = InvocationId(1);
        runtime.bind_invocation_buffer(id, buffer);

        sender.send(IngressEvent::Invocation {
            id,
            event: InvocationEvent::Output {
                record: serde_json::json!({"value": "hello"}),
            },
        });
        sender.send(IngressEvent::Invocation {
            id,
            event: InvocationEvent::Message {
                level: InvocationMessageLevel::Warning,
                text: "careful".into(),
            },
        });
        sender.send(IngressEvent::InvocationClosed { id });
        // After the close, further events for this id are unrouted and skipped.
        sender.send(IngressEvent::Invocation {
            id,
            event: InvocationEvent::Output {
                record: serde_json::json!({"value": "late"}),
            },
        });
        // Events for an unbound id never land anywhere.
        sender.send(IngressEvent::Invocation {
            id: InvocationId(99),
            event: InvocationEvent::Output {
                record: serde_json::json!({"value": "stray"}),
            },
        });

        runtime.drain_ingress(&mut ingress);

        assert_eq!(
            runtime.host().buffer(buffer).unwrap().text(),
            "{\"value\":\"hello\"}\n[warning] careful\n"
        );
        assert!(runtime.is_dirty());
    }

    #[test]
    fn drain_ingress_budget_yields_between_batches_while_input_stays_live() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Log, "log");
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(20.0, 10.0));
        let mut ingress = test_ingress();
        let sender = ingress.sender();
        let total = INGRESS_DRAIN_BUDGET * 3 + 7;
        for _ in 0..total {
            sender.send(IngressEvent::BufferAppend {
                buffer,
                text: "x".into(),
            });
        }

        let mut drains = 0;
        let mut pointer_turns = 0;
        loop {
            let outcome = runtime.drain_ingress(&mut ingress);
            drains += 1;
            assert!(outcome.applied <= INGRESS_DRAIN_BUDGET);
            // Input is processed between drain batches, never starved by the
            // backlog.
            let result = runtime.handle_pointer(pointer_event(
                5.0,
                5.0,
                crate::input::PointerEventKind::Move,
            ));
            assert!(result.turn.snapshot_rebuilt || pointer_turns > 0);
            pointer_turns += 1;
            if !outcome.remaining {
                break;
            }
        }

        assert_eq!(drains, 4);
        assert_eq!(pointer_turns, drains);
        assert_eq!(runtime.host().buffer(buffer).unwrap().text().len(), total);
    }

    fn cursor_fill(frame: &Frame) -> Option<crate::render::Rect> {
        frame
            .scene
            .items()
            .iter()
            .find_map(|item| match item.primitive {
                crate::render::Primitive::FillRect { rect, .. } => Some(rect),
                _ => None,
            })
    }

    fn runtime_with_text_view() -> HostRuntime {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(200.0, 40.0));
        runtime.select_pane(LEFT);
        runtime.rebuild_snapshot_if_dirty();
        runtime
    }

    fn key_a() -> KeyboardEvent {
        KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Character("a".to_string()),
                text: Some("a".to_string()),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        })
    }

    #[test]
    fn typing_advances_painted_cursor() {
        let mut runtime = runtime_with_text_view();
        let before = cursor_fill(runtime.frame()).expect("cursor paints for the selected view");

        runtime.handle_keyboard(key_a());
        runtime.rebuild_snapshot_if_dirty();

        let after = cursor_fill(runtime.frame()).expect("cursor still paints after typing");
        assert!(after.origin.x > before.origin.x);
    }

    #[test]
    fn keyboard_turn_resets_cursor_blink_phase() {
        let mut runtime = runtime_with_text_view();
        runtime.set_cursor_blink_visible(false);
        runtime.rebuild_snapshot_if_dirty();
        assert!(cursor_fill(runtime.frame()).is_none());

        runtime.handle_keyboard(key_a());
        runtime.rebuild_snapshot_if_dirty();

        assert!(cursor_fill(runtime.frame()).is_some());
    }

    /// Control chords arrive from the native adapter with produced text
    /// suppressed, so the test event mirrors that: no text.
    fn ctrl_char_key(ch: &str) -> KeyboardEvent {
        KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Character(ch.to_string()),
                text: None,
                modifiers: Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        })
    }

    fn verb_router() -> FunctionRouter {
        let mut registry = crate::function::FunctionRegistry::new();
        registry.register_rust(
            FunctionName::new("pane.split-below").unwrap(),
            Arc::new(|context| context.split_active_pane(crate::host::Axis::Vertical)),
        );
        registry.register_rust(
            FunctionName::new("pane.other").unwrap(),
            Arc::new(|context| context.focus_next_pane()),
        );
        registry.register_rust(
            FunctionName::new("buffer.new").unwrap(),
            Arc::new(|context| context.open_scratch_buffer()),
        );
        FunctionRouter::new(registry)
    }

    fn verbs_keymap() -> Keymap {
        let ctrl_x = BindingChord {
            key: Key::Character("x".to_string()),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
            location: KeyLocation::Standard,
        };
        let mut keymap = Keymap::new();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![ctrl_x.clone(), chord("2")]).unwrap(),
                FunctionName::new("pane.split-below").unwrap(),
            )
            .unwrap();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![ctrl_x.clone(), chord("o")]).unwrap(),
                FunctionName::new("pane.other").unwrap(),
            )
            .unwrap();
        keymap
            .bind(
                crate::keymap::KeySequence::new(vec![ctrl_x, chord("b")]).unwrap(),
                FunctionName::new("buffer.new").unwrap(),
            )
            .unwrap();
        keymap
    }

    fn verbs_runtime() -> (HostRuntime, crate::host::BufferId) {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello");
        let view = host.create_view(buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(200.0, 80.0));
        runtime.select_pane(LEFT);
        runtime.set_functions(verb_router());
        runtime.set_keymaps(vec![("global".into(), verbs_keymap())]);
        (runtime, buffer)
    }

    #[test]
    fn ctrl_x_2_splits_active_pane_and_keeps_focus() {
        let (mut runtime, _) = verbs_runtime();

        let pending = runtime.handle_keyboard(ctrl_char_key("x"));
        assert!(matches!(pending.keymap, KeymapResolution::Pending { .. }));
        let matched = runtime.handle_keyboard(char_key("2"));

        assert!(matches!(matched.keymap, KeymapResolution::Matched { .. }));
        assert!(matched.functions[0].result.is_ok());
        assert_eq!(runtime.host().pane_order().len(), 2);
        // Emacs split behavior: focus stays on the original pane.
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
        assert!(runtime.is_dirty());
    }

    #[test]
    fn ctrl_x_o_cycles_focus_to_next_pane_and_wraps() {
        let (mut runtime, _) = verbs_runtime();
        runtime.handle_keyboard(ctrl_char_key("x"));
        runtime.handle_keyboard(char_key("2"));
        let new_pane = runtime.host().pane_order()[1];
        let new_focus = runtime.host().pane_focus_id(new_pane).unwrap();

        runtime.handle_keyboard(ctrl_char_key("x"));
        runtime.handle_keyboard(char_key("o"));
        assert_eq!(runtime.host().active_pane(), Some(new_pane));
        assert_eq!(runtime.focus_state().focused(), Some(new_focus));

        runtime.handle_keyboard(ctrl_char_key("x"));
        runtime.handle_keyboard(char_key("o"));
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
    }

    #[test]
    fn other_pane_with_single_pane_is_noop() {
        let (mut runtime, _) = verbs_runtime();

        runtime.handle_keyboard(ctrl_char_key("x"));
        let result = runtime.handle_keyboard(char_key("o"));

        assert!(result.functions[0].result.is_ok());
        assert_eq!(runtime.host().active_pane(), Some(LEFT));
    }

    #[test]
    fn ctrl_x_b_replaces_view_and_receives_typing() {
        let (mut runtime, original) = verbs_runtime();

        runtime.handle_keyboard(ctrl_char_key("x"));
        runtime.handle_keyboard(char_key("b"));

        let view = runtime.host().selected_view().unwrap();
        let new_buffer = runtime.host().view(view).unwrap().buffer();
        assert_ne!(new_buffer, original);

        runtime.handle_keyboard(char_key("a"));
        assert_eq!(runtime.host().buffer(new_buffer).unwrap().text(), "a");
        assert_eq!(runtime.host().buffer(original).unwrap().text(), "hello");
    }

    #[test]
    fn editing_one_view_leaves_sibling_view_cursor_stale_but_safe() {
        let (mut runtime, buffer) = verbs_runtime();
        runtime.handle_keyboard(ctrl_char_key("x"));
        runtime.handle_keyboard(char_key("2"));
        let sibling = runtime.host().pane_order()[1];
        let sibling_view = runtime.host().pane(sibling).unwrap().view().unwrap();
        let before = runtime.host().view(sibling_view).unwrap().cursor();

        runtime.handle_keyboard(char_key("z"));

        // The shared buffer changed through the focused view; the sibling
        // view's cursor is deliberately not synchronized (the watched
        // rope/transactions trigger) and painting stays safe.
        assert!(
            runtime
                .host()
                .buffer(buffer)
                .unwrap()
                .text()
                .starts_with('z')
        );
        assert_eq!(runtime.host().view(sibling_view).unwrap().cursor(), before);
        runtime.rebuild_snapshot_if_dirty();
        assert!(!runtime.frame().scene.items().is_empty());
    }

    #[test]
    fn split_effect_without_active_pane_is_skipped() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let mut runtime = HostRuntime::new(host, Size::new(100.0, 50.0));
        runtime.set_functions(verb_router());
        runtime.set_keymaps(vec![("global".into(), verbs_keymap())]);

        runtime.handle_keyboard(ctrl_char_key("x"));
        let result = runtime.handle_keyboard(char_key("2"));

        // The function ran, but the effect found no active pane and skipped.
        assert!(result.functions[0].result.is_ok());
        assert_eq!(runtime.host().pane_order().len(), 1);
    }

    #[test]
    fn select_pane_resets_cursor_blink_phase() {
        let mut host = InterfaceHost::new(horizontal_tree());
        let left_buffer = host.create_buffer(BufferKind::Text, "left");
        let left_view = host.create_view(left_buffer);
        let right_buffer = host.create_buffer(BufferKind::Text, "right");
        let right_view = host.create_view(right_buffer);
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_focusable(true)
                .with_view(left_view),
        );
        host.insert_pane(
            Pane::new(RIGHT, PaneContent::empty())
                .with_focusable(true)
                .with_view(right_view),
        );
        let mut runtime = HostRuntime::new(host, Size::new(200.0, 40.0));
        runtime.select_pane(LEFT);
        runtime.set_cursor_blink_visible(false);
        runtime.rebuild_snapshot_if_dirty();
        assert!(cursor_fill(runtime.frame()).is_none());

        runtime.select_pane(RIGHT);
        runtime.rebuild_snapshot_if_dirty();

        assert!(cursor_fill(runtime.frame()).is_some());
    }
}
