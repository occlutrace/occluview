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

    /// **Axial** preset (transverse/horizontal cut): normal = `+Y`, the
    /// chewing-surface plane. Distance 0 = through the bbox center. Positive
    /// distance moves the cut upward (toward the crown tips). This is the
    /// primary dental cross-section view.
    ///
    /// `offset_mm` is relative to the bbox center, in millimeters.
    #[must_use]
    pub fn axial(offset_mm: f32) -> Self {
        Self::new([0.0, 1.0, 0.0], offset_mm)
    }

    /// **Coronal** preset (frontal cut): normal = `+Z`, the buccal-lingual
    /// plane. Distance 0 = through the bbox center.
    #[must_use]
    pub fn coronal(offset_mm: f32) -> Self {
        Self::new([0.0, 0.0, 1.0], offset_mm)
    }

    /// **Sagittal** preset (midline cut): normal = `+X`, the mesial-distal
    /// plane. Distance 0 = through the bbox center (midline).
    #[must_use]
    pub fn sagittal(offset_mm: f32) -> Self {
        Self::new([1.0, 0.0, 0.0], offset_mm)
    }

    /// **Custom** preset: a plane rotated by yaw (around Y) and pitch (around
    /// X) from the axial (`+Y` normal) orientation, for oblique cuts.
    ///
    /// `yaw_rad` / `pitch_rad` are in radians. At (0, 0) the normal is `+Y`
    /// (same as axial). Pitch=90° rotates `+Y` to `+Z`; yaw then rotates the
    /// horizontal component around Y.
    #[must_use]
    pub fn custom(yaw_rad: f32, pitch_rad: f32, offset_mm: f32) -> Self {
        // Start from +Y = [0,1,0]. Pitch around X: Y -> [0, cos(p), sin(p)].
        // Yaw around Y rotates the (x,z) components: x = sin(y)*sin(p),
        // z = cos(y)*sin(p), y unchanged = cos(p).
        let sp = pitch_rad.sin();
        let cp = pitch_rad.cos();
        let n = [yaw_rad.sin() * sp, cp, yaw_rad.cos() * sp];
        Self::new(n, offset_mm)
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

/// Build a cap quad: 4 position vertices + 2 triangles forming a large
/// square centered on the plane origin, lying in the plane's tangent space,
/// oversized to `half_extent` on each side so the stencil test clips it to
/// the cross-section.
///
/// The plane origin is `plane.normal * plane.distance`. Two orthonormal
/// basis vectors `u`/`v` span the plane.
///
/// Returns `(vertices: Vec<[f32;3]>, indices: Vec<u32>)` — 4 verts, 6 indices
/// (2 triangles).
#[must_use]
pub fn cap_quad(plane: &ClipPlane, half_extent: f32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let origin = clipping_inner::plane_origin(plane);
    let (u, v) = clipping_inner::orthonormal_basis(&plane.normal);
    let h = half_extent;
    // Four corners of a square in the plane.
    let c0 = [
        origin[0] + h * (u[0] + v[0]),
        origin[1] + h * (u[1] + v[1]),
        origin[2] + h * (u[2] + v[2]),
    ];
    let c1 = [
        origin[0] + h * (u[0] - v[0]),
        origin[1] + h * (u[1] - v[1]),
        origin[2] + h * (u[2] - v[2]),
    ];
    let c2 = [
        origin[0] + h * (-u[0] - v[0]),
        origin[1] + h * (-u[1] - v[1]),
        origin[2] + h * (-u[2] - v[2]),
    ];
    let c3 = [
        origin[0] + h * (-u[0] + v[0]),
        origin[1] + h * (-u[1] + v[1]),
        origin[2] + h * (-u[2] + v[2]),
    ];
    (vec![c0, c1, c2, c3], vec![0, 1, 2, 0, 2, 3])
}

/// Internal math helpers, kept in a submodule so the public surface of
/// `clipping` stays focused on the high-level types.
mod clipping_inner {
    /// Compute the plane origin: `normal * distance`.
    pub(super) fn plane_origin(plane: &super::ClipPlane) -> [f32; 3] {
        [
            plane.normal[0] * plane.distance,
            plane.normal[1] * plane.distance,
            plane.normal[2] * plane.distance,
        ]
    }

    /// Two orthonormal vectors spanning the plane perpendicular to `normal`.
    /// Picks an arbitrary up vector that isn't parallel to `normal`, then
    /// cross-products.
    pub(super) fn orthonormal_basis(normal: &[f32; 3]) -> ([f32; 3], [f32; 3]) {
        // Pick a helper not parallel to normal.
        let helper = if normal[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        // u = normalize(cross(normal, helper))
        let u = normalize(&cross(normal, &helper));
        // v = cross(normal, u) (already unit since normal and u are orthonormal)
        let v = cross(normal, &u);
        (u, v)
    }

    fn cross(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn normalize(a: &[f32; 3]) -> [f32; 3] {
        let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        if len < 1e-6 {
            return [0.0, 0.0, 1.0];
        }
        [a[0] / len, a[1] / len, a[2] / len]
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

    #[test]
    fn orthonormal_basis_is_perpendicular() {
        let n = [0.0, 0.0, 1.0];
        let (u, v) = super::clipping_inner::orthonormal_basis(&n);
        // u and v must be perpendicular to n (dot ~ 0).
        assert!(dot3(&u, &n).abs() < 1e-5);
        assert!(dot3(&v, &n).abs() < 1e-5);
        // u and v must be perpendicular to each other.
        assert!(dot3(&u, &v).abs() < 1e-5);
        // u and v must be unit length.
        assert!((len3(&u) - 1.0).abs() < 1e-5);
        assert!((len3(&v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn orthonormal_basis_handles_vertical_normal() {
        // normal = +Y (the degenerate helper case) must still produce a valid basis.
        let n = [0.0, 1.0, 0.0];
        let (u, v) = super::clipping_inner::orthonormal_basis(&n);
        assert!(dot3(&u, &n).abs() < 1e-5);
        assert!(dot3(&v, &n).abs() < 1e-5);
    }

    #[test]
    fn cap_quad_produces_4_verts_2_triangles() {
        let plane = ClipPlane::new([0.0, 1.0, 0.0], 0.0);
        let (verts, indices) = cap_quad(&plane, 10.0);
        assert_eq!(verts.len(), 4);
        assert_eq!(indices.len(), 6);
        // All verts lie in the Y=0 plane (since normal=+Y, distance=0).
        for v in &verts {
            assert!(v[1].abs() < 1e-5, "vert not in plane: {v:?}");
        }
    }

    #[test]
    fn axial_preset_is_y_normal() {
        let p = ClipPlane::axial(0.0);
        assert_eq!(p.normal, [0.0, 1.0, 0.0]);
        assert_eq!(p.enabled, 1);
    }

    #[test]
    fn coronal_preset_is_z_normal() {
        let p = ClipPlane::coronal(5.0);
        assert_eq!(p.normal, [0.0, 0.0, 1.0]);
        assert_eq!(p.distance, 5.0);
    }

    #[test]
    fn sagittal_preset_is_x_normal() {
        let p = ClipPlane::sagittal(-3.0);
        assert_eq!(p.normal, [1.0, 0.0, 0.0]);
        assert_eq!(p.distance, -3.0);
    }

    #[test]
    fn custom_preset_at_zero_rotation_is_axial() {
        // yaw=0, pitch=0 should give +Y normal (same as axial).
        let p = ClipPlane::custom(0.0, 0.0, 0.0);
        assert!((p.normal[0]).abs() < 1e-5);
        assert!((p.normal[1] - 1.0).abs() < 1e-5);
        assert!((p.normal[2]).abs() < 1e-5);
    }

    #[test]
    fn custom_preset_pitch_rotates_normal() {
        // pitch = 90° should rotate +Y to +Z.
        let p = ClipPlane::custom(0.0, std::f32::consts::FRAC_PI_2, 0.0);
        assert!((p.normal[1]).abs() < 1e-5, "y should be ~0: {:?}", p.normal);
        assert!(
            (p.normal[2] - 1.0).abs() < 1e-5,
            "z should be ~1: {:?}",
            p.normal
        );
    }

    #[test]
    fn presets_are_enabled() {
        assert_eq!(ClipPlane::axial(0.0).enabled, 1);
        assert_eq!(ClipPlane::coronal(0.0).enabled, 1);
        assert_eq!(ClipPlane::sagittal(0.0).enabled, 1);
        assert_eq!(ClipPlane::custom(0.1, 0.1, 0.0).enabled, 1);
    }

    fn dot3(a: &[f32; 3], b: &[f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn len3(a: &[f32; 3]) -> f32 {
        dot3(a, a).sqrt()
    }
}
