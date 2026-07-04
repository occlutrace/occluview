//! The mesh data model.
//!
//! A [`Mesh`] is the unit of geometry that flows from a format loader, through
//! the scene graph, into the renderer. It is GPU- and I/O-agnostic on purpose:
//! `occluview-render` owns the GPU buffers, `occluview-formats` owns the
//! readers/writers.
//!
//! ## Layout
//!
//! Vertices store position, normal, and an optional RGBA color (dental color /
//! NIR scans — ADR-0009). Indices are 32-bit triangles. Colors are optional so
//! plain STL (no color) doesn't pay for a zeroed channel.

use crate::bbox::Aabb;
use crate::error::CoreError;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// A vertex. `#[repr(C)]` so it can be uploaded to the GPU verbatim via
/// `bytemuck::cast_slice`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable, PartialEq)]
pub struct Vertex {
    /// Position in OccluView's canonical frame, in millimeters.
    pub position: [f32; 3],
    /// Unit normal, in canonical frame. Defaults to zero (flat-shaded on load).
    pub normal: [f32; 3],
    /// sRGBA color, packed 0..=255. `(255, 255, 255, 255)` when color is absent.
    pub color: [u8; 4],
}

impl Vertex {
    /// Construct a vertex with position only (normal zeroed, color white).
    #[inline]
    #[must_use]
    pub fn at(position: Vec3) -> Self {
        Self {
            position: position.to_array(),
            normal: [0.0; 3],
            color: [255, 255, 255, 255],
        }
    }

    /// Construct a vertex with position and normal (color white).
    #[inline]
    #[must_use]
    pub fn with_normal(mut self, normal: Vec3) -> Self {
        self.normal = normal.to_array();
        self
    }

    /// Construct a vertex with a vertex color.
    #[inline]
    #[must_use]
    pub fn with_color(mut self, rgba: [u8; 4]) -> Self {
        self.color = rgba;
        self
    }
}

/// A triangle mesh, the central geometry type.
#[derive(Clone, Debug)]
pub struct Mesh {
    name: Option<String>,
    vertices: Vec<Vertex>,
    /// Triangle indices into `vertices`. Length must be a multiple of 3.
    indices: Vec<u32>,
    /// True if any vertex carries a non-default color.
    has_vertex_colors: bool,
    /// Cached bounding box, lazily computed.
    cached_bbox: Option<Aabb>,
}

impl Mesh {
    /// An empty mesh — no vertices, no triangles.
    ///
    /// Cheap to construct; used as a placeholder (e.g. for `Default`).
    #[must_use]
    #[inline]
    pub fn empty() -> Self {
        Self {
            name: None,
            vertices: Vec::new(),
            indices: Vec::new(),
            has_vertex_colors: false,
            cached_bbox: None,
        }
    }

    /// Construct from parts. Validates the index range.
    ///
    /// # Errors
    /// - [`CoreError::IndexOutOfRange`] if any index exceeds the vertex count.
    /// - [`CoreError::IndexCountNotMultipleOfThree`] if `indices.len() % 3 != 0`.
    pub fn new(
        name: Option<String>,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
    ) -> Result<Self, CoreError> {
        if indices.len() % 3 != 0 {
            return Err(CoreError::IndexCountNotMultipleOfThree {
                index_count: indices.len(),
            });
        }
        let vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        if let Some((i, bad)) = indices.iter().enumerate().find(|(_, &v)| v >= vertex_count) {
            return Err(CoreError::IndexOutOfRange {
                at_index: i,
                value: *bad,
                vertex_count,
            });
        }
        let has_vertex_colors = vertices.iter().any(|v| v.color != [255, 255, 255, 255]);
        Ok(Self {
            name,
            vertices,
            indices,
            has_vertex_colors,
            cached_bbox: None,
        })
    }

    /// Optional human-readable name (e.g. file stem, "upper arch").
    #[inline]
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// All vertices, in upload order.
    #[inline]
    #[must_use]
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Triangle indices (every 3 = one triangle).
    #[inline]
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Number of triangles.
    #[inline]
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// True if any vertex carries a non-default color.
    #[inline]
    #[must_use]
    pub fn has_vertex_colors(&self) -> bool {
        self.has_vertex_colors
    }

