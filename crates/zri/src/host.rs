use std::collections::HashMap;

mod model;
mod runtime;
mod text_input;

use crate::input::{
    FocusId, HitBehavior, HitRegionId, InputListenerRegistry, KeyboardHandler, PointerHandler,
};
use crate::render::{
    Color, Coord, Frame, Layer, PaintContext, Point, Rect, Size, Stroke, SurfaceFallback,
    SurfaceKind, SurfaceSlotId, TextStyle,
};
use crate::text::TextSystem;

/// Buffer text presentation until theming exists.
const BUFFER_TEXT_STYLE: TextStyle = TextStyle::new(Color::WHITE, 12.0);
const TEXT_INSET_X: Coord = 4.0;
/// Width reference for the cursor cell when no character sits under it
/// (end of line): shaped trailing-space advances are unreliable, an em is not.
const CURSOR_EM_CELL: &str = "m";

pub use model::{
    Buffer, BufferId, BufferKind, BufferStore, InterfaceFrame, InterfaceFrameId, View, ViewCursor,
    ViewId, ViewScroll, ViewStore,
};
pub use runtime::{
    HostRuntime, HostTurnResult, InputTurnResult, KeyboardTurnResult, PointerTurnResult,
};
pub use text_input::{TextInputResult, apply_keyboard_event};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SplitId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostError {
    message: String,
}

impl HostError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSlotBinding {
    pub id: SurfaceSlotId,
    pub kind: SurfaceKind,
    pub fallback: SurfaceFallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaneChrome {
    pub background: Option<Color>,
    pub border: Option<Stroke>,
    pub layer: Layer,
}

impl Default for PaneChrome {
    fn default() -> Self {
        Self {
            background: None,
            border: None,
            layer: Layer::default(),
        }
    }
}

impl PaneChrome {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn with_border(mut self, stroke: Stroke) -> Self {
        self.border = Some(stroke);
        self
    }

    pub fn with_layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }
}

#[derive(Default)]
pub struct PaneContent {
    surface_slot: Option<SurfaceSlotBinding>,
    pointer_handlers: Vec<PointerHandler>,
    keyboard_handlers: Vec<KeyboardHandler>,
}

impl PaneContent {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_surface_slot(mut self, binding: SurfaceSlotBinding) -> Self {
        self.surface_slot = Some(binding);
        self
    }

    pub fn on_pointer(&mut self, handler: PointerHandler) {
        self.pointer_handlers.push(handler);
    }

    pub fn on_keyboard(&mut self, handler: KeyboardHandler) {
        self.keyboard_handlers.push(handler);
    }

    fn paint(&self, rect: Rect, paint: &mut PaintContext) {
        if let Some(slot) = &self.surface_slot {
            paint.surface_slot(slot.id, rect, slot.kind, slot.fallback.clone());
        }
    }

    fn register_listeners(
        &self,
        hit_id: HitRegionId,
        focus_id: Option<FocusId>,
        registry: &mut InputListenerRegistry,
    ) {
        for handler in &self.pointer_handlers {
            registry.pointer.register(hit_id, handler.clone());
        }

        if let Some(focus_id) = focus_id {
            for handler in &self.keyboard_handlers {
                registry.keyboard.register(focus_id, handler.clone());
            }
        }
    }
}

pub struct Pane {
    id: PaneId,
    view: Option<ViewId>,
    content: PaneContent,
    chrome: Option<PaneChrome>,
    focusable: bool,
    hit_behavior: HitBehavior,
    /// Interaction identity, stamped by `InterfaceHost::insert_pane` from the
    /// host's monotonic allocators. Never derived from the pane id: region
    /// identity and pane identity are separate namespaces related only through
    /// the interaction target map.
    hit_id: Option<HitRegionId>,
    focus_id: Option<FocusId>,
}

impl Pane {
    pub fn new(id: PaneId, content: PaneContent) -> Self {
        Self {
            id,
            view: None,
            content,
            chrome: None,
            focusable: false,
            hit_behavior: HitBehavior::Normal,
            hit_id: None,
            focus_id: None,
        }
    }

    pub fn id(&self) -> PaneId {
        self.id
    }

