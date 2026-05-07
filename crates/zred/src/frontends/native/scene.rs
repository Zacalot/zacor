use super::surface::{self, SurfaceHotspotAction};
use crate::kernel::{MessageLevel, MinibufferMode, SplitAxis};
use crate::session::{
    SessionJobKindView, SessionJobStatusView, SessionPaneNode, SessionPaneView,
    SessionSelectedItemView, SessionView,
};
use winit::dpi::PhysicalSize;

const GLYPH_WIDTH: u32 = 6;
const GLYPH_HEIGHT: u32 = 8;
const GLYPH_SPACING: u32 = 1;
const TEXT_SCALE: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FillRect {
    pub rect: Rect,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub rect: Rect,
    pub text: String,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneHitTarget {
    pub pane_id: u64,
    pub rect: Rect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureSurface {
    pub rect: Rect,
    pub image: TextureImage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneHotspot {
    pub rect: Rect,
    pub action: SceneHotspotAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneHotspotAction {
    SelectRecordRow {
        pane_id: u64,
        row: usize,
        open: bool,
    },
    SelectTreeNode {
        pane_id: u64,
        node_id: String,
        open: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeScene {
    pub fills: Vec<FillRect>,
    pub labels: Vec<Label>,
    pub textures: Vec<TextureSurface>,
    pub pane_hits: Vec<PaneHitTarget>,
    pub hotspots: Vec<SceneHotspot>,
}

pub fn build(view: &SessionView, size: PhysicalSize<u32>) -> NativeScene {
    let root = root_layout(size);
    let mut scene = NativeScene {
        fills: Vec::new(),
        labels: Vec::new(),
        textures: Vec::new(),
        pane_hits: Vec::new(),
        hotspots: Vec::new(),
    };

    paint_pane_node(&mut scene, &view.pane_tree, root.pane_area);
    paint_bar(
        &mut scene,
        root.jobs_area,
        format_jobs_line(view),
        Color::rgb(0.16, 0.18, 0.22),
        Color::rgb(0.82, 0.85, 0.9),
    );
    paint_bar(
        &mut scene,
        root.message_area,
        view.messages
            .last()
            .map(|message| message.text.clone())
            .unwrap_or_default(),
        message_background(view.messages.last().map(|m| m.level)),
        Color::rgb(0.9, 0.92, 0.95),
    );
    paint_bar(
        &mut scene,
        root.status_area,
        format_status_line(view),
        Color::rgb(0.22, 0.24, 0.3),
        Color::rgb(0.98, 0.98, 0.99),
    );

    scene
}

pub fn hit_test_pane(scene: &NativeScene, x: f64, y: f64) -> Option<u64> {
    let x = x as u32;
    let y = y as u32;
    scene
        .pane_hits
        .iter()
        .find(|target| target.rect.contains(x, y))
        .map(|target| target.pane_id)
}

pub fn hit_test_hotspot(scene: &NativeScene, x: f64, y: f64) -> Option<SceneHotspotAction> {
    let x = x as u32;
    let y = y as u32;
    scene
        .hotspots
        .iter()
        .find(|target| target.rect.contains(x, y))
        .map(|target| target.action.clone())
}

pub fn label_quads(scene: &NativeScene) -> Vec<FillRect> {
    let mut quads = Vec::new();
    for label in &scene.labels {
        quads.extend(rasterize_label(label));
    }
    quads
}

#[derive(Clone, Copy)]
struct RootLayout {
    pane_area: Rect,
    jobs_area: Rect,
    message_area: Rect,
    status_area: Rect,
}

fn root_layout(size: PhysicalSize<u32>) -> RootLayout {
    let width = size.width.max(1);
    let height = size.height.max(4);
    RootLayout {
        pane_area: Rect {
            x: 0,
            y: 0,
            width,
            height: height.saturating_sub(3),
        },
        jobs_area: Rect {
            x: 0,
            y: height.saturating_sub(3),
            width,
            height: 1,
        },
        message_area: Rect {
            x: 0,
            y: height.saturating_sub(2),
            width,
            height: 1,
        },
        status_area: Rect {
            x: 0,
            y: height.saturating_sub(1),
            width,
            height: 1,
        },
    }
}

fn paint_pane_node(scene: &mut NativeScene, node: &SessionPaneNode, area: Rect) {
    match node {
        SessionPaneNode::Leaf(pane) => paint_pane_leaf(scene, pane, area),
        SessionPaneNode::Split {
            axis,
            ratio_percent,
            first,
            second,
        } => {
            let (first_area, second_area) = split_rect(area, *axis, *ratio_percent);
            paint_pane_node(scene, first, first_area);
            paint_pane_node(scene, second, second_area);
        }
    }
}

fn paint_pane_leaf(scene: &mut NativeScene, pane: &SessionPaneView, area: Rect) {
    let base = if pane.active {
        Color::rgb(0.18, 0.19, 0.24)
    } else {
        Color::rgb(0.11, 0.12, 0.15)
    };
    scene.fills.push(FillRect {
        rect: area,
        color: base,
    });
    scene.pane_hits.push(PaneHitTarget {
        pane_id: pane.pane_id,
        rect: area,
    });

    if area.width > 2 && area.height > 2 {
        scene.fills.push(FillRect {
            rect: Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width - 2,
                height: area.height - 2,
            },
            color: Color::rgb(0.08, 0.09, 0.11),
        });
    }

    let title = format!("{} [pane:{}]", pane.buffer_name, pane.pane_id);
    scene.labels.push(Label {
        rect: Rect {
            x: area.x + 4,
            y: area.y + 2,
            width: area.width.saturating_sub(8),
            height: GLYPH_HEIGHT * TEXT_SCALE,
        },
        text: title,
        color: if pane.active {
            Color::rgb(1.0, 0.86, 0.38)
        } else {
            Color::rgb(0.74, 0.77, 0.82)
        },
    });

    let body = Rect {
        x: area.x.saturating_add(6),
        y: area.y.saturating_add(20),
        width: area.width.saturating_sub(12),
        height: area.height.saturating_sub(26),
    };
    if let Some(surface) = surface::build_pane_surface(pane, body) {
        scene.textures.push(surface.texture);
        scene
            .hotspots
            .extend(surface.hotspots.into_iter().map(|hotspot| SceneHotspot {
                rect: hotspot.rect,
                action: match hotspot.action {
                    SurfaceHotspotAction::SelectRecordRow { row, open } => {
                        SceneHotspotAction::SelectRecordRow {
                            pane_id: pane.pane_id,
                            row,
                            open,
                        }
                    }
                    SurfaceHotspotAction::SelectTreeNode { node_id, open } => {
                        SceneHotspotAction::SelectTreeNode {
                            pane_id: pane.pane_id,
                            node_id,
                            open,
                        }
                    }
                },
            }));
    }
}

fn split_rect(area: Rect, axis: SplitAxis, ratio_percent: u8) -> (Rect, Rect) {
    let ratio = u32::from(ratio_percent.clamp(1, 99));
    match axis {
        SplitAxis::Vertical => {
            let first_width = (area.width * ratio) / 100;
            let second_width = area.width.saturating_sub(first_width);
            (
                Rect {
                    x: area.x,
                    y: area.y,
                    width: first_width,
                    height: area.height,
                },
                Rect {
                    x: area.x + first_width,
                    y: area.y,
                    width: second_width,
                    height: area.height,
                },
            )
        }
        SplitAxis::Horizontal => {
            let first_height = (area.height * ratio) / 100;
            let second_height = area.height.saturating_sub(first_height);
            (
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: first_height,
                },
                Rect {
                    x: area.x,
                    y: area.y + first_height,
                    width: area.width,
                    height: second_height,
                },
            )
        }
    }
}

fn paint_bar(
    scene: &mut NativeScene,
    rect: Rect,
    text: String,
    background: Color,
    foreground: Color,
) {
    scene.fills.push(FillRect {
        rect,
        color: background,
    });
    if !text.is_empty() {
        scene.labels.push(Label {
            rect: Rect {
                x: rect.x + 4,
                y: rect.y + 2,
                width: rect.width.saturating_sub(8),
                height: GLYPH_HEIGHT * TEXT_SCALE,
            },
            text,
            color: foreground,
        });
    }
}

fn message_background(level: Option<MessageLevel>) -> Color {
    match level {
        Some(MessageLevel::Info) => Color::rgb(0.11, 0.19, 0.3),
        Some(MessageLevel::Warning) => Color::rgb(0.32, 0.24, 0.08),
        Some(MessageLevel::Error) => Color::rgb(0.34, 0.1, 0.12),
        None => Color::rgb(0.12, 0.13, 0.16),
    }
}

fn format_jobs_line(view: &SessionView) -> String {
    if view.jobs.is_empty() {
        return String::new();
    }

    let jobs = view
        .jobs
        .iter()
        .map(format_job_summary)
        .collect::<Vec<_>>()
        .join(" | ");
    format!("jobs: {jobs}")
}

fn format_status_line(view: &SessionView) -> String {
    if matches!(view.minibuffer_mode, MinibufferMode::Message)
        && view.minibuffer_text == "Ready"
        && let Some(selected) = &view.selected_item
    {
        return format_selected_item(selected);
    }

    let prefix = match view.minibuffer_mode {
        MinibufferMode::Command => ":",
        MinibufferMode::Message => "",
    };
    format!("{prefix}{}", view.minibuffer_text)
}

fn format_selected_item(item: &SessionSelectedItemView) -> String {
    match item {
        SessionSelectedItemView::Record { row, value } => format!("record {}: {}", row + 1, value),
        SessionSelectedItemView::TreeNode {
            id,
            label,
            linked_buffer_id,
        } => match linked_buffer_id {
            Some(buffer_id) => format!("tree {}: {} -> buffer {}", id, label, buffer_id),
            None => format!("tree {}: {}", id, label),
        },
    }
}

fn format_job_summary(job: &crate::session::SessionJobView) -> String {
    let kind = match &job.kind {
        SessionJobKindView::Generic => job.name.clone(),
        SessionJobKindView::PackageInvoke {
            package, command, ..
        } => format!("{package} {command}"),
    };
    format!("{}:{} [{}]", job.id, kind, format_job_status(&job.status))
}

fn format_job_status(status: &SessionJobStatusView) -> &str {
    match status {
        SessionJobStatusView::Pending => "pending",
        SessionJobStatusView::Running => "running",
        SessionJobStatusView::Succeeded => "succeeded",
        SessionJobStatusView::Failed(_) => "failed",
        SessionJobStatusView::Cancelled => "cancelled",
    }
}

fn rasterize_label(label: &Label) -> Vec<FillRect> {
    let mut fills = Vec::new();
    let mut cursor_x = label.rect.x;
    let max_x = label.rect.x.saturating_add(label.rect.width);
    for ch in label.text.chars() {
        let glyph_width = (GLYPH_WIDTH + GLYPH_SPACING) * TEXT_SCALE;
        if cursor_x.saturating_add(glyph_width) > max_x {
            break;
        }
        fills.extend(rasterize_glyph(ch, cursor_x, label.rect.y, label.color));
        cursor_x = cursor_x.saturating_add(glyph_width);
    }
    fills
}

fn rasterize_glyph(ch: char, x: u32, y: u32, color: Color) -> Vec<FillRect> {
    let bitmap = glyph_bitmap(ch);
    let mut fills = Vec::new();
    for (row, pattern) in bitmap.iter().enumerate() {
        for col in 0..GLYPH_WIDTH as usize {
            if (pattern >> (GLYPH_WIDTH as usize - 1 - col)) & 1 == 1 {
                fills.push(FillRect {
                    rect: Rect {
                        x: x + (col as u32 * TEXT_SCALE),
                        y: y + (row as u32 * TEXT_SCALE),
                        width: TEXT_SCALE,
                        height: TEXT_SCALE,
                    },
                    color,
                });
            }
        }
    }
    fills
}

fn glyph_bitmap(ch: char) -> [u8; GLYPH_HEIGHT as usize] {
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
        '-' => [0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x00],
        '*' => [0x00, 0x12, 0x0C, 0x3F, 0x0C, 0x12, 0x00, 0x00],
        '/' => [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x00, 0x00],
        '|' => [0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C, 0x00],
        ' ' => [0x00; 8],
        _ => [0x3F, 0x21, 0x05, 0x09, 0x11, 0x21, 0x3F, 0x00],
    }
}

impl Color {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }
}

impl Rect {
    pub fn normalized_bounds(&self, size: PhysicalSize<u32>) -> [f32; 4] {
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let left = (self.x as f32 / width) * 2.0 - 1.0;
        let right = ((self.x + self.width) as f32 / width) * 2.0 - 1.0;
        let top = 1.0 - (self.y as f32 / height) * 2.0;
        let bottom = 1.0 - ((self.y + self.height) as f32 / height) * 2.0;
        [left, right, top, bottom]
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x
            && x < self.x.saturating_add(self.width)
            && y >= self.y
            && y < self.y.saturating_add(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{BufferKind, PanePresentation, Viewport};
    use crate::session::{Session, SessionPaneContentView, SessionPaneNode};

    #[test]
    fn scene_builds_bars_and_leafs_from_view() {
        let session = Session::new();
        let view = session.view();

        let scene = build(&view, PhysicalSize::new(100, 30));

        assert!(scene.fills.len() >= 4);
        assert!(scene.labels.iter().any(|label| label.text == "Ready"));
        assert!(
            scene
                .labels
                .iter()
                .any(|label| label.text.contains("*scratch*"))
        );
        assert_eq!(scene.pane_hits.len(), 1);
        assert_eq!(scene.textures.len(), 1);
    }

    #[test]
    fn scene_splits_follow_pane_tree() {
        let mut session = Session::new();
        session.dispatch_command("pane.split.vertical");
        let view = session.view();

        let scene = build(&view, PhysicalSize::new(100, 30));
        let pane_titles = scene
            .labels
            .iter()
            .filter(|label| label.text.contains("[pane:"))
            .collect::<Vec<_>>();

        assert!(matches!(view.pane_tree, SessionPaneNode::Split { .. }));
        assert_eq!(pane_titles.len(), 2);
        assert_eq!(scene.pane_hits.len(), 2);
        assert_eq!(scene.textures.len(), 2);
    }

    #[test]
    fn hit_testing_returns_pane_id_for_leaf_rect() {
        let session = Session::new();
        let view = session.view();
        let scene = build(&view, PhysicalSize::new(100, 30));

        assert_eq!(hit_test_pane(&scene, 10.0, 10.0), Some(1));
    }

    #[test]
    fn label_quads_expand_text_into_fill_rects() {
        let label = Label {
            rect: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 16,
            },
            text: "A".to_string(),
            color: Color::rgb(1.0, 1.0, 1.0),
        };

        let quads = label_quads(&NativeScene {
            fills: Vec::new(),
            labels: vec![label],
            textures: Vec::new(),
            pane_hits: Vec::new(),
            hotspots: Vec::new(),
        });

        assert!(!quads.is_empty());
    }

    #[test]
    fn scene_texture_body_respects_viewport_changes() {
        let mut pane = SessionPaneView {
            buffer_name: "notes".to_string(),
            buffer_id: 1,
            buffer_kind: BufferKind::Text,
            pane_id: 1,
            viewport: Viewport::default(),
            presentation: PanePresentation::Default,
            selection: None,
            content: SessionPaneContentView::Text(vec!["alpha".to_string(), "beta".to_string()]),
            active: true,
        };
        let mut scene = NativeScene {
            fills: Vec::new(),
            labels: Vec::new(),
            textures: Vec::new(),
            pane_hits: Vec::new(),
            hotspots: Vec::new(),
        };
        paint_pane_leaf(
            &mut scene,
            &pane,
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 80,
            },
        );
        let base = scene.textures[0].image.pixels.clone();

        pane.viewport = Viewport::new(0, 1);
        let mut shifted_scene = NativeScene {
            fills: Vec::new(),
            labels: Vec::new(),
            textures: Vec::new(),
            pane_hits: Vec::new(),
            hotspots: Vec::new(),
        };
        paint_pane_leaf(
            &mut shifted_scene,
            &pane,
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 80,
            },
        );

        assert_ne!(base, shifted_scene.textures[0].image.pixels);
    }

    #[test]
    fn scene_hotspots_cover_visible_structured_rows() {
        let pane = SessionPaneView {
            buffer_name: "results".to_string(),
            buffer_id: 1,
            buffer_kind: BufferKind::Records,
            pane_id: 7,
            viewport: Viewport::default(),
            presentation: PanePresentation::Default,
            selection: None,
            content: SessionPaneContentView::Records(vec![serde_json::json!({"a": 1})]),
            active: true,
        };
        let mut scene = NativeScene {
            fills: Vec::new(),
            labels: Vec::new(),
            textures: Vec::new(),
            pane_hits: Vec::new(),
            hotspots: Vec::new(),
        };

        paint_pane_leaf(
            &mut scene,
            &pane,
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 80,
            },
        );

        assert!(matches!(
            scene.hotspots.first().map(|hotspot| &hotspot.action),
            Some(SceneHotspotAction::SelectRecordRow {
                pane_id: 7,
                row: 0,
                open: true
            })
        ));
    }
}
