//! glTF/GLB reader (ADR-0010).
//!
//! Native Rust reader for the dental-viewer subset of glTF 2.0. Only the GLB
//! binary-container form is supported in v1 (the entire OccluTrace corpus is
//! GLB). External `.gltf` with separate buffer URIs is intentionally out of
//! scope for v1 — adding it later requires the path-traversal protection
//! described in SECURITY.md, not just parsing.
//!
//! Mesh subset: `POSITION` (FLOAT VEC3), `indices` (`UINT`/`USHORT`/`UBYTE`),
//! optional `NORMAL` (FLOAT VEC3), optional `COLOR_0` (FLOAT VEC3/VEC4 or
//! `UNSIGNED_BYTE` VEC3/VEC4). Primitive mode 4 (triangles) only.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

pub mod glb;
pub mod json;

use crate::error::FormatError;
use glam::Vec3;
use occluview_core::{Mesh, MeshBuilder, Vertex};

/// Read a GLB from raw bytes into a [`Mesh`].
///
/// # Errors
/// - [`FormatError::BadSignature`] if not a GLB.
/// - [`FormatError::Malformed`] for invalid JSON or an unsupported feature.
/// - [`FormatError::Truncated`] for a buffer view past end of BIN chunk.
/// - [`FormatError::Core`] for index-out-of-range.
pub fn read(bytes: &[u8]) -> Result<Mesh, FormatError> {
    if !bytes.starts_with(b"glTF") {
        return Err(FormatError::BadSignature {
            format: "glTF",
            offset: 0,
        });
    }
    let (json_bytes, bin_chunk) = glb::split(bytes)?;
    let doc: json::GltfDoc =
        serde_json::from_slice(&json_bytes).map_err(|e| FormatError::Malformed {
            format: "glTF",
            offset: 0,
            reason: format!("invalid JSON: {e}"),
        })?;
    read_doc(&doc, bin_chunk)
}

fn read_doc(doc: &json::GltfDoc, bin_chunk: &[u8]) -> Result<Mesh, FormatError> {
    let scene_idx = doc.scene.unwrap_or(0);
    let scene = doc
        .scenes
        .get(scene_idx)
        .ok_or_else(|| malformed("scene out of range"))?;
    let mut builder = MeshBuilder::new().with_name("glTF");
    for &node_idx in &scene.nodes {
        walk_node(doc, node_idx, bin_chunk, &mut builder)?;
    }
    builder.build().map_err(FormatError::Core)
}

fn walk_node(
    doc: &json::GltfDoc,
    node_idx: usize,
    bin_chunk: &[u8],
    builder: &mut MeshBuilder,
) -> Result<(), FormatError> {
    let node = doc
        .nodes
        .get(node_idx)
        .ok_or_else(|| malformed("node out of range"))?;
    if let Some(mesh_idx) = node.mesh {
        let mesh = doc
            .meshes
            .get(mesh_idx)
            .ok_or_else(|| malformed("mesh out of range"))?;
        for prim in &mesh.primitives {
            emit_primitive(doc, prim, bin_chunk, builder)?;
        }
    }
    for &child_idx in &node.children {
        walk_node(doc, child_idx, bin_chunk, builder)?;
    }
    Ok(())
}

