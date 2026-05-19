use super::{
    Color, Coord, Frame, Point, Primitive, Rect, RenderCapabilities, RenderError, RenderResult,
    Renderer, Size,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalCell {
    pub ch: char,
    pub fg: Option<Color>,
    pub bg: Color,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: Color::Transparent,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalGrid {
    pub width: u16,
    pub height: u16,
    pub cells: Vec<TerminalCell>,
}

impl TerminalGrid {
    pub fn new(width: u16, height: u16) -> Self {
        let len = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            cells: vec![TerminalCell::default(); len],
        }
    }

    pub fn cell(&self, col: u16, row: u16) -> Option<TerminalCell> {
        self.index(col, row).map(|index| self.cells[index])
    }

    fn set_cell(&mut self, col: i32, row: i32, cell: TerminalCell) -> bool {
        if col < 0 || row < 0 {
            return false;
        }
        let Some(index) = self.index(col as u16, row as u16) else {
            return false;
        };
        self.cells[index] = cell;
        true
    }

    fn update_cell(&mut self, col: i32, row: i32, update: impl FnOnce(&mut TerminalCell)) -> bool {
        if col < 0 || row < 0 {
            return false;
        }
        let Some(index) = self.index(col as u16, row as u16) else {
            return false;
        };
        update(&mut self.cells[index]);
        true
    }

    fn index(&self, col: u16, row: u16) -> Option<usize> {
        if col >= self.width || row >= self.height {
            return None;
        }
        Some((row as usize * self.width as usize) + col as usize)
    }
}

#[derive(Debug)]
pub struct TerminalRenderer {
    grid: TerminalGrid,
}

impl TerminalRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            grid: TerminalGrid::new(width, height),
        }
    }

    pub fn grid(&self) -> &TerminalGrid {
        &self.grid
    }

    fn validate_target(&self, frame: &Frame) -> Result<(), RenderError> {
        if self.grid.width == 0 || self.grid.height == 0 {
            return Err(RenderError::new(
                "terminal grid dimensions must be non-zero",
            ));
        }
        if frame.size.width <= 0.0 || frame.size.height <= 0.0 {
            return Err(RenderError::new("frame dimensions must be positive"));
        }
        Ok(())
    }

    fn clear(&mut self, color: Color) {
        self.grid.cells.fill(TerminalCell {
            ch: ' ',
            fg: None,
            bg: color,
        });
    }

    fn effective_clip(&self, frame_size: Size, clip: Option<Rect>) -> Rect {
        let frame_rect = Rect::from_xywh(0.0, 0.0, frame_size.width, frame_size.height);
        match clip {
            Some(clip) => clip.intersect(&frame_rect),
            None => frame_rect,
        }
    }

    fn fill_rect(
        &mut self,
        frame_size: Size,
        rect: Rect,
        clip: Option<Rect>,
        color: Color,
    ) -> bool {
        let rect = rect.intersect(&self.effective_clip(frame_size, clip));
        if rect.is_empty() {
            return false;
        }

        let bounds = self.project_rect(frame_size, rect);
        let mut drew = false;
        for row in bounds.top..bounds.bottom {
            for col in bounds.left..bounds.right {
                drew |= self.grid.set_cell(
                    col,
                    row,
                    TerminalCell {
                        ch: ' ',
                        fg: None,
                        bg: color,
                    },
                );
            }
        }
        drew
    }

    fn stroke_rect(
        &mut self,
        frame_size: Size,
        rect: Rect,
        clip: Option<Rect>,
        color: Color,
        width: Coord,
    ) -> bool {
        if rect.is_empty() || width <= 0.0 {
            return false;
        }
        let clip = self.project_rect(frame_size, self.effective_clip(frame_size, clip));
        if clip.is_empty() {
            return false;
        }

        let bounds = self.project_rect_unclamped(frame_size, rect);
        if bounds.is_empty() {
            return false;
        }

        let left = bounds.left;
        let right = bounds.right - 1;
        let top = bounds.top;
        let bottom = bounds.bottom - 1;

        let mut drew = false;
        for col in left..=right {
            drew |= self.write_glyph(col, top, '-', color, clip);
            drew |= self.write_glyph(col, bottom, '-', color, clip);
        }
        for row in top..=bottom {
            drew |= self.write_glyph(left, row, '|', color, clip);
            drew |= self.write_glyph(right, row, '|', color, clip);
        }
        drew |= self.write_glyph(left, top, '+', color, clip);
        drew |= self.write_glyph(right, top, '+', color, clip);
        drew |= self.write_glyph(left, bottom, '+', color, clip);
        drew |= self.write_glyph(right, bottom, '+', color, clip);
        drew
    }

    fn text(
        &mut self,
        frame_size: Size,
        position: Point,
        text: &str,
        clip: Option<Rect>,
        color: Color,
    ) -> bool {
        let clip = self.project_rect(frame_size, self.effective_clip(frame_size, clip));
        if clip.is_empty() {
            return false;
        }

        let (mut col, row) = self.project_point_floor(frame_size, position);
        let mut drew = false;
        for ch in text.chars() {
            if ch == '\n' {
                break;
            }
            drew |= self.write_glyph(col, row, ch, color, clip);
            col += 1;
        }
        drew
    }

    fn line(
        &mut self,
        frame_size: Size,
        from: Point,
        to: Point,
        clip: Option<Rect>,
        color: Color,
    ) -> LineResult {
        let clip = self.project_rect(frame_size, self.effective_clip(frame_size, clip));
        if clip.is_empty() {
            return LineResult::Rendered(false);
        }

        let (from_col, from_row) = self.project_point_floor(frame_size, from);
        let (to_col, to_row) = self.project_point_floor(frame_size, to);

        if from.y == to.y {
            let left = from_col.min(to_col);
            let right = from_col.max(to_col);
            let mut drew = false;
            for col in left..=right {
                drew |= self.write_glyph(col, from_row, '-', color, clip);
            }
            LineResult::Rendered(drew)
        } else if from.x == to.x {
            let top = from_row.min(to_row);
            let bottom = from_row.max(to_row);
            let mut drew = false;
            for row in top..=bottom {
                drew |= self.write_glyph(from_col, row, '|', color, clip);
            }
            LineResult::Rendered(drew)
        } else {
            LineResult::Unsupported
        }
    }

    fn write_glyph(
        &mut self,
        col: i32,
        row: i32,
        ch: char,
        color: Color,
        clip: TerminalRect,
    ) -> bool {
        if !clip.contains(col, row) {
            return false;
        }
        self.grid.update_cell(col, row, |cell| {
            cell.ch = ch;
            cell.fg = Some(color);
        })
    }

    fn project_point_floor(&self, frame_size: Size, point: Point) -> (i32, i32) {
        (
            ((point.x * self.grid.width as Coord) / frame_size.width).floor() as i32,
            ((point.y * self.grid.height as Coord) / frame_size.height).floor() as i32,
        )
    }

    fn project_rect(&self, frame_size: Size, rect: Rect) -> TerminalRect {
        self.project_rect_unclamped(frame_size, rect)
            .clamp_to_grid(self.grid.width, self.grid.height)
    }

    fn project_rect_unclamped(&self, frame_size: Size, rect: Rect) -> TerminalRect {
        TerminalRect {
            left: ((rect.left() * self.grid.width as Coord) / frame_size.width).floor() as i32,
            top: ((rect.top() * self.grid.height as Coord) / frame_size.height).floor() as i32,
            right: ((rect.right() * self.grid.width as Coord) / frame_size.width).ceil() as i32,
            bottom: ((rect.bottom() * self.grid.height as Coord) / frame_size.height).ceil() as i32,
        }
    }
}

