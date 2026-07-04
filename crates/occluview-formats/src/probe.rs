//! Format detection by extension + magic bytes.

use crate::error::FormatError;

/// The kind of format detected for a file.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FormatKind {
    /// Binary or ASCII STL.
    Stl,
    /// PLY (binary or ASCII).
    Ply,
    /// Wavefront OBJ (+ companion `.mtl`).
    Obj,
    /// glTF 2.0 — JSON `.gltf` or binary `.glb`.
    Gltf,
    /// 3MF (XML-in-ZIP).
    Threemf,
}

/// Map a file extension (lowercase, no dot) to a [`FormatKind`].
///
/// # Errors
/// Returns [`FormatError::Unsupported`] if the extension is unknown.
#[must_use]
pub fn by_extension(ext: &str) -> Option<FormatKind> {
    match ext {
        "stl" => Some(FormatKind::Stl),
        "ply" => Some(FormatKind::Ply),
        "obj" => Some(FormatKind::Obj),
        "gltf" | "glb" => Some(FormatKind::Gltf),
        "3mf" => Some(FormatKind::Threemf),
        _ => None,
    }
}

/// Probe both the extension and the leading magic bytes and return the most
/// likely [`FormatKind`]. Magic bytes win when extension and magic disagree
/// (some scanners mislabel files).
///
/// # Errors
/// - [`FormatError::Unsupported`] if neither extension nor magic match.
pub fn probe(extension: Option<&str>, magic: &[u8]) -> Result<FormatKind, FormatError> {
    // Magic-byte first, since scanners sometimes mislabel files.
    if magic.len() >= 4 {
        if &magic[..4] == b"glTF" {
            return Ok(FormatKind::Gltf);
        }
        // PK\x03\x04 = ZIP — used by 3MF.
        if magic[..4] == [0x50, 0x4B, 0x03, 0x04] {
            return Ok(FormatKind::Threemf);
        }
    }
    if magic.starts_with(b"ply\n")
        || magic.starts_with(b"ply\r\n")
        || magic.starts_with(b"ply\t")
        || magic.starts_with(b"ply ")
    {
        return Ok(FormatKind::Ply);
    }
    if magic.starts_with(b"solid") {
        // "solid" is the ASCII STL header — but binary STL can also start with
        // arbitrary 80-byte headers that occasionally begin with "solid".
        // Disambiguation happens in the STL reader; here we hint STL.
        return Ok(FormatKind::Stl);
    }
    // glTF .gltf (JSON) — probe for leading `{` or whitespace then `"asset"`.
    if magic.iter().take_while(|b| b.is_ascii_whitespace()).count() < magic.len()
        && magic.first() == Some(&b'{')
    {
        return Ok(FormatKind::Gltf);
    }

    // Fall back to the extension.
    if let Some(ext) = extension {
        if let Some(kind) = by_extension(ext) {
            return Ok(kind);
        }
    }

    Err(FormatError::Unsupported {
        extension: extension.unwrap_or("").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_map_covers_v1_formats() {
        assert_eq!(by_extension("stl"), Some(FormatKind::Stl));
        assert_eq!(by_extension("STL"), None); // caller lowercases; document behavior
        assert_eq!(by_extension("ply"), Some(FormatKind::Ply));
        assert_eq!(by_extension("obj"), Some(FormatKind::Obj));
        assert_eq!(by_extension("gltf"), Some(FormatKind::Gltf));
        assert_eq!(by_extension("glb"), Some(FormatKind::Gltf));
        assert_eq!(by_extension("3mf"), Some(FormatKind::Threemf));
        assert_eq!(by_extension("foo"), None);
    }

    #[test]
    fn probe_glb_magic_wins_without_extension() {
        let magic = b"glTF\x02\x00\x00\x00";
        assert_eq!(probe(None, magic).unwrap(), FormatKind::Gltf);
    }

    #[test]
    fn probe_ply_magic() {
        assert_eq!(
            probe(None, b"ply\nformat ascii 1.0\n").unwrap(),
            FormatKind::Ply
        );
    }

    #[test]
    fn probe_3mf_zip_magic() {
        let magic = [0x50, 0x4B, 0x03, 0x04, 0x00, 0x00];
        assert_eq!(probe(None, &magic).unwrap(), FormatKind::Threemf);
    }

    #[test]
    fn probe_falls_back_to_extension_when_magic_silent() {
        // Binary STL with an empty (zero) header — no recognizable magic.
        let magic = [0u8; 84];
        assert_eq!(probe(Some("stl"), &magic).unwrap(), FormatKind::Stl);
    }

    #[test]
    fn probe_returns_unsupported_for_unknown() {
        assert!(probe(Some("xyz"), &[1, 2, 3, 4]).is_err());
    }
}
