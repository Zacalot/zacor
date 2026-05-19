use crate::input::{FocusRegion, HitRegion};

use super::{Color, Point, Rect, Size, Stroke, TextStyle};

#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Clear {
        color: Color,
    },
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        stroke: Stroke,
    },
    Line {
        from: Point,
        to: Point,
        stroke: Stroke,
    },
    Text {
        position: Point,
        text: String,
        style: TextStyle,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Layer(pub i16);

#[derive(Clone, Debug, PartialEq)]
pub struct SceneItem {
    pub primitive: Primitive,
    pub clip: Option<Rect>,
    pub layer: Layer,
}

impl SceneItem {
    pub fn new(primitive: Primitive) -> Self {
        Self {
            primitive,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    items: Vec<SceneItem>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, primitive: Primitive) {
        self.push_item(SceneItem::new(primitive));
    }

    pub fn push_item(&mut self, item: SceneItem) {
        self.items.push(item);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &[SceneItem] {
        &self.items
    }

    pub fn items_in_paint_order(&self) -> Vec<&SceneItem> {
        let mut items = self.items.iter().collect::<Vec<_>>();
        items.sort_by_key(|item| item.layer);
        items
    }

    pub fn clear_color(&mut self, color: Color) {
        self.push(Primitive::Clear { color });
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub size: Size,
    pub scene: Scene,
    pub hit_regions: Vec<HitRegion>,
    pub focus_regions: Vec<FocusRegion>,
}

impl Frame {
    pub const fn new(size: Size, scene: Scene) -> Self {
        Self {
            size,
            scene,
            hit_regions: Vec::new(),
            focus_regions: Vec::new(),
        }
    }

    pub const fn with_hit_regions(size: Size, scene: Scene, hit_regions: Vec<HitRegion>) -> Self {
        Self {
            size,
            scene,
            hit_regions,
            focus_regions: Vec::new(),
        }
    }

    pub const fn with_interaction_regions(
        size: Size,
        scene: Scene,
        hit_regions: Vec<HitRegion>,
        focus_regions: Vec<FocusRegion>,
    ) -> Self {
        Self {
            size,
            scene,
            hit_regions,
            focus_regions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_starts_empty() {
        assert!(Scene::new().items().is_empty());
    }

    #[test]
    fn scene_preserves_primitive_order() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.fill_rect(Rect::from_xywh(1.0, 2.0, 3.0, 4.0), Color::WHITE);

        assert_eq!(
            scene.items(),
            &[
                SceneItem {
                    primitive: Primitive::Clear {
                        color: Color::BLACK
                    },
                    clip: None,
                    layer: Layer(0),
                },
                SceneItem {
                    primitive: Primitive::FillRect {
                        rect: Rect::from_xywh(1.0, 2.0, 3.0, 4.0),
                        color: Color::WHITE
                    },
                    clip: None,
                    layer: Layer(0),
                }
            ]
        );
    }

    #[test]
    fn text_preserves_logical_position() {
        let mut scene = Scene::new();
        let style = TextStyle::new(Color::WHITE, 14.0);
        scene.text(Point::new(1.5, 2.25), "hello", style);
        assert_eq!(
            scene.items(),
            &[SceneItem {
                primitive: Primitive::Text {
                    position: Point::new(1.5, 2.25),
                    text: "hello".to_string(),
                    style
                },
                clip: None,
                layer: Layer(0),
            }]
        );
    }

    #[test]
    fn scene_items_can_carry_clip_and_layer_metadata() {
        let item = SceneItem::new(Primitive::FillRect {
            rect: Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
            color: Color::WHITE,
        })
        .clipped(Rect::from_xywh(1.0, 1.0, 2.0, 2.0))
        .layered(Layer(2));

        let mut scene = Scene::new();
        scene.push_item(item.clone());

        assert_eq!(scene.items(), &[item]);
    }

    #[test]
    fn scene_items_sort_by_layer_while_preserving_same_layer_order() {
        let mut scene = Scene::new();
        scene.push_item(
            SceneItem::new(Primitive::Clear {
                color: Color::BLACK,
            })
            .layered(Layer(1)),
        );
        scene.push_item(
            SceneItem::new(Primitive::Clear {
                color: Color::WHITE,
            })
            .layered(Layer(0)),
        );
        scene.push_item(
            SceneItem::new(Primitive::Clear {
                color: Color::Transparent,
            })
            .layered(Layer(1)),
        );

        let colors = scene
            .items_in_paint_order()
            .into_iter()
            .map(|item| match item.primitive {
                Primitive::Clear { color } => color,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();

        assert_eq!(colors, vec![Color::WHITE, Color::BLACK, Color::Transparent]);
    }

    #[test]
    fn frame_stores_size_and_scene() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        let frame = Frame::new(Size::new(80.0, 40.0), scene.clone());
        assert_eq!(frame.size, Size::new(80.0, 40.0));
        assert_eq!(frame.scene, scene);
        assert!(frame.hit_regions.is_empty());
        assert!(frame.focus_regions.is_empty());
    }

    #[test]
    fn frame_with_hit_regions_preserves_hit_regions() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        let hit_regions = vec![HitRegion::new(
            crate::input::HitRegionId(7),
            Rect::from_xywh(1.0, 2.0, 3.0, 4.0),
        )];
        let frame =
            Frame::with_hit_regions(Size::new(80.0, 40.0), scene.clone(), hit_regions.clone());

        assert_eq!(frame.size, Size::new(80.0, 40.0));
        assert_eq!(frame.scene, scene);
        assert_eq!(frame.hit_regions, hit_regions);
        assert!(frame.focus_regions.is_empty());
    }

    #[test]
    fn frame_with_interaction_regions_preserves_focus_regions() {
        let scene = Scene::new();
        let focus_regions = vec![FocusRegion::new(
            crate::input::FocusId(9),
            Rect::from_xywh(1.0, 2.0, 3.0, 4.0),
        )];
        let frame = Frame::with_interaction_regions(
            Size::new(80.0, 40.0),
            scene.clone(),
            Vec::new(),
            focus_regions.clone(),
        );

        assert_eq!(frame.scene, scene);
        assert_eq!(frame.focus_regions, focus_regions);
    }
}
