use crate::input::{FocusId, FocusRegion, HitBehavior, HitRegion, HitRegionId};

use super::{Color, Layer, Point, Primitive, Rect, Scene, SceneItem, Stroke, TextStyle};

#[derive(Clone, Debug, Default)]
pub struct PaintContext {
    scene: Scene,
    hit_regions: Vec<HitRegion>,
    focus_regions: Vec<FocusRegion>,
    current_layer: Layer,
    clip_stack: Vec<Rect>,
}

impl PaintContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finish(self) -> Scene {
        self.scene
    }

    pub fn finish_frame(self, size: super::Size) -> super::Frame {
        super::Frame::with_interaction_regions(
            size,
            self.scene,
            self.hit_regions,
            self.focus_regions,
        )
    }

    pub fn clear_color(&mut self, color: Color) {
        self.scene.clear_color(color);
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.push(Primitive::FillRect { rect, color });
    }

    pub fn stroke_rect(&mut self, rect: Rect, stroke: Stroke) {
        self.push(Primitive::StrokeRect { rect, stroke });
    }

    pub fn line(&mut self, from: Point, to: Point, stroke: Stroke) {
        self.push(Primitive::Line { from, to, stroke });
    }

    pub fn text(&mut self, position: Point, text: impl Into<String>, style: TextStyle) {
        self.push(Primitive::Text {
            position,
            text: text.into(),
            style,
        });
    }

    pub fn hit_region(&mut self, id: HitRegionId, rect: Rect) {
        self.hit_region_with_behavior(id, rect, HitBehavior::Normal);
    }

    pub fn hit_region_with_behavior(&mut self, id: HitRegionId, rect: Rect, behavior: HitBehavior) {
        if self.active_clip().is_some_and(|clip| clip.is_empty()) {
            return;
        }

        self.hit_regions.push(HitRegion {
            id,
            rect,
            clip: self.active_clip(),
            layer: self.current_layer,
            behavior,
        });
    }

    pub fn focus_region(&mut self, id: FocusId, rect: Rect) {
        if self.active_clip().is_some_and(|clip| clip.is_empty()) {
            return;
        }

        self.focus_regions.push(FocusRegion {
            id,
            rect,
            clip: self.active_clip(),
            layer: self.current_layer,
        });
    }

    pub fn with_clip(&mut self, clip: Rect, draw: impl FnOnce(&mut Self)) {
        let clip = match self.active_clip() {
            Some(active) => active.intersect(&clip),
            None => clip,
        };
        self.clip_stack.push(clip);
        draw(self);
        self.clip_stack.pop();
    }

    pub fn with_layer(&mut self, layer: Layer, draw: impl FnOnce(&mut Self)) {
        let previous = self.current_layer;
        self.current_layer = layer;
        draw(self);
        self.current_layer = previous;
    }

    fn push(&mut self, primitive: Primitive) {
        if self.active_clip().is_some_and(|clip| clip.is_empty()) {
            return;
        }

        self.scene.push_item(SceneItem {
            primitive,
            clip: self.active_clip(),
            layer: self.current_layer,
        });
    }

    fn active_clip(&self) -> Option<Rect> {
        self.clip_stack.last().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{HitBehavior, HitRegionId};
    use crate::render::Size;

    #[test]
    fn paint_context_applies_clip_metadata() {
        let mut paint = PaintContext::new();
        paint.with_clip(Rect::from_xywh(1.0, 2.0, 3.0, 4.0), |paint| {
            paint.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::WHITE);
        });

        let scene = paint.finish();
        assert_eq!(
            scene.items()[0].clip,
            Some(Rect::from_xywh(1.0, 2.0, 3.0, 4.0))
        );
    }

    #[test]
    fn paint_context_intersects_nested_clips() {
        let mut paint = PaintContext::new();
        paint.with_clip(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), |paint| {
            paint.with_clip(Rect::from_xywh(5.0, 5.0, 10.0, 10.0), |paint| {
                paint.fill_rect(Rect::from_xywh(0.0, 0.0, 20.0, 20.0), Color::WHITE);
            });
        });

        let scene = paint.finish();
        assert_eq!(
            scene.items()[0].clip,
            Some(Rect::from_xywh(5.0, 5.0, 5.0, 5.0))
        );
    }

    #[test]
    fn paint_context_skips_items_inside_empty_clip() {
        let mut paint = PaintContext::new();
        paint.with_clip(Rect::from_xywh(0.0, 0.0, 0.0, 1.0), |paint| {
            paint.fill_rect(Rect::from_xywh(0.0, 0.0, 20.0, 20.0), Color::WHITE);
        });

        assert!(paint.finish().items().is_empty());
    }

    #[test]
    fn paint_context_applies_layer_metadata_temporarily() {
        let mut paint = PaintContext::new();
        paint.with_layer(Layer(3), |paint| {
            paint.fill_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        });
        paint.fill_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::BLACK);

        let scene = paint.finish();
        assert_eq!(scene.items()[0].layer, Layer(3));
        assert_eq!(scene.items()[1].layer, Layer(0));
    }

    #[test]
    fn paint_context_records_hit_region() {
        let mut paint = PaintContext::new();
        paint.hit_region(HitRegionId(1), Rect::from_xywh(1.0, 2.0, 3.0, 4.0));

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert_eq!(frame.hit_regions.len(), 1);
        assert_eq!(frame.hit_regions[0].id, HitRegionId(1));
        assert_eq!(
            frame.hit_regions[0].rect,
            Rect::from_xywh(1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(frame.hit_regions[0].clip, None);
        assert_eq!(frame.hit_regions[0].layer, Layer(0));
        assert_eq!(frame.hit_regions[0].behavior, HitBehavior::Normal);
    }

    #[test]
    fn paint_context_records_focus_region() {
        let mut paint = PaintContext::new();
        paint.focus_region(FocusId(1), Rect::from_xywh(1.0, 2.0, 3.0, 4.0));

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert_eq!(frame.focus_regions.len(), 1);
        assert_eq!(frame.focus_regions[0].id, FocusId(1));
        assert_eq!(
            frame.focus_regions[0].rect,
            Rect::from_xywh(1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(frame.focus_regions[0].clip, None);
        assert_eq!(frame.focus_regions[0].layer, Layer(0));
    }

    #[test]
    fn paint_context_applies_clip_and_layer_to_focus_region() {
        let mut paint = PaintContext::new();
        paint.with_layer(Layer(3), |paint| {
            paint.with_clip(Rect::from_xywh(1.0, 2.0, 3.0, 4.0), |paint| {
                paint.focus_region(FocusId(1), Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
            });
        });

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert_eq!(
            frame.focus_regions[0].clip,
            Some(Rect::from_xywh(1.0, 2.0, 3.0, 4.0))
        );
        assert_eq!(frame.focus_regions[0].layer, Layer(3));
    }

    #[test]
    fn paint_context_skips_focus_region_inside_empty_clip() {
        let mut paint = PaintContext::new();
        paint.with_clip(Rect::from_xywh(0.0, 0.0, 0.0, 1.0), |paint| {
            paint.focus_region(FocusId(1), Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
        });

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert!(frame.focus_regions.is_empty());
    }

    #[test]
    fn paint_context_applies_layer_to_hit_region() {
        let mut paint = PaintContext::new();
        paint.with_layer(Layer(3), |paint| {
            paint.hit_region(HitRegionId(1), Rect::from_xywh(0.0, 0.0, 1.0, 1.0));
        });

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert_eq!(frame.hit_regions[0].layer, Layer(3));
    }

    #[test]
    fn paint_context_applies_clip_to_hit_region() {
        let mut paint = PaintContext::new();
        paint.with_clip(Rect::from_xywh(1.0, 2.0, 3.0, 4.0), |paint| {
            paint.hit_region(HitRegionId(1), Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
        });

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert_eq!(
            frame.hit_regions[0].clip,
            Some(Rect::from_xywh(1.0, 2.0, 3.0, 4.0))
        );
    }

    #[test]
    fn paint_context_skips_hit_region_inside_empty_clip() {
        let mut paint = PaintContext::new();
        paint.with_clip(Rect::from_xywh(0.0, 0.0, 0.0, 1.0), |paint| {
            paint.hit_region(HitRegionId(1), Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
        });

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert!(frame.hit_regions.is_empty());
    }

    #[test]
    fn paint_context_finish_frame_preserves_scene_and_hit_regions() {
        let mut paint = PaintContext::new();
        paint.fill_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        paint.hit_region_with_behavior(
            HitRegionId(1),
            Rect::from_xywh(0.0, 0.0, 1.0, 1.0),
            HitBehavior::BlockPointer,
        );

        let frame = paint.finish_frame(Size::new(10.0, 10.0));
        assert_eq!(frame.scene.items().len(), 1);
        assert_eq!(frame.hit_regions.len(), 1);
        assert_eq!(frame.hit_regions[0].behavior, HitBehavior::BlockPointer);
    }
}
