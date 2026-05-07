use super::scene::{Color, Rect, TextureImage, TextureSurface};
use crate::kernel::Selection;
use crate::session::{SessionPaneContentView, SessionPaneView, SessionTreeNodeView};

const CELL_WIDTH: u32 = 6;
const CELL_HEIGHT: u32 = 8;
const CELL_ADVANCE: u32 = 7;
const ROW_ADVANCE: u32 = 10;
const PADDING_X: u32 = 6;
const PADDING_Y: u32 = 6;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SurfaceHotspotAction {
    SelectRecordRow { row: usize, open: bool },
    SelectTreeNode { node_id: String, open: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceHotspot {
    pub rect: Rect,
    pub action: SurfaceHotspotAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneSurfaceOutput {
    pub texture: TextureSurface,
    pub hotspots: Vec<SurfaceHotspot>,
}

pub fn build_pane_surface(pane: &SessionPaneView, rect: Rect) -> Option<PaneSurfaceOutput> {
    if rect.width == 0 || rect.height == 0 {
        return None;
    }

    let (image, hotspots) = build_body_texture(pane, rect.width.max(1), rect.height.max(1));
    Some(PaneSurfaceOutput {
        texture: TextureSurface { rect, image },
        hotspots: hotspots
            .into_iter()
            .map(|hotspot| SurfaceHotspot {
                rect: Rect {
                    x: rect.x + hotspot.rect.x,
                    y: rect.y + hotspot.rect.y,
                    width: hotspot.rect.width,
                    height: hotspot.rect.height,
                },
                action: hotspot.action,
            })
            .collect(),
    })
}

fn build_body_texture(
    pane: &SessionPaneView,
    width: u32,
    height: u32,
) -> (TextureImage, Vec<SurfaceHotspot>) {
    match &pane.content {
        SessionPaneContentView::Text(lines) => render_text_surface(
            pane,
            width,
            height,
            lines
                .iter()
                .map(|line| RenderLine::plain(line.clone()))
                .collect(),
            Color::rgb(0.08, 0.09, 0.11),
            Color::rgb(0.86, 0.88, 0.92),
        ),
        SessionPaneContentView::Records(records) => {
            let lines = records_lines(pane, records);
            let hotspots = hotspot_rows(
                pane,
                lines
                    .iter()
                    .filter_map(|line| line.hotspot.clone())
                    .collect(),
                width,
                height,
            );
            let (image, _) = render_text_surface(
                pane,
                width,
                height,
                lines,
                Color::rgb(0.08, 0.09, 0.11),
                Color::rgb(0.8, 0.86, 0.96),
            );
            (image, hotspots)
        }
        SessionPaneContentView::Tree(roots) => {
            let lines = tree_lines(pane, roots);
            let hotspots = hotspot_rows(
                pane,
                lines
                    .iter()
                    .filter_map(|line| line.hotspot.clone())
                    .collect(),
                width,
                height,
            );
            let (image, _) = render_text_surface(
                pane,
                width,
                height,
                lines,
                Color::rgb(0.08, 0.09, 0.11),
                Color::rgb(0.82, 0.92, 0.82),
            );
            (image, hotspots)
        }
        SessionPaneContentView::Terminal { transcript } => {
            let lines = if transcript.is_empty() {
                vec![RenderLine::plain("[terminal buffer]".to_string())]
            } else {
                transcript.iter().cloned().map(RenderLine::plain).collect()
            };
            render_text_surface(
                pane,
                width,
                height,
                lines,
                Color::rgb(0.03, 0.06, 0.05),
                Color::rgb(0.64, 0.98, 0.76),
            )
        }
        SessionPaneContentView::Browser { url, title } => (
            render_browser_surface(pane, width, height, url.as_deref(), title.as_deref()),
            Vec::new(),
        ),
        SessionPaneContentView::Media { source } => (
            render_media_surface(pane, width, height, source.as_deref()),
            Vec::new(),
        ),
        SessionPaneContentView::Canvas { name } => (
            render_canvas_surface(pane, width, height, name.as_deref()),
            Vec::new(),
        ),
    }
}

fn render_text_surface(
    pane: &SessionPaneView,
    width: u32,
    height: u32,
    lines: Vec<RenderLine>,
    background: Color,
    foreground: Color,
) -> (TextureImage, Vec<SurfaceHotspot>) {
    let mut image = TextureImage::new(width, height, background);
    let visible = visible_line_count(height);
    let cols = visible_column_count(width);
    let start = pane.viewport.offset_y;
    for (visible_index, line) in lines.iter().skip(start).take(visible).enumerate() {
        let y = PADDING_Y + visible_index as u32 * ROW_ADVANCE;
        if line.selected {
            image.fill_rect(
                0,
                y.saturating_sub(1),
                width,
                ROW_ADVANCE.min(height.saturating_sub(y.saturating_sub(1))),
                Color::rgb(0.16, 0.24, 0.38),
            );
        }
        let text = clip_text(&line.text, pane.viewport.offset_x, cols);
        draw_text(&mut image, PADDING_X, y, &text, foreground);
    }

    if matches!(pane.selection, Some(Selection::Surface(_))) {
        image.stroke_rect(
            1,
            1,
            width.saturating_sub(2),
            height.saturating_sub(2),
            Color::rgb(1.0, 0.86, 0.38),
        );
    }

    (image, Vec::new())
}

fn render_browser_surface(
    pane: &SessionPaneView,
    width: u32,
    height: u32,
    url: Option<&str>,
    title: Option<&str>,
) -> TextureImage {
    let mut image = TextureImage::new(width, height, Color::rgb(0.92, 0.93, 0.96));
    let header_height = height.min(28);
    image.fill_rect(0, 0, width, header_height, Color::rgb(0.86, 0.88, 0.92));
    image.fill_circle(
        14,
        14.min(height.saturating_sub(1)),
        4,
        Color::rgb(0.94, 0.33, 0.31),
    );
    image.fill_circle(
        28,
        14.min(height.saturating_sub(1)),
        4,
        Color::rgb(0.96, 0.77, 0.27),
    );
    image.fill_circle(
        42,
        14.min(height.saturating_sub(1)),
        4,
        Color::rgb(0.35, 0.79, 0.39),
    );
    image.fill_rect(
        58,
        8,
        width.saturating_sub(66),
        12.min(height.saturating_sub(8)),
        Color::rgb(1.0, 1.0, 1.0),
    );
    image.fill_rect(
        10,
        40.min(height),
        width.saturating_sub(20),
        18.min(height.saturating_sub(40.min(height))),
        Color::rgb(1.0, 1.0, 1.0),
    );
    image.fill_rect(
        10,
        64.min(height),
        width.saturating_sub(20),
        height.saturating_sub(74.min(height)),
        Color::rgb(0.98, 0.99, 1.0),
    );

    let meta = [
        title.map(|title| format!("title: {title}")),
        url.map(|url| format!("url: {url}")),
    ]
    .into_iter()
    .flatten()
    .map(RenderLine::plain)
    .collect::<Vec<_>>();
    overlay_lines(&mut image, pane, meta, 12, 44, Color::rgb(0.18, 0.2, 0.25));
    image
}

fn render_media_surface(
    pane: &SessionPaneView,
    width: u32,
    height: u32,
    source: Option<&str>,
) -> TextureImage {
    let mut image = TextureImage::new(width, height, Color::rgb(0.02, 0.02, 0.03));
    image.fill_vertical_gradient(Color::rgb(0.05, 0.06, 0.08), Color::rgb(0.22, 0.24, 0.32));
    let frame_x = width / 10;
    let frame_y = height / 8;
    let frame_width = width.saturating_sub(frame_x * 2);
    let frame_height = height.saturating_sub(frame_y * 2).max(20);
    image.fill_rect(
        frame_x,
        frame_y,
        frame_width,
        frame_height,
        Color::rgb(0.1, 0.11, 0.14),
    );
    image.fill_rect(
        frame_x.saturating_add(4),
        frame_y.saturating_add(4),
        frame_width.saturating_sub(8),
        frame_height.saturating_sub(8),
        Color::rgb(0.2, 0.28, 0.42),
    );
    image.fill_triangle_play(
        frame_x.saturating_add(frame_width / 2).saturating_sub(10),
        frame_y.saturating_add(frame_height / 2).saturating_sub(14),
        28,
        Color::rgb(0.96, 0.96, 0.98),
    );
    let lines = vec![RenderLine::plain(
        source.unwrap_or("[media buffer]").to_string(),
    )];
    overlay_lines(
        &mut image,
        pane,
        lines,
        12,
        height.saturating_sub(20),
        Color::rgb(0.96, 0.96, 0.98),
    );
    image
}

fn render_canvas_surface(
    pane: &SessionPaneView,
    width: u32,
    height: u32,
    name: Option<&str>,
) -> TextureImage {
    let mut image = TextureImage::new(width, height, Color::rgb(0.08, 0.09, 0.12));
    image.fill_checker(Color::rgb(0.14, 0.16, 0.2), Color::rgb(0.1, 0.12, 0.16), 16);
    image.fill_circle(
        width.saturating_mul(3) / 4,
        height / 3,
        height.min(width) / 7,
        Color::rgb(0.95, 0.45, 0.28),
    );
    image.fill_circle(
        width / 4,
        height.saturating_mul(3) / 4,
        height.min(width) / 6,
        Color::rgb(0.26, 0.76, 0.96),
    );
    image.fill_circle(
        width.saturating_mul(2) / 3,
        height.saturating_mul(2) / 3,
        height.min(width) / 8,
        Color::rgb(0.98, 0.86, 0.34),
    );
    image.stroke_rect(
        10,
        10,
        width.saturating_sub(20),
        height.saturating_sub(20),
        Color::rgb(0.96, 0.96, 0.98),
    );
    let lines = vec![RenderLine::plain(
        name.unwrap_or("[canvas buffer]").to_string(),
    )];
    overlay_lines(
        &mut image,
        pane,
        lines,
        12,
        height.saturating_sub(20),
        Color::rgb(0.96, 0.96, 0.98),
    );
    image
}

fn overlay_lines(
    image: &mut TextureImage,
    pane: &SessionPaneView,
    lines: Vec<RenderLine>,
    x: u32,
    start_y: u32,
    color: Color,
) {
    let cols = ((image.width.saturating_sub(x)) / CELL_ADVANCE).max(1) as usize;
    for (visible_index, line) in lines
        .iter()
        .skip(pane.viewport.offset_y)
        .take(visible_line_count(image.height.saturating_sub(start_y)))
        .enumerate()
    {
        let text = clip_text(&line.text, pane.viewport.offset_x, cols);
        draw_text(
            image,
            x,
            start_y + visible_index as u32 * ROW_ADVANCE,
            &text,
            color,
        );
    }
}

fn records_lines(pane: &SessionPaneView, records: &[serde_json::Value]) -> Vec<RenderLine> {
    if records.is_empty() {
        return vec![RenderLine::plain("[no records]".to_string())];
    }

    records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let selected = is_selected_record_row(pane, index);
            if pane.buffer_name == "*jobs*" {
                RenderLine {
                    text: format_job_record_line(record),
                    selected,
                    hotspot: Some(SurfaceHotspotAction::SelectRecordRow {
                        row: index,
                        open: true,
                    }),
                }
            } else {
                RenderLine {
                    text: format!("{}{}", if selected { "> " } else { "  " }, record),
                    selected,
                    hotspot: Some(SurfaceHotspotAction::SelectRecordRow {
                        row: index,
                        open: true,
                    }),
                }
            }
        })
        .collect()
}

fn format_job_record_line(record: &serde_json::Value) -> String {
    let prefix = "  ";
    if let Some(summary) = record.get("summary").and_then(serde_json::Value::as_str) {
        return format!("{prefix}{summary}");
    }

    let id = record
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "?".to_string());
    let name = record
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("job");
    let status = record
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    format!("{prefix}{id}: {name} [{status}]")
}

