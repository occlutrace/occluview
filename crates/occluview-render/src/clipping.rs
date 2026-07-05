//! Cross-section / clipping-plane support for the cut-view feature.
//!
//! `ClipPlane` is a world-space plane defined by a unit normal and a
//! signed distance from the origin: points `p` where `dot(p, normal) -
//! distance < 0` are on the "below" side and get discarded by the fragment
//! shader. Setting `enabled = 0` disables clipping entirely (the identity
//! path — existing thumbnails and tests render identically).
//!
//! ## Stencil capping (Approach B, ADR-0011)
//!
//! For a **solid** cross-section (the cut surface appears filled, like 3D
//! Slicer and `MeshMixer`), the renderer runs a 3-pass stencil sequence before
//! the shaded draw: back faces increment the stencil, front faces decrement
//! it, then a cap polygon in the plane is drawn testing `stencil != 0`.
//! See [`crate::pipeline::Renderer`] and the cut-view ADR for the algorithm.
//!
//! ## Layout
//!
//! [`ClipPlane`] is 32 bytes, `#[repr(C)]`, Pod — bound at group 3, binding 0.

use bytemuck::{Pod, Zeroable};

/// A world-space clipping plane. Bound as a uniform at group 3, binding 0.
///
/// Fragments where `dot(world_pos, normal) - distance < 0` are discarded
/// (when `enabled != 0`). The "below" side is the cut-away side.
///
/// WGSL uniform structs follow std140 layout rules: `vec3` rounds up to 16
/// bytes, so the on-GPU size is 32 bytes. The Rust struct is padded to match
/// (`pad: [u32; 3]` = 12 bytes after the two u32 scalars, totaling 32).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ClipPlane {
    /// Unit-length plane normal (world space). Points away from the kept side.
    /// In std140 this vec3 occupies 16 bytes (padded to a vec4 boundary).
    pub normal: [f32; 3],
    /// Signed distance from the origin along the normal. `distance = 0` puts
    /// the plane through the world origin; positive moves it along `+normal`.
    pub distance: f32,
    /// `0` = clipping disabled (identity render). `1` = clip active.
    pub enabled: u32,
    /// Padding to round the struct to the std140-required 32 bytes.
    pub pad: [u32; 3],
}

impl ClipPlane {
    /// A disabled clip plane — the identity (no clipping). Used by the
    /// existing thumbnail / single-mesh render paths so they are unaffected
    /// by the clip-plane binding.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            normal: [0.0, 0.0, 1.0],
            distance: 0.0,
            enabled: 0,
            pad: [0, 0, 0],
        }
    }

    /// Construct an enabled clip plane from a unit normal and distance.
    ///
    /// `normal` is normalized internally (a zero-length normal is treated as
    /// `+Z`, which disables nothing because `enabled` stays as given — callers
    /// should pass a real normal).
    #[must_use]
    pub fn new(normal: [f32; 3], distance: f32) -> Self {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        let n = if len > 1e-6 {
            [normal[0] / len, normal[1] / len, normal[2] / len]
        } else {
            [0.0, 0.0, 1.0]
        };
        Self {
            normal: n,
            distance,
            enabled: 1,
            pad: [0, 0, 0],
        }
    }
}

impl Default for ClipPlane {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Configuration for a cut-view render. Owned by the app; passed to
/// [`crate::offscreen::Offscreen::render_with_cut`].
#[derive(Clone, Debug)]
pub struct CutViewSpec {
    /// The clipping plane. `enabled` is honored; set `0` for no cut.
    pub plane: ClipPlane,
    /// RGBA fill color (linear, 0..1) for the cut-surface cap polygon.
    /// Default is a warm gingiva-like pink.
    pub cap_color: [f32; 4],
    /// When `true`, skip the stencil cap passes — render only with fragment
    /// discard (Approach A, "hollow cut"). Useful as a fast preview and for
    /// point clouds (which have no closed surface to cap).
    pub show_hollow: bool,
}

impl Default for CutViewSpec {
    fn default() -> Self {
        Self {
            plane: ClipPlane::disabled(),
            // Gingiva-warm pink: #E8 4C 4B in sRGB, converted to linear.
            cap_color: [0.776, 0.182, 0.175, 1.0],
            show_hollow: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_plane_disabled_is_identity() {
        let c = ClipPlane::disabled();
        assert_eq!(c.enabled, 0);
    }

    #[test]
    fn clip_plane_normalizes() {
        let c = ClipPlane::new([0.0, 0.0, 5.0], 0.0);
        assert_eq!(c.normal, [0.0, 0.0, 1.0]);
        assert_eq!(c.enabled, 1);
    }

    #[test]
    fn clip_plane_zero_normal_falls_back_to_z() {
        let c = ClipPlane::new([0.0, 0.0, 0.0], 1.0);
        assert_eq!(c.normal, [0.0, 0.0, 1.0]);
        assert_eq!(c.distance, 1.0);
    }

    #[test]
    fn clip_plane_is_32_bytes_std140() {
        // std140: vec3 rounds up to 16-byte boundary, so the WGSL struct is
        // 16 (vec3 normal) + 4 (distance) + 4 (enabled) + 12 (pad to 32) = 32.
        // The Rust struct must match for the uniform buffer to validate.
        assert_eq!(std::mem::size_of::<ClipPlane>(), 32);
    }

    #[test]
    fn cut_view_spec_defaults() {
        let s = CutViewSpec::default();
        assert_eq!(s.plane.enabled, 0);
        assert!(!s.show_hollow);
        // Pink-ish.
        assert!(s.cap_color[0] > s.cap_color[1]);
        assert!(s.cap_color[0] > s.cap_color[2]);
    }
}
