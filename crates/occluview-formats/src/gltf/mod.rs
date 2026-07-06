//! glTF/GLB reader (ADR-0010).
//! file-size-exempt: GLB accessor validation is kept local until external glTF buffers land.
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
use occluview_core::{Mesh, MeshBuilder, MeshTexture, Vertex};

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
    // Track the first primitive's material so we can resolve a texture after
    // the build (the builder only handles geometry).
    let mut first_material: Option<usize> = None;
    for &node_idx in &scene.nodes {
        walk_node(doc, node_idx, bin_chunk, &mut builder)?;
        if first_material.is_none() {
            first_material = first_primitive_material(doc, node_idx);
        }
    }
    let mut mesh = builder.build().map_err(FormatError::Core)?;
    // If the first primitive references a textured material, decode + attach.
    if let Some(mat_idx) = first_material {
        if let Some(tex) = resolve_material_texture(doc, mat_idx, bin_chunk)? {
            mesh.set_texture(tex);
        }
    }
    Ok(mesh)
}

/// Return the material index of the first primitive of the mesh referenced by
/// `node_idx`, if any.
fn first_primitive_material(doc: &json::GltfDoc, node_idx: usize) -> Option<usize> {
    let node = doc.nodes.get(node_idx)?;
    let mesh = doc.meshes.get(node.mesh?)?;
    mesh.primitives.first()?.material
}

/// Resolve a material's base-color texture to a decoded [`MeshTexture`].
///
/// glTF material → `pbrMetallicRoughness.baseColorTexture.index` →
/// `textures[idx].source` → `images[source].bufferView` → decode PNG/JPEG.
///
/// Returns `None` if the material has no base-color texture, or if the texture
/// chain references an external URI (out of scope for v1).
fn resolve_material_texture(
    doc: &json::GltfDoc,
    material_idx: usize,
    bin_chunk: &[u8],
) -> Result<Option<MeshTexture>, FormatError> {
    let material = doc
        .materials
        .get(material_idx)
        .ok_or_else(|| malformed("material out of range"))?;
    // materials are opaque serde_json::Value — dig into pbrMetallicRoughness.
    let pbr = material
        .get("pbrMetallicRoughness")
        .ok_or_else(|| malformed("material has no pbrMetallicRoughness"))?;
    let Some(base_color_tex) = pbr.get("baseColorTexture") else {
        return Ok(None); // no texture on this material
    };
    let tex_idx = base_color_tex
        .get("index")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| malformed("baseColorTexture has no index"))? as usize;
    let texture = doc
        .textures
        .get(tex_idx)
        .ok_or_else(|| malformed("texture out of range"))?;
    let source = texture
        .source
        .ok_or_else(|| malformed("texture has no source"))?;
    let image = doc
        .images
        .get(source)
        .ok_or_else(|| malformed("image out of range"))?;
    // Only bufferView-embedded images are supported (external URI rejected).
    let bv_idx = image
        .buffer_view
        .ok_or_else(|| malformed("image has no bufferView (external URI unsupported)"))?;
    let bv = doc
        .buffer_views
        .get(bv_idx)
        .ok_or_else(|| malformed("image bufferView out of range"))?;
    let offset = bv.byte_offset.unwrap_or(0);
    let end = offset + bv.byte_length as usize;
    let img_bytes = bin_chunk.get(offset..end).ok_or(FormatError::Truncated {
        format: "glTF",
        expected: end,
        got: bin_chunk.len(),
    })?;
    // Decode via the `image` crate (PNG or JPEG).
    let decoded = image::load_from_memory(img_bytes)
        .map_err(|e| malformed(&format!("image decode failed: {e}")))?;
    let rgba = decoded.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(Some(MeshTexture::new(w, h, rgba.into_raw())))
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
    let uvs = prim
        .attributes
        .texcoord_0
        .map(|i| read_texcoord(doc, i, bin_chunk))
        .transpose()?;

    let vertex_count = positions.len();
    let base = builder_push_vertices(
        &positions,
        normals.as_deref(),
        colors.as_deref(),
        uvs.as_deref(),
        builder,
    );

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

