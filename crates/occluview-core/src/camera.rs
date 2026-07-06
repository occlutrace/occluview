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
    /// Yaw (around world Y), in radians.
    pub yaw: f32,
    /// Pitch (elevation from the horizontal plane), in radians.
    pub pitch: f32,
    /// Vertical field of view, in radians.
    pub fovy: f32,
    /// Near plane, millimeters.
    pub near: f32,
    /// Far plane, millimeters.
    pub far: f32,
}

/// Named camera presets exposed by the desktop viewer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CameraPreset {
    /// Dental default, looking onto the occlusal plane.
    Occlusal,
    /// Frontal view from +Z.
    Front,
    /// Right-side view from +X.
    Right,
    /// Left-side view from -X.
    Left,
}

impl CameraPreset {
    /// Stable toolbar order.
    pub const ALL: [Self; 4] = [Self::Occlusal, Self::Front, Self::Right, Self::Left];

    /// Short UI label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Occlusal => "Occlusal",
            Self::Front => "Front",
            Self::Right => "Right",
            Self::Left => "Left",
        }
    }

    /// Build a camera that frames the provided bbox from this preset.
    #[must_use]
    pub fn frame_bbox(self, bbox: Aabb, fovy: f32) -> Camera {
        match self {
            Self::Occlusal => Camera::default().frame_occlusal(bbox, fovy),
            Self::Front => frame_planar(bbox, fovy, 0.0, 0.0),
            Self::Right => frame_planar(bbox, fovy, core::f32::consts::FRAC_PI_2, 0.0),
            Self::Left => frame_planar(bbox, fovy, -core::f32::consts::FRAC_PI_2, 0.0),
        }
    }
}

fn frame_planar(bbox: Aabb, fovy: f32, yaw: f32, pitch: f32) -> Camera {
    let mut camera = Camera::default();
    if bbox.is_empty() {
        return camera;
    }

    let size = bbox.size();
    let radius = (0.5 * size.length()).max(1.0);
    let half_fov = 0.5 * fovy;

    camera.target = bbox.center();
    camera.yaw = yaw;
    camera.pitch = pitch;
    camera.fovy = fovy;
    camera.distance = if half_fov > 1e-5 {
        radius / half_fov.tan() / 0.7
    } else {
        radius * 2.0
    };
    camera.near = camera.distance * 0.01;
    camera.far = camera.distance * 100.0 + radius * 4.0;
    camera
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
        assert!(
            (eye - Vec3::new(0.0, 0.0, 10.0)).length() < 1e-4,
            "eye={eye}"
        );
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

    #[test]
    fn camera_presets_have_toolbar_labels() {
        let labels = CameraPreset::ALL.map(CameraPreset::label);
        assert_eq!(labels, ["Occlusal", "Front", "Right", "Left"]);
    }

    #[test]
    fn occlusal_preset_matches_default_framing() {
        let bbox = cube_bbox();
        let fovy = 45.0_f32.to_radians();
        let preset = CameraPreset::Occlusal.frame_bbox(bbox, fovy);
        let direct = Camera::default().frame_occlusal(bbox, fovy);

        assert_eq!(preset, direct);
    }

    #[test]
    fn front_preset_views_from_positive_z() {
        let c = CameraPreset::Front.frame_bbox(cube_bbox(), 45.0_f32.to_radians());
        let eye = c.eye();

        assert!((eye.x - c.target.x).abs() < 1e-4, "eye={eye}");
        assert!((eye.y - c.target.y).abs() < 1e-4, "eye={eye}");
        assert!(eye.z > c.target.z + 10.0, "eye={eye}");
    }

    #[test]
    fn right_and_left_presets_view_from_opposite_x_sides() {
        let right = CameraPreset::Right.frame_bbox(cube_bbox(), 45.0_f32.to_radians());
        let left = CameraPreset::Left.frame_bbox(cube_bbox(), 45.0_f32.to_radians());

        assert!(right.eye().x > right.target.x + 10.0);
        assert!(left.eye().x < left.target.x - 10.0);
        assert!((right.eye().z - right.target.z).abs() < 1e-4);
        assert!((left.eye().z - left.target.z).abs() < 1e-4);
    }
}
