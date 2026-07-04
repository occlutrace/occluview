//! The safe, Windows-agnostic core of thumbnail generation.
//!
//! The COM class (to be implemented in a follow-up PR) calls into
//! [`render_thumbnail`] - this function does all the work and is unit-testable
//! without Windows. It loads the file via `occluview-formats`, frames the
//! camera with the dental occlusal default, and renders an offscreen frame via
//! `occluview-render`.

use crate::ShellError;
use glam::{Mat4, Vec3};
use occluview_core::Camera;
use occluview_formats::dispatch_by_extension;
use occluview_render::{GpuCamera, Offscreen, ThumbnailSpec};

/// Load `bytes` (a file with the given lowercase extension, no dot) and render
/// a thumbnail per `spec`. Returns RGBA8 pixels in row-major order, length
/// `spec.size_px * spec.size_px * 4`, top-to-bottom.
///
/// Blocking: runs the offscreen render to completion on the calling thread.
/// The COM stub invokes this on a worker thread (ADR-0005 addendum) under a
/// Job Object with a watchdog.
///
/// # Errors
/// See [`ShellError`]. The COM layer translates any error into a branded
/// placeholder returned to Windows.
pub fn render_thumbnail(
    extension: &str,
    bytes: &[u8],
    spec: ThumbnailSpec,
) -> Result<Vec<u8>, ShellError> {
    let mut mesh = dispatch_by_extension(extension, bytes)?;
    let bbox = mesh.bbox();
    let cam = Camera::default().frame_occlusal(bbox, 45.0_f32.to_radians());
    let view = Mat4::look_at_rh(cam.eye(), cam.target, Vec3::Y);
    let proj = Mat4::perspective_rh(cam.fovy, 1.0, cam.near, cam.far);
    let gpu_cam = GpuCamera::new(view, proj, Vec3::new(0.4, 0.8, 0.5), cam.eye());

    let offscreen = pollster::block_on(Offscreen::new())?;
    let pixels = pollster::block_on(offscreen.render(&mesh, &gpu_cam, spec))?;
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_extension_is_a_shell_error() {
        // The render path should surface an error (which the COM layer turns
        // into a placeholder) rather than panic or fake success.
        let res = render_thumbnail("xyz", &[0u8; 4], ThumbnailSpec::default());
        assert!(matches!(res, Err(ShellError::Format(_))));
    }

    #[test]
    fn malformed_stl_returns_format_error_without_panic() {
        // Truncated STL header (fewer than 84 bytes) -> FormatError, not panic.
        let res = render_thumbnail("stl", &[0u8; 10], ThumbnailSpec::default());
        assert!(matches!(res, Err(ShellError::Format(_))));
    }
}
