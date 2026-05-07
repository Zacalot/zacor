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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    primitives: Vec<Primitive>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, primitive: Primitive) {
        self.primitives.push(primitive);
    }

    pub fn clear(&mut self) {
        self.primitives.clear();
    }

    pub fn primitives(&self) -> &[Primitive] {
        &self.primitives
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
}

impl Frame {
    pub const fn new(size: Size, scene: Scene) -> Self {
        Self { size, scene }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_starts_empty() {
        assert!(Scene::new().primitives().is_empty());
    }

    #[test]
    fn scene_preserves_primitive_order() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.fill_rect(Rect::from_xywh(1.0, 2.0, 3.0, 4.0), Color::WHITE);

        assert_eq!(
            scene.primitives(),
            &[
                Primitive::Clear {
                    color: Color::BLACK
                },
                Primitive::FillRect {
                    rect: Rect::from_xywh(1.0, 2.0, 3.0, 4.0),
                    color: Color::WHITE
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
            scene.primitives(),
            &[Primitive::Text {
                position: Point::new(1.5, 2.25),
                text: "hello".to_string(),
                style
            }]
        );
    }

    #[test]
    fn frame_stores_size_and_scene() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        let frame = Frame::new(Size::new(80.0, 40.0), scene.clone());
        assert_eq!(frame.size, Size::new(80.0, 40.0));
        assert_eq!(frame.scene, scene);
    }
}
