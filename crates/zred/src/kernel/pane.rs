use crate::kernel::ids::{BufferId, PaneId};
use crate::kernel::selection::Selection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pane {
    id: PaneId,
    buffer_id: BufferId,
    viewport: Viewport,
    presentation: PanePresentation,
    selection: Option<Selection>,
}

impl Pane {
    pub fn new(id: PaneId, buffer_id: BufferId) -> Self {
        Self {
            id,
            buffer_id,
            viewport: Viewport::default(),
            presentation: PanePresentation::Default,
            selection: None,
        }
    }

    #[allow(dead_code)]
    pub fn id(&self) -> PaneId {
        self.id
    }

    pub fn buffer_id(&self) -> BufferId {
        self.buffer_id
    }

    pub fn set_buffer_id(&mut self, buffer_id: BufferId) {
        self.buffer_id = buffer_id;
        self.viewport = Viewport::default();
        self.selection = None;
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    #[allow(dead_code)]
    pub fn presentation(&self) -> PanePresentation {
        self.presentation
    }

    #[allow(dead_code)]
    pub fn set_presentation(&mut self, presentation: PanePresentation) {
        self.presentation = presentation;
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Viewport {
    pub offset_x: usize,
    pub offset_y: usize,
}

impl Viewport {
    pub fn new(offset_x: usize, offset_y: usize) -> Self {
        Self { offset_x, offset_y }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PanePresentation {
    #[default]
    Default,
    Source,
    Preview,
    Inspector,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_switching_buffers_resets_view_state() {
        let mut pane = Pane::new(PaneId::new(1), BufferId::new(1));
        pane.set_viewport(Viewport::new(4, 8));
        pane.set_selection(Some(Selection::Surface(
            crate::kernel::SurfaceSelection::new("hotspot"),
        )));

        pane.set_buffer_id(BufferId::new(2));

        assert_eq!(pane.buffer_id(), BufferId::new(2));
        assert_eq!(pane.viewport(), Viewport::default());
        assert!(pane.selection().is_none());
    }
}
