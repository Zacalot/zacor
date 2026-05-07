pub type Coord = f32;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: Coord,
    pub y: Coord,
}

impl Point {
    pub const fn new(x: Coord, y: Coord) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: Coord,
    pub height: Coord,
}

impl Size {
    pub const fn new(width: Coord, height: Coord) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn from_xywh(x: Coord, y: Coord, width: Coord, height: Coord) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }

    pub fn left(&self) -> Coord {
        self.origin.x
    }

    pub fn right(&self) -> Coord {
        self.origin.x + self.size.width
    }

    pub fn top(&self) -> Coord {
        self.origin.y
    }

    pub fn bottom(&self) -> Coord {
        self.origin.y + self.size.height
    }

    pub fn is_empty(&self) -> bool {
        self.size.width <= 0.0 || self.size.height <= 0.0
    }

    pub fn contains(&self, point: Point) -> bool {
        !self.is_empty()
            && point.x >= self.left()
            && point.x < self.right()
            && point.y >= self.top()
            && point.y < self.bottom()
    }

    pub fn intersect(&self, other: &Self) -> Self {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self::from_xywh(left, top, (right - left).max(0.0), (bottom - top).max(0.0))
    }

    pub fn inset(&self, insets: Insets) -> Self {
        let x = self.left() + insets.left;
        let y = self.top() + insets.top;
        let width = (self.size.width - insets.left - insets.right).max(0.0);
        let height = (self.size.height - insets.top - insets.bottom).max(0.0);
        Self::from_xywh(x, y, width, height)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: Coord,
    pub right: Coord,
    pub bottom: Coord,
    pub left: Coord,
}

impl Insets {
    pub const fn new(top: Coord, right: Coord, bottom: Coord, left: Coord) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn all(value: Coord) -> Self {
        Self::new(value, value, value, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_or_negative_size_rects_are_empty() {
        assert!(Rect::from_xywh(0.0, 0.0, 0.0, 1.0).is_empty());
        assert!(Rect::from_xywh(0.0, 0.0, 1.0, -1.0).is_empty());
    }

    #[test]
    fn intersects_overlapping_rects() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 8.0);
        let b = Rect::from_xywh(4.0, 2.0, 10.0, 10.0);
        assert_eq!(a.intersect(&b), Rect::from_xywh(4.0, 2.0, 6.0, 6.0));
    }

    #[test]
    fn disjoint_intersection_is_empty_at_overlap_edge() {
        let a = Rect::from_xywh(0.0, 0.0, 2.0, 2.0);
        let b = Rect::from_xywh(5.0, 5.0, 1.0, 1.0);
        let intersection = a.intersect(&b);
        assert!(intersection.is_empty());
        assert_eq!(intersection.origin, Point::new(5.0, 5.0));
    }

    #[test]
    fn contains_uses_left_top_inclusive_right_bottom_exclusive() {
        let rect = Rect::from_xywh(1.0, 2.0, 3.0, 4.0);
        assert!(rect.contains(Point::new(1.0, 2.0)));
        assert!(rect.contains(Point::new(3.99, 5.99)));
        assert!(!rect.contains(Point::new(4.0, 6.0)));
        assert!(!rect.contains(Point::new(0.99, 2.0)));
    }

    #[test]
    fn inset_shrinks_rect_without_negative_size() {
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 8.0);
        assert_eq!(
            rect.inset(Insets::new(1.0, 2.0, 3.0, 4.0)),
            Rect::from_xywh(4.0, 1.0, 4.0, 4.0)
        );
        assert!(rect.inset(Insets::all(20.0)).is_empty());
    }
}
