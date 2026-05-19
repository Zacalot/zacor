use super::{
    Color, Coord, Frame, Point, Primitive, Rect, RenderCapabilities, RenderError, RenderResult,
    Renderer, SurfaceFallback,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
}

impl RasterImage {
    pub fn new(width: u32, height: u32, color: Color) -> Self {
        let len = width.saturating_mul(height) as usize;
        Self {
            width,
            height,
            pixels: vec![color; len],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        self.index(x, y).map(|index| self.pixels[index])
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color) -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        let Some(index) = self.index(x as u32, y as u32) else {
            return false;
        };
        self.pixels[index] = color;
        true
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some((y * self.width + x) as usize)
    }
}

fn snap(value: Coord) -> i32 {
    value.floor() as i32
}

#[derive(Debug)]
pub struct RasterRenderer {
    image: RasterImage,
}

impl RasterRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            image: RasterImage::new(width, height, Color::Transparent),
        }
    }

    pub fn image(&self) -> &RasterImage {
        &self.image
    }

    fn clear(&mut self, color: Color) {
        self.image.pixels.fill(color);
    }

    fn effective_clip(&self, clip: Option<Rect>) -> Rect {
        let image_rect = Rect::from_xywh(
            0.0,
            0.0,
            self.image.width as Coord,
            self.image.height as Coord,
        );
        match clip {
            Some(clip) => clip.intersect(&image_rect),
            None => image_rect,
        }
    }

    fn fill_rect(&mut self, rect: Rect, clip: Option<Rect>, color: Color) -> bool {
        let rect = rect.intersect(&self.effective_clip(clip));
        if rect.is_empty() {
            return false;
        }

        let left = snap(rect.left());
        let top = snap(rect.top());
        let right = snap(rect.right());
        let bottom = snap(rect.bottom());
        let mut drew = false;
        for y in top..bottom {
            for x in left..right {
                drew |= self.image.set_pixel(x, y, color);
            }
        }
        drew
    }

    fn stroke_rect(&mut self, rect: Rect, clip: Option<Rect>, color: Color) -> bool {
        if rect.is_empty() {
            return false;
        }
        let left = snap(rect.left());
        let top = snap(rect.top());
        let right = snap(rect.right()) - 1;
        let bottom = snap(rect.bottom()) - 1;
        let clip = self.effective_clip(clip);
        if clip.is_empty() {
            return false;
        }

        let mut drew = false;
        for x in left..=right {
            drew |= self.set_pixel_clipped(x, top, color, clip);
            drew |= self.set_pixel_clipped(x, bottom, color, clip);
        }
        for y in top..=bottom {
            drew |= self.set_pixel_clipped(left, y, color, clip);
            drew |= self.set_pixel_clipped(right, y, color, clip);
        }
        drew
    }

    fn line(&mut self, from: Point, to: Point, clip: Option<Rect>, color: Color) -> bool {
        let clip = self.effective_clip(clip);
        if clip.is_empty() {
            return false;
        }

        let mut x0 = snap(from.x);
        let mut y0 = snap(from.y);
        let x1 = snap(to.x);
        let y1 = snap(to.y);

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut drew = false;

        loop {
            drew |= self.set_pixel_clipped(x0, y0, color, clip);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
        drew
    }

    fn set_pixel_clipped(&mut self, x: i32, y: i32, color: Color, clip: Rect) -> bool {
        if !clip.contains(Point::new(x as Coord, y as Coord)) {
            return false;
        }
        self.image.set_pixel(x, y, color)
    }
}