fn tree_lines(pane: &SessionPaneView, roots: &[SessionTreeNodeView]) -> Vec<RenderLine> {
    if roots.is_empty() {
        return vec![RenderLine::plain("[empty tree]".to_string())];
    }

    let mut lines = Vec::new();
    for root in roots {
        push_tree_lines(&mut lines, pane, root, 0);
    }
    lines
}

fn push_tree_lines(
    lines: &mut Vec<RenderLine>,
    pane: &SessionPaneView,
    node: &SessionTreeNodeView,
    depth: usize,
) {
    let selected = is_selected_tree_node(pane, node.id.as_str());
    let prefix = if selected { "> " } else { "  " };
    lines.push(RenderLine {
        text: format!("{prefix}{}{}", "  ".repeat(depth), node.label),
        selected,
        hotspot: Some(SurfaceHotspotAction::SelectTreeNode {
            node_id: node.id.clone(),
            open: true,
        }),
    });
    for child in &node.children {
        push_tree_lines(lines, pane, child, depth + 1);
    }
}

fn hotspot_rows(
    pane: &SessionPaneView,
    actions: Vec<SurfaceHotspotAction>,
    width: u32,
    height: u32,
) -> Vec<SurfaceHotspot> {
    let visible = visible_line_count(height);
    let start = pane.viewport.offset_y;
    actions
        .into_iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(visible_index, action)| SurfaceHotspot {
            rect: Rect {
                x: 0,
                y: PADDING_Y + visible_index as u32 * ROW_ADVANCE,
                width,
                height: ROW_ADVANCE,
            },
            action,
        })
        .collect()
}

