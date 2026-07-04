//! Axis-aligned bounding box.
//!
//! Computed from a mesh's vertices; used for camera framing, scale-bar sizing,
//! and the (future) Properties tab in Explorer. Reported dimensions are in the
//! [`crate::units::Millimeters`] unit.

use crate::units::Millimeters;
use glam::Vec3;

/// An axis-aligned bounding box in OccluView's canonical frame (Y-up, RH).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl Aabb {
    /// An empty bounding box — the identity for [`Aabb::enclose`].
    pub const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    }

    /// Construct from explicit min/max corners.
    #[must_use]
    #[inline]
    pub const fn from_min_max(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Enclose a single point, returning the new box.
    #[inline]
    #[must_use]
    pub fn enclose_point(mut self, p: Vec3) -> Self {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
        self
    }

    /// Enclose an iterator of points. Returns [`Aabb::EMPTY`] if empty.
    #[inline]
    #[must_use]
    pub fn enclose_points(points: impl IntoIterator<Item = Vec3>) -> Self {
        points
            .into_iter()
            .fold(Self::EMPTY, |acc, p| acc.enclose_point(p))
    }

    /// True if no points have been enclosed yet.
    #[inline]
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.min.x > self.max.x
    }

    /// Geometric center.
    #[inline]
    #[must_use]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Per-axis extent (max - min).
    #[inline]
    #[must_use]
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    /// Per-axis dimensions in millimeters.
    ///
    /// Returned as `(width, height, depth)` where each is a [`Millimeters`].
    /// Useful for display ("38.2 × 22.1 × 18.7 mm") and the scale bar.
    #[inline]
    #[must_use]
    pub fn dimensions_mm(self) -> [Millimeters; 3] {
        let s = self.size();
        [
            Millimeters(s.x),
            Millimeters(s.y),
            Millimeters(s.z),
        ]
    }

    /// Half-diagonal length from the center — the radius of the tightest sphere.
    #[inline]
    #[must_use]
    pub fn half_diagonal(self) -> f32 {
        ((self.max - self.min) * 0.5).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_empty() {
        assert!(Aabb::EMPTY.is_empty());
    }

    #[test]
    fn enclose_single_point() {
        let b = Aabb::EMPTY.enclose_point(Vec3::new(1.0, 2.0, 3.0));
        assert!(!b.is_empty());
        assert_eq!(b.min, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(b.max, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn enclose_two_points_grows_bounds() {
        let b = Aabb::enclose_points([
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(2.0, 4.0, -3.0),
        ]);
        assert_eq!(b.min, Vec3::new(-1.0, 0.0, -3.0));
        assert_eq!(b.max, Vec3::new(2.0, 4.0, 0.0));
        assert_eq!(b.center(), Vec3::new(0.5, 2.0, -1.5));
    }

    #[test]
    fn dimensions_report_in_mm() {
        let b = Aabb::from_min_max(Vec3::ZERO, Vec3::new(10.0, 20.0, 30.0));
        let [w, h, d] = b.dimensions_mm();
        assert_eq!(w.as_mm(), 10.0);
        assert_eq!(h.as_mm(), 20.0);
        assert_eq!(d.as_mm(), 30.0);
    }

    #[test]
    fn half_diagonal_of_unit_cube() {
        let b = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        assert!((b.half_diagonal() - core::f32::consts::SQRT_2 * 0.5) < 1e-5);
    }
}
