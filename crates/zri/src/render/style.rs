use super::{Color, Coord};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: Coord,
}

impl Stroke {
    pub const fn new(color: Color, width: Coord) -> Self {
        Self { color, width }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub size: Coord,
}

impl TextStyle {
    pub const fn new(color: Color, size: Coord) -> Self {
        Self { color, size }
    }
}