fn is_selected_record_row(pane: &SessionPaneView, index: usize) -> bool {
    matches!(
        pane.selection.as_ref(),
        Some(Selection::Records(selection)) if selection.rows().first() == Some(&index)
    )
}

fn is_selected_tree_node(pane: &SessionPaneView, node_id: &str) -> bool {
    matches!(
        pane.selection.as_ref(),
        Some(Selection::Tree(selection)) if selection.node_ids().first().map(String::as_str) == Some(node_id)
    )
}

fn visible_line_count(height: u32) -> usize {
    height.saturating_sub(PADDING_Y * 2).max(ROW_ADVANCE) as usize / ROW_ADVANCE as usize
}

fn visible_column_count(width: u32) -> usize {
    width.saturating_sub(PADDING_X * 2).max(CELL_ADVANCE) as usize / CELL_ADVANCE as usize
}

fn clip_text(text: &str, offset_x: usize, max_cols: usize) -> String {
    text.chars().skip(offset_x).take(max_cols).collect()
}

fn draw_text(image: &mut TextureImage, x: u32, y: u32, text: &str, color: Color) {
    let mut cursor_x = x;
    for ch in text.chars() {
        draw_glyph(image, ch, cursor_x, y, color);
        cursor_x = cursor_x.saturating_add(CELL_ADVANCE);
        if cursor_x >= image.width {
            break;
        }
    }
}

