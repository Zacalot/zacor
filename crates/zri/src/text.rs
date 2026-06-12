use cosmic_text::{Attrs, Buffer, FontSystem, LayoutGlyph, Metrics, Shaping, SwashCache, Wrap};

use crate::render::{Color, Coord, Point, Size, TextStyle};

const DEFAULT_LINE_HEIGHT_SCALE: f32 = 1.2;

/// The line-box height the kernel lays text out with for a given style. This
/// is the same value `layout_line` shapes against, so paint paths that advance
/// by it can never diverge from shaped output.
pub fn line_height(style: TextStyle) -> Coord {
    style.size * DEFAULT_LINE_HEIGHT_SCALE
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlyphKey(cosmic_text::CacheKey);

impl GlyphKey {
    pub(crate) fn raw(self) -> cosmic_text::CacheKey {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphImageFormat {
    Mask,
    Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphImage {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub format: GlyphImageFormat,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaidOutGlyph {
    pub key: GlyphKey,
    pub x: i32,
    pub y: i32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub size: Size,
    pub baseline: Coord,
    pub glyphs: Vec<LaidOutGlyph>,
}

pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl TextSystem {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    pub fn measure_line(&mut self, text: &str, style: TextStyle) -> Size {
        self.layout_line(Point::new(0.0, 0.0), text, style).size
    }

    pub fn layout_line(&mut self, position: Point, text: &str, style: TextStyle) -> TextLayout {
        let mut buffer = Buffer::new(&mut self.font_system, buffer_metrics(style));
        buffer.set_wrap(Wrap::None);
        buffer.set_size(None, None);
        buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut glyphs = Vec::new();
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        let mut baseline = position.y;
        let mut first_run = true;

        for run in buffer.layout_runs() {
            if first_run {
                baseline = position.y + run.line_y;
                first_run = false;
            }

            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);

            for glyph in run.glyphs {
                glyphs.push(layout_glyph(
                    glyph,
                    (position.x, position.y + run.line_y),
                    style.color,
                ));
            }
        }

        TextLayout {
            size: Size::new(width, height),
            baseline,
            glyphs,
        }
    }

    pub fn rasterize_glyph(&mut self, key: GlyphKey) -> Option<GlyphImage> {
        let image = self
            .swash_cache
            .get_image(&mut self.font_system, key.raw())
            .as_ref()?;

        Some(GlyphImage {
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            format: match image.content {
                cosmic_text::SwashContent::Mask => GlyphImageFormat::Mask,
                cosmic_text::SwashContent::Color => GlyphImageFormat::Color,
                cosmic_text::SwashContent::SubpixelMask => GlyphImageFormat::Mask,
            },
            data: image.data.clone(),
        })
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn buffer_metrics(style: TextStyle) -> Metrics {
    Metrics::new(style.size, line_height(style))
}

fn layout_glyph(glyph: &LayoutGlyph, offset: (f32, f32), color: Color) -> LaidOutGlyph {
    let physical = glyph.physical(offset, 1.0);
    LaidOutGlyph {
        key: GlyphKey(physical.cache_key),
        x: physical.x,
        y: physical.y,
        color: glyph.color_opt.map_or(color, from_cosmic_color),
    }
}

fn from_cosmic_color(color: cosmic_text::Color) -> Color {
    let [r, g, b, a] = color.as_rgba();
    Color::rgba(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_empty_line_has_zero_width() {
        let mut text = TextSystem::new();
        let size = text.measure_line("", TextStyle::new(Color::WHITE, 12.0));

        assert_eq!(size.width, 0.0);
        assert!(size.height >= 0.0);
    }

    #[test]
    fn measure_nonempty_line_has_positive_width() {
        let mut text = TextSystem::new();
        let size = text.measure_line("hello", TextStyle::new(Color::WHITE, 12.0));

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
    }

    #[test]
    fn line_height_matches_layout_metrics() {
        let style = TextStyle::new(Color::WHITE, 12.0);
        let mut text = TextSystem::new();
        let measured = text.measure_line("hi", style);

        assert_eq!(measured.height, line_height(style));
    }

    #[test]
    fn larger_font_size_increases_line_height() {
        let mut text = TextSystem::new();
        let small = text.measure_line("hello", TextStyle::new(Color::WHITE, 12.0));
        let large = text.measure_line("hello", TextStyle::new(Color::WHITE, 24.0));

        assert!(large.height > small.height);
    }

    #[test]
    fn layout_line_preserves_origin_in_baseline() {
        let mut text = TextSystem::new();
        let layout = text.layout_line(
            Point::new(10.0, 20.0),
            "hi",
            TextStyle::new(Color::WHITE, 12.0),
        );

        assert!(layout.baseline >= 20.0);
    }

    #[test]
    fn layout_line_returns_glyphs_for_visible_text() {
        let mut text = TextSystem::new();
        let layout = text.layout_line(
            Point::new(0.0, 0.0),
            "hi",
            TextStyle::new(Color::WHITE, 12.0),
        );

        assert!(!layout.glyphs.is_empty());
    }

    #[test]
    fn rasterize_glyph_returns_pixels_for_visible_glyph() {
        let mut text = TextSystem::new();
        let layout = text.layout_line(
            Point::new(0.0, 0.0),
            "x",
            TextStyle::new(Color::WHITE, 12.0),
        );
        let image = text.rasterize_glyph(layout.glyphs[0].key).unwrap();

        assert!(image.width > 0);
        assert!(image.height > 0);
        assert!(!image.data.is_empty());
    }
}
