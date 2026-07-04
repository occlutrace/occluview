//! Typed glTF 2.0 JSON schema (subset).
//!
//! Field names are camelCase to match the glTF spec; we apply
//! `#[serde(rename_all = "camelCase")]` per struct. Only fields the v1 reader
//! consumes are present; unknown fields are ignored, so we tolerate extensions
//! we don't ship yet.

#![allow(clippy::missing_docs_in_private_items)]

use serde::{Deserialize, Serialize};

/// Root glTF document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfDoc {
    /// All scenes in the file.
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// All nodes.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// All meshes (a node references one by index).
    #[serde(default)]
    pub meshes: Vec<Mesh>,
    /// Materials (parsed opaquely in v1; we don't read them yet).
    #[serde(default)]
    pub materials: Vec<serde_json::Value>,
    /// Accessors describe how to interpret buffer-view bytes.
    #[serde(default)]
    pub accessors: Vec<Accessor>,
    /// Buffer views: slices into a buffer.
    #[serde(default)]
    pub buffer_views: Vec<BufferView>,
    /// Buffers (only the embedded GLB BIN chunk is honored in v1).
    #[serde(default)]
    pub buffers: Vec<Buffer>,
    /// Index of the default scene.
    #[serde(default)]
    pub scene: Option<usize>,
    /// Asset metadata (`version` is required by the spec).
    #[serde(default)]
    pub asset: Asset,
}

/// A scene: the set of root nodes to render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// Root node indices.
    #[serde(default)]
    pub nodes: Vec<usize>,
}

/// A node in the scene graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Mesh index referenced by this node, if any.
    #[serde(default)]
    pub mesh: Option<usize>,
    /// Child node indices.
    #[serde(default)]
    pub children: Vec<usize>,
    /// Optional display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional 4x4 matrix (we don't apply TRS in v1).
    #[serde(default)]
    pub matrix: Option<Vec<f32>>,
    /// Optional translation.
    #[serde(default)]
    pub translation: Option<[f32; 3]>,
    /// Optional rotation quaternion.
    #[serde(default)]
    pub rotation: Option<[f32; 4]>,
    /// Optional scale.
    #[serde(default)]
    pub scale: Option<[f32; 3]>,
}

/// A mesh: one or more primitives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    /// Geometry primitives.
    #[serde(default)]
    pub primitives: Vec<Primitive>,
    /// Optional display name.
    #[serde(default)]
    pub name: Option<String>,
}

/// A primitive: one drawable (vertices + optional indices).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Primitive {
    /// Vertex attributes (`POSITION`, `NORMAL`, `COLOR_0`, ...).
    #[serde(default)]
    pub attributes: Attributes,
    /// Optional index accessor.
    #[serde(default)]
    pub indices: Option<usize>,
    /// Rendering mode (4 = triangles; the only v1-supported value).
    #[serde(default)]
    pub mode: Option<u32>,
    /// Optional material index (ignored in v1).
    #[serde(default)]
    pub material: Option<usize>,
}

/// Vertex attributes referenced by a primitive. Morph targets are
/// intentionally absent (not supported in v1).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Attributes {
    /// POSITION accessor index (FLOAT VEC3).
    #[serde(default, rename = "POSITION")]
    pub position: Option<usize>,
    /// NORMAL accessor index (FLOAT VEC3).
    #[serde(default, rename = "NORMAL")]
    pub normal: Option<usize>,
    /// `COLOR_0` accessor index (FLOAT or `UNSIGNED_BYTE` VEC3/VEC4).
    #[serde(default, rename = "COLOR_0")]
    pub color_0: Option<usize>,
    /// `TEXCOORD_0` accessor index (parsed, unused in v1).
    #[serde(default, rename = "TEXCOORD_0")]
    pub texcoord_0: Option<usize>,
}

/// An accessor: typed view over a buffer view's bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accessor {
    /// Buffer-view index.
    #[serde(default)]
    pub buffer_view: usize,
    /// Byte offset within the buffer view.
    #[serde(default)]
    pub byte_offset: Option<usize>,
    /// Number of elements.
    pub count: usize,
    /// Element type (`SCALAR`, `VEC2`, `VEC3`, `VEC4`).
    #[serde(rename = "type")]
    pub type_: String,
    /// Component type (glTF enum: 5120-5126).
    pub component_type: u32,
    /// Whether integer components are normalized to 0..1.
    #[serde(default)]
    pub normalized: Option<bool>,
}

/// A buffer view: a byte slice into a buffer, with optional stride.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferView {
    /// Buffer index (0 = embedded GLB BIN chunk in v1).
    pub buffer: usize,
    /// Byte offset within the buffer.
    #[serde(default)]
    pub byte_offset: Option<usize>,
    /// Length in bytes.
    pub byte_length: u32,
    /// Byte stride between elements (for interleaved layouts).
    #[serde(default)]
    pub byte_stride: Option<usize>,
}

/// A buffer. In v1 only the embedded GLB BIN chunk (buffer 0, no URI) is
/// honored; external URIs are rejected upstream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Buffer {
    /// Optional URI (data: or external); rejected in v1.
    #[serde(default)]
    pub uri: Option<String>,
    /// Declared length in bytes.
    #[serde(default)]
    pub byte_length: u32,
}

/// Asset metadata. `version` is required by the glTF spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// glTF version string (e.g. `"2.0"`).
    #[serde(default)]
    pub version: String,
    /// Optional generator identifier.
    #[serde(default)]
    pub generator: Option<String>,
}