fn draw_glyph(image: &mut TextureImage, ch: char, x: u32, y: u32, color: Color) {
    let bitmap = glyph_bitmap(ch);
    for (row, pattern) in bitmap.iter().enumerate() {
        for col in 0..CELL_WIDTH as usize {
            if (pattern >> (CELL_WIDTH as usize - 1 - col)) & 1 == 1 {
                let px = x + col as u32;
                let py = y + row as u32;
                if px < image.width && py < image.height {
                    image.set_pixel(px, py, color);
                }
            }
        }
    }
}

#[derive(Clone)]
struct RenderLine {
    text: String,
    selected: bool,
    hotspot: Option<SurfaceHotspotAction>,
}

impl RenderLine {
    fn plain(text: String) -> Self {
        Self {
            text,
            selected: false,
            hotspot: None,
        }
    }
}

impl TextureImage {
    fn new(width: u32, height: u32, background: Color) -> Self {
        let mut pixels = vec![0; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                set_pixel_raw(&mut pixels, width, x, y, background);
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        let max_x = (x + width).min(self.width);
        let max_y = (y + height).min(self.height);
        for py in y.min(self.height)..max_y {
            for px in x.min(self.width)..max_x {
                self.set_pixel(px, py, color);
            }
        }
    }

    fn stroke_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        if width == 0 || height == 0 {
            return;
        }
        self.fill_rect(x, y, width, 1, color);
        self.fill_rect(x, y + height.saturating_sub(1), width, 1, color);
        self.fill_rect(x, y, 1, height, color);
        self.fill_rect(x + width.saturating_sub(1), y, 1, height, color);
    }