impl Renderer for RasterRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities {
            fills: true,
            strokes: true,
            lines: true,
            text: false,
        }
    }

    fn render(&mut self, frame: &Frame) -> Result<RenderResult, RenderError> {
        let mut result = RenderResult::default();

        for item in frame.scene.items() {
            if let Primitive::Clear { color } = &item.primitive {
                self.clear(*color);
                result.rendered_primitives += 1;
            }
        }

        for item in frame.scene.items_in_paint_order() {
            match &item.primitive {
                Primitive::Clear { .. } => {}
                Primitive::FillRect { rect, color } => {
                    if self.fill_rect(*rect, item.clip, *color) {
                        result.rendered_primitives += 1;
                    }
                }
                Primitive::StrokeRect { rect, stroke } => {
                    if self.stroke_rect(*rect, item.clip, stroke.color) {
                        result.rendered_primitives += 1;
                    }
                }
                Primitive::Line { from, to, stroke } => {
                    if self.line(*from, *to, item.clip, stroke.color) {
                        result.rendered_primitives += 1;
                    }
                }
                Primitive::SurfaceSlot { rect, fallback, .. } => match fallback {
                    SurfaceFallback::None => {
                        result.unsupported_primitives += 1;
                    }
                    SurfaceFallback::FillRect { color } => {
                        if self.fill_rect(*rect, item.clip, *color) {
                            result.rendered_primitives += 1;
                        }
                    }
                    SurfaceFallback::StrokeRect { stroke } => {
                        if self.stroke_rect(*rect, item.clip, stroke.color) {
                            result.rendered_primitives += 1;
                        }
                    }
                    SurfaceFallback::Text { .. } => {
                        result.unsupported_primitives += 1;
                    }
                },
                Primitive::Text { .. } => {
                    result.unsupported_primitives += 1;
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{
        Layer, Scene, SceneItem, Size, Stroke, SurfaceKind, SurfaceSlotId, TextStyle,
    };

    const RED: Color = Color::rgb(255, 0, 0);
    const BLUE: Color = Color::rgb(0, 0, 255);
    const GREEN: Color = Color::rgb(0, 255, 0);

    fn render_scene(scene: Scene, width: u32, height: u32) -> RasterRenderer {
        let mut renderer = RasterRenderer::new(width, height);
        renderer
            .render(&Frame::new(
                Size::new(width as Coord, height as Coord),
                scene,
            ))
            .unwrap();
        renderer
    }

    #[test]
    fn clear_fills_whole_image() {
        let mut scene = Scene::new();
        scene.clear_color(RED);
        let renderer = render_scene(scene, 3, 2);
        assert!(renderer.image().pixels.iter().all(|pixel| *pixel == RED));
    }

    #[test]
    fn fill_rect_changes_expected_pixels_and_clips_to_frame() {
        let mut scene = Scene::new();
        scene.clear_color(Color::Transparent);
        scene.fill_rect(Rect::from_xywh(1.0, 1.0, 4.0, 4.0), BLUE);
        let renderer = render_scene(scene, 3, 3);

        assert_eq!(renderer.image().pixel(0, 0), Some(Color::Transparent));
        assert_eq!(renderer.image().pixel(1, 1), Some(BLUE));
        assert_eq!(renderer.image().pixel(2, 2), Some(BLUE));
    }

    #[test]
    fn fill_rect_respects_item_clip() {
        let mut scene = Scene::new();
        scene.clear_color(Color::Transparent);
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
                color: BLUE,
            })
            .clipped(Rect::from_xywh(1.0, 1.0, 1.0, 1.0)),
        );
        let renderer = render_scene(scene, 4, 4);

        assert_eq!(renderer.image().pixel(0, 0), Some(Color::Transparent));
        assert_eq!(renderer.image().pixel(1, 1), Some(BLUE));
        assert_eq!(renderer.image().pixel(2, 2), Some(Color::Transparent));
    }

    #[test]
    fn higher_layers_draw_over_lower_layers() {
        let mut scene = Scene::new();
        scene.clear_color(Color::Transparent);
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
                color: BLUE,
            })
            .layered(Layer(0)),
        );
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(1.0, 1.0, 2.0, 2.0),
                color: GREEN,
            })
            .layered(Layer(5)),
        );
        let renderer = render_scene(scene, 4, 4);

        assert_eq!(renderer.image().pixel(0, 0), Some(BLUE));
        assert_eq!(renderer.image().pixel(1, 1), Some(GREEN));
    }

    #[test]
    fn stroke_rect_draws_border_pixels() {
        let mut scene = Scene::new();
        scene.stroke_rect(Rect::from_xywh(1.0, 1.0, 3.0, 3.0), Stroke::new(RED, 1.0));
        let renderer = render_scene(scene, 5, 5);

        assert_eq!(renderer.image().pixel(1, 1), Some(RED));
        assert_eq!(renderer.image().pixel(2, 2), Some(Color::Transparent));
        assert_eq!(renderer.image().pixel(3, 3), Some(RED));
    }

    #[test]
    fn line_draws_deterministic_pixels() {
        let mut scene = Scene::new();
        scene.line(
            Point::new(0.0, 0.0),
            Point::new(3.0, 3.0),
            Stroke::new(RED, 1.0),
        );
        let renderer = render_scene(scene, 4, 4);

        for i in 0..4 {
            assert_eq!(renderer.image().pixel(i, i), Some(RED));
        }
    }

    #[test]
    fn line_respects_item_clip() {
        let mut scene = Scene::new();
        scene.push_item(
            SceneItem::new(Primitive::Line {
                from: Point::new(0.0, 0.0),
                to: Point::new(3.0, 3.0),
                stroke: Stroke::new(RED, 1.0),
            })
            .clipped(Rect::from_xywh(1.0, 1.0, 1.0, 1.0)),
        );
        let renderer = render_scene(scene, 4, 4);

        assert_eq!(renderer.image().pixel(0, 0), Some(Color::Transparent));
        assert_eq!(renderer.image().pixel(1, 1), Some(RED));
        assert_eq!(renderer.image().pixel(2, 2), Some(Color::Transparent));
    }

    #[test]
    fn text_is_explicitly_unsupported_for_now() {
        let mut scene = Scene::new();
        scene.text(Point::new(0.0, 0.0), "hello", TextStyle::new(RED, 12.0));
        let mut renderer = RasterRenderer::new(10, 10);
        assert_eq!(
            renderer
                .render(&Frame::new(Size::new(10.0, 10.0), scene))
                .unwrap(),
            RenderResult {
                rendered_primitives: 0,
                unsupported_primitives: 1,
            }
        );
    }

    #[test]
    fn surface_slot_fill_fallback_renders_pixels() {
        let mut scene = Scene::new();
        scene.surface_slot(
            SurfaceSlotId(1),
            Rect::from_xywh(1.0, 1.0, 2.0, 2.0),
            SurfaceKind::Canvas,
            SurfaceFallback::FillRect {
                color: Color::WHITE,
            },
        );
        let renderer = render_scene(scene, 4, 4);

        assert_eq!(renderer.image().pixel(1, 1), Some(Color::WHITE));
    }

    #[test]
    fn surface_slot_none_fallback_is_unsupported() {
        let mut scene = Scene::new();
        scene.surface_slot(
            SurfaceSlotId(1),
            Rect::from_xywh(1.0, 1.0, 2.0, 2.0),
            SurfaceKind::Browser,
            SurfaceFallback::None,
        );
        let mut renderer = RasterRenderer::new(4, 4);

        assert_eq!(
            renderer
                .render(&Frame::new(Size::new(4.0, 4.0), scene))
                .unwrap(),
            RenderResult {
                rendered_primitives: 0,
                unsupported_primitives: 1,
            }
        );
    }
}
