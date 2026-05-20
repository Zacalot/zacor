use crate::text::{LaidOutGlyph, TextSystem};

use super::{Color, Frame, Point, Primitive, Rect, Size, Stroke, SurfaceFallback};

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedFrame {
    pub size: Size,
    pub clear_color: Option<Color>,
    pub rect_vertices: Vec<PreparedRectVertex>,
    pub text_runs: Vec<PreparedTextRun>,
    pub draws: Vec<PreparedDraw>,
    pub rendered_primitives: usize,
    pub unsupported_primitives: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedRectVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedDraw {
    Rects { start: usize, count: usize },
    Text { index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedTextGlyph {
    pub glyph: LaidOutGlyph,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedTextRun {
    pub glyphs: Vec<PreparedTextGlyph>,
}

pub fn prepare_frame(frame: &Frame) -> PreparedFrame {
    let mut prepared = PreparedFrame {
        size: frame.size,
        clear_color: None,
        rect_vertices: Vec::new(),
        text_runs: Vec::new(),
        draws: Vec::new(),
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
                let start = prepared.rect_vertices.len();
                if push_fill_rect(
                    &mut prepared.rect_vertices,
                    frame.size,
                    *rect,
                    item.clip,
                    *color,
                ) {
                    prepared.rendered_primitives += 1;
                    prepared.draws.push(PreparedDraw::Rects {
                        start,
                        count: prepared.rect_vertices.len() - start,
                    });
                }
            }
            Primitive::StrokeRect { rect, stroke } => {
                let start = prepared.rect_vertices.len();
                if push_stroke_rect(
                    &mut prepared.rect_vertices,
                    frame.size,
                    *rect,
                    item.clip,
                    *stroke,
                ) {
                    prepared.rendered_primitives += 1;
                    prepared.draws.push(PreparedDraw::Rects {
                        start,
                        count: prepared.rect_vertices.len() - start,
                    });
                }
            }
            Primitive::SurfaceSlot { rect, fallback, .. } => match fallback {
                SurfaceFallback::None => {
                    prepared.unsupported_primitives += 1;
                }
                SurfaceFallback::FillRect { color } => {
                    let start = prepared.rect_vertices.len();
                    if push_fill_rect(
                        &mut prepared.rect_vertices,
                        frame.size,
                        *rect,
                        item.clip,
                        *color,
                    ) {
                        prepared.rendered_primitives += 1;
                        prepared.draws.push(PreparedDraw::Rects {
                            start,
                            count: prepared.rect_vertices.len() - start,
                        });
                    }
                }
                SurfaceFallback::StrokeRect { stroke } => {
                    let start = prepared.rect_vertices.len();
                    if push_stroke_rect(
                        &mut prepared.rect_vertices,
                        frame.size,
                        *rect,
                        item.clip,
                        *stroke,
                    ) {
                        prepared.rendered_primitives += 1;
                        prepared.draws.push(PreparedDraw::Rects {
                            start,
                            count: prepared.rect_vertices.len() - start,
                        });
                    }
                }
                SurfaceFallback::Text { .. } => {
                    prepared.unsupported_primitives += 1;
                }
            },
            Primitive::Line { .. } | Primitive::Text { .. } => {
                prepared.unsupported_primitives += 1;
            }
        }
    }

    prepared
}

pub fn prepare_frame_with_text(frame: &Frame, text_system: &mut TextSystem) -> PreparedFrame {
    let mut prepared = PreparedFrame {
        size: frame.size,
        clear_color: None,
        rect_vertices: Vec::new(),
        text_runs: Vec::new(),
        draws: Vec::new(),
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
                let start = prepared.rect_vertices.len();
                if push_fill_rect(
                    &mut prepared.rect_vertices,
                    frame.size,
                    *rect,
                    item.clip,
                    *color,
                ) {
                    prepared.rendered_primitives += 1;
                    prepared.draws.push(PreparedDraw::Rects {
                        start,
                        count: prepared.rect_vertices.len() - start,
                    });
                }
            }
            Primitive::StrokeRect { rect, stroke } => {
                let start = prepared.rect_vertices.len();
                if push_stroke_rect(
                    &mut prepared.rect_vertices,
                    frame.size,
                    *rect,
                    item.clip,
                    *stroke,
                ) {
                    prepared.rendered_primitives += 1;
                    prepared.draws.push(PreparedDraw::Rects {
                        start,
                        count: prepared.rect_vertices.len() - start,
                    });
                }
            }
            Primitive::Text {
                position,
                text,
                style,
            } => {
                let layout = text_system.layout_line(*position, text, *style);
                if layout.glyphs.is_empty() {
                    continue;
                }

                let run_index = prepared.text_runs.len();
                prepared.text_runs.push(PreparedTextRun {
                    glyphs: layout
                        .glyphs
                        .into_iter()
                        .filter(|glyph| glyph_visible(text_system, glyph, item.clip, frame.size))
                        .map(|glyph| PreparedTextGlyph { glyph })
                        .collect(),
                });

                if !prepared.text_runs[run_index].glyphs.is_empty() {
                    prepared.rendered_primitives += 1;
                    prepared.draws.push(PreparedDraw::Text { index: run_index });
                }
            }
            Primitive::SurfaceSlot { rect, fallback, .. } => match fallback {
                SurfaceFallback::None => {
                    prepared.unsupported_primitives += 1;
                }
                SurfaceFallback::FillRect { color } => {
                    let start = prepared.rect_vertices.len();
                    if push_fill_rect(
                        &mut prepared.rect_vertices,
                        frame.size,
                        *rect,
                        item.clip,
                        *color,
                    ) {
                        prepared.rendered_primitives += 1;
                        prepared.draws.push(PreparedDraw::Rects {
                            start,
                            count: prepared.rect_vertices.len() - start,
                        });
                    }
                }
                SurfaceFallback::StrokeRect { stroke } => {
                    let start = prepared.rect_vertices.len();
                    if push_stroke_rect(
                        &mut prepared.rect_vertices,
                        frame.size,
                        *rect,
                        item.clip,
                        *stroke,
                    ) {
                        prepared.rendered_primitives += 1;
                        prepared.draws.push(PreparedDraw::Rects {
                            start,
                            count: prepared.rect_vertices.len() - start,
                        });
                    }
                }
                SurfaceFallback::Text {
                    position,
                    text,
                    style,
                } => {
                    let layout = text_system.layout_line(*position, text, *style);
                    if layout.glyphs.is_empty() {
                        continue;
                    }

                    let run_index = prepared.text_runs.len();
                    prepared.text_runs.push(PreparedTextRun {
                        glyphs: layout
                            .glyphs
                            .into_iter()
                            .filter(|glyph| {
                                glyph_visible(text_system, glyph, item.clip, frame.size)
                            })
                            .map(|glyph| PreparedTextGlyph { glyph })
                            .collect(),
                    });

                    if !prepared.text_runs[run_index].glyphs.is_empty() {
                        prepared.rendered_primitives += 1;
                        prepared.draws.push(PreparedDraw::Text { index: run_index });
                    }
                }
            },
            Primitive::Line { .. } => {
                prepared.unsupported_primitives += 1;
            }
        }
    }

    prepared
}