    /// Axis-aligned bounding box, computed once and cached.
    #[inline]
    #[must_use]
    pub fn bbox(&mut self) -> Aabb {
        if let Some(b) = self.cached_bbox {
            return b;
        }
        let b = Aabb::enclose_points(self.vertices.iter().map(|v| Vec3::from_array(v.position)));
        self.cached_bbox = Some(b);
        b
    }
}

/// Builder for a [`Mesh`]. Useful when a loader streams vertices/indices.
#[derive(Default, Debug)]
pub struct MeshBuilder {
    name: Option<String>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    /// Construct an empty builder.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the optional name.
    #[inline]
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Reserve space for `n` vertices and `i` indices.
    #[inline]
    #[must_use]
    pub fn reserve(mut self, vertices: usize, indices: usize) -> Self {
        self.vertices.reserve(vertices);
        self.indices.reserve(indices);
        self
    }

    /// Push a vertex; returns its index for convenience.
    #[inline]
    pub fn push_vertex(&mut self, v: Vertex) -> u32 {
        let idx = u32::try_from(self.vertices.len()).unwrap_or(u32::MAX);
        self.vertices.push(v);
        idx
    }

    /// Push a triangle by vertex indices.
    #[inline]
    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }

    /// Finalize into a [`Mesh`], validating indices.
    ///
    /// # Errors
    /// See [`Mesh::new`].
    pub fn build(self) -> Result<Mesh, CoreError> {
        Mesh::new(self.name, self.vertices, self.indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vertex {
        Vertex::at(Vec3::new(x, y, z))
    }

    #[test]
    fn valid_mesh_constructs() {
        let mesh = Mesh::new(
            Some("tri".into()),
            vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            vec![0, 1, 2],
        )
        .expect("valid mesh");
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.name(), Some("tri"));
        assert!(!mesh.has_vertex_colors());
    }

    #[test]
    fn bad_index_count_is_rejected() {
        let err = Mesh::new(None, vec![v(0.0, 0.0, 0.0)], vec![0, 1]).unwrap_err();
        assert!(matches!(
            err,
            CoreError::IndexCountNotMultipleOfThree { .. }
        ));
    }

    #[test]
    fn out_of_range_index_is_rejected() {
        let err = Mesh::new(None, vec![v(0.0, 0.0, 0.0)], vec![0, 1, 5]).unwrap_err();
        assert!(matches!(err, CoreError::IndexOutOfRange { .. }));
    }

    #[test]
    fn bbox_is_computed_and_cached() {
        let mut mesh = Mesh::new(
            None,
            vec![v(-1.0, -2.0, 0.0), v(3.0, 4.0, 0.0), v(0.0, 0.0, 0.0)],
            vec![0, 1, 2],
        )
        .expect("valid");
        let b = mesh.bbox();
        assert_eq!(b.min, Vec3::new(-1.0, -2.0, 0.0));
        assert_eq!(b.max, Vec3::new(3.0, 4.0, 0.0));
        // Cached: second call must return the same value.
        assert_eq!(mesh.bbox(), b);
    }

    #[test]
    fn vertex_color_is_detected() {
        let mesh = Mesh::new(
            None,
            vec![
                Vertex::at(Vec3::ZERO).with_color([10, 20, 30, 255]),
                v(1.0, 0.0, 0.0),
                v(0.0, 1.0, 0.0),
            ],
            vec![0, 1, 2],
        )
        .expect("valid");
        assert!(mesh.has_vertex_colors());
    }

    #[test]
    fn builder_round_trip() {
        let mut b = MeshBuilder::new().with_name("built").reserve(3, 3);
        let a = b.push_vertex(v(0.0, 0.0, 0.0));
        let c = b.push_vertex(v(1.0, 0.0, 0.0));
        let d = b.push_vertex(v(0.0, 1.0, 0.0));
        b.push_triangle(a, c, d);
        let mesh = b.build().expect("valid");
        assert_eq!(mesh.name(), Some("built"));
        assert_eq!(mesh.triangle_count(), 1);
    }
}