/// Push `positions` (with optional matching normals/colors/uvs) and return the
/// handle of the first pushed vertex, used as the index base.
fn builder_push_vertices(
    positions: &[[f32; 3]],
    normals: Option<&[[f32; 3]]>,
    colors: Option<&[[u8; 4]]>,
    uvs: Option<&[[f32; 2]]>,
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
        if let Some(uvs) = uvs {
            if i < uvs.len() {
                v = v.with_uv(uvs[i]);
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

/// Read a `TEXCOORD_0` accessor as `Vec<[f32; 2]>`. Supports FLOAT (5126)
/// and normalized `UNSIGNED_BYTE` (5121) / `UNSIGNED_SHORT` (5123) VEC2.
fn read_texcoord(
    doc: &json::GltfDoc,
    acc_idx: usize,
    bin_chunk: &[u8],
) -> Result<Vec<[f32; 2]>, FormatError> {
    let acc = doc
        .accessors
        .get(acc_idx)
        .ok_or_else(|| malformed("accessor out of range"))?;
    if acc.type_ != "VEC2" {
        return Err(malformed(&format!(
            "VEC2 TEXCOORD_0 required, got type {}",
            acc.type_
        )));
    }
    let normalized = acc.normalized.unwrap_or(false);
    match acc.component_type {
        5126 => {
            // FLOAT VEC2.
            let bytes = read_accessor_bytes(doc, acc_idx, 8, bin_chunk)?;
            let mut out = Vec::with_capacity(acc.count);
            for i in 0..acc.count {
                let off = i * 8;
                let u = f32_at(bytes.get(off..off + 4).unwrap_or(&[0; 4]));
                let v = f32_at(bytes.get(off + 4..off + 8).unwrap_or(&[0; 4]));
                out.push([u, v]);
            }
            Ok(out)
        }
        5121 => {
            // UNSIGNED_BYTE VEC2, optionally normalized.
            let bytes = read_accessor_bytes(doc, acc_idx, 2, bin_chunk)?;
            let mut out = Vec::with_capacity(acc.count);
            for i in 0..acc.count {
                let off = i * 2;
                let u = bytes.get(off).copied().unwrap_or(0);
                let v = bytes.get(off + 1).copied().unwrap_or(0);
                let uf = if normalized {
                    f32::from(u) / 255.0
                } else {
                    f32::from(u)
                };
                let vf = if normalized {
                    f32::from(v) / 255.0
                } else {
                    f32::from(v)
                };
                out.push([uf, vf]);
            }
            Ok(out)
        }
        5123 => {
            // UNSIGNED_SHORT VEC2, optionally normalized.
            let bytes = read_accessor_bytes(doc, acc_idx, 4, bin_chunk)?;
            let mut out = Vec::with_capacity(acc.count);
            for i in 0..acc.count {
                let off = i * 4;
                let u = u16_at(bytes.get(off..off + 2).unwrap_or(&[0; 2]));
                let v = u16_at(bytes.get(off + 2..off + 4).unwrap_or(&[0; 2]));
                let uf = if normalized {
                    f32::from(u) / 65535.0
                } else {
                    f32::from(u)
                };
                let vf = if normalized {
                    f32::from(v) / 65535.0
                } else {
                    f32::from(v)
                };
                out.push([uf, vf]);
            }
            Ok(out)
        }
        other => Err(malformed(&format!(
            "TEXCOORD_0 component_type {other} not supported (use FLOAT/UBYTE/USHORT)"
        ))),
    }
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
                let r = to_u8(f32_at(bytes.get(off..off + 4).unwrap_or(&[0; 4])));
                let g = to_u8(f32_at(bytes.get(off + 4..off + 8).unwrap_or(&[0; 4])));
                let b = to_u8(f32_at(bytes.get(off + 8..off + 12).unwrap_or(&[0; 4])));
                let a = if comp_per_elem == 4 {
                    to_u8(f32_at(bytes.get(off + 12..off + 16).unwrap_or(&[0; 4])))
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
            4 => u32_at(bytes.get(off..off + 4).unwrap_or(&[0; 4])),
            2 => u32::from(u16_at(bytes.get(off..off + 2).unwrap_or(&[0; 2]))),
            1 => u32::from(*bytes.get(off).unwrap_or(&0)),
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

    /// Regression: index values must round-trip correctly. An earlier bug had
    /// `read_indices` pass `&bytes[off..]` (a tail slice, not exactly 4 bytes)
    /// to `u32_at`, whose `try_into().unwrap_or([0;4])` then silently zeroed
    /// every index — producing degenerate `(0,0,0)` triangles and empty renders
    /// for any real GLB. This test uses non-trivial index values so the bug
    /// surfaces as a value mismatch, not just a count check.
    #[test]
    fn index_values_round_trip_exactly() {
        // 6 vertices, 2 triangles with indices (1,4,2) and (5,0,3).
        let json = br#"{"asset":{"version":"2.0"},
"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
"accessors":[{"bufferView":0,"count":6,"type":"VEC3","componentType":5126},
             {"bufferView":1,"count":6,"type":"SCALAR","componentType":5125}],
"bufferViews":[{"buffer":0,"byteLength":72},{"buffer":0,"byteOffset":72,"byteLength":24}],
"buffers":[{"byteLength":96}]}"#;
        let mut bin = Vec::new();
        // 6 positions (values irrelevant to this test).
        for _ in 0..18 {
            bin.extend_from_slice(&0.0_f32.to_le_bytes());
        }
        // 6 u32 indices: 1,4,2,5,0,3
        for i in [1u32, 4, 2, 5, 0, 3] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let bytes = glb::build_glb(json, &bin);
        let mesh = read(&bytes).expect("valid GLB");
        assert_eq!(mesh.indices(), &[1, 4, 2, 5, 0, 3]);
    }

    /// Regression companion: USHORT (5123) indices also round-trip exactly.
    #[test]
    fn ushort_index_values_round_trip() {
        let json = br#"{"asset":{"version":"2.0"},
"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
"accessors":[{"bufferView":0,"count":6,"type":"VEC3","componentType":5126},
             {"bufferView":1,"count":6,"type":"SCALAR","componentType":5123}],
"bufferViews":[{"buffer":0,"byteLength":72},{"buffer":0,"byteOffset":72,"byteLength":12}],
"buffers":[{"byteLength":84}]}"#;
        let mut bin = Vec::new();
        for _ in 0..18 {
            bin.extend_from_slice(&0.0_f32.to_le_bytes());
        }
        for i in [1u16, 4, 2, 5, 0, 3] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let bytes = glb::build_glb(json, &bin);
        let mesh = read(&bytes).expect("valid GLB");
        assert_eq!(mesh.indices(), &[1, 4, 2, 5, 0, 3]);
    }

    /// End-to-end: a GLB with `TEXCOORD_0` + a material base-color texture
    /// (PNG embedded in a bufferView) round-trips UVs and decodes the texture.
    #[test]
    fn textured_glb_round_trips_uvs_and_texture() {
        // Encode a 2×2 red PNG as the texture image bytes.
        let png_bytes: Vec<u8> = {
            let img = image::RgbaImage::from_raw(
                2,
                2,
                vec![
                    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
                ],
            )
            .expect("image dims");
            let mut buf = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut buf, image::ImageFormat::Png)
                .expect("encode png");
            buf.into_inner()
        };
        let png_len = png_bytes.len();

        // BIN layout:
        //   [0..72)      positions (6 verts × 12 bytes = 72, values irrelevant)
        //   [72..120)    uvs (6 verts × 8 bytes = 48)
        //   [120..132)   indices (6 × u16 = 12)
        //   [132..132+png_len)  PNG image bytes
        let uv_start = 72usize;
        let idx_start = uv_start + 48;
        let img_start = idx_start + 12;
        let total = img_start + png_len;

        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},
"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"indices":2,"material":0}}]}}],
"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
"textures":[{{"source":0}}],
"images":[{{"bufferView":3,"mimeType":"image/png"}}],
"accessors":[{{"bufferView":0,"count":6,"type":"VEC3","componentType":5126}},
             {{"bufferView":1,"count":6,"type":"VEC2","componentType":5126}},
             {{"bufferView":2,"count":6,"type":"SCALAR","componentType":5123}}],
"bufferViews":[{{"buffer":0,"byteLength":72}},
               {{"buffer":0,"byteOffset":{uv_start},"byteLength":48}},
               {{"buffer":0,"byteOffset":{idx_start},"byteLength":12}},
               {{"buffer":0,"byteOffset":{img_start},"byteLength":{png_len}}}],
"buffers":[{{"byteLength":{total}}}]}}"#
        );

        let mut bin = Vec::with_capacity(total);
        // 6 positions (zeros).
        bin.extend(std::iter::repeat(0u8).take(72));
        // 6 UVs: (0,0) (1,0) (0.5,1) (0,0) (1,0) (0.5,1).
        for &(u, v) in &[
            (0.0f32, 0.0f32),
            (1.0, 0.0),
            (0.5, 1.0),
            (0.0, 0.0),
            (1.0, 0.0),
            (0.5, 1.0),
        ] {
            bin.extend_from_slice(&u.to_le_bytes());
            bin.extend_from_slice(&v.to_le_bytes());
        }
        // 6 u16 indices: 0,1,2,3,4,5.
        for i in [0u16, 1, 2, 3, 4, 5] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        // PNG bytes.
        bin.extend_from_slice(&png_bytes);
        assert_eq!(bin.len(), total);

        let bytes = glb::build_glb(json.as_bytes(), &bin);
        let mesh = read(&bytes).expect("valid textured GLB");

        // UVs round-tripped.
        assert!(mesh.has_uvs());
        let verts = mesh.vertices();
        assert_eq!(verts.len(), 6);
        assert_eq!(verts[0].uv, [0.0, 0.0]);
        assert_eq!(verts[1].uv, [1.0, 0.0]);
        assert_eq!(verts[2].uv, [0.5, 1.0]);

        // Texture decoded + attached.
        let tex = mesh.texture().expect("texture should be attached");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 2);
        // Every pixel is red.
        assert!(tex.rgba.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    }

    /// A GLB with no texture (plain geometry) must not attach a texture.
    #[test]
    fn untextured_glb_has_no_texture() {
        let bytes = one_triangle_glb();
        let mesh = read(&bytes).expect("valid GLB");
        assert!(mesh.texture().is_none(), "untextured mesh got a texture");
        assert!(!mesh.has_uvs(), "mesh with no TEXCOORD_0 reported has_uvs");
    }
}
