use std::collections::HashMap;

use crate::input::{
    FocusId, HitBehavior, HitRegionId, InputListenerRegistry, KeyboardHandler, PointerHandler,
};
use crate::render::{
    Color, Frame, Layer, PaintContext, Rect, Size, Stroke, SurfaceFallback, SurfaceKind,
    SurfaceSlotId,
};

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
        ids: InteractionIds,
        focusable: bool,
        registry: &mut InputListenerRegistry,
    ) {
        for handler in &self.pointer_handlers {
            registry.pointer.register(ids.hit, handler.clone());
        }

        if focusable {
            for handler in &self.keyboard_handlers {
                registry.keyboard.register(ids.focus, handler.clone());
            }
        }
    }
}

pub struct Pane {
    id: PaneId,
    content: PaneContent,
    chrome: Option<PaneChrome>,
    focusable: bool,
    hit_behavior: HitBehavior,
}

impl Pane {
    pub fn new(id: PaneId, content: PaneContent) -> Self {
        Self {
            id,
            content,
            chrome: None,
            focusable: false,
            hit_behavior: HitBehavior::Normal,
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
    panes: HashMap<PaneId, Pane>,
    tree: PaneTree,
    active_pane: Option<PaneId>,
    background: Option<Color>,
}

impl InterfaceHost {
    pub fn new(tree: PaneTree) -> Self {
        Self {
            panes: HashMap::new(),
            tree,
            active_pane: None,
            background: None,
        }
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn insert_pane(&mut self, pane: Pane) -> Option<Pane> {
        self.panes.insert(pane.id(), pane)
    }

    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    pub fn select_pane(&mut self, id: PaneId) {
        self.active_pane = Some(id);
    }

    pub fn active_pane(&self) -> Option<PaneId> {
        self.active_pane
    }

    pub fn build_snapshot(&self, size: Size) -> HostSnapshot {
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

            let ids = interaction_ids_for_pane(pane.id());

            targets.hit_regions.insert(ids.hit, pane.id());
            paint.hit_region_with_behavior(ids.hit, pane_rect.rect, pane.hit_behavior);

            if pane.focusable {
                targets.focus_regions.insert(ids.focus, pane.id());
                paint.focus_region(ids.focus, pane_rect.rect);
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
                });
            } else {
                pane.content.paint(pane_rect.rect, &mut paint);
            }

            pane.content
                .register_listeners(ids, pane.focusable, &mut listeners);
        }

        HostSnapshot {
            frame: paint.finish_frame(size),
            listeners,
            targets,
        }
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

#[derive(Clone, Copy)]
struct InteractionIds {
    hit: HitRegionId,
    focus: FocusId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LaidOutPane {
    id: PaneId,
    rect: Rect,
}

fn interaction_ids_for_pane(id: PaneId) -> InteractionIds {
    InteractionIds {
        hit: HitRegionId(id.0),
        focus: FocusId(id.0),
    }
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
        let mut left_content = PaneContent::empty();
        left_content.on_pointer(Arc::new({
            let events = events.clone();
            move |_| {
                events.lock().unwrap().push("left-pointer");
                PointerDispatchResult::handled_by(HitRegionId(LEFT.0))
            }
        }));
        host.insert_pane(
            Pane::new(LEFT, left_content)
                .with_chrome(PaneChrome::new().with_background(Color::WHITE))
                .with_focusable(true),
        );
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()));

        let snapshot = host.build_snapshot(Size::new(100.0, 20.0));

        assert_eq!(snapshot.frame.size, Size::new(100.0, 20.0));
        assert!(
            snapshot
                .targets
                .pane_for_hit_region(HitRegionId(LEFT.0))
                .is_some()
        );
        assert!(
            snapshot
                .listeners
                .pointer
                .handlers_for(HitRegionId(LEFT.0))
                .is_some()
        );
    }

