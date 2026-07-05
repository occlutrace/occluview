//! Principal Component Analysis for **dental auto-orientation**.
//!
//! A scanner exports a mesh in whatever frame its sensor used. To frame a
//! correct occlusal thumbnail we need the chewing surface facing the camera.
//! PCA on the vertex covariance gives us the principal axes:
//!
//! - The axis of **smallest** variance is the thin direction — for a dental
//!   arch this is the occlusal normal (vertical, Y-up).
//! - The axis of **largest** variance is the arch's long (left-right) axis.
//! - The middle axis is buccal-lingual (front-back).
//!
//! [`principal_axes`] returns a rotation that maps the original frame into
//! OccluView's canonical dental frame, so that afterward the smallest-variance
//! axis points along +Y. Applying it to the mesh (or composing it into the
//! camera view matrix) makes [`crate::camera::Camera::frame_occlusal`] correct
//! regardless of how the source file was oriented.
//!
//! ## Implementation note
//!
//! glam does not ship an eigendecomposition, so we implement the **Jacobi
//! eigenvalue algorithm** for symmetric 3×3 matrices: deterministic, robust to
//! repeated eigenvalues (planar meshes, point clouds), and converges in a
//! handful of sweeps. No external dependency, no `unsafe`.

use glam::{Mat3, Vec3};
use std::ops::Add;

/// Solver threshold: off-diagonal magnitude below this counts as zero.
const EPS: f32 = 1e-6;
/// Maximum Jacobi sweeps before we give up (3×3 converges in ~10 worst case).
const MAX_SWEEPS: usize = 32;

/// Eigendecomposition of a real symmetric 3×3 matrix.
///
/// Returns eigenvalues paired with their unit eigenvectors, sorted by
/// eigenvalue **ascending** (smallest variance first). Eigenvectors form a
/// right-handed orthonormal basis when the input is positive semi-definite;
/// for indefinite inputs we still return an orthonormal set.
///
/// Internally uses the cyclic Jacobi algorithm.
#[must_use]
pub fn symmetric_eig(matrix: Mat3) -> [(f32, Vec3); 3] {
    // Work on a local copy. `a` holds the matrix being diagonalized; `v`
    // accumulates the rotation. Both row-major for clarity.
    let mut a = [
        [matrix.x_axis.x, matrix.y_axis.x, matrix.z_axis.x],
        [matrix.x_axis.y, matrix.y_axis.y, matrix.z_axis.y],
        [matrix.x_axis.z, matrix.y_axis.z, matrix.z_axis.z],
    ];
    let mut v = [[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    for _ in 0..MAX_SWEEPS {
        // Off-diagonal sum of squares.
        let off = a[0][1] * a[0][1] + a[0][2] * a[0][2] + a[1][2] * a[1][2];
        if off <= EPS * EPS {
            break;
        }
        // Zero each off-diagonal in turn (cyclic sweep).
        jacobi_rotate(&mut a, &mut v, 0, 1);
        jacobi_rotate(&mut a, &mut v, 0, 2);
        jacobi_rotate(&mut a, &mut v, 1, 2);
    }

    let mut eig = [
        (a[0][0], Vec3::new(v[0][0], v[1][0], v[2][0])),
        (a[1][1], Vec3::new(v[0][1], v[1][1], v[2][1])),
        (a[2][2], Vec3::new(v[0][2], v[1][2], v[2][2])),
    ];
    // Sort ascending by eigenvalue (insertion sort — n=3).
    eig.sort_by(|l, r| l.0.partial_cmp(&r.0).unwrap_or(std::cmp::Ordering::Equal));
    // Normalize eigenvectors defensively (they are already ~unit).
    for (lambda, axis) in &mut eig {
        let len = axis.length();
        if len > EPS {
            *axis /= len;
        }
        // Silence unused-mut on lambda in case of future normalization hooks.
        let _ = lambda;
    }
    ensure_right_handed(&mut eig);
    eig
}

/// Apply one Jacobi rotation that zeros element (p,q).
///
/// Names follow the standard Jacobi algorithm notation (matrix elements
/// `a[p][p]`→`app`, indices `p`/`q`); the pedantic lints are relaxed here to
/// keep the math readable.
#[allow(
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::needless_range_loop,
    clippy::many_single_char_names
)]
fn jacobi_rotate(a: &mut [[f32; 3]; 3], v: &mut [[f32; 3]; 3], p: usize, q: usize) {
    let apq = a[p][q];
    if apq.abs() <= f32::MIN_POSITIVE {
        return;
    }
    let app = a[p][p];
    let aqq = a[q][q];
    // Rotation angle: theta = 0.5 * atan2(2*apq, aqq - app).
    let theta = 0.5_f32 * (2.0 * apq).atan2(aqq - app);
    let c = theta.cos();
    let s = theta.sin();

    // Update A: A' = J^T A J, where J is the Givens rotation in (p,q).
    for k in 0..3 {
        let akp = a[k][p];
        let akq = a[k][q];
        a[k][p] = c * akp - s * akq;
        a[k][q] = s * akp + c * akq;
    }
    for k in 0..3 {
        let apk = a[p][k];
        let aqk = a[q][k];
        a[p][k] = c * apk - s * aqk;
        a[q][k] = s * apk + c * aqk;
    }
    // Force the targeted off-diagonal to exact zero against float drift.
    a[p][q] = 0.0;
    a[q][p] = 0.0;

    // Accumulate the rotation into V.
    for k in 0..3 {
        let vkp = v[k][p];
        let vkq = v[k][q];
        v[k][p] = c * vkp - s * vkq;
        v[k][q] = s * vkp + c * vkq;
    }
}