fn emit_primitive(
    doc: &json::GltfDoc,
    prim: &json::Primitive,
    bin_chunk: &[u8],
    builder: &mut MeshBuilder,
) -> Result<(), FormatError> {
    if let Some(mode) = prim.mode {
        if mode != 4 {
            return Err(malformed(&format!(
                "primitive mode {mode} not supported (only triangles=4)"
            )));
        }
    }

    let pos_acc_idx = prim
        .attributes
        .position
        .ok_or_else(|| malformed("primitive has no POSITION"))?;
    let positions = read_f32_vec3(doc, pos_acc_idx, bin_chunk)?;
    let normals = prim
        .attributes
        .normal
        .map(|i| read_f32_vec3(doc, i, bin_chunk))
        .transpose()?;
    let colors = prim
        .attributes
        .color_0
        .map(|i| read_color_f32(doc, i, bin_chunk))
        .transpose()?;

    let vertex_count = positions.len();
    let base = builder_push_vertices(&positions, normals.as_deref(), colors.as_deref(), builder);

    if let Some(idx_acc) = prim.indices {
        let indices = read_indices(doc, idx_acc, bin_chunk)?;
        for chunk in indices.chunks_exact(3) {
            let (a, b, c) = (chunk[0], chunk[1], chunk[2]);
            builder.push_triangle(base + a, base + b, base + c);
        }
    } else if vertex_count % 3 == 0 {
        for i in (0..vertex_count).step_by(3) {
            builder.push_triangle(
                base + i as u32,
                base + (i + 1) as u32,
                base + (i + 2) as u32,
            );
        }
    } else {
        return Err(malformed(
            "non-indexed primitive with vertex count not divisible by 3",
        ));
    }
    Ok(())
}

/// Push `positions` (with optional matching normals/colors) and return the
/// handle of the first pushed vertex, used as the index base.
fn builder_push_vertices(
    positions: &[[f32; 3]],
    normals: Option<&[[f32; 3]]>,
    colors: Option<&[[u8; 4]]>,
    builder: &mut MeshBuilder,
) -> u32 {
    let mut first = 0u32;
    for (i, p) in positions.iter().enumerate() {
        let mut v = Vertex::at(Vec3::from_array(*p));
        if let Some(ns) = normals {
            if i < ns.len() {
                v = v.with_normal(Vec3::from_array(ns[i]));
            }
        }
        if let Some(cs) = colors {
            if i < cs.len() {
                v = v.with_color(cs[i]);
            }
        }
        let h = builder.push_vertex(v);
        if i == 0 {
            first = h;
        }
    }
    first
}

/// Read a FLOAT VEC3 accessor as `Vec<[f32; 3]>`.
fn read_f32_vec3(
    doc: &json::GltfDoc,
    acc_idx: usize,
    bin_chunk: &[u8],
) -> Result<Vec<[f32; 3]>, FormatError> {
    let acc = doc
        .accessors
        .get(acc_idx)
        .ok_or_else(|| malformed("accessor out of range"))?;
    if acc.component_type != 5126 {
        return Err(malformed(&format!(
            "FLOAT (5126) required, got component_type {}",
            acc.component_type
        )));
    }
    if acc.type_ != "VEC3" {
        return Err(malformed(&format!("VEC3 required, got type {}", acc.type_)));
    }
    let bytes = read_accessor_bytes(doc, acc_idx, 12, bin_chunk)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let off = i * 12;
        let x = f32_at(&bytes[off..off + 4]);
        let y = f32_at(&bytes[off + 4..off + 8]);
        let z = f32_at(&bytes[off + 8..off + 12]);
        out.push([x, y, z]);
    }
    Ok(out)
}

