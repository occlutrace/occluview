//! Camera model and the dental **occlusal default** framing.
//!
//! The default camera looks down onto the occlusal plane — the chewing surface
//! — fit to the mesh bounding box (ADR-0009). This is the single most visible
//! "this is a dental tool" signal, and it is shared by the app and the thumbnail
//! renderer so the two match pixel-for-pixel.

use crate::bbox::Aabb;
use glam::{Quat, Vec3};

/// An orbital camera, the natural model for inspecting a mesh.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera {
    /// World-space point the camera orbits around.
    pub target: Vec3,
    /// Distance from target to eye, in millimeters.
    pub distance: f32,
    /// Yaw (around world Y) and pitch (from horizontal plane), in radians.
    pub yaw: f32,
    pub pitch: f32,
    /// Vertical field of view, in radians.
    pub fovy: f32,
    /// Near plane, millimeters.
    pub near: f32,
    /// Far plane, millimeters.
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 100.0,
            yaw: 0.0,
            // Looking slightly down from above — the occlusal bias.
            pitch: 1.0,
            fovy: 45.0_f32.to_radians(),
            near: 0.1,
            far: 10_000.0,
        }
    }
}

impl Camera {
    /// Position of the eye in world space.
    #[must_use]
    pub fn eye(self) -> Vec3 {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        // Orbit: eye = target + distance * direction.
        let dir = Vec3::new(cp * sy, sp, cp * cy);
        self.target + dir * self.distance
    }

    /// Frame a bounding box with the **occlusal default** orientation.
    ///
    /// The occlusal view looks down the mesh's vertical (Y) axis onto the XZ
    /// plane, which corresponds to the chewing surface for a dental arch lying
    /// in XZ. The exact up-vector heuristic for arbitrary meshes is a follow-up
    /// (see ROADMAP); for the common dental case this framing is correct.
    #[must_use]
    pub fn frame_occlusal(mut self, bbox: Aabb, fovy: f32) -> Self {
        if bbox.is_empty() {
            return self;
        }
        let center = bbox.center();
        let size = bbox.size();
        // Half-extent of the bbox in the occlusal (XZ) plane.
        let planar_half = 0.5_f32 * size.x.max(size.z);
        // Vertical half-extent — keep the arch depth in view too.
        let vertical_half = 0.5_f32 * size.y;
        let radius = planar_half.hypot(vertical_half).max(1.0);

        // Place the camera above, looking down at the occlusal plane.
        self.target = center;
        self.pitch = 0.6; // ~34° from horizontal: occlusal bias, not straight down
        self.yaw = 0.0;
        self.fovy = fovy;
        // Fit so the bbox radius fills ~70% of the half-FOV.
        let half_fov = 0.5 * fovy;
        self.distance = if half_fov > 1e-5 {
            radius / half_fov.tan() / 0.7
        } else {
            radius * 2.0
        };
        // Conservative near/far in millimeters around the bbox.
        self.near = self.distance * 0.01;
        self.far = self.distance * 100.0 + radius * 4.0;
        self
    }
}

/// Default occlusal-bias orientation quaternion (for direct GL placement if a
/// caller doesn't use the orbital camera).
#[must_use]
pub fn occlusal_orientation() -> Quat {
    Quat::from_rotation_y(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_bbox() -> Aabb {
        Aabb::from_min_max(Vec3::new(-10.0, -10.0, -10.0), Vec3::new(10.0, 10.0, 10.0))
    }

    #[test]
    fn eye_orbits_target() {
        let c = Camera {
            target: Vec3::ZERO,
            distance: 10.0,
            yaw: 0.0,
            pitch: 0.0,
            ..Camera::default()
        };
        let eye = c.eye();
        // Pitch=0, yaw=0 → eye at (0, 0, +distance) (looking at origin from +Z).
        assert!((eye - Vec3::new(0.0, 0.0, 10.0)).length() < 1e-4, "eye={eye}");
    }

    #[test]
    fn frame_occlusal_centers_on_bbox() {
        let c = Camera::default().frame_occlusal(cube_bbox(), 45.0_f32.to_radians());
        assert!((c.target - Vec3::ZERO).length() < 1e-4);
        // Distance must be positive and greater than the bbox half-extent.
        assert!(c.distance > 10.0);
    }

    #[test]
    fn frame_occlusal_handles_empty_bbox() {
        let c = Camera::default().frame_occlusal(Aabb::EMPTY, 45.0_f32.to_radians());
        // Empty bbox: camera is unchanged.
        assert_eq!(c, Camera::default());
    }

    #[test]
    fn near_far_bracket_the_scene() {
        let c = Camera::default().frame_occlusal(cube_bbox(), 45.0_f32.to_radians());
        assert!(c.near < c.distance);
        assert!(c.far > c.distance + 40.0);
    }
}
