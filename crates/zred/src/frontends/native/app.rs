use super::compositor::{Compositor, WindowCompositorState};
use super::input;
use super::scene::{self, SceneHotspotAction};
use super::window::{WindowBinding, WindowRegistry};
use crate::kernel::{PaneId, WorkspaceId};
use crate::shell::App;
use crate::session::SessionFrontendEffect;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{WindowAttributes, WindowId};

pub fn run() -> Result<()> {
    let event_loop = EventLoop::new().context("failed to create native event loop")?;
    let mut app = NativeApp::new()?;
    event_loop
        .run_app(&mut app)
        .context("native app loop failed")
}

struct NativeWindow {
    state: WindowCompositorState,
    cursor_position: Option<PhysicalPosition<f64>>,
    app: App,
}

struct NativeApp {
    compositor: Compositor,
    windows: WindowRegistry,
    window_states: BTreeMap<WindowId, NativeWindow>,
    modifiers: ModifiersState,
}

static NEXT_NATIVE_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

fn allocate_native_workspace_id() -> WorkspaceId {
    WorkspaceId::new(NEXT_NATIVE_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed))
}

fn native_window_title(workspace_id: WorkspaceId) -> String {
    format!("zred [{}]", workspace_id.raw())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeFrontendAction {
    CreateWindow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionFrontendEffect;

    #[test]
    fn native_workspace_ids_are_unique() {
        let first = allocate_native_workspace_id();
        let second = allocate_native_workspace_id();

        assert_ne!(first, second);
        assert!(second.raw() > first.raw());
    }

    #[test]
    fn native_window_title_includes_workspace_id() {
        assert_eq!(native_window_title(WorkspaceId::new(42)), "zred [42]");
    }

    #[test]
    fn native_frontend_actions_expand_new_window_effects() {
        let actions = native_frontend_actions([
            SessionFrontendEffect::NewWindow,
            SessionFrontendEffect::NewWindow,
        ]);

        assert_eq!(
            actions,
            vec![
                NativeFrontendAction::CreateWindow,
                NativeFrontendAction::CreateWindow,
            ]
        );
    }

    #[test]
    fn redraw_targets_focused_window_when_available() {
        let mut windows = WindowRegistry::new();
        let first = WindowId::dummy();

        windows.bind_workspace(first, WorkspaceId::new(1));
        windows.set_focused_window(first);

        assert_eq!(redraw_targets(&windows), vec![first]);
    }

    #[test]
    fn redraw_targets_all_windows_when_none_are_focused() {
        let mut windows = WindowRegistry::new();
        let first = WindowId::dummy();

        windows.bind_workspace(first, WorkspaceId::new(1));

        assert_eq!(redraw_targets(&windows), vec![first]);
    }
}

fn native_frontend_actions(
    effects: impl IntoIterator<Item = SessionFrontendEffect>,
) -> Vec<NativeFrontendAction> {
    effects
        .into_iter()
        .map(|effect| match effect {
            SessionFrontendEffect::NewWindow => NativeFrontendAction::CreateWindow,
        })
        .collect()
}

fn redraw_targets(windows: &WindowRegistry) -> Vec<WindowId> {
    if let Some(window_id) = windows.focused_window() {
        return vec![window_id];
    }

    windows.window_ids().collect()
}

impl NativeApp {
    fn new() -> Result<Self> {
        Ok(Self {
            compositor: Compositor::new()?,
            windows: WindowRegistry::new(),
            window_states: BTreeMap::new(),
            modifiers: ModifiersState::default(),
        })
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let workspace_id = allocate_native_workspace_id();
        let app = App::with_workspace_id(workspace_id)
            .expect("native app state should initialize");
        let workspace_id = app.workspace_id();
        let attributes = WindowAttributes::default()
            .with_title(native_window_title(workspace_id))
            .with_inner_size(PhysicalSize::new(1280, 800));

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("native window creation should succeed"),
        );
        let window_id = window.id();
        let state = self
            .compositor
            .create_window_state(window)
            .expect("window compositor state should initialize");

        self.windows.insert(window_id, WindowBinding { workspace_id });
        self.window_states.insert(
            window_id,
            NativeWindow {
                state,
                cursor_position: None,
                app,
            },
        );
        self.request_redraw(window_id);
    }

    fn sync_window_binding(&mut self, window_id: WindowId) {
        let Some(window) = self.window_states.get_mut(&window_id) else {
            return;
        };

        let mut workspace_id = window.app.workspace_id();
        if let Some(existing_window_id) = self.windows.window_for_workspace(workspace_id)
            && existing_window_id != window_id
        {
            workspace_id = allocate_native_workspace_id();
            window.app.set_workspace_id(workspace_id);
        }

        self.windows.bind_workspace(window_id, workspace_id);
        window
            .state
            .window()
            .set_title(&native_window_title(workspace_id));
    }

    fn request_redraw(&self, window_id: WindowId) {
        if let Some(window) = self.window_states.get(&window_id) {
            window.state.window().request_redraw();
        }
    }

    fn drain_frontend_actions(&mut self, window_id: WindowId) -> Vec<NativeFrontendAction> {
        let Some(window) = self.window_states.get_mut(&window_id) else {
            return Vec::new();
        };

        native_frontend_actions(window.app.drain_frontend_effects())
    }

    fn apply_frontend_actions(
        &mut self,
        event_loop: &ActiveEventLoop,
        actions: impl IntoIterator<Item = NativeFrontendAction>,
    ) {
        for action in actions {
            match action {
                NativeFrontendAction::CreateWindow => self.create_window(event_loop),
            }
        }
    }

    fn drain_pending_frontend_actions(&mut self, event_loop: &ActiveEventLoop) {
        let window_ids: Vec<WindowId> = self.window_states.keys().copied().collect();
        let mut actions = Vec::new();

        for window_id in window_ids {
            actions.extend(self.drain_frontend_actions(window_id));
        }

        self.apply_frontend_actions(event_loop, actions);
    }

    fn handle_native_keyboard_shortcut(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: &winit::event::KeyEvent,
    ) -> bool {
        if event.state != ElementState::Pressed || event.repeat {
            return false;
        }

        let Some(window) = self.window_states.get_mut(&window_id) else {
            return false;
        };

        let handled = match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => window.app.adjust_active_pane_viewport(0, -1),
            Key::Named(NamedKey::ArrowDown) => window.app.adjust_active_pane_viewport(0, 1),
            Key::Named(NamedKey::ArrowLeft) => window.app.adjust_active_pane_viewport(-1, 0),
            Key::Named(NamedKey::ArrowRight) => window.app.adjust_active_pane_viewport(1, 0),
            Key::Named(NamedKey::PageUp) => window.app.adjust_active_pane_viewport(0, -8),
            Key::Named(NamedKey::PageDown) => window.app.adjust_active_pane_viewport(0, 8),
            Key::Character(text)
                if self.modifiers.control_key()
                    && self.modifiers.shift_key()
                    && text.eq_ignore_ascii_case("n") =>
            {
                window.app.run_command("window.new");
                self.sync_window_binding(window_id);
                let actions = self.drain_frontend_actions(window_id);
                self.apply_frontend_actions(event_loop, actions);
                true
            }
            _ => false,
        };

        if handled {
            self.request_redraw(window_id);
        }
        handled
    }

    fn handle_click(&mut self, window_id: WindowId) {
        let click_target = {
            let Some(window) = self.window_states.get(&window_id) else {
                return;
            };
            let view = window.app.view();
            let cursor = window.cursor_position;
            let scene = scene::build(&view, window.state.size());
            cursor.map(|position| {
                (
                    scene::hit_test_hotspot(&scene, position.x, position.y),
                    scene::hit_test_pane(&scene, position.x, position.y),
                )
            })
        };

        let Some(window) = self.window_states.get_mut(&window_id) else {
            return;
        };

        if let Some((hotspot, pane_id)) = click_target {
            if let Some(action) = hotspot {
                match action {
                    SceneHotspotAction::SelectRecordRow { pane_id, row, open } => {
                        let _ = window.app.focus_pane(PaneId::new(pane_id));
                        let _ = window.app.select_record_row_in_active_pane(row);
                        if open {
                            window.app.open_active_structured_selection();
                        }
                    }
                    SceneHotspotAction::SelectTreeNode {
                        pane_id,
                        node_id,
                        open,
                    } => {
                        let _ = window.app.focus_pane(PaneId::new(pane_id));
                        let _ = window.app.select_tree_node_in_active_pane(&node_id);
                        if open {
                            window.app.open_active_structured_selection();
                        }
                    }
                }
                self.request_redraw(window_id);
            } else if let Some(pane_id) = pane_id {
                let _ = window.app.focus_pane(PaneId::new(pane_id));
                self.request_redraw(window_id);
            }
        }
    }

    fn handle_scroll(&mut self, window_id: WindowId, delta: MouseScrollDelta) {
        let Some(window) = self.window_states.get_mut(&window_id) else {
            return;
        };
        let (delta_x, delta_y) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x.round() as isize, -(y.round() as isize)),
            MouseScrollDelta::PixelDelta(position) => (
                (position.x / 24.0).round() as isize,
                -((position.y / 24.0).round() as isize),
            ),
        };
        let horizontal = if self.modifiers.shift_key() {
            delta_y
        } else {
            delta_x
        };
        if horizontal != 0 || delta_y != 0 {
            let _ = window.app.adjust_active_pane_viewport(horizontal, delta_y);
            self.request_redraw(window_id);
        }
    }

    fn close_window(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId) {
        let _ = self.windows.remove(window_id);
        self.window_states.remove(&window_id);
        if self.window_states.is_empty() {
            event_loop.exit();
        }
    }

    fn close_quitting_windows(&mut self, event_loop: &ActiveEventLoop) {
        let quitting: Vec<WindowId> = self
            .window_states
            .iter()
            .filter_map(|(window_id, window)| window.app.should_quit().then_some(*window_id))
            .collect();

        for window_id in quitting {
            self.close_window(event_loop, window_id);
        }
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window_states.is_empty() {
            self.create_window(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.windows.binding(window_id).is_none() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => self.close_window(event_loop, window_id),
            WindowEvent::Focused(true) => {
                self.windows.set_focused_window(window_id);
                self.request_redraw(window_id);
            }
            WindowEvent::Focused(false) => {
                self.windows.clear_focused_window(window_id);
                self.request_redraw(window_id);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.handle_native_keyboard_shortcut(event_loop, window_id, &event) {
                    return;
                }

                if let Some(input_event) = input::app_input_event_from_winit(&event, self.modifiers)
                    && let Some(window) = self.window_states.get_mut(&window_id)
                {
                    window.app.handle_input(input_event);
                    self.sync_window_binding(window_id);
                    let actions = self.drain_frontend_actions(window_id);
                    self.apply_frontend_actions(event_loop, actions);
                    self.close_quitting_windows(event_loop);
                    if self.windows.binding(window_id).is_none() {
                        return;
                    }
                    self.request_redraw(window_id);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(window) = self.window_states.get_mut(&window_id) {
                    window.cursor_position = Some(position);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.handle_click(window_id),
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_scroll(window_id, delta);
                self.sync_window_binding(window_id);
            }
            WindowEvent::Resized(size) => {
                if let Some(window) = self.window_states.get_mut(&window_id) {
                    self.compositor.resize(&mut window.state, size);
                }
                self.sync_window_binding(window_id);
                self.request_redraw(window_id);
            }
            WindowEvent::RedrawRequested => {
                self.sync_window_binding(window_id);
                if let Some(window) = self.window_states.get_mut(&window_id) {
                    let view = window.app.view();
                    let scene = scene::build(&view, window.state.size());
                    let _ = self.compositor.render(&mut window.state, &scene);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.drain_pending_frontend_actions(event_loop);
        self.close_quitting_windows(event_loop);
        if self.window_states.is_empty() {
            return;
        }

        for window_id in redraw_targets(&self.windows) {
            self.sync_window_binding(window_id);
            self.request_redraw(window_id);
        }
    }
}