    fn fill_checker(&mut self, a: Color, b: Color, cell: u32) {
        for y in 0..self.height {
            for x in 0..self.width {
                let color = if ((x / cell) + (y / cell)).is_multiple_of(2) {
                    a
                } else {
                    b
                };
                self.set_pixel(x, y, color);
            }
        }
    }

    fn fill_vertical_gradient(&mut self, top: Color, bottom: Color) {
        let denom = self.height.saturating_sub(1).max(1) as f32;
        for y in 0..self.height {
            let t = y as f32 / denom;
            let color = Color {
                r: top.r + (bottom.r - top.r) * t,
                g: top.g + (bottom.g - top.g) * t,
                b: top.b + (bottom.b - top.b) * t,
                a: 1.0,
            };
            for x in 0..self.width {
                self.set_pixel(x, y, color);
            }
        }
    }

    fn fill_circle(&mut self, cx: u32, cy: u32, radius: u32, color: Color) {
        let radius_sq = (radius * radius) as i64;
        let min_x = cx.saturating_sub(radius);
        let max_x = (cx + radius).min(self.width.saturating_sub(1));
        let min_y = cy.saturating_sub(radius);
        let max_y = (cy + radius).min(self.height.saturating_sub(1));
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as i64 - cx as i64;
                let dy = y as i64 - cy as i64;
                if dx * dx + dy * dy <= radius_sq {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    fn fill_triangle_play(&mut self, x: u32, y: u32, size: u32, color: Color) {
        for row in 0..size {
            let width = row / 2 + 1;
            for col in 0..width {
                let px = x + col;
                let py = y + row;
                if px < self.width && py < self.height {
                    self.set_pixel(px, py, color);
                }
            }
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        set_pixel_raw(&mut self.pixels, self.width, x, y, color);
    }
}

fn set_pixel_raw(pixels: &mut [u8], width: u32, x: u32, y: u32, color: Color) {
    let index = ((y * width + x) * 4) as usize;
    pixels[index] = float_to_byte(color.r);
    pixels[index + 1] = float_to_byte(color.g);
    pixels[index + 2] = float_to_byte(color.b);
    pixels[index + 3] = float_to_byte(color.a);
}

fn float_to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn glyph_bitmap(ch: char) -> [u8; CELL_HEIGHT as usize] {
    match ch {
        'A' | 'a' => [0x1E, 0x21, 0x21, 0x3F, 0x21, 0x21, 0x21, 0x00],
        'B' | 'b' => [0x3E, 0x21, 0x21, 0x3E, 0x21, 0x21, 0x3E, 0x00],
        'C' | 'c' => [0x1E, 0x21, 0x20, 0x20, 0x20, 0x21, 0x1E, 0x00],
        'D' | 'd' => [0x3C, 0x22, 0x21, 0x21, 0x21, 0x22, 0x3C, 0x00],
        'E' | 'e' => [0x3F, 0x20, 0x20, 0x3E, 0x20, 0x20, 0x3F, 0x00],
        'F' | 'f' => [0x3F, 0x20, 0x20, 0x3E, 0x20, 0x20, 0x20, 0x00],
        'G' | 'g' => [0x1E, 0x21, 0x20, 0x27, 0x21, 0x21, 0x1F, 0x00],
        'H' | 'h' => [0x21, 0x21, 0x21, 0x3F, 0x21, 0x21, 0x21, 0x00],
        'I' | 'i' => [0x1E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x1E, 0x00],
        'J' | 'j' => [0x0F, 0x04, 0x04, 0x04, 0x24, 0x24, 0x18, 0x00],
        'K' | 'k' => [0x21, 0x22, 0x24, 0x38, 0x24, 0x22, 0x21, 0x00],
        'L' | 'l' => [0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3F, 0x00],
        'M' | 'm' => [0x21, 0x33, 0x2D, 0x2D, 0x21, 0x21, 0x21, 0x00],
        'N' | 'n' => [0x21, 0x31, 0x29, 0x25, 0x23, 0x21, 0x21, 0x00],
        'O' | 'o' => [0x1E, 0x21, 0x21, 0x21, 0x21, 0x21, 0x1E, 0x00],
        'P' | 'p' => [0x3E, 0x21, 0x21, 0x3E, 0x20, 0x20, 0x20, 0x00],
        'Q' | 'q' => [0x1E, 0x21, 0x21, 0x21, 0x25, 0x22, 0x1D, 0x00],
        'R' | 'r' => [0x3E, 0x21, 0x21, 0x3E, 0x24, 0x22, 0x21, 0x00],
        'S' | 's' => [0x1F, 0x20, 0x20, 0x1E, 0x01, 0x01, 0x3E, 0x00],
        'T' | 't' => [0x3F, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00],
        'U' | 'u' => [0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x1E, 0x00],
        'V' | 'v' => [0x21, 0x21, 0x21, 0x21, 0x21, 0x12, 0x0C, 0x00],
        'W' | 'w' => [0x21, 0x21, 0x21, 0x2D, 0x2D, 0x33, 0x21, 0x00],
        'X' | 'x' => [0x21, 0x21, 0x12, 0x0C, 0x12, 0x21, 0x21, 0x00],
        'Y' | 'y' => [0x21, 0x21, 0x12, 0x0C, 0x08, 0x08, 0x08, 0x00],
        'Z' | 'z' => [0x3F, 0x01, 0x02, 0x0C, 0x10, 0x20, 0x3F, 0x00],
        '0' => [0x1E, 0x21, 0x23, 0x25, 0x29, 0x31, 0x1E, 0x00],
        '1' => [0x08, 0x18, 0x08, 0x08, 0x08, 0x08, 0x1C, 0x00],
        '2' => [0x1E, 0x21, 0x01, 0x06, 0x18, 0x20, 0x3F, 0x00],
        '3' => [0x1E, 0x21, 0x01, 0x0E, 0x01, 0x21, 0x1E, 0x00],
        '4' => [0x04, 0x0C, 0x14, 0x24, 0x3F, 0x04, 0x04, 0x00],
        '5' => [0x3F, 0x20, 0x3E, 0x01, 0x01, 0x21, 0x1E, 0x00],
        '6' => [0x0E, 0x10, 0x20, 0x3E, 0x21, 0x21, 0x1E, 0x00],
        '7' => [0x3F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10, 0x00],
        '8' => [0x1E, 0x21, 0x21, 0x1E, 0x21, 0x21, 0x1E, 0x00],
        '9' => [0x1E, 0x21, 0x21, 0x1F, 0x01, 0x02, 0x1C, 0x00],
        ':' => [0x00, 0x08, 0x08, 0x00, 0x08, 0x08, 0x00, 0x00],
        '[' => [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E, 0x00],
        ']' => [0x1C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1C, 0x00],
        '{' => [0x06, 0x08, 0x08, 0x10, 0x08, 0x08, 0x06, 0x00],
        '}' => [0x18, 0x04, 0x04, 0x02, 0x04, 0x04, 0x18, 0x00],
        '(' => [0x04, 0x08, 0x10, 0x10, 0x10, 0x08, 0x04, 0x00],
        ')' => [0x10, 0x08, 0x04, 0x04, 0x04, 0x08, 0x10, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x00],
        '*' => [0x00, 0x12, 0x0C, 0x3F, 0x0C, 0x12, 0x00, 0x00],
        '/' => [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x00, 0x00],
        '\\' => [0x20, 0x10, 0x08, 0x04, 0x02, 0x01, 0x00, 0x00],
        '|' => [0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x08, 0x10],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, 0x00],
        '=' => [0x00, 0x00, 0x1E, 0x00, 0x1E, 0x00, 0x00, 0x00],
        '"' => [0x12, 0x12, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00],
        '\'' => [0x08, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00],
        ' ' => [0x00; 8],
        _ => [0x3F, 0x21, 0x05, 0x09, 0x11, 0x21, 0x3F, 0x00],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{BufferKind, PanePresentation, Viewport};

    fn text_pane(lines: Vec<&str>) -> SessionPaneView {
        SessionPaneView {
            buffer_name: "notes".to_string(),
            buffer_id: 1,
            buffer_kind: BufferKind::Text,
            pane_id: 1,
            viewport: Viewport::default(),
            presentation: PanePresentation::Default,
            selection: None,
            content: SessionPaneContentView::Text(lines.into_iter().map(str::to_string).collect()),
            active: true,
        }
    }

    #[test]
    fn pane_surface_respects_vertical_viewport() {
        let mut pane = text_pane(vec!["alpha", "beta", "gamma"]);
        let base = build_body_texture(&pane, 120, 60).0;
        pane.viewport = Viewport::new(0, 1);
        let shifted = build_body_texture(&pane, 120, 60).0;

        assert_ne!(base.pixels, shifted.pixels);
    }

    #[test]
    fn pane_surface_respects_horizontal_viewport() {
        let mut pane = text_pane(vec!["abcdef"]);
        let base = build_body_texture(&pane, 120, 40).0;
        pane.viewport = Viewport::new(2, 0);
        let shifted = build_body_texture(&pane, 120, 40).0;

        assert_ne!(base.pixels, shifted.pixels);
    }

    #[test]
    fn pane_surface_builds_texture_for_text_buffers() {
        let pane = text_pane(vec!["hello"]);
        let surface = build_pane_surface(
            &pane,
            Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
        )
        .expect("surface should build");

        assert_eq!(surface.texture.image.width, 100);
        assert_eq!(surface.texture.image.height, 50);
        assert!(
            surface
                .texture
                .image
                .pixels
                .iter()
                .any(|component| *component != 0)
        );
    }

    #[test]
    fn records_surface_exposes_hotspots_for_visible_rows() {
        let pane = SessionPaneView {
            buffer_name: "results".to_string(),
            buffer_id: 1,
            buffer_kind: BufferKind::Records,
            pane_id: 1,
            viewport: Viewport::default(),
            presentation: PanePresentation::Default,
            selection: None,
            content: SessionPaneContentView::Records(vec![
                serde_json::json!({"a": 1}),
                serde_json::json!({"a": 2}),
            ]),
            active: true,
        };
        let output = build_pane_surface(
            &pane,
            Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
        )
        .expect("surface should build");

        assert!(matches!(
            output.hotspots[0].action,
            SurfaceHotspotAction::SelectRecordRow { row: 0, open: true }
        ));
    }
}