impl Renderer for TerminalRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities {
            fills: true,
            strokes: true,
            lines: true,
            text: true,
        }
    }

    fn render(&mut self, frame: &Frame) -> Result<RenderResult, RenderError> {
        self.validate_target(frame)?;
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
                    if self.fill_rect(frame.size, *rect, item.clip, *color) {
                        result.rendered_primitives += 1;
                    }
                }
                Primitive::StrokeRect { rect, stroke } => {
                    if self.stroke_rect(frame.size, *rect, item.clip, stroke.color, stroke.width) {
                        result.rendered_primitives += 1;
                    }
                }
                Primitive::Line { from, to, stroke } => {
                    match self.line(frame.size, *from, *to, item.clip, stroke.color) {
                        LineResult::Rendered(true) => result.rendered_primitives += 1,
                        LineResult::Rendered(false) => {}
                        LineResult::Unsupported => result.unsupported_primitives += 1,
                    }
                }
                Primitive::Text {
                    position,
                    text,
                    style,
                } => {
                    if self.text(frame.size, *position, text, item.clip, style.color) {
                        result.rendered_primitives += 1;
                    }
                }
            }
        }

        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineResult {
    Rendered(bool),
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl TerminalRect {
    fn is_empty(&self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    fn contains(&self, col: i32, row: i32) -> bool {
        !self.is_empty()
            && col >= self.left
            && col < self.right
            && row >= self.top
            && row < self.bottom
    }

    fn clamp_to_grid(self, width: u16, height: u16) -> Self {
        Self {
            left: self.left.clamp(0, width as i32),
            top: self.top.clamp(0, height as i32),
            right: self.right.clamp(0, width as i32),
            bottom: self.bottom.clamp(0, height as i32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Layer, Scene, SceneItem, Stroke, TextStyle};

    const RED: Color = Color::rgb(255, 0, 0);
    const BLUE: Color = Color::rgb(0, 0, 255);
    const GREEN: Color = Color::rgb(0, 255, 0);
    const WHITE: Color = Color::WHITE;

    fn render_scene(scene: Scene, width: u16, height: u16) -> TerminalRenderer {
        let mut renderer = TerminalRenderer::new(width, height);
        renderer
            .render(&Frame::new(
                Size::new(width as Coord, height as Coord),
                scene,
            ))
            .unwrap();
        renderer
    }

    fn render_scaled_scene(
        scene: Scene,
        frame_size: Size,
        width: u16,
        height: u16,
    ) -> TerminalRenderer {
        let mut renderer = TerminalRenderer::new(width, height);
        renderer.render(&Frame::new(frame_size, scene)).unwrap();
        renderer
    }

    fn cell(renderer: &TerminalRenderer, col: u16, row: u16) -> TerminalCell {
        renderer.grid().cell(col, row).unwrap()
    }

    #[test]
    fn terminal_grid_allocates_default_cells() {
        let grid = TerminalGrid::new(3, 2);
        assert_eq!(grid.width, 3);
        assert_eq!(grid.height, 2);
        assert_eq!(grid.cells, vec![TerminalCell::default(); 6]);
    }

    #[test]
    fn terminal_grid_indexes_cells_by_column_and_row() {
        let mut grid = TerminalGrid::new(3, 2);
        grid.set_cell(
            2,
            1,
            TerminalCell {
                ch: 'x',
                fg: Some(RED),
                bg: BLUE,
            },
        );

        assert_eq!(grid.cell(2, 1).unwrap().ch, 'x');
        assert_eq!(grid.cell(3, 1), None);
        assert_eq!(grid.cell(2, 2), None);
    }

    #[test]
    fn clear_fills_all_cells() {
        let mut scene = Scene::new();
        scene.clear_color(BLUE);
        let renderer = render_scene(scene, 3, 2);

        assert!(renderer.grid().cells.iter().all(|cell| {
            *cell
                == TerminalCell {
                    ch: ' ',
                    fg: None,
                    bg: BLUE,
                }
        }));
    }

    #[test]
    fn fill_rect_projects_logical_rect_to_cells() {
        let mut scene = Scene::new();
        scene.clear_color(Color::Transparent);
        scene.fill_rect(Rect::from_xywh(2.5, 1.0, 5.0, 2.0), RED);
        let renderer = render_scaled_scene(scene, Size::new(10.0, 4.0), 4, 4);

        assert_eq!(cell(&renderer, 0, 1).bg, Color::Transparent);
        assert_eq!(cell(&renderer, 1, 1).bg, RED);
        assert_eq!(cell(&renderer, 2, 2).bg, RED);
        assert_eq!(cell(&renderer, 3, 1).bg, Color::Transparent);
    }

    #[test]
    fn fill_rect_clips_to_frame_bounds() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(-2.0, -2.0, 4.0, 4.0), BLUE);
        let renderer = render_scene(scene, 4, 4);

        assert_eq!(cell(&renderer, 0, 0).bg, BLUE);
        assert_eq!(cell(&renderer, 1, 1).bg, BLUE);
        assert_eq!(cell(&renderer, 2, 2).bg, Color::Transparent);
    }

    #[test]
    fn fill_rect_respects_item_clip() {
        let mut scene = Scene::new();
        scene.push_item(
            SceneItem::new(Primitive::FillRect {
                rect: Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
                color: BLUE,
            })
            .clipped(Rect::from_xywh(1.0, 1.0, 1.0, 1.0)),
        );
        let renderer = render_scene(scene, 4, 4);

        assert_eq!(cell(&renderer, 0, 0).bg, Color::Transparent);
        assert_eq!(cell(&renderer, 1, 1).bg, BLUE);
        assert_eq!(cell(&renderer, 2, 1).bg, Color::Transparent);
    }

    #[test]
    fn higher_layers_overwrite_lower_layers() {
        let mut scene = Scene::new();
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

        assert_eq!(cell(&renderer, 0, 0).bg, BLUE);
        assert_eq!(cell(&renderer, 1, 1).bg, GREEN);
    }

    #[test]
    fn same_layer_order_is_preserved() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), BLUE);
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), GREEN);
        let renderer = render_scene(scene, 2, 2);

        assert_eq!(cell(&renderer, 0, 0).bg, GREEN);
    }

    #[test]
    fn fill_rect_occludes_existing_text() {
        let mut scene = Scene::new();
        scene.text(Point::new(0.0, 0.0), "x", TextStyle::new(WHITE, 12.0));
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), BLUE);
        let renderer = render_scene(scene, 2, 2);

        assert_eq!(cell(&renderer, 0, 0).ch, ' ');
        assert_eq!(cell(&renderer, 0, 0).fg, None);
        assert_eq!(cell(&renderer, 0, 0).bg, BLUE);
    }

    #[test]
    fn stroke_rect_draws_ascii_border() {
        let mut scene = Scene::new();
        scene.stroke_rect(Rect::from_xywh(1.0, 1.0, 3.0, 3.0), Stroke::new(RED, 1.0));
        let renderer = render_scene(scene, 5, 5);

        assert_eq!(cell(&renderer, 1, 1).ch, '+');
        assert_eq!(cell(&renderer, 2, 1).ch, '-');
        assert_eq!(cell(&renderer, 1, 2).ch, '|');
        assert_eq!(cell(&renderer, 2, 2).ch, ' ');
        assert_eq!(cell(&renderer, 3, 3).ch, '+');
    }

    #[test]
    fn stroke_rect_respects_item_clip() {
        let mut scene = Scene::new();
        scene.push_item(
            SceneItem::new(Primitive::StrokeRect {
                rect: Rect::from_xywh(1.0, 1.0, 3.0, 3.0),
                stroke: Stroke::new(RED, 1.0),
            })
            .clipped(Rect::from_xywh(1.0, 1.0, 3.0, 1.0)),
        );
        let renderer = render_scene(scene, 5, 5);

        assert_eq!(cell(&renderer, 1, 1).ch, '+');
        assert_eq!(cell(&renderer, 2, 1).ch, '-');
        assert_eq!(cell(&renderer, 1, 2).ch, ' ');
    }

    #[test]
    fn text_draws_at_projected_position() {
        let mut scene = Scene::new();
        scene.text(Point::new(2.5, 1.0), "hi", TextStyle::new(RED, 12.0));
        let renderer = render_scaled_scene(scene, Size::new(10.0, 4.0), 4, 4);

        assert_eq!(cell(&renderer, 1, 1).ch, 'h');
        assert_eq!(cell(&renderer, 2, 1).ch, 'i');
        assert_eq!(cell(&renderer, 1, 1).fg, Some(RED));
    }

    #[test]
    fn text_preserves_existing_background() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), BLUE);
        scene.text(Point::new(0.0, 0.0), "x", TextStyle::new(WHITE, 12.0));
        let renderer = render_scene(scene, 2, 2);

        assert_eq!(cell(&renderer, 0, 0).ch, 'x');
        assert_eq!(cell(&renderer, 0, 0).fg, Some(WHITE));
        assert_eq!(cell(&renderer, 0, 0).bg, BLUE);
    }

    #[test]
    fn text_respects_item_clip() {
        let mut scene = Scene::new();
        scene.push_item(
            SceneItem::new(Primitive::Text {
                position: Point::new(0.0, 0.0),
                text: "abc".to_string(),
                style: TextStyle::new(WHITE, 12.0),
            })
            .clipped(Rect::from_xywh(1.0, 0.0, 1.0, 1.0)),
        );
        let renderer = render_scene(scene, 4, 2);

        assert_eq!(cell(&renderer, 0, 0).ch, ' ');
        assert_eq!(cell(&renderer, 1, 0).ch, 'b');
        assert_eq!(cell(&renderer, 2, 0).ch, ' ');
    }

    #[test]
    fn horizontal_line_draws_ascii_cells() {
        let mut scene = Scene::new();
        scene.line(
            Point::new(1.0, 2.0),
            Point::new(3.0, 2.0),
            Stroke::new(RED, 1.0),
        );
        let renderer = render_scene(scene, 5, 5);

        assert_eq!(cell(&renderer, 1, 2).ch, '-');
        assert_eq!(cell(&renderer, 2, 2).ch, '-');
        assert_eq!(cell(&renderer, 3, 2).ch, '-');
    }

    #[test]
    fn vertical_line_draws_ascii_cells() {
        let mut scene = Scene::new();
        scene.line(
            Point::new(2.0, 1.0),
            Point::new(2.0, 3.0),
            Stroke::new(RED, 1.0),
        );
        let renderer = render_scene(scene, 5, 5);

        assert_eq!(cell(&renderer, 2, 1).ch, '|');
        assert_eq!(cell(&renderer, 2, 2).ch, '|');
        assert_eq!(cell(&renderer, 2, 3).ch, '|');
    }

    #[test]
    fn diagonal_line_is_unsupported() {
        let mut scene = Scene::new();
        scene.line(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Stroke::new(RED, 1.0),
        );
        let mut renderer = TerminalRenderer::new(4, 4);
        let result = renderer
            .render(&Frame::new(Size::new(4.0, 4.0), scene))
            .unwrap();

        assert_eq!(
            result,
            RenderResult {
                rendered_primitives: 0,
                unsupported_primitives: 1,
            }
        );
    }

    #[test]
    fn zero_sized_grid_returns_render_error() {
        let mut renderer = TerminalRenderer::new(0, 1);
        let frame = Frame::new(Size::new(1.0, 1.0), Scene::new());

        assert_eq!(
            renderer.render(&frame).unwrap_err().message(),
            "terminal grid dimensions must be non-zero"
        );
    }

    #[test]
    fn non_positive_frame_size_returns_render_error() {
        let mut renderer = TerminalRenderer::new(1, 1);
        let frame = Frame::new(Size::new(0.0, 1.0), Scene::new());

        assert_eq!(
            renderer.render(&frame).unwrap_err().message(),
            "frame dimensions must be positive"
        );
    }
}