    #[test]
    fn build_snapshot_paints_pane_background() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty())
                .with_chrome(PaneChrome::new().with_background(Color::WHITE)),
        );

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.scene.items().len(), 1);
    }

    #[test]
    fn build_snapshot_registers_hit_regions() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(
            Pane::new(LEFT, PaneContent::empty()).with_hit_behavior(HitBehavior::BlockPointer),
        );

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.hit_regions.len(), 1);
        assert_eq!(snapshot.frame.hit_regions[0].id, HitRegionId(LEFT.0));
        assert_eq!(
            snapshot.frame.hit_regions[0].behavior,
            HitBehavior::BlockPointer
        );
    }

    #[test]
    fn build_snapshot_registers_focus_regions_for_focusable_panes() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));

        assert_eq!(snapshot.frame.focus_regions.len(), 1);
        assert_eq!(snapshot.frame.focus_regions[0].id, FocusId(LEFT.0));
    }

    #[test]
    fn build_snapshot_omits_focus_regions_for_unfocusable_panes() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));

        assert!(snapshot.frame.focus_regions.is_empty());
    }

    #[test]
    fn target_map_maps_regions_to_panes() {
        let mut host = InterfaceHost::new(horizontal_tree());
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()).with_focusable(true));
        host.insert_pane(Pane::new(RIGHT, PaneContent::empty()));

        let snapshot = host.build_snapshot(Size::new(100.0, 20.0));

        assert_eq!(
            snapshot.targets.pane_for_hit_region(HitRegionId(LEFT.0)),
            Some(LEFT)
        );
        assert_eq!(
            snapshot.targets.pane_for_focus_region(FocusId(LEFT.0)),
            Some(LEFT)
        );
        assert_eq!(
            snapshot.targets.pane_for_hit_region(HitRegionId(RIGHT.0)),
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

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));

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
    fn build_snapshot_registers_keyboard_handlers_only_for_focusable_panes() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let mut content = PaneContent::empty();
        content.on_keyboard(Arc::new(|_| {
            KeyboardDispatchResult::handled_by(FocusId(LEFT.0))
        }));
        host.insert_pane(Pane::new(LEFT, content));

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));

        assert!(
            snapshot
                .listeners
                .keyboard
                .handlers_for(FocusId(LEFT.0))
                .is_none()
        );
    }

    #[test]
    fn selected_pane_is_retained_across_snapshot_builds() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        host.insert_pane(Pane::new(LEFT, PaneContent::empty()));
        host.select_pane(LEFT);

        let _first = host.build_snapshot(Size::new(20.0, 10.0));
        let _second = host.build_snapshot(Size::new(30.0, 15.0));

        assert_eq!(host.active_pane(), Some(LEFT));
    }

    #[test]
    fn pointer_and_keyboard_handlers_survive_snapshot_registration() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(LEFT)));
        let mut content = PaneContent::empty();
        content.on_pointer(Arc::new(|_| {
            PointerDispatchResult::handled_by(HitRegionId(LEFT.0))
        }));
        content.on_keyboard(Arc::new(|context| {
            assert!(context.plan.is_key_down());
            KeyboardDispatchResult::handled_by(FocusId(LEFT.0))
        }));
        host.insert_pane(Pane::new(LEFT, content).with_focusable(true));

        let snapshot = host.build_snapshot(Size::new(20.0, 10.0));
        let keyboard_handlers = snapshot
            .listeners
            .keyboard
            .handlers_for(FocusId(LEFT.0))
            .unwrap();
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
            focused: Some(FocusId(LEFT.0)),
            target: Some(FocusId(LEFT.0)),
            kind: crate::input::KeyboardDispatchKind::KeyDown,
            propagation: crate::input::KeyboardPropagation::FocusOnly,
        };

        assert_eq!(
            snapshot
                .listeners
                .pointer
                .handlers_for(HitRegionId(LEFT.0))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(keyboard_handlers.len(), 1);
        assert_eq!(
            keyboard_handlers[0](&crate::input::KeyboardDispatchContext { plan: &plan }).consumer,
            Some(FocusId(LEFT.0))
        );
    }
}