    pub fn content(&self) -> &PaneContent {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut PaneContent {
        &mut self.content
    }

    pub fn view(&self) -> Option<ViewId> {
        self.view
    }

    pub fn set_view(&mut self, view: Option<ViewId>) {
        self.view = view;
    }

    pub fn with_view(mut self, view: ViewId) -> Self {
        self.view = Some(view);
        self
    }

    pub fn with_chrome(mut self, chrome: PaneChrome) -> Self {
        self.chrome = Some(chrome);
        self
    }

    pub fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    pub fn with_hit_behavior(mut self, behavior: HitBehavior) -> Self {
        self.hit_behavior = behavior;
        self
    }

    pub fn focusable(&self) -> bool {
        self.focusable
    }

    pub fn hit_region_id(&self) -> Option<HitRegionId> {
        self.hit_id
    }

    pub fn focus_id(&self) -> Option<FocusId> {
        self.focus_id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaneNode {
    Pane(PaneId),
    Split(SplitNode),
}

impl PaneNode {
    pub fn pane(id: PaneId) -> Self {
        Self::Pane(id)
    }

    pub fn split(split: SplitNode) -> Self {
        Self::Split(split)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitNode {
    id: SplitId,
    axis: Axis,
    children: Vec<PaneNode>,
    weights: Vec<f32>,
}

impl SplitNode {
    pub fn new(id: SplitId, axis: Axis, children: Vec<PaneNode>) -> Result<Self, HostError> {
        if children.is_empty() {
            return Err(HostError::new("split nodes require at least one child"));
        }

        Ok(Self {
            id,
            axis,
            weights: vec![1.0; children.len()],
            children,
        })
    }

    pub fn with_weights(mut self, weights: Vec<f32>) -> Result<Self, HostError> {
        if weights.len() != self.children.len() {
            return Err(HostError::new("split weights must match child count"));
        }

        if weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight <= 0.0)
        {
            return Err(HostError::new("split weights must be finite and positive"));
        }

        self.weights = weights;
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaneTree {
    root: PaneNode,
}

impl PaneTree {
    pub fn new(root: PaneNode) -> Self {
        Self { root }
    }

    fn layout(&self, bounds: Rect) -> Vec<LaidOutPane> {
        let mut panes = Vec::new();
        layout_node(&self.root, bounds, &mut panes);
        panes
    }
}

pub struct InterfaceHost {
    buffers: BufferStore,
    views: ViewStore,
    frame: InterfaceFrame,
    panes: HashMap<PaneId, Pane>,
    tree: PaneTree,
    active_pane: Option<PaneId>,
    background: Option<Color>,
    // Monotonic, never-reset interaction id allocators (GPUI HitboxId
    // semantics): stale ids miss rather than alias, and non-pane surfaces
    // (overlays, prompt views) can allocate from the same namespaces.
    next_hit_region_id: u64,
    next_focus_id: u64,
    /// Cursor blink phase; painting skips the cursor while hidden.
    cursor_visible: bool,
}

impl InterfaceHost {
    pub fn new(tree: PaneTree) -> Self {
        Self {
            buffers: BufferStore::new(),
            views: ViewStore::new(),
            frame: InterfaceFrame::default(),
            panes: HashMap::new(),
            tree,
            active_pane: None,
            background: None,
            next_hit_region_id: 1,
            next_focus_id: 1,
            cursor_visible: true,
        }
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub(crate) fn allocate_hit_region_id(&mut self) -> HitRegionId {
        let id = HitRegionId(self.next_hit_region_id);
        self.next_hit_region_id += 1;
        id
    }

    pub(crate) fn allocate_focus_id(&mut self) -> FocusId {
        let id = FocusId(self.next_focus_id);
        self.next_focus_id += 1;
        id
    }

    /// Insert a pane, stamping its interaction identity. Replacing an existing
    /// pane id allocates fresh region ids; any stale ids held elsewhere
    /// (e.g. focus state) simply stop matching.
    pub fn insert_pane(&mut self, mut pane: Pane) -> Option<Pane> {
        pane.hit_id = Some(self.allocate_hit_region_id());
        pane.focus_id = pane.focusable.then(|| self.allocate_focus_id());
        self.panes.insert(pane.id(), pane)
    }

    pub fn pane_hit_region_id(&self, id: PaneId) -> Option<HitRegionId> {
        self.panes.get(&id).and_then(Pane::hit_region_id)
    }

    pub fn pane_focus_id(&self, id: PaneId) -> Option<FocusId> {
        self.panes.get(&id).and_then(Pane::focus_id)
    }

    pub fn create_buffer(&mut self, kind: BufferKind, name: impl Into<String>) -> BufferId {
        self.buffers.create(kind, name)
    }

    pub fn create_buffer_with_text(
        &mut self,
        kind: BufferKind,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> BufferId {
        self.buffers.create_with_text(kind, name, text)
    }

    pub fn buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(id)
    }

    pub fn buffer_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(id)
    }

    pub fn append_to_buffer(&mut self, id: BufferId, text: impl AsRef<str>) -> bool {
        self.buffers.append_text(id, text)
    }

    pub fn create_view(&mut self, buffer: BufferId) -> ViewId {
        self.views.create(buffer)
    }

    pub fn view(&self, id: ViewId) -> Option<&View> {
        self.views.get(id)
    }

    pub fn view_mut(&mut self, id: ViewId) -> Option<&mut View> {
        self.views.get_mut(id)
    }

    pub fn set_pane_view(&mut self, pane: PaneId, view: Option<ViewId>) -> bool {
        let Some(pane) = self.panes.get_mut(&pane) else {
            return false;
        };
        pane.set_view(view);
        true
    }

    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    /// Follower setter for the derived active pane. Selection authority lives
    /// in `HostRuntime`'s focus state; only the runtime's transactional
    /// selection path (and in-module tests) may write this.
    fn set_active_pane(&mut self, id: Option<PaneId>) {
        self.active_pane = id;
    }

    pub fn active_pane(&self) -> Option<PaneId> {
        self.active_pane
    }

    /// Follower setter for the cursor blink phase. Owned by `HostRuntime`'s
    /// blink API, like `set_active_pane` is owned by its selection path.
    fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn interface_frame(&self) -> &InterfaceFrame {
        &self.frame
    }

    /// The selected view is a pure derivation of the active pane, never
    /// stored: one selection authority, everything else derived.
    pub fn selected_view(&self) -> Option<ViewId> {
        self.active_pane
            .and_then(|id| self.panes.get(&id))
            .and_then(Pane::view)
    }

    pub fn build_snapshot(&self, size: Size, text: &mut TextSystem) -> HostSnapshot {
        let bounds = Rect::from_xywh(0.0, 0.0, size.width, size.height);
        let laid_out = self.tree.layout(bounds);
        let mut paint = PaintContext::new();
        let mut listeners = InputListenerRegistry::new();
        let mut targets = InteractionTargetMap::default();

        if let Some(color) = self.background {
            paint.clear_color(color);
        }

        for pane_rect in laid_out {
            if pane_rect.rect.is_empty() {
                continue;
            }

            let Some(pane) = self.panes.get(&pane_rect.id) else {
                continue;
            };

            let hit_id = pane
                .hit_id
                .expect("pane interaction ids are stamped on insert");

            targets.hit_regions.insert(hit_id, pane.id());
            paint.hit_region_with_behavior(hit_id, pane_rect.rect, pane.hit_behavior);

            if let Some(focus_id) = pane.focus_id {
                targets.focus_regions.insert(focus_id, pane.id());
                paint.focus_region(focus_id, pane_rect.rect);
            }

            if let Some(chrome) = &pane.chrome {
                paint.with_layer(chrome.layer, |paint| {
                    if let Some(color) = chrome.background {
                        paint.fill_rect(pane_rect.rect, color);
                    }
                    if let Some(stroke) = chrome.border {
                        paint.stroke_rect(pane_rect.rect, stroke);
                    }
                    pane.content.paint(pane_rect.rect, paint);
                    self.paint_pane_view(pane, pane_rect.rect, text, paint);
                });
            } else {
                pane.content.paint(pane_rect.rect, &mut paint);
                self.paint_pane_view(pane, pane_rect.rect, text, &mut paint);
            }

            pane.content
                .register_listeners(hit_id, pane.focus_id, &mut listeners);
        }

        HostSnapshot {
            frame: paint.finish_frame(size),
            listeners,
            targets,
        }
    }

    fn paint_pane_view(
        &self,
        pane: &Pane,
        rect: Rect,
        text: &mut TextSystem,
        paint: &mut PaintContext,
    ) {
        let Some(view_id) = pane.view() else {
            return;
        };
        let Some(view) = self.views.get(view_id) else {
            return;
        };
        let Some(buffer) = self.buffers.get(view.buffer()) else {
            return;
        };

        paint.with_clip(rect, |paint| {
            paint_buffer_lines(buffer, view, rect, paint);
            // The cursor renders only for the focused pane (the active pane
            // follows the runtime's focus authority) and only in the visible
            // blink phase.
            if self.cursor_visible && self.active_pane == Some(pane.id()) {
                let background = pane
                    .chrome
                    .as_ref()
                    .and_then(|chrome| chrome.background)
                    .or(self.background)
                    .unwrap_or(Color::BLACK);
                paint_view_cursor(buffer, view, rect, background, text, paint);
            }
        });
    }
}

pub struct HostSnapshot {
    pub frame: Frame,
    pub listeners: InputListenerRegistry,
    pub targets: InteractionTargetMap,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InteractionTargetMap {
    pub hit_regions: HashMap<HitRegionId, PaneId>,
    pub focus_regions: HashMap<FocusId, PaneId>,
}

impl InteractionTargetMap {
    pub fn pane_for_hit_region(&self, id: HitRegionId) -> Option<PaneId> {
        self.hit_regions.get(&id).copied()
    }

    pub fn pane_for_focus_region(&self, id: FocusId) -> Option<PaneId> {
        self.focus_regions.get(&id).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LaidOutPane {
    id: PaneId,
    rect: Rect,
}

fn layout_node(node: &PaneNode, bounds: Rect, panes: &mut Vec<LaidOutPane>) {
    match node {
        PaneNode::Pane(id) => panes.push(LaidOutPane {
            rect: bounds,
            id: *id,
        }),
        PaneNode::Split(split) => layout_split(split, bounds, panes),
    }
}

fn layout_split(split: &SplitNode, bounds: Rect, panes: &mut Vec<LaidOutPane>) {
    let total_weight = split.weights.iter().sum::<f32>();
    let child_count = split.children.len();
    let mut cursor_x = bounds.origin.x;
    let mut cursor_y = bounds.origin.y;

    for (index, child) in split.children.iter().enumerate() {
        let rect = if index + 1 == child_count {
            Rect::from_xywh(
                cursor_x,
                cursor_y,
                bounds.right() - cursor_x,
                bounds.bottom() - cursor_y,
            )
        } else {
            let fraction = split.weights[index] / total_weight;
            match split.axis {
                Axis::Horizontal => {
                    let width = bounds.size.width * fraction;
                    let rect = Rect::from_xywh(cursor_x, cursor_y, width, bounds.size.height);
                    cursor_x += width;
                    rect
                }
                Axis::Vertical => {
                    let height = bounds.size.height * fraction;
                    let rect = Rect::from_xywh(cursor_x, cursor_y, bounds.size.width, height);
                    cursor_y += height;
                    rect
                }
            }
        };

        layout_node(child, rect, panes);
    }
}

fn visible_line_budget(rect: Rect, line_height: Coord) -> usize {
    (rect.size.height / line_height).ceil().max(0.0) as usize
}

fn paint_buffer_lines(buffer: &Buffer, view: &View, rect: Rect, paint: &mut PaintContext) {
    // `Primitive::Text.position` is the top of the line box (the GPU path
    // hands it straight to `TextSystem::layout_line`), so rows advance from
    // the pane top by the kernel's line height.
    let line_height = crate::text::line_height(BUFFER_TEXT_STYLE);
    let max_lines = visible_line_budget(rect, line_height);
    let start_line = view.scroll().line;
    let x = rect.origin.x + TEXT_INSET_X;
    let mut y = rect.origin.y;

    for line in buffer.lines().skip(start_line).take(max_lines) {
        paint.text(Point::new(x, y), line.to_string(), BUFFER_TEXT_STYLE);
        y += line_height;
    }
}

/// Paint the view's cursor as an opaque block over the character cell, with
/// the covered character re-drawn in the background color (Emacs
/// inverse-video). The overlay glyph is shaped alone, so kerning context can
/// shift it sub-pixel relative to the full line; the single-line kernel scope
/// accepts this.
fn paint_view_cursor(
    buffer: &Buffer,
    view: &View,
    rect: Rect,
    background: Color,
    text: &mut TextSystem,
    paint: &mut PaintContext,
) {
    let line_height = crate::text::line_height(BUFFER_TEXT_STYLE);
    let cursor = view.cursor();
    let scroll = view.scroll();
    if cursor.line < scroll.line {
        return;
    }
    let row = cursor.line - scroll.line;
    if row >= visible_line_budget(rect, line_height) {
        return;
    }

    // A cursor on a trailing empty line has no backing `lines()` entry.
    let line = buffer.lines().nth(cursor.line).unwrap_or("");
    let column = cursor.column.min(line.chars().count());
    let prefix: String = line.chars().take(column).collect();
    let prefix_width = text.measure_line(&prefix, BUFFER_TEXT_STYLE).width;

    let x = rect.origin.x + TEXT_INSET_X + prefix_width;
    let y = rect.origin.y + row as Coord * line_height;
    let covered = line.chars().nth(column);
    let width = match covered {
        Some(covered) => {
            let mut with_covered = prefix;
            with_covered.push(covered);
            text.measure_line(&with_covered, BUFFER_TEXT_STYLE).width - prefix_width
        }
        None => text.measure_line(CURSOR_EM_CELL, BUFFER_TEXT_STYLE).width,
    };

    paint.fill_rect(
        Rect::from_xywh(x, y, width, line_height),
        BUFFER_TEXT_STYLE.color,
    );
    if let Some(covered) = covered {
        paint.text(
            Point::new(x, y),
            covered.to_string(),
            TextStyle::new(background, BUFFER_TEXT_STYLE.size),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::input::{
        HitBehavior, Key, KeyDownEvent, KeyLocation, KeyboardDispatchResult, KeyboardEvent,
        Keystroke, Modifiers, PointerDispatchResult,
    };
    use crate::render::{SurfaceFallback, SurfaceKind};

    use super::*;

    const LEFT: PaneId = PaneId(1);
    const RIGHT: PaneId = PaneId(2);
    const BOTTOM: PaneId = PaneId(3);

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

    fn snapshot(host: &InterfaceHost, size: Size) -> HostSnapshot {
        host.build_snapshot(size, &mut TextSystem::new())
    }

    fn fill_rects(snapshot: &HostSnapshot) -> Vec<Rect> {
        snapshot
            .frame
            .scene
            .items()
            .iter()
            .filter_map(|item| match item.primitive {
                crate::render::Primitive::FillRect { rect, .. } => Some(rect),
                _ => None,
            })
            .collect()
    }

    fn text_items(snapshot: &HostSnapshot) -> Vec<(Point, String, TextStyle)> {
        snapshot
            .frame
            .scene
            .items()
            .iter()
            .filter_map(|item| match &item.primitive {
                crate::render::Primitive::Text {
                    position,
                    text,
                    style,
                } => Some((*position, text.clone(), *style)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn split_rejects_empty_children() {
        let error = SplitNode::new(SplitId(1), Axis::Horizontal, Vec::new()).unwrap_err();
        assert_eq!(error.message(), "split nodes require at least one child");
    }

    #[test]
    fn split_rejects_weight_count_mismatch() {
        let error = SplitNode::new(SplitId(1), Axis::Horizontal, vec![PaneNode::pane(LEFT)])
            .unwrap()
            .with_weights(vec![1.0, 2.0])
            .unwrap_err();

        assert_eq!(error.message(), "split weights must match child count");
    }

    #[test]
    fn split_rejects_non_positive_weights() {
        let error = SplitNode::new(
            SplitId(1),
            Axis::Horizontal,
            vec![PaneNode::pane(LEFT), PaneNode::pane(RIGHT)],
        )
        .unwrap()
        .with_weights(vec![1.0, 0.0])
        .unwrap_err();

        assert_eq!(error.message(), "split weights must be finite and positive");
    }

    #[test]
    fn horizontal_layout_assigns_equal_widths() {
        let layout = horizontal_tree().layout(Rect::from_xywh(0.0, 0.0, 100.0, 30.0));

        assert_eq!(layout.len(), 2);
        assert_eq!(
            layout[0],
            LaidOutPane {
                id: LEFT,
                rect: Rect::from_xywh(0.0, 0.0, 50.0, 30.0),
            }
        );
        assert_eq!(
            layout[1],
            LaidOutPane {
                id: RIGHT,
                rect: Rect::from_xywh(50.0, 0.0, 50.0, 30.0),
            }
        );
    }

    #[test]
    fn vertical_layout_assigns_equal_heights() {
        let tree = PaneTree::new(PaneNode::split(
            SplitNode::new(
                SplitId(1),
                Axis::Vertical,
                vec![PaneNode::pane(LEFT), PaneNode::pane(RIGHT)],
            )
            .unwrap(),
        ));
        let layout = tree.layout(Rect::from_xywh(0.0, 0.0, 40.0, 80.0));

        assert_eq!(layout[0].rect, Rect::from_xywh(0.0, 0.0, 40.0, 40.0));
        assert_eq!(layout[1].rect, Rect::from_xywh(0.0, 40.0, 40.0, 40.0));
    }

    #[test]
    fn weighted_layout_respects_weights() {
        let tree = PaneTree::new(PaneNode::split(
            SplitNode::new(
                SplitId(1),
                Axis::Horizontal,
                vec![PaneNode::pane(LEFT), PaneNode::pane(RIGHT)],
            )
            .unwrap()
            .with_weights(vec![1.0, 3.0])
            .unwrap(),
        ));
        let layout = tree.layout(Rect::from_xywh(0.0, 0.0, 80.0, 10.0));

        assert_eq!(layout[0].rect, Rect::from_xywh(0.0, 0.0, 20.0, 10.0));
        assert_eq!(layout[1].rect, Rect::from_xywh(20.0, 0.0, 60.0, 10.0));
    }

    #[test]
    fn last_child_absorbs_rounding_remainder() {
        let tree = PaneTree::new(PaneNode::split(
            SplitNode::new(
                SplitId(1),
                Axis::Horizontal,
                vec![
                    PaneNode::pane(LEFT),
                    PaneNode::pane(RIGHT),
                    PaneNode::pane(BOTTOM),
                ],
            )
            .unwrap(),
        ));
        let layout = tree.layout(Rect::from_xywh(0.0, 0.0, 10.0, 5.0));

        assert_eq!(
            layout[0].rect.size.width + layout[1].rect.size.width + layout[2].rect.size.width,
            10.0
        );
        assert_eq!(layout[2].rect.right(), 10.0);
    }

    #[test]
    fn build_snapshot_contains_frame_and_listeners_from_same_layout() {
        let mut host = InterfaceHost::new(horizontal_tree()).with_background(Color::BLACK);
        let events = Arc::new(Mutex::new(Vec::new()));
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_chrome(PaneChrome::new().with_background(Color::WHITE))
                .with_focusable(true),
        );
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()));
        let left_hit = host.pane_hit_region_id(LEFT).unwrap();
        host.pane_mut(LEFT)
            .unwrap()
            .content_mut()
            .on_pointer(Arc::new({
                let events = events.clone();
                move |_| {
                    events.lock().unwrap().push("left-pointer");
                    PointerDispatchResult::handled_by(left_hit)
                }
            }));

        let snapshot = snapshot(&host, Size::new(100.0, 20.0));

        assert_eq!(snapshot.frame.size, Size::new(100.0, 20.0));
        assert_eq!(snapshot.targets.pane_for_hit_region(left_hit), Some(LEFT));
        assert!(snapshot.listeners.pointer.handlers_for(left_hit).is_some());
    }

    #[test]
    fn build_snapshot_paints_pane_background() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_chrome(PaneChrome::new().with_background(Color::WHITE)),
        );

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.scene.items().len(), 1);
    }

    #[test]
    fn build_snapshot_registers_hit_regions() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty()).with_hit_behavior(HitBehavior::BlockPointer),
        );

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.hit_regions.len(), 1);
        assert_eq!(
            snapshot.frame.hit_regions[0].id,
            host.pane_hit_region_id(LEFT).unwrap()
        );
        assert_eq!(
            snapshot.frame.hit_regions[0].behavior,
            HitBehavior::BlockPointer
        );
    }

    #[test]
    fn build_snapshot_registers_focus_regions_for_focusable_panes() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.focus_regions.len(), 1);
        assert_eq!(
            snapshot.frame.focus_regions[0].id,
            host.pane_focus_id(LEFT).unwrap()
        );
    }

    #[test]
    fn build_snapshot_omits_focus_regions_for_unfocusable_panes() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        assert!(snapshot.frame.focus_regions.is_empty());
    }

    #[test]
    fn target_map_maps_regions_to_panes() {
        let mut host = InterfaceHost::new(horizontal_tree());
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()));

        let snapshot = snapshot(&host, Size::new(100.0, 20.0));

        assert_eq!(
            snapshot
                .targets
                .pane_for_hit_region(host.pane_hit_region_id(LEFT).unwrap()),
            Some(LEFT)
        );
        assert_eq!(
            snapshot
                .targets
                .pane_for_focus_region(host.pane_focus_id(LEFT).unwrap()),
            Some(LEFT)
        );
        assert_eq!(
            snapshot
                .targets
                .pane_for_hit_region(host.pane_hit_region_id(RIGHT).unwrap()),
            Some(RIGHT)
        );
    }

