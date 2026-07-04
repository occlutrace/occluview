//! GPU-side camera uniform, matching the WGSL `Camera` struct.
//!
//! Layout (160 bytes total): two 4x4 float matrices (view, projection), then a
//! 3-component light direction plus one float of padding, then a 3-component
//! camera position plus one float of padding. The `#[repr(C)]` field order
//! plus the explicit padding satisfies the WGSL uniform-layout alignment rule
//! (a 3-component vector followed by a scalar occupies 16 bytes).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// GPU uniform: view + projection + lighting, uploaded once per frame.
///
/// The `_pad0`/`_pad1` fields are explicit alignment padding required by the
/// `WGSL` uniform layout; they are public because `#[repr(C)]` exposes the
/// full layout, and underscore-prefixed because they carry no semantic value.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[allow(clippy::struct_field_names, dead_code)]
pub struct GpuCamera {
    /// View matrix (world -> camera).
    pub view: [f32; 16],
    /// Projection matrix (camera -> clip).
    pub projection: [f32; 16],
    /// Direction TO the light from the scene (unit length expected).
    pub light_dir: [f32; 3],
    /// Struct padding to satisfy WGSL vec3+scalar alignment.
    pub pad0: f32,
    /// Camera position in world space (for specular; unused in v1 Lambertian).
    pub camera_pos: [f32; 3],
    /// Struct padding.
    pub pad1: f32,
}

impl GpuCamera {
    /// Build from a `glam` view/projection plus lighting parameters.
    #[must_use]
    pub fn new(view: Mat4, projection: Mat4, light_dir: Vec3, camera_pos: Vec3) -> Self {
        Self {
            view: view.to_cols_array(),
            projection: projection.to_cols_array(),
            light_dir: light_dir.to_array(),
            pad0: 0.0,
            camera_pos: camera_pos.to_array(),
            pad1: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn size_matches_wgsl_layout() {
        // 2 matrices (64 each) + 2 * (vec3 + pad) (16 each) = 160 bytes.
        assert_eq!(size_of::<GpuCamera>(), 160);
    }

    #[test]
    fn constructs_from_glam() {
        let c = GpuCamera::new(Mat4::IDENTITY, Mat4::IDENTITY, Vec3::Y, Vec3::ZERO);
        assert_eq!(c.light_dir, [0.0, 1.0, 0.0]);
        assert_eq!(c.pad0, 0.0);
    }
}
