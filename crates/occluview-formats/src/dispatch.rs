//! Dispatch a file to its format reader by extension.
//!
//! This is the integration seam used by `occluview-app`, `occluview-cli`, and
//! `occluview-shell`. Each concrete reader (to be implemented in dedicated PRs
//! per the roadmap) is wired in here.

use crate::error::FormatError;
use crate::probe::FormatKind;
use occluview_core::Mesh;

/// Read `bytes` as the format indicated by `kind`, returning a [`Mesh`].
///
/// # Errors
/// - [`FormatError::Unsupported`] for formats whose reader is not implemented
///   yet (this stub returns that for everything until the readers land).
pub fn dispatch_by_kind(kind: FormatKind, _bytes: &[u8]) -> Result<Mesh, FormatError> {
    match kind {
        // TODO(formats/stl):  implement in `stl` module (binary + ASCII, mmap streaming).
        // TODO(formats/ply):  implement in `ply` module (vertex colors, BE/LE binary, ASCII).
        // TODO(formats/obj):  implement in `obj` module (+ lenient MTL, fan triangulation).
        // TODO(formats/gltf): implement in `gltf` module via cgltf (zip-slip-safe).
        // TODO(formats/threemf): implement in `threemf` module via lib3mf FFI.
        FormatKind::Stl | FormatKind::Ply | FormatKind::Obj | FormatKind::Gltf
        | FormatKind::Threemf => Err(FormatError::Malformed {
            format: "occluview-formats",
            offset: 0,
            reason: format!(
                "reader for {kind:?} not yet implemented (see ROADMAP and ADR-0004)"
            ),
        }),
    }
}

/// Convenience: read `bytes` using the reader selected by file extension.
///
/// # Errors
/// See [`FormatError`] and [`dispatch_by_kind`].
pub fn dispatch_by_extension(extension: &str, bytes: &[u8]) -> Result<Mesh, FormatError> {
    let kind = crate::probe::by_extension(extension)
        .ok_or(FormatError::Unsupported {
            extension: extension.to_string(),
        })?;
    dispatch_by_kind(kind, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_reader_returns_malformed() {
        let res = dispatch_by_extension("stl", &[0u8; 84]);
        let Err(FormatError::Malformed { reason, .. }) = res else {
            panic!("expected Malformed stub error, got {res:?}");
        };
        assert!(reason.contains("not yet implemented"));
    }

    #[test]
    fn unknown_extension_is_unsupported() {
        let res = dispatch_by_extension("xyz", &[0u8; 4]);
        assert!(matches!(res, Err(FormatError::Unsupported { .. })));
    }
}
