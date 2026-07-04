//! OFF reader (Object File Format).
//!
//! Standard Princeton OFF: `OFF BINARY\n` header + 3x LE i32 counts + LE f64
//! positions + LE i32 face indices; ASCII variant also supported. N-gon faces
//! are fan-triangulated. Note: the exocad CAD suite emits a non-standard
//! binary OFF variant (BE floats, compressed) that is NOT supported here.
//!
//! Index/count casts are allowed at module scope.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use crate::error::FormatError;
use glam::Vec3;
use occluview_core::{Mesh, MeshBuilder, Vertex};

/// Read an OFF file from raw bytes.
///
/// # Errors
/// - [`FormatError::BadSignature`] if not an OFF file.
/// - [`FormatError::Malformed`] for truncated or structurally invalid data.
/// - [`FormatError::Core`] for index-out-of-range.
pub fn read(bytes: &[u8]) -> Result<Mesh, FormatError> {
    // Detect ASCII vs binary by the first line.
    if bytes.starts_with(b"OFF BINARY") {
        read_binary(bytes)
    } else if bytes.starts_with(b"OFF")
        || bytes.starts_with(b"OFF\n")
        || bytes.starts_with(b"OFF\r")
        || bytes.starts_with(b"OFFST")
    {
        // "OFF", "OFF\n", "OFF\r\n", or "OFF ST" (with normals) — ASCII.
        read_ascii(bytes)
    } else {
        Err(FormatError::BadSignature {
            format: "OFF",
            offset: 0,
        })
    }
}

fn read_binary(bytes: &[u8]) -> Result<Mesh, FormatError> {
    // Skip the "OFF BINARY\n" header (10 bytes).
    let header = b"OFF BINARY\n";
    let cursor_start = header.len();
    if bytes.len() < cursor_start + 12 {
        return Err(FormatError::Truncated {
            format: "OFF (binary)",
            expected: cursor_start + 12,
            got: bytes.len(),
        });
    }
    let mut cur = cursor_start;
    let read_i32 = |b: &[u8], off: &mut usize| -> Result<i32, FormatError> {
        let arr: [u8; 4] = b[*off..*off + 4]
            .try_into()
            .map_err(|_| FormatError::Truncated {
                format: "OFF (binary)",
                expected: *off + 4,
                got: b.len(),
            })?;
        *off += 4;
        Ok(i32::from_le_bytes(arr))
    };

    let v_count = read_i32(bytes, &mut cur)? as usize;
    let f_count = read_i32(bytes, &mut cur)? as usize;
    let _e_count = read_i32(bytes, &mut cur)? as usize; // edges: unused

    let mut positions: Vec<Vec3> = Vec::with_capacity(v_count);
    for _ in 0..v_count {
        let x = read_f64_le(bytes, &mut cur)?;
        let y = read_f64_le(bytes, &mut cur)?;
        let z = read_f64_le(bytes, &mut cur)?;
        positions.push(Vec3::new(x as f32, y as f32, z as f32));
    }

    let mut builder = MeshBuilder::new().with_name("OFF");
    // Pre-push all vertices so face indices reference them by 0-based handle.
    for p in &positions {
        builder.push_vertex(Vertex::at(*p));
    }

    for _ in 0..f_count {
        let n = read_i32(bytes, &mut cur)? as usize;
        if n < 3 {
            // Skip degenerate face's indices.
            for _ in 0..n {
                let _ = read_i32(bytes, &mut cur)?;
            }
            continue;
        }
        let mut idxs: Vec<u32> = Vec::with_capacity(n);
        for k in 0..n {
            let raw = read_i32(bytes, &mut cur)?;
            let idx = u32::try_from(raw.max(0)).map_err(|_| FormatError::Malformed {
                format: "OFF (binary)",
                offset: cur,
                reason: format!("negative vertex index {raw} in face"),
            })?;
            if idx as usize >= v_count {
                return Err(FormatError::Core(
                    occluview_core::CoreError::IndexOutOfRange {
                        at_index: k,
                        value: idx,
                        vertex_count: v_count as u32,
                    },
                ));
            }
            idxs.push(idx);
        }
        // Fan triangulate.
        let f0 = idxs[0];
        for w in idxs[1..].windows(2) {
            builder.push_triangle(f0, w[0], w[1]);
        }
    }

    builder.build().map_err(FormatError::Core)
}

