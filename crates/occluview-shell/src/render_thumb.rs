//! The safe, Windows-agnostic core of thumbnail generation.
//!
//! The COM class (to be implemented in a follow-up PR) calls into
//! [`render_thumbnail`] — this function does all the work and is unit-testable
//! without Windows. It loads the file via `occluview-formats`, renders an
//! offscreen frame via `occluview-render`, and returns RGBA pixels.

use crate::ShellError;
use occluview_formats::{dispatch_by_extension, FormatError};
use occluview_render::{Offscreen, ThumbnailSpec};

/// Load `bytes` (a file with the given lowercase extension, no dot) and render
/// a thumbnail per `spec`. Returns RGBA8 pixels in row-major order, length
/// `spec.size_px * spec.size_px * 4`.
///
/// # Errors
/// See [`ShellError`]. The COM layer translates any error into a branded
/// placeholder returned to Windows.
pub fn render_thumbnail(
    extension: &str,
    bytes: &[u8],
    spec: ThumbnailSpec,
) -> Result<Vec<u8>, ShellError> {
    let mesh = dispatch_by_extension(extension, bytes).map_err(|e| match e {
        // Re-wrap so the shell layer owns the error type, not the format layer.
        FormatError::Unsupported { .. } => FormatError::Unsupported {
            extension: extension.to_string(),
        },
        other => other,
    })?;
    let pixels = Offscreen.render(&mesh, spec)?;
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
    fn stub_render_surfaces_as_render_error() {
        // Even a "supported" extension returns an error today because no loader
        // is implemented yet — the shell layer must translate that to a
        // placeholder, never propagate a crash.
        let res = render_thumbnail("stl", &[0u8; 84], ThumbnailSpec::default());
        assert!(res.is_err());
    }
}