fn glyph_visible(
    text_system: &mut TextSystem,
    glyph: &LaidOutGlyph,
    clip: Option<Rect>,
    frame_size: Size,
) -> bool {
    let Some(image) = text_system.rasterize_glyph(glyph.key) else {
        return false;
    };

    let bounds = Rect::from_xywh(
        (glyph.x + image.left) as f32,
        (glyph.y - image.top) as f32,
        image.width as f32,
        image.height as f32,
    );

    !bounds
        .intersect(&effective_clip(frame_size, clip))
        .is_empty()
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
    use crate::render::{Layer, Scene, SurfaceKind, SurfaceSlotId, TextStyle};
    use crate::text::TextSystem;

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

    #[test]
    fn prepare_frame_with_text_lowers_text_runs() {
        let mut scene = Scene::new();
        scene.text(Point::new(4.0, 8.0), "hello", TextStyle::new(RED, 12.0));
        let mut text_system = TextSystem::new();

        let prepared = prepare_frame_with_text(&frame(scene), &mut text_system);

        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.unsupported_primitives, 0);
        assert_eq!(prepared.text_runs.len(), 1);
        assert!(!prepared.text_runs[0].glyphs.is_empty());
        assert_eq!(prepared.draws, vec![PreparedDraw::Text { index: 0 }]);
    }

    #[test]
    fn prepare_frame_with_text_preserves_draw_order_between_rects() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), RED);
        scene.push_item(
            item(Primitive::Text {
                position: Point::new(2.0, 8.0),
                text: "x".to_string(),
                style: TextStyle::new(GREEN, 12.0),
            })
            .layered(Layer(1)),
        );
        scene.push_item(
            item(Primitive::FillRect {
                rect: Rect::from_xywh(2.0, 2.0, 4.0, 4.0),
                color: BLUE,
            })
            .layered(Layer(2)),
        );
        let mut text_system = TextSystem::new();

        let prepared = prepare_frame_with_text(&frame(scene), &mut text_system);

        assert_eq!(
            prepared.draws,
            vec![
                PreparedDraw::Rects { start: 0, count: 6 },
                PreparedDraw::Text { index: 0 },
                PreparedDraw::Rects { start: 6, count: 6 },
            ]
        );
    }

    #[test]
    fn surface_slot_fill_fallback_lowers_to_rect_vertices() {
        let mut scene = Scene::new();
        scene.surface_slot(
            SurfaceSlotId(1),
            Rect::from_xywh(10.0, 10.0, 20.0, 20.0),
            SurfaceKind::Canvas,
            SurfaceFallback::FillRect { color: RED },
        );
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 1);
        assert_eq!(prepared.unsupported_primitives, 0);
        assert_eq!(prepared.rect_vertices.len(), 6);
    }

    #[test]
    fn surface_slot_none_fallback_is_unsupported() {
        let mut scene = Scene::new();
        scene.surface_slot(
            SurfaceSlotId(1),
            Rect::from_xywh(10.0, 10.0, 20.0, 20.0),
            SurfaceKind::Browser,
            SurfaceFallback::None,
        );
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 0);
        assert_eq!(prepared.unsupported_primitives, 1);
    }

    #[test]
    fn surface_slot_text_fallback_is_unsupported_in_prepared_gpu_draws() {
        let mut scene = Scene::new();
        scene.surface_slot(
            SurfaceSlotId(1),
            Rect::from_xywh(10.0, 10.0, 20.0, 20.0),
            SurfaceKind::Terminal,
            SurfaceFallback::Text {
                position: Point::new(12.0, 14.0),
                text: "slot".to_string(),
                style: TextStyle::new(RED, 12.0),
            },
        );
        let prepared = prepare_frame(&frame(scene));

        assert_eq!(prepared.rendered_primitives, 0);
        assert_eq!(prepared.unsupported_primitives, 1);
    }
}