fn read_ascii(bytes: &[u8]) -> Result<Mesh, FormatError> {
    let text = std::str::from_utf8(bytes).map_err(|_| FormatError::Malformed {
        format: "OFF (ascii)",
        offset: 0,
        reason: "file is not valid UTF-8".to_string(),
    })?;
    let mut lines = text.lines();
    // First line: OFF (optionally with normals/colors flags). Skip it.
    let _ = lines.next();
    // Comment lines start with '#'.
    let counts_line = lines
        .by_ref()
        .find(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .ok_or(FormatError::Truncated {
            format: "OFF (ascii)",
            expected: 0,
            got: 0,
        })?;
    let mut counts = counts_line.split_whitespace();
    let v_count: usize = counts
        .next()
        .ok_or_else(|| malformed("vertex count missing"))?
        .parse()
        .map_err(|_| malformed("bad vertex count"))?;
    let f_count: usize = counts
        .next()
        .ok_or_else(|| malformed("face count missing"))?
        .parse()
        .map_err(|_| malformed("bad face count"))?;

    let mut positions: Vec<Vec3> = Vec::with_capacity(v_count);
    let mut lexer = Lexer::new(lines);

    for _ in 0..v_count {
        let x = lexer.next_f32()?;
        let y = lexer.next_f32()?;
        let z = lexer.next_f32()?;
        positions.push(Vec3::new(x, y, z));
    }

    let mut builder = MeshBuilder::new().with_name("OFF");
    for p in &positions {
        builder.push_vertex(Vertex::at(*p));
    }

    for _ in 0..f_count {
        let n_tok = lexer.next_f32()?;
        let n = n_tok as usize;
        if n < 3 {
            for _ in 0..n {
                let _ = lexer.next_f32()?;
            }
            continue;
        }
        let mut idxs: Vec<u32> = Vec::with_capacity(n);
        for k in 0..n {
            let raw = lexer.next_f32()?;
            let idx = raw as u32;
            if idx as usize >= v_count {
                return Err(FormatError::Core(
                    occluview_core::CoreError::IndexOutOfRange {
                        at_index: k,
                        value: idx,
                        vertex_count: v_count as u32,
                    },
                ));
            }
            idxs.push(idx);
        }
        let f0 = idxs[0];
        for w in idxs[1..].windows(2) {
            builder.push_triangle(f0, w[0], w[1]);
        }
    }

    builder.build().map_err(FormatError::Core)
}

/// Token-stream lexer: yields whitespace-split f32 values, skipping comments
/// and blank lines. Replaces the closure-lifetime tangle above with a struct.
struct Lexer<'a> {
    lines: std::str::Lines<'a>,
    tokens: std::vec::IntoIter<&'a str>,
}

impl<'a> Lexer<'a> {
    fn new(lines: std::str::Lines<'a>) -> Self {
        Self {
            lines,
            tokens: Vec::new().into_iter(),
        }
    }

    fn next_f32(&mut self) -> Result<f32, FormatError> {
        loop {
            if let Some(t) = self.tokens.next() {
                return t
                    .parse::<f32>()
                    .map_err(|_| malformed(&format!("bad number {t:?}")));
            }
            let line = self.lines.next().ok_or(FormatError::Truncated {
                format: "OFF (ascii)",
                expected: 0,
                got: 0,
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            self.tokens = trimmed.split_whitespace().collect::<Vec<_>>().into_iter();
        }
    }
}

fn read_f64_le(b: &[u8], off: &mut usize) -> Result<f64, FormatError> {
    let arr: [u8; 8] = b[*off..*off + 8]
        .try_into()
        .map_err(|_| FormatError::Truncated {
            format: "OFF (binary)",
            expected: *off + 8,
            got: b.len(),
        })?;
    *off += 8;
    Ok(f64::from_le_bytes(arr))
}

fn malformed(reason: &str) -> FormatError {
    FormatError::Malformed {
        format: "OFF",
        offset: 0,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_minimal_binary_off() {
        // OFF BINARY, 3 verts, 1 face (triangle). LE per the exocad convention.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"OFF BINARY\n");
        bytes.extend_from_slice(&3i32.to_le_bytes()); // verts
        bytes.extend_from_slice(&1i32.to_le_bytes()); // faces
        bytes.extend_from_slice(&0i32.to_le_bytes()); // edges
                                                      // 3 vertices (f64 LE): (0,0,0), (1,0,0), (0,1,0)
        for f in [0.0f64, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        // 1 triangle face: n=3, indices 0,1,2
        bytes.extend_from_slice(&3i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&2i32.to_le_bytes());

        let mesh = read(&bytes).expect("valid binary OFF");
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertices().len(), 3);
        assert_eq!(mesh.vertices()[1].position, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn reads_minimal_ascii_off() {
        let text = "OFF\n3 1 0\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n";
        let mesh = read(text.as_bytes()).expect("valid ASCII OFF");
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertices().len(), 3);
    }

    #[test]
    fn fan_triangulates_quad() {
        let text = "OFF\n4 1 0\n0 0 0\n1 0 0\n1 1 0\n0 1 0\n4 0 1 2 3\n";
        let mesh = read(text.as_bytes()).expect("valid");
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn rejects_bad_signature() {
        assert!(read(b"NOTOFF...").is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        assert!(read(b"OFF BINARY\nshort").is_err());
    }

    #[test]
    fn rejects_out_of_range_index() {
        let text = "OFF\n3 1 0\n0 0 0\n1 0 0\n0 1 0\n3 0 9 2\n";
        let err = read(text.as_bytes()).unwrap_err();
        assert!(matches!(
            err,
            FormatError::Core(occluview_core::CoreError::IndexOutOfRange { .. })
        ));
    }

    #[test]
    fn tolerates_comments_and_blanks() {
        let text = "OFF\n# a comment\n\n3 1 0\n# another\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n";
        let mesh = read(text.as_bytes()).expect("valid");
        assert_eq!(mesh.triangle_count(), 1);
    }
}
