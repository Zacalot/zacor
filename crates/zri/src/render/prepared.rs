use super::{Color, Frame, Point, Primitive, Rect, Size, Stroke};

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedFrame {
    pub size: Size,
    pub clear_color: Option<Color>,
    pub rect_vertices: Vec<PreparedRectVertex>,
    pub rendered_primitives: usize,
    pub unsupported_primitives: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedRectVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

pub fn prepare_frame(frame: &Frame) -> PreparedFrame {
    let mut prepared = PreparedFrame {
        size: frame.size,
        clear_color: None,
        rect_vertices: Vec::new(),
        rendered_primitives: 0,
        unsupported_primitives: 0,
    };

    for item in frame.scene.items() {
        if let Primitive::Clear { color } = &item.primitive {
            prepared.clear_color = Some(*color);
            prepared.rendered_primitives += 1;
        }
    }

    for item in frame.scene.items_in_paint_order() {
        match &item.primitive {
            Primitive::Clear { .. } => {}
            Primitive::FillRect { rect, color } => {
                if push_fill_rect(
                    &mut prepared.rect_vertices,
                    frame.size,
                    *rect,
                    item.clip,
                    *color,
                ) {
                    prepared.rendered_primitives += 1;
                }
            }
            Primitive::StrokeRect { rect, stroke } => {
                if push_stroke_rect(
                    &mut prepared.rect_vertices,
                    frame.size,
                    *rect,
                    item.clip,
                    *stroke,
                ) {
                    prepared.rendered_primitives += 1;
                }
            }
            Primitive::Line { .. } | Primitive::Text { .. } => {
                prepared.unsupported_primitives += 1;
            }
        }
    }

    prepared
}

fn effective_clip(frame_size: Size, clip: Option<Rect>) -> Rect {
    let frame_rect = Rect::from_xywh(0.0, 0.0, frame_size.width, frame_size.height);
    match clip {
        Some(clip) => clip.intersect(&frame_rect),
        None => frame_rect,
    }
}

fn push_stroke_rect(
    vertices: &mut Vec<PreparedRectVertex>,
    frame_size: Size,
    rect: Rect,
    clip: Option<Rect>,
    stroke: Stroke,
) -> bool {
    if rect.is_empty() || stroke.width <= 0.0 {
        return false;
    }

    let width = stroke.width.min(rect.size.width).min(rect.size.height);
    let left = rect.left();
    let right = rect.right();
    let top = rect.top();
    let bottom = rect.bottom();

    let top_rect = Rect::from_xywh(left, top, rect.size.width, width);
    let bottom_rect = Rect::from_xywh(left, bottom - width, rect.size.width, width);
    let left_rect = Rect::from_xywh(
        left,
        top + width,
        width,
        (rect.size.height - width * 2.0).max(0.0),
    );
    let right_rect = Rect::from_xywh(
        right - width,
        top + width,
        width,
        (rect.size.height - width * 2.0).max(0.0),
    );

    let before = vertices.len();
    push_fill_rect(vertices, frame_size, top_rect, clip, stroke.color);
    push_fill_rect(vertices, frame_size, bottom_rect, clip, stroke.color);
    push_fill_rect(vertices, frame_size, left_rect, clip, stroke.color);
    push_fill_rect(vertices, frame_size, right_rect, clip, stroke.color);
    vertices.len() > before
}

fn push_fill_rect(
    vertices: &mut Vec<PreparedRectVertex>,
    frame_size: Size,
    rect: Rect,
    clip: Option<Rect>,
    color: Color,
) -> bool {
    let rect = rect.intersect(&effective_clip(frame_size, clip));
    if rect.is_empty() || frame_size.width <= 0.0 || frame_size.height <= 0.0 {
        return false;
    }

    let top_left = logical_to_ndc(Point::new(rect.left(), rect.top()), frame_size);
    let top_right = logical_to_ndc(Point::new(rect.right(), rect.top()), frame_size);
    let bottom_right = logical_to_ndc(Point::new(rect.right(), rect.bottom()), frame_size);
    let bottom_left = logical_to_ndc(Point::new(rect.left(), rect.bottom()), frame_size);
    let color = color.to_f32_rgba();

    vertices.extend_from_slice(&[
        PreparedRectVertex {
            position: top_left,
            color,
        },
        PreparedRectVertex {
            position: top_right,
            color,
        },
        PreparedRectVertex {
            position: bottom_right,
            color,
        },
        PreparedRectVertex {
            position: top_left,
            color,
        },
        PreparedRectVertex {
            position: bottom_right,
            color,
        },
        PreparedRectVertex {
            position: bottom_left,
            color,
        },
    ]);
    true
}

fn logical_to_ndc(point: Point, size: Size) -> [f32; 2] {
    [
        (point.x / size.width) * 2.0 - 1.0,
        1.0 - (point.y / size.height) * 2.0,
    ]
}

#[cfg(test)]
fn item(primitive: Primitive) -> super::SceneItem {
    super::SceneItem::new(primitive)
}

#[cfg(test)]
fn fill_rect_color_at_index(prepared: &PreparedFrame, rect_index: usize) -> [f32; 4] {
    prepared.rect_vertices[rect_index * 6].color
}

#[cfg(test)]
fn build_scene(items: Vec<super::SceneItem>) -> Frame {
    let mut scene = super::Scene::new();
    for item in items {
        scene.push_item(item);
    }
    Frame::new(Size::new(100.0, 100.0), scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Layer, Scene, TextStyle};

    const RED: Color = Color::rgb(255, 0, 0);
    const GREEN: Color = Color::rgb(0, 255, 0);
    const BLUE: Color = Color::rgb(0, 0, 255);

    fn frame(scene: Scene) -> Frame {
        Frame::new(Size::new(100.0, 100.0), scene)
    }

    fn assert_position_near(actual: [f32; 2], expected: [f32; 2]) {
        let epsilon = 0.00001;
        assert!(
            (actual[0] - expected[0]).abs() < epsilon,
            "x: {actual:?} != {expected:?}"
        );
        assert!(
            (actual[1] - expected[1]).abs() < epsilon,
            "y: {actual:?} != {expected:?}"
        );
    }

    #[test]
    fn logical_coordinates_convert_to_ndc() {
        let size = Size::new(100.0, 50.0);
        assert_eq!(logical_to_ndc(Point::new(0.0, 0.0), size), [-1.0, 1.0]);
        assert_eq!(logical_to_ndc(Point::new(100.0, 50.0), size), [1.0, -1.0]);
        assert_eq!(logical_to_ndc(Point::new(50.0, 25.0), size), [0.0, 0.0]);
    }

    #[test]
    fn clear_sets_prepared_clear_color() {
        let mut scene = Scene::new();
        scene.clear_color(BLUE);
        let prepared = prepare_frame(&frame(scene));
        assert_eq!(prepared.clear_color, Some(BLUE));
        assert_eq!(prepared.rendered_primitives, 1);
        assert!(prepared.rect_vertices.is_empty());
    }

    #[test]
    fn fill_rect_lowers_to_two_triangles() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(10.0, 20.0, 30.0, 40.0), RED);
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.rect_vertices.len(), 6);
        assert_position_near(prepared.rect_vertices[0].position, [-0.8, 0.6]);
        assert_position_near(prepared.rect_vertices[1].position, [-0.2, 0.6]);
        assert_position_near(prepared.rect_vertices[2].position, [-0.2, -0.2]);
        assert_position_near(prepared.rect_vertices[5].position, [-0.8, -0.2]);
        assert!(
            prepared
                .rect_vertices
                .iter()
                .all(|vertex| vertex.color == RED.to_f32_rgba())
        );
    }

    #[test]
    fn fill_rects_preserve_same_layer_insertion_order() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), RED);
        scene.fill_rect(Rect::from_xywh(10.0, 0.0, 10.0, 10.0), GREEN);
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 2);
        assert_eq!(prepared.rect_vertices.len(), 12);
        assert_eq!(fill_rect_color_at_index(&prepared, 0), RED.to_f32_rgba());
        assert_eq!(fill_rect_color_at_index(&prepared, 1), GREEN.to_f32_rgba());
    }

    #[test]
    fn layer_order_draws_lower_layers_first() {
        let frame = build_scene(vec![
            item(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
                color: RED,
            })
            .layered(Layer(10)),
            item(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
                color: GREEN,
            })
            .layered(Layer(0)),
        ]);
        let prepared = prepare_frame(&frame);

        assert_eq!(prepared.rendered_primitives, 2);
        assert_eq!(fill_rect_color_at_index(&prepared, 0), GREEN.to_f32_rgba());
        assert_eq!(fill_rect_color_at_index(&prepared, 1), RED.to_f32_rgba());
    }

    #[test]
    fn fill_rects_are_clipped_to_frame_bounds() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(-10.0, -20.0, 30.0, 40.0), BLUE);
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.rect_vertices.len(), 6);
        assert_position_near(prepared.rect_vertices[0].position, [-1.0, 1.0]);
        assert_position_near(prepared.rect_vertices[2].position, [-0.6, 0.6]);
    }

    #[test]
    fn item_clip_clips_fill_rect_vertices() {
        let frame = build_scene(vec![
            item(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 20.0, 20.0),
                color: BLUE,
            })
            .clipped(Rect::from_xywh(5.0, 5.0, 5.0, 5.0)),
        ]);
        let prepared = prepare_frame(&frame);

        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.rect_vertices.len(), 6);
        assert_position_near(prepared.rect_vertices[0].position, [-0.9, 0.9]);
        assert_position_near(prepared.rect_vertices[2].position, [-0.8, 0.8]);
    }

    #[test]
    fn empty_item_clip_emits_no_vertices() {
        let frame = build_scene(vec![
            item(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 20.0, 20.0),
                color: BLUE,
            })
            .clipped(Rect::from_xywh(5.0, 5.0, 0.0, 5.0)),
        ]);
        let prepared = prepare_frame(&frame);

        assert_eq!(prepared.rendered_primitives, 0);
        assert!(prepared.rect_vertices.is_empty());
    }

    #[test]
    fn stroke_rect_lowers_to_four_rectangles() {
        let mut scene = Scene::new();
        scene.stroke_rect(
            Rect::from_xywh(10.0, 10.0, 20.0, 20.0),
            Stroke::new(RED, 2.0),
        );
        let prepared = prepare_frame(&frame(scene));
        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.rect_vertices.len(), 24);
        assert!(
            prepared
                .rect_vertices
                .iter()
                .all(|vertex| vertex.color == RED.to_f32_rgba())
        );
    }

    #[test]
    fn stroke_rect_clip_can_reduce_visible_edges() {
        let frame = build_scene(vec![
            item(Primitive::StrokeRect {
                rect: Rect::from_xywh(10.0, 10.0, 20.0, 20.0),
                stroke: Stroke::new(RED, 2.0),
            })
            .clipped(Rect::from_xywh(10.0, 10.0, 20.0, 2.0)),
        ]);
        let prepared = prepare_frame(&frame);

        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.rect_vertices.len(), 6);
    }

    #[test]
    fn zero_width_strokes_emit_no_vertices() {
        let mut scene = Scene::new();
        scene.stroke_rect(
            Rect::from_xywh(10.0, 10.0, 20.0, 20.0),
            Stroke::new(RED, 0.0),
        );
        let prepared = prepare_frame(&frame(scene));
        assert_eq!(prepared.rendered_primitives, 0);
        assert!(prepared.rect_vertices.is_empty());
    }

    #[test]
    fn text_and_line_are_unsupported_for_prepared_gpu_draws() {
        let mut scene = Scene::new();
        scene.line(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Stroke::new(RED, 1.0),
        );
        scene.text(Point::new(0.0, 0.0), "hello", TextStyle::new(RED, 12.0));
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 0);
        assert_eq!(prepared.unsupported_primitives, 2);
        assert!(prepared.rect_vertices.is_empty());
    }
}