/// Read a `COLOR_0` accessor as `Vec<[u8; 4]>` (RGBA, normalized to `0..=255`).
///
/// Supports FLOAT (5126) VEC3/VEC4 (values `0.0..=1.0`) and `UNSIGNED_BYTE`
/// (5121) VEC3/VEC4 (values `0..=255`; `normalized` is irrelevant since the
/// byte is
/// already what we store).
fn read_color_f32(
    doc: &json::GltfDoc,
    acc_idx: usize,
    bin_chunk: &[u8],
) -> Result<Vec<[u8; 4]>, FormatError> {
    let acc = doc
        .accessors
        .get(acc_idx)
        .ok_or_else(|| malformed("accessor out of range"))?;
    let comp_per_elem: usize = match acc.type_.as_str() {
        "VEC3" => 3,
        "VEC4" => 4,
        other => {
            return Err(malformed(&format!(
                "VEC3/VEC4 COLOR_0 required, got {other}"
            )))
        }
    };

    match acc.component_type {
        5126 => {
            // FLOAT.
            let bytes = read_accessor_bytes(doc, acc_idx, comp_per_elem * 4, bin_chunk)?;
            let to_u8 = |f: f32| (f.clamp(0.0, 1.0) * 255.0) as u8;
            let mut out = Vec::with_capacity(acc.count);
            for i in 0..acc.count {
                let off = i * comp_per_elem * 4;
                let r = to_u8(f32_at(&bytes[off..]));
                let g = to_u8(f32_at(&bytes[off + 4..]));
                let b = to_u8(f32_at(&bytes[off + 8..]));
                let a = if comp_per_elem == 4 {
                    to_u8(f32_at(&bytes[off + 12..]))
                } else {
                    255
                };
                out.push([r, g, b, a]);
            }
            Ok(out)
        }
        5121 => {
            // UNSIGNED_BYTE: the raw byte IS the 0..=255 channel we store.
            let bytes = read_accessor_bytes(doc, acc_idx, comp_per_elem, bin_chunk)?;
            let mut out = Vec::with_capacity(acc.count);
            for i in 0..acc.count {
                let off = i * comp_per_elem;
                let r = bytes[off];
                let g = bytes[off + 1];
                let b = bytes[off + 2];
                let a = if comp_per_elem == 4 {
                    bytes[off + 3]
                } else {
                    255
                };
                out.push([r, g, b, a]);
            }
            Ok(out)
        }
        other => Err(malformed(&format!(
            "COLOR_0 component_type {other} not supported (FLOAT or UNSIGNED_BYTE only)"
        ))),
    }
}

/// Read an index accessor as `Vec<u32>`. Component types: 5125 (UINT),
/// 5123 (USHORT), 5121 (UBYTE).
fn read_indices(
    doc: &json::GltfDoc,
    acc_idx: usize,
    bin_chunk: &[u8],
) -> Result<Vec<u32>, FormatError> {
    let acc = doc
        .accessors
        .get(acc_idx)
        .ok_or_else(|| malformed("accessor out of range"))?;
    let bytes_per = match acc.component_type {
        5125 => 4,
        5123 => 2,
        5121 => 1,
        other => {
            return Err(malformed(&format!(
                "index component_type {other} not supported"
            )))
        }
    };
    let bytes = read_accessor_bytes(doc, acc_idx, bytes_per, bin_chunk)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let off = i * bytes_per;
        let v = match bytes_per {
            4 => u32_at(&bytes[off..]),
            2 => u32::from(u16_at(&bytes[off..])),
            1 => u32::from(bytes[off]),
            _ => unreachable!(),
        };
        out.push(v);
    }
    Ok(out)
}

/// Read 2 little-endian bytes as `u16`. Caller guarantees `b.len() >= 2`.
fn u16_at(b: &[u8]) -> u16 {
    let arr: [u8; 2] = b.try_into().unwrap_or([0; 2]);
    u16::from_le_bytes(arr)
}

/// Read 4 little-endian bytes as `u32`. Caller guarantees `b.len() >= 4`.
fn u32_at(b: &[u8]) -> u32 {
    let arr: [u8; 4] = b.try_into().unwrap_or([0; 4]);
    u32::from_le_bytes(arr)
}