    #[test]
    fn pane_content_can_emit_surface_slot_without_pane_knowing_backend_details() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(
            LEFT,
            PaneContent::empty().with_surface_slot(SurfaceSlotBinding {
                id: SurfaceSlotId(9),
                kind: SurfaceKind::Canvas,
                fallback: SurfaceFallback::None,
            }),
        ));

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.scene.items().len(), 1);
        assert_eq!(
            snapshot.frame.scene.items()[0].primitive,
            crate::render::Primitive::SurfaceSlot {
                id: SurfaceSlotId(9),
                rect: Rect::from_xywh(0.0, 0.0, 20.0, 10.0),
                kind: SurfaceKind::Canvas,
                fallback: SurfaceFallback::None,
            }
        );
    }

    #[test]
    fn pane_can_host_view_over_buffer() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello");
        let view = host.create_view(buffer);

        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));

        assert_eq!(host.pane(LEFT).unwrap().view(), Some(view));
        assert_eq!(host.view(view).unwrap().buffer(), buffer);
    }

    #[test]
    fn build_snapshot_paints_hosted_buffer_text() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello\nworld");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));

        let snapshot = snapshot(&host, Size::new(80.0, 40.0));

        let text_items = snapshot
            .frame
            .scene
            .items()
            .iter()
            .filter_map(|item| match &item.primitive {
                crate::render::Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(text_items, vec!["hello", "world"]);
    }

    #[test]
    fn hosted_view_with_missing_buffer_is_ignored() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let view = host.create_view(BufferId(999));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));

        let snapshot = snapshot(&host, Size::new(80.0, 40.0));

        assert!(snapshot.frame.scene.items().is_empty());
    }

    #[test]
    fn build_snapshot_registers_keyboard_handlers_only_for_focusable_panes() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let mut content = PaneContent::empty();
        content.on_keyboard(Arc::new(|_| KeyboardDispatchResult::ignored()));
        host.insert_pane(Pane::new(LEFT, content));

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        // An unfocusable pane has no focus identity at all, so no keyboard
        // handlers can be registered for it.
        assert!(host.pane_focus_id(LEFT).is_none());
        assert!(snapshot.frame.focus_regions.is_empty());
    }

    #[test]
    fn active_pane_is_retained_across_snapshot_builds() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        host.set_active_pane(Some(LEFT));

        let _first = snapshot(&host, Size::new(20.0, 10.0));
        let _second = snapshot(&host, Size::new(30.0, 15.0));

        assert_eq!(host.active_pane(), Some(LEFT));
    }

    #[test]
    fn selected_view_derives_from_active_pane() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Log, "log");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));

        host.set_active_pane(Some(LEFT));

        assert_eq!(host.active_pane(), Some(LEFT));
        assert_eq!(host.selected_view(), Some(view));
    }

    #[test]
    fn selected_view_is_none_without_active_pane() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));

        assert_eq!(host.selected_view(), None);
    }

    #[test]
    fn selected_view_is_none_for_viewless_pane() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));

        host.set_active_pane(Some(LEFT));

        assert_eq!(host.selected_view(), None);
    }

    #[test]
    fn assigning_pane_view_updates_derived_selected_view() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer(BufferKind::Text, "scratch");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        host.set_active_pane(Some(LEFT));
        assert_eq!(host.selected_view(), None);

        assert!(host.set_pane_view(LEFT, Some(view)));

        assert_eq!(host.selected_view(), Some(view));
    }

    #[test]
    fn pointer_and_keyboard_handlers_survive_snapshot_registration() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let hit_id = host.pane_hit_region_id(LEFT).unwrap();
        let focus_id = host.pane_focus_id(LEFT).unwrap();
        {
            let content = host.pane_mut(LEFT).unwrap().content_mut();
            content.on_pointer(Arc::new(move |_| PointerDispatchResult::handled_by(hit_id)));
            content.on_keyboard(Arc::new(move |context| {
                assert!(context.plan.is_key_down());
                KeyboardDispatchResult::handled_by(focus_id)
            }));
        }

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));
        let keyboard_handlers = snapshot.listeners.keyboard.handlers_for(focus_id).unwrap();
        let plan = crate::input::KeyboardDispatchPlan {
            event: KeyboardEvent::KeyDown(KeyDownEvent {
                keystroke: Keystroke {
                    key: Key::Character("a".to_string()),
                    text: Some("a".to_string()),
                    modifiers: Modifiers::default(),
                    location: KeyLocation::Standard,
                },
                repeat: false,
                prefer_text: false,
            }),
            focused: Some(focus_id),
            target: Some(focus_id),
            kind: crate::input::KeyboardDispatchKind::KeyDown,
            propagation: crate::input::KeyboardPropagation::FocusOnly,
        };

        assert_eq!(
            snapshot
                .listeners
                .pointer
                .handlers_for(hit_id)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(keyboard_handlers.len(), 1);
        assert_eq!(
            keyboard_handlers[0](&crate::input::KeyboardDispatchContext { plan: &plan }).consumer,
            Some(focus_id)
        );
    }

    #[test]
    fn region_ids_are_stable_across_snapshots_and_unique_per_pane() {
        let mut host = InterfaceHost::new(horizontal_tree());
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()).with_focusable(true));

        let left_hit = host.pane_hit_region_id(LEFT).unwrap();
        let right_hit = host.pane_hit_region_id(RIGHT).unwrap();
        let left_focus = host.pane_focus_id(LEFT).unwrap();
        let right_focus = host.pane_focus_id(RIGHT).unwrap();

        assert_ne!(left_hit, right_hit);
        assert_ne!(left_focus, right_focus);

        let _first = snapshot(&host, Size::new(100.0, 20.0));
        let second = snapshot(&host, Size::new(100.0, 20.0));

        assert_eq!(host.pane_hit_region_id(LEFT), Some(left_hit));
        assert_eq!(host.pane_focus_id(RIGHT), Some(right_focus));
        assert_eq!(second.targets.pane_for_hit_region(left_hit), Some(LEFT));
    }

    #[test]
    fn region_ids_are_not_derived_from_pane_ids() {
        // Insert in reverse pane-number order so any derivation scheme would
        // be exposed: allocation order, not pane numbering, defines ids.
        let mut host = InterfaceHost::new(horizontal_tree());
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()).with_focusable(true));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));

        assert_eq!(host.pane_hit_region_id(RIGHT), Some(HitRegionId(1)));
        assert_eq!(host.pane_hit_region_id(LEFT), Some(HitRegionId(2)));
        assert_ne!(host.pane_hit_region_id(RIGHT).unwrap().0, RIGHT.0);
    }

    #[test]
    fn reinserting_pane_allocates_fresh_ids_and_stale_ids_miss() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let stale_hit = host.pane_hit_region_id(LEFT).unwrap();
        let stale_focus = host.pane_focus_id(LEFT).unwrap();

        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        let snapshot = snapshot(&host, Size::new(20.0, 10.0));

        assert_ne!(host.pane_hit_region_id(LEFT), Some(stale_hit));
        assert_eq!(snapshot.targets.pane_for_hit_region(stale_hit), None);
        assert_eq!(snapshot.targets.pane_for_focus_region(stale_focus), None);
    }

    #[test]
    fn allocators_serve_non_pane_regions_without_collision() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));

        // The overlay path (future prompt/minibuffer) allocates from the same
        // namespaces and can never collide with pane regions.
        let overlay_hit = host.allocate_hit_region_id();
        let overlay_focus = host.allocate_focus_id();

        assert_ne!(Some(overlay_hit), host.pane_hit_region_id(LEFT));
        assert_ne!(Some(overlay_focus), host.pane_focus_id(LEFT));

        let snapshot = snapshot(&host, Size::new(20.0, 10.0));
        assert_eq!(snapshot.targets.pane_for_hit_region(overlay_hit), None);
        assert_eq!(snapshot.targets.pane_for_focus_region(overlay_focus), None);
    }

    #[test]
    fn buffer_lines_lay_out_with_kernel_line_height() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello\nworld");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));

        let snap = snapshot(&host, Size::new(80.0, 40.0));

        let line_height = crate::text::line_height(BUFFER_TEXT_STYLE);
        let texts = text_items(&snap);
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].0, Point::new(TEXT_INSET_X, 0.0));
        assert_eq!(texts[1].0, Point::new(TEXT_INSET_X, line_height));
        assert_eq!(texts[0].2, BUFFER_TEXT_STYLE);
    }

    #[test]
    fn focused_pane_paints_block_cursor_at_measured_column() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello");
        let view = host.create_view(buffer);
        host.view_mut(view)
            .unwrap()
            .set_cursor(ViewCursor { line: 0, column: 3 });
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));
        host.set_active_pane(Some(LEFT));

        let snap = snapshot(&host, Size::new(200.0, 40.0));

        let mut text = TextSystem::new();
        let line_height = crate::text::line_height(BUFFER_TEXT_STYLE);
        let prefix_width = text.measure_line("hel", BUFFER_TEXT_STYLE).width;
        let advance = text.measure_line("hell", BUFFER_TEXT_STYLE).width - prefix_width;

        let fills = fill_rects(&snap);
        assert_eq!(
            fills,
            vec![Rect::from_xywh(
                TEXT_INSET_X + prefix_width,
                0.0,
                advance,
                line_height
            )]
        );

        // The covered character repaints over the block in the background
        // color (inverse video); no chrome/host background means black.
        let texts = text_items(&snap);
        let overlay = texts.last().unwrap();
        assert_eq!(overlay.1, "l");
        assert_eq!(overlay.0, Point::new(TEXT_INSET_X + prefix_width, 0.0));
        assert_eq!(
            overlay.2,
            TextStyle::new(Color::BLACK, BUFFER_TEXT_STYLE.size)
        );
    }

    #[test]
    fn cursor_at_line_end_paints_em_cell_without_overlay_glyph() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hi");
        let view = host.create_view(buffer);
        host.view_mut(view)
            .unwrap()
            .set_cursor(ViewCursor { line: 0, column: 2 });
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));
        host.set_active_pane(Some(LEFT));

        let snap = snapshot(&host, Size::new(200.0, 40.0));

        let mut text = TextSystem::new();
        let em_width = text.measure_line(CURSOR_EM_CELL, BUFFER_TEXT_STYLE).width;
        let fills = fill_rects(&snap);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.width, em_width);
        // Only the buffer line itself; no inverse-video overlay glyph.
        assert_eq!(text_items(&snap).len(), 1);
    }

    #[test]
    fn unfocused_pane_paints_no_cursor() {
        let mut host = InterfaceHost::new(horizontal_tree());
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()));

        // No active pane at all.
        assert!(fill_rects(&snapshot(&host, Size::new(200.0, 40.0))).is_empty());

        // A different pane is active.
        host.set_active_pane(Some(RIGHT));
        assert!(fill_rects(&snapshot(&host, Size::new(200.0, 40.0))).is_empty());
    }

    #[test]
    fn cursor_scrolled_out_of_view_is_not_painted() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "a\nb\nc");
        let view = host.create_view(buffer);
        host.view_mut(view)
            .unwrap()
            .set_scroll(ViewScroll { line: 2, column: 0 });
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));
        host.set_active_pane(Some(LEFT));

        // Cursor stays at (0, 0), which is above the scrolled window.
        assert!(fill_rects(&snapshot(&host, Size::new(200.0, 40.0))).is_empty());
    }

    #[test]
    fn cursor_hidden_blink_phase_paints_no_cursor() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let buffer = host.create_buffer_with_text(BufferKind::Text, "scratch", "hello");
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_view(view));
        host.set_active_pane(Some(LEFT));
        host.set_cursor_visible(false);

        assert!(fill_rects(&snapshot(&host, Size::new(200.0, 40.0))).is_empty());

        host.set_cursor_visible(true);
        assert_eq!(
            fill_rects(&snapshot(&host, Size::new(200.0, 40.0))).len(),
            1
        );
    }
}
