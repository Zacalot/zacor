use crate::kernel::WorkspaceId;
use std::collections::BTreeMap;
use winit::window::WindowId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowBinding {
    pub workspace_id: WorkspaceId,
}

#[derive(Debug, Default)]
pub struct WindowRegistry {
    windows: BTreeMap<WindowId, WindowBinding>,
    focused_window: Option<WindowId>,
}

impl WindowRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_workspace(&mut self, window_id: WindowId, workspace_id: WorkspaceId) -> bool {
        let binding = WindowBinding { workspace_id };
        if self.binding(window_id) == Some(binding) {
            return false;
        }
        self.windows.insert(window_id, binding);
        true
    }

    pub fn insert(&mut self, window_id: WindowId, binding: WindowBinding) {
        self.windows.insert(window_id, binding);
    }

    pub fn remove(&mut self, window_id: WindowId) -> Option<WindowBinding> {
        if self.focused_window == Some(window_id) {
            self.focused_window = None;
        }
        self.windows.remove(&window_id)
    }

    pub fn binding(&self, window_id: WindowId) -> Option<WindowBinding> {
        self.windows.get(&window_id).copied()
    }

    pub fn window_for_workspace(&self, workspace_id: WorkspaceId) -> Option<WindowId> {
        self.windows.iter().find_map(|(window_id, binding)| {
            (binding.workspace_id == workspace_id).then_some(*window_id)
        })
    }

    pub fn set_focused_window(&mut self, window_id: WindowId) -> bool {
        if !self.windows.contains_key(&window_id) || self.focused_window == Some(window_id) {
            return false;
        }
        self.focused_window = Some(window_id);
        true
    }

    pub fn clear_focused_window(&mut self, window_id: WindowId) -> bool {
        if self.focused_window != Some(window_id) {
            return false;
        }
        self.focused_window = None;
        true
    }

    pub fn focused_window(&self) -> Option<WindowId> {
        self.focused_window
    }

    pub fn window_ids(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.windows.keys().copied()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tracks_workspace_binding_by_window() {
        let mut registry = WindowRegistry::new();
        let window_id = WindowId::dummy();
        let binding = WindowBinding {
            workspace_id: WorkspaceId::new(7),
        };

        registry.insert(window_id, binding);

        assert_eq!(registry.binding(window_id), Some(binding));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.remove(window_id), Some(binding));
        assert_eq!(registry.binding(window_id), None);
    }

    #[test]
    fn registry_updates_workspace_binding_for_existing_window() {
        let mut registry = WindowRegistry::new();
        let window_id = WindowId::dummy();

        assert!(registry.bind_workspace(window_id, WorkspaceId::new(3)));
        assert!(!registry.bind_workspace(window_id, WorkspaceId::new(3)));
        assert!(registry.bind_workspace(window_id, WorkspaceId::new(9)));
        assert_eq!(
            registry.binding(window_id),
            Some(WindowBinding {
                workspace_id: WorkspaceId::new(9),
            })
        );
    }

    #[test]
    fn registry_remove_clears_updated_binding() {
        let mut registry = WindowRegistry::new();
        let window_id = WindowId::dummy();

        registry.bind_workspace(window_id, WorkspaceId::new(1));
        registry.bind_workspace(window_id, WorkspaceId::new(2));

        assert_eq!(
            registry.remove(window_id),
            Some(WindowBinding {
                workspace_id: WorkspaceId::new(2),
            })
        );
        assert_eq!(registry.binding(window_id), None);
    }

    #[test]
    fn registry_can_find_window_by_workspace() {
        let mut registry = WindowRegistry::new();
        let window_id = WindowId::dummy();

        registry.bind_workspace(window_id, WorkspaceId::new(11));

        assert_eq!(registry.window_for_workspace(WorkspaceId::new(11)), Some(window_id));
        assert_eq!(registry.window_for_workspace(WorkspaceId::new(12)), None);
    }

    #[test]
    fn registry_tracks_focused_window() {
        let mut registry = WindowRegistry::new();
        let window_id = WindowId::dummy();

        assert!(!registry.set_focused_window(window_id));
        registry.bind_workspace(window_id, WorkspaceId::new(5));
        assert!(registry.set_focused_window(window_id));
        assert!(!registry.set_focused_window(window_id));
        assert_eq!(registry.focused_window(), Some(window_id));
        assert!(registry.clear_focused_window(window_id));
        assert_eq!(registry.focused_window(), None);
    }

    #[test]
    fn registry_clears_focus_when_focused_window_is_removed() {
        let mut registry = WindowRegistry::new();
        let window_id = WindowId::dummy();

        registry.bind_workspace(window_id, WorkspaceId::new(8));
        registry.set_focused_window(window_id);

        registry.remove(window_id);

        assert_eq!(registry.focused_window(), None);
    }
}
