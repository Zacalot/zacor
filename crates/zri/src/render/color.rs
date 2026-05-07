#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    Transparent,
    Rgba { r: u8, g: u8, b: u8, a: u8 },
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::Rgba { r, g, b, a }
    }

    pub fn to_f32_rgba(self) -> [f32; 4] {
        match self {
            Self::Transparent => [0.0, 0.0, 0.0, 0.0],
            Self::Rgba { r, g, b, a } => [
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ],
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::Transparent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_is_opaque_rgba() {
        assert_eq!(
            Color::rgb(1, 2, 3),
            Color::Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 255
            }
        );
    }

    #[test]
    fn color_converts_to_normalized_float_channels() {
        assert_eq!(Color::Transparent.to_f32_rgba(), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            Color::rgba(255, 128, 0, 64).to_f32_rgba(),
            [1.0, 128.0 / 255.0, 0.0, 64.0 / 255.0]
        );
    }
}