/// Flip the last eigenvector if the basis is left-handed, keeping it
/// orthonormal and right-handed (so the rotation has det = +1).
fn ensure_right_handed(eig: &mut [(f32, Vec3); 3]) {
    let cross = eig[0].1.cross(eig[1].1);
    if cross.dot(eig[2].1) < 0.0 {
        eig[2].1 = -eig[2].1;
    }
}

/// Compute the principal axes of a set of points and return a rotation matrix
/// that maps the points into OccluView's canonical dental frame.
///
/// After applying the returned rotation:
/// - The **+Y** axis is the occlusal normal (smallest-variance / thinnest
///   direction) — the chewing plane faces up.
/// - The **+X** axis is the arch's long (largest-variance) axis.
/// - **+Z** completes the right-handed basis (buccal-lingual).
///
/// Points are first recentered on their centroid; the caller is responsible
/// for translating if needed (the camera framer already recenteres on the
/// bounding box, so the rotation is what matters).
///
/// Returns the identity when there are fewer than two distinct points.
#[must_use]
#[allow(clippy::similar_names, clippy::cast_precision_loss)]
pub fn principal_axes(points: &[Vec3]) -> Mat3 {
    if points.len() < 2 {
        return Mat3::IDENTITY;
    }
    let n = points.len() as f32;
    let centroid = points.iter().copied().fold(Vec3::ZERO, Vec3::add) / n;

    // Symmetric covariance: C = sum((p - c)(p - c)^T) / n.
    let mut cxx = 0.0_f32;
    let mut cyy = 0.0;
    let mut czz = 0.0;
    let mut cxy = 0.0;
    let mut cxz = 0.0;
    let mut cyz = 0.0;
    for p in points {
        let d = *p - centroid;
        cxx += d.x * d.x;
        cyy += d.y * d.y;
        czz += d.z * d.z;
        cxy += d.x * d.y;
        cxz += d.x * d.z;
        cyz += d.y * d.z;
    }
    let cov = Mat3::from_cols(
        Vec3::new(cxx / n, cxy / n, cxz / n),
        Vec3::new(cxy / n, cyy / n, cyz / n),
        Vec3::new(cxz / n, cyz / n, czz / n),
    );

    let eig = symmetric_eig(cov);
    // eig is sorted ascending: [0] = smallest (→ +Y), [2] = largest (→ +X).
    // Build R whose rows are the canonical axes expressed in the original
    // frame, i.e. R maps original-frame vectors into canonical frame.
    // Row 0 = +X = largest-variance eigenvector.
    // Row 1 = +Y = smallest-variance eigenvector (occlusal normal).
    // Row 2 = +Z = middle eigenvector (buccal-lingual).
    let x_axis = eig[2].1; // largest
    let y_axis = eig[0].1; // smallest
    let z_axis = eig[1].1; // middle
                           // from_cols treats arguments as columns; we want rows, so transpose by
                           // building from the transposed vectors.
    Mat3::from_cols(x_axis, y_axis, z_axis).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matrix_eigen_decomposition() {
        let eig = symmetric_eig(Mat3::IDENTITY);
        // All eigenvalues = 1; eigenvectors form an orthonormal basis.
        for (lambda, _) in &eig {
            assert!((lambda - 1.0).abs() < 1e-4, "lambda = {lambda}");
        }
        // Orthonormality of eigenvectors.
        let v0 = eig[0].1;
        let v1 = eig[1].1;
        let v2 = eig[2].1;
        assert!(v0.dot(v1).abs() < 1e-4);
        assert!(v0.dot(v2).abs() < 1e-4);
        assert!(v1.dot(v2).abs() < 1e-4);
        assert!((v0.length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn diagonal_matrix_returns_its_diagonal() {
        let m = Mat3::from_cols(
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 2.0),
        );
        let eig = symmetric_eig(m);
        // Sorted ascending: 1, 2, 3.
        assert!((eig[0].0 - 1.0).abs() < 1e-4);
        assert!((eig[1].0 - 2.0).abs() < 1e-4);
        assert!((eig[2].0 - 3.0).abs() < 1e-4);
    }

    #[test]
    fn eigenvectors_reconstruct_the_matrix() {
        // A non-trivial symmetric matrix.
        let m = Mat3::from_cols(
            Vec3::new(4.0, 1.0, 2.0),
            Vec3::new(1.0, 3.0, 0.5),
            Vec3::new(2.0, 0.5, 5.0),
        );
        let eig = symmetric_eig(m);
        // Reconstruct: M = sum lambda_i * (v_i v_i^T).
        let mut recon = Mat3::ZERO;
        for (lambda, v) in &eig {
            recon += *lambda * Mat3::from_cols(v * v.x, v * v.y, v * v.z);
        }
        for i in 0..3 {
            for j in 0..3 {
                let orig = m.col(i)[j];
                let got = recon.col(i)[j];
                assert!(
                    (orig - got).abs() < 1e-3,
                    "m[{i}][{j}] orig={orig} got={got}"
                );
            }
        }
    }

    #[test]
    fn right_handed_basis() {
        let m = Mat3::from_cols(
            Vec3::new(4.0, 1.0, 2.0),
            Vec3::new(1.0, 3.0, 0.5),
            Vec3::new(2.0, 0.5, 5.0),
        );
        let eig = symmetric_eig(m);
        let det = eig[0].1.cross(eig[1].1).dot(eig[2].1);
        assert!(det > 1.0 - 1e-3, "basis is left-handed, det = {det}");
    }

    #[test]
    fn principal_axes_aligns_thin_axis_to_y() {
        // A flat slab lying in the XZ plane (thin in Y) should map +Y to the
        // original thin axis. Build points spread in X and Z, thin in Y.
        let mut pts = Vec::new();
        for x in -10..=10 {
            for z in -10..=10 {
                pts.push(Vec3::new(x as f32, 0.0, z as f32));
            }
        }
        let r = principal_axes(&pts);
        // After rotation, the original Y axis (0,1,0) — the thin direction —
        // should map near a canonical axis. Concretely: R * (0,1,0) should be
        // near (0, ±1, 0) since the slab is already aligned.
        let mapped = r * Vec3::new(0.0, 1.0, 0.0);
        assert!(mapped.x.abs() < 1e-3, "x leak: {mapped}");
        assert!(mapped.z.abs() < 1e-3, "z leak: {mapped}");
        assert!((mapped.y.abs() - 1.0).abs() < 1e-3, "y not unit: {mapped}");
    }

    #[test]
    fn principal_axes_recovers_a_rotation() {
        // Take a 3D cloud with three distinct axis spreads, rotate it by a
        // known rotation, and check that principal_axes recovers a frame
        // aligned with the cloud's natural axes.
        let mut pts = Vec::new();
        // Cloud: long in X (0..20), medium in Y (0..4), thin in Z (0..1).
        for x in 0..20 {
            for y in 0..4 {
                for z in 0..2 {
                    pts.push(Vec3::new(x as f32, y as f32, z as f32));
                }
            }
        }
        // Rotate the whole cloud 30° around Z.
        let rot = Mat3::from_rotation_z(30.0_f32.to_radians());
        let rotated: Vec<Vec3> = pts.iter().map(|p| rot * *p).collect();
        let r = principal_axes(&rotated);
        // The thin axis (original Z) is invariant under Z-rotation, so the
        // recovered canonical Y (occlusal normal) must be ±world Z.
        let recovered_y = Vec3::new(r.x_axis.y, r.y_axis.y, r.z_axis.y);
        assert!(recovered_y.x.abs() < 1e-3, "y has x: {recovered_y}");
        assert!(recovered_y.y.abs() < 1e-3, "y has y: {recovered_y}");
        assert!(
            (recovered_y.z.abs() - 1.0).abs() < 1e-3,
            "y not unit-Z: {recovered_y}"
        );
        // The recovered X (long axis) stays in the XY plane.
        let recovered_x = Vec3::new(r.x_axis.x, r.y_axis.x, r.z_axis.x);
        assert!(recovered_x.z.abs() < 1e-3, "x has z: {recovered_x}");
    }

    #[test]
    fn degenerate_single_point_returns_identity() {
        let r = principal_axes(&[Vec3::new(1.0, 2.0, 3.0)]);
        assert_eq!(r, Mat3::IDENTITY);
    }

    #[test]
    fn degenerate_collinear_points_do_not_panic() {
        // All points on a line — covariance has rank 1, two zero eigenvalues.
        let pts = vec![
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(2.0, 2.0, 2.0),
            Vec3::new(3.0, 3.0, 3.0),
        ];
        let r = principal_axes(&pts);
        // Should be a valid rotation (orthonormal).
        let det = r.x_axis.cross(r.y_axis).dot(r.z_axis);
        assert!((det - 1.0).abs() < 1e-3, "not a rotation, det = {det}");
    }
}
