//! Physical pixel points and rectangles. Every coordinate in screen-capture is in physical
//! pixels: global ones on the virtual desktop (negative left of or above the primary monitor),
//! local ones from a monitor's top-left corner.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

impl PhysicalPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A rectangle covering `x..x + width` and `y..y + height` (the right and bottom edges are
/// outside it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PhysicalRect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The rectangle between two corners, in either order: a drag from `a` to `b`.
    pub fn from_corners(a: PhysicalPoint, b: PhysicalPoint) -> Self {
        Self::from_edges(
            i64::from(a.x.min(b.x)),
            i64::from(a.y.min(b.y)),
            i64::from(a.x.max(b.x)),
            i64::from(a.y.max(b.y)),
        )
    }

    /// Edges in `i64`, so that adding a width to an `i32` cannot overflow.
    fn from_edges(left: i64, top: i64, right: i64, bottom: i64) -> Self {
        let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let (left, top) = (clamp(left), clamp(top));
        let size = |from: i32, to: i64| (to - i64::from(from)).clamp(0, i64::from(u32::MAX)) as u32;
        Self::new(left, top, size(left, right), size(top, bottom))
    }

    pub fn left(&self) -> i64 {
        i64::from(self.x)
    }

    pub fn top(&self) -> i64 {
        i64::from(self.y)
    }

    pub fn right(&self) -> i64 {
        self.left() + i64::from(self.width)
    }

    pub fn bottom(&self) -> i64 {
        self.top() + i64::from(self.height)
    }

    /// No pixel inside: zero width or zero height.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn contains(&self, p: PhysicalPoint) -> bool {
        let (x, y) = (i64::from(p.x), i64::from(p.y));
        self.left() <= x && x < self.right() && self.top() <= y && y < self.bottom()
    }

    /// The common part, or `None` when they share no pixel.
    pub fn intersection(&self, other: &PhysicalRect) -> Option<PhysicalRect> {
        let rect = Self::from_edges(
            self.left().max(other.left()),
            self.top().max(other.top()),
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        );
        (!rect.is_empty()).then_some(rect)
    }

    /// The smallest rectangle containing both.
    pub fn union(&self, other: &PhysicalRect) -> PhysicalRect {
        Self::from_edges(
            self.left().min(other.left()),
            self.top().min(other.top()),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    pub fn translate(&self, dx: i32, dy: i32) -> PhysicalRect {
        Self::from_edges(
            self.left() + i64::from(dx),
            self.top() + i64::from(dy),
            self.right() + i64::from(dx),
            self.bottom() + i64::from(dy),
        )
    }
}

/// A point of a drag kept on a monitor of `size`: the far edges are allowed, since the
/// rectangle `from_corners(start, end)` ends there.
pub fn clamp_point(p: PhysicalPoint, size: (u32, u32)) -> PhysicalPoint {
    let max = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    PhysicalPoint::new(p.x.clamp(0, max(size.0)), p.y.clamp(0, max(size.1)))
}

/// A local rectangle stopped at the edges of a monitor of `size` (a selection never crosses
/// to another monitor). Outside the monitor, it becomes empty on its nearest edge.
pub fn clamp_to_monitor(local: PhysicalRect, size: (u32, u32)) -> PhysicalRect {
    let start = clamp_point(PhysicalPoint::new(local.x, local.y), size);
    let bounds = PhysicalRect::new(0, 0, size.0, size.1);
    let far = PhysicalPoint::new(
        local.right().clamp(0, bounds.right()) as i32,
        local.bottom().clamp(0, bounds.bottom()) as i32,
    );
    PhysicalRect::from_corners(start, far)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> PhysicalPoint {
        PhysicalPoint::new(x, y)
    }

    #[test]
    fn a_drag_makes_the_same_rectangle_in_any_direction() {
        let rect = PhysicalRect::new(10, 20, 30, 40);
        assert_eq!(PhysicalRect::from_corners(p(10, 20), p(40, 60)), rect);
        assert_eq!(PhysicalRect::from_corners(p(40, 60), p(10, 20)), rect);
        assert_eq!(PhysicalRect::from_corners(p(40, 20), p(10, 60)), rect);
    }

    #[test]
    fn tells_empty_rectangles() {
        assert!(PhysicalRect::new(5, 5, 0, 10).is_empty());
        assert!(PhysicalRect::new(5, 5, 10, 0).is_empty());
        assert!(PhysicalRect::from_corners(p(3, 3), p(3, 9)).is_empty());
        assert!(!PhysicalRect::new(5, 5, 1, 1).is_empty());
    }

    #[test]
    fn contains_excludes_the_far_edges() {
        let rect = PhysicalRect::new(-10, -10, 10, 10);
        assert!(rect.contains(p(-10, -10)));
        assert!(rect.contains(p(-1, -1)));
        assert!(!rect.contains(p(0, -5)));
        assert!(!rect.contains(p(-5, 0)));
    }

    #[test]
    fn intersects_and_unites() {
        let a = PhysicalRect::new(0, 0, 100, 100);
        let b = PhysicalRect::new(50, -20, 100, 40);
        assert_eq!(a.intersection(&b), Some(PhysicalRect::new(50, 0, 50, 20)));
        assert_eq!(a.union(&b), PhysicalRect::new(0, -20, 150, 120));
        // Touching edges share no pixel.
        assert_eq!(a.intersection(&PhysicalRect::new(100, 0, 10, 10)), None);
    }

    #[test]
    fn a_drag_past_the_edge_stops_at_the_edge() {
        let size = (1920, 1080);
        let dragged = PhysicalRect::from_corners(p(1800, 1000), p(2500, 1400));
        assert_eq!(
            clamp_to_monitor(dragged, size),
            PhysicalRect::new(1800, 1000, 120, 80)
        );
        let back = PhysicalRect::from_corners(p(100, 100), p(-300, -50));
        assert_eq!(
            clamp_to_monitor(back, size),
            PhysicalRect::new(0, 0, 100, 100)
        );
        // The same with the end point clamped first.
        assert_eq!(
            PhysicalRect::from_corners(p(1800, 1000), clamp_point(p(2500, 1400), size)),
            PhysicalRect::new(1800, 1000, 120, 80)
        );
    }

    #[test]
    fn a_rectangle_outside_the_monitor_becomes_empty() {
        let outside = PhysicalRect::new(2000, 10, 50, 50);
        assert!(clamp_to_monitor(outside, (1920, 1080)).is_empty());
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let huge = PhysicalRect::new(i32::MAX - 1, 0, u32::MAX, 1);
        assert_eq!(huge.right(), i64::from(i32::MAX) - 1 + i64::from(u32::MAX));
        assert_eq!(
            huge.translate(10, 0),
            PhysicalRect::new(i32::MAX, 0, u32::MAX, 1)
        );
    }
}