/// Read `acc.count` elements of `bytes_per_elem` from the BIN chunk into a
/// flat `Vec<u8>`, honoring buffer view offset/stride.
fn read_accessor_bytes(
    doc: &json::GltfDoc,
    acc_idx: usize,
    bytes_per_elem: usize,
    bin_chunk: &[u8],
) -> Result<Vec<u8>, FormatError> {
    let acc = doc
        .accessors
        .get(acc_idx)
        .ok_or_else(|| malformed("accessor out of range"))?;
    let view = doc
        .buffer_views
        .get(acc.buffer_view)
        .ok_or_else(|| malformed("buffer_view out of range"))?;
    let _buffer = doc
        .buffers
        .get(view.buffer)
        .ok_or_else(|| malformed("buffer out of range"))?;
    // v1: only embedded GLB BIN chunk (buffer 0, no URI). External buffers are
    // rejected upstream; if buffer.uri is Some, this is a .gltf we shouldn't
    // have reached (read() takes only GLB). Defensive check:
    if view.buffer != 0 || doc.buffers.first().is_some_and(|b| b.uri.is_some()) {
        return Err(malformed(
            "external buffer URIs not supported in v1 (GLB only)",
        ));
    }

    let stride = view.byte_stride.unwrap_or(bytes_per_elem);
    let start = view.byte_offset.unwrap_or(0) + acc.byte_offset.unwrap_or(0);
    let end = start + acc.count * bytes_per_elem;
    if end > bin_chunk.len() {
        return Err(FormatError::Truncated {
            format: "glTF",
            expected: end,
            got: bin_chunk.len(),
        });
    }
    let mut out = Vec::with_capacity(acc.count * bytes_per_elem);
    for i in 0..acc.count {
        let off = start + i * stride;
        out.extend_from_slice(bin_chunk.get(off..off + bytes_per_elem).ok_or_else(|| {
            FormatError::Truncated {
                format: "glTF",
                expected: off + bytes_per_elem,
                got: bin_chunk.len(),
            }
        })?);
    }
    Ok(out)
}

fn malformed(reason: &str) -> FormatError {
    FormatError::Malformed {
        format: "glTF",
        offset: 0,
        reason: reason.to_string(),
    }
}

/// Read 4 little-endian bytes as `f32`. Caller guarantees `b.len() >= 4`.
fn f32_at(b: &[u8]) -> f32 {
    let arr: [u8; 4] = b.try_into().unwrap_or([0; 4]);
    f32::from_le_bytes(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal GLB: one triangle, FLOAT VEC3 positions + UINT indices.
    fn one_triangle_glb() -> Vec<u8> {
        let json = br#"{"asset":{"version":"2.0"},
"scenes":[{"nodes":[0]}],
"nodes":[{"mesh":0}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
"accessors":[{"bufferView":0,"count":3,"type":"VEC3","componentType":5126},
             {"bufferView":1,"count":3,"type":"SCALAR","componentType":5125}],
"bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12}],
"buffers":[{"byteLength":48}]}"#;
        let mut bin = Vec::new();
        // 3 positions: (0,0,0),(1,0,0),(0,1,0)
        for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&f.to_le_bytes());
        }
        // 3 indices
        for i in 0u32..3 {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        glb::build_glb(json, &bin)
    }

    #[test]
    fn reads_minimal_triangle() {
        let bytes = one_triangle_glb();
        let mesh = read(&bytes).expect("valid GLB");
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertices().len(), 3);
        assert_eq!(mesh.vertices()[1].position, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn rejects_non_glb() {
        assert!(read(b"not gltf").is_err());
    }

    #[test]
    fn rejects_unsupported_primitive_mode() {
        // mode 6 (triangle fan) -> rejected.
        let json = br#"{"asset":{"version":"2.0"},
"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
"meshes":[{"primitives":[{"mode":6,"attributes":{"POSITION":0}}]}],
"accessors":[{"bufferView":0,"count":3,"type":"VEC3","componentType":5126}],
"bufferViews":[{"buffer":0,"byteLength":36}],"buffers":[{"byteLength":36}]}"#;
        let bin = [0u8; 36];
        let bytes = glb::build_glb(json, &bin);
        let err = read(&bytes).unwrap_err();
        assert!(matches!(err, FormatError::Malformed { .. }));
    }

    #[test]
    fn handles_non_indexed_primitive() {
        // No indices attribute; 3 vertices -> 1 implicit triangle.
        let json = br#"{"asset":{"version":"2.0"},
"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
"accessors":[{"bufferView":0,"count":3,"type":"VEC3","componentType":5126}],
"bufferViews":[{"buffer":0,"byteLength":36}],"buffers":[{"byteLength":36}]}"#;
        let bin = [0u8; 36];
        let bytes = glb::build_glb(json, &bin);
        let mesh = read(&bytes).expect("valid");
        assert_eq!(mesh.triangle_count(), 1);
    }
}
