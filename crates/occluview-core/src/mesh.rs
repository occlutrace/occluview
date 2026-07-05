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
///
/// Layout (36 bytes, no padding, max align 4):
/// - `position` `[f32;3]` @ 0
/// - `normal`   `[f32;3]` @ 12
/// - `color`    `[u8;4]`  @ 24
/// - `uv`       `[f32;2]` @ 28
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable, PartialEq)]
pub struct Vertex {
    /// Position in OccluView's canonical frame, in millimeters.
    pub position: [f32; 3],
    /// Unit normal, in canonical frame. Defaults to zero (flat-shaded on load).
    pub normal: [f32; 3],
    /// sRGBA color, packed 0..=255. `(255, 255, 255, 255)` when color is absent.
    pub color: [u8; 4],
    /// Texture coordinates (UV). `[0.0, 0.0]` when the vertex has no UV
    /// (plain STL, untextured PLY, etc.). Loaders set this from `TEXCOORD_0`
    /// (glTF), `vt` (OBJ), or `texcoord`/`s`/`t` (PLY).
    pub uv: [f32; 2],
}

impl Vertex {
    /// Construct a vertex with position only (normal zeroed, color white,
    /// UV zeroed).
    #[inline]
    #[must_use]
    pub fn at(position: Vec3) -> Self {
        Self {
            position: position.to_array(),
            normal: [0.0; 3],
            color: [255, 255, 255, 255],
            uv: [0.0, 0.0],
        }
    }

    /// Construct a vertex with position and normal (color white, UV zeroed).
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

    /// Construct a vertex with texture coordinates.
    #[inline]
    #[must_use]
    pub fn with_uv(mut self, uv: [f32; 2]) -> Self {
        self.uv = uv;
        self
    }
}

/// Whether a [`Mesh`] carries triangle connectivity or is just a point cloud.
///
/// Dental color scanners frequently emit PLY files with `element vertex` but
/// no `element face` — a point cloud. The renderer draws these with a
/// different pipeline (point list, not triangle list); the loader sets the
/// kind so downstream code can branch.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum MeshKind {
    /// Triangle mesh: `indices` is non-empty and indexes triangle corners.
    #[default]
    TriangleMesh,
    /// Point cloud: `indices` is empty; each vertex is drawn as a point.
    PointCloud,
}

/// A CPU-side decoded texture image attached to a mesh (glTF `image` /
/// `texture`, decoded from PNG/JPEG/etc. by `occluview-formats`). The
/// renderer uploads this to a `wgpu::Texture` and samples it via the UV
/// channel of [`Vertex`].
///
/// Stored as RGBA8 — the decoder normalizes whatever the source format was.
#[derive(Clone, Debug)]
pub struct MeshTexture {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// `width * height * 4` RGBA8 pixels, row-major top-to-bottom.
    pub rgba: Vec<u8>,
}

impl MeshTexture {
    /// Construct from decoded RGBA8 pixels. Asserts the length matches
    /// `width * height * 4`.
    #[must_use]
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        debug_assert_eq!(rgba.len(), (width as usize) * (height as usize) * 4);
        Self {
            width,
            height,
            rgba,
        }
    }

    /// A 1×1 opaque white texture — the neutral fallback used when a mesh has
    /// UVs but no decoded image, or when the renderer needs a bound texture
    /// for an untextured mesh (so the shader's textureless branch still has a
    /// valid binding).
    #[must_use]
    pub fn white_1x1() -> Self {
        Self {
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255],
        }
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
    /// True if any vertex carries non-zero UV coordinates.
    has_uvs: bool,
    /// Whether this is a triangle mesh or a point cloud.
    kind: MeshKind,
    /// Decoded texture image, if the source file provided one (glTF
    /// `image`/`texture`). `None` for plain STL / untextured meshes.
    texture: Option<MeshTexture>,
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
            has_uvs: false,
            kind: MeshKind::default(),
            texture: None,
            cached_bbox: None,
        }
    }

    /// Construct a point cloud from vertices only (no indices). Sets
    /// [`MeshKind::PointCloud`].
    #[must_use]
    pub fn point_cloud(name: Option<String>, vertices: Vec<Vertex>) -> Self {
        let has_vertex_colors = vertices.iter().any(|v| v.color != [255, 255, 255, 255]);
        let has_uvs = vertices.iter().any(|v| v.uv != [0.0, 0.0]);
        Self {
            name,
            vertices,
            indices: Vec::new(),
            has_vertex_colors,
            has_uvs,
            kind: MeshKind::PointCloud,
            texture: None,
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
        let has_uvs = vertices.iter().any(|v| v.uv != [0.0, 0.0]);
        Ok(Self {
            name,
            vertices,
            indices,
            has_vertex_colors,
            has_uvs,
            kind: MeshKind::TriangleMesh,
            texture: None,
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

    /// True if any vertex carries non-zero UV coordinates. Loaders set UVs
    /// from `TEXCOORD_0` (glTF), `vt` (OBJ), or `texcoord` (PLY).
    #[inline]
    #[must_use]
    pub fn has_uvs(&self) -> bool {
        self.has_uvs
    }

    /// The decoded texture image, if the source file provided one.
    #[inline]
    #[must_use]
    pub fn texture(&self) -> Option<&MeshTexture> {
        self.texture.as_ref()
    }

    /// Attach a decoded texture image (e.g. from a glTF `image`). Used by
    /// loaders after constructing the mesh.
    #[inline]
    pub fn set_texture(&mut self, texture: MeshTexture) {
        self.texture = Some(texture);
    }

    /// Whether this is a triangle mesh or a point cloud.
    #[inline]
    #[must_use]
    pub fn kind(&self) -> MeshKind {
        self.kind
    }

    /// True if this geometry has no triangle connectivity (point cloud).
    #[inline]
    #[must_use]
    pub fn is_point_cloud(&self) -> bool {
        self.kind == MeshKind::PointCloud
    }

    /// Bounding box computed fresh from vertices, **without** touching the
    /// cache. Used by scene-level composition (which folds many meshes' boxes
    /// without wanting to mutate each one).
    #[must_use]
    pub fn bbox_uncached(&self) -> Aabb {
        Aabb::enclose_points(self.vertices.iter().map(|v| Vec3::from_array(v.position)))
    }

    /// Axis-aligned bounding box, computed once and cached.
    #[inline]
    #[must_use]
    pub fn bbox(&mut self) -> Aabb {
        if let Some(b) = self.cached_bbox {
            return b;
        }
        let b = self.bbox_uncached();
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
    /// If true, `build()` produces a [`MeshKind::PointCloud`] regardless of
    /// indices. Set by loaders that know there is no face element.
    force_point_cloud: bool,
}

impl MeshBuilder {
    /// Construct an empty builder.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark the result as a point cloud (no triangle connectivity).
    /// Loaders call this when the source format declares vertices but no faces.
    #[inline]
    #[must_use]
    pub const fn as_point_cloud(mut self) -> Self {
        self.force_point_cloud = true;
        self
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
        if self.force_point_cloud {
            return Ok(Mesh::point_cloud(self.name, self.vertices));
        }
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

    #[test]
    fn vertex_uv_is_detected() {
        let mesh = Mesh::new(
            None,
            vec![
                Vertex::at(Vec3::ZERO).with_uv([0.0, 0.0]),
                Vertex::at(Vec3::new(1.0, 0.0, 0.0)).with_uv([1.0, 0.0]),
                Vertex::at(Vec3::new(0.0, 1.0, 0.0)).with_uv([0.0, 1.0]),
            ],
            vec![0, 1, 2],
        )
        .expect("valid");
        assert!(mesh.has_uvs());
    }

    #[test]
    fn vertex_no_uv_is_not_detected() {
        let mesh = Mesh::new(
            None,
            vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            vec![0, 1, 2],
        )
        .expect("valid");
        assert!(!mesh.has_uvs());
    }

    #[test]
    fn vertex_layout_has_uv_appended() {
        // Adding `uv` ([f32;2] = 8 bytes) after `color` grew the struct from
        // 28 to 36 bytes. The layout is position@0, normal@12, color@24,
        // uv@28 — no padding holes, all naturally aligned (max align = 4).
        assert_eq!(std::mem::size_of::<Vertex>(), 36);
        let sample = Vertex {
            position: [1.0, 2.0, 3.0],
            normal: [4.0, 5.0, 6.0],
            color: [7, 8, 9, 10],
            uv: [11.0, 12.0],
        };
        let base = &sample as *const Vertex as usize;
        assert_eq!(&sample.position as *const _ as usize - base, 0);
        assert_eq!(&sample.normal as *const _ as usize - base, 12);
        assert_eq!(&sample.color as *const _ as usize - base, 24);
        assert_eq!(&sample.uv as *const _ as usize - base, 28);
    }

    #[test]
    fn mesh_texture_white_1x1() {
        let t = MeshTexture::white_1x1();
        assert_eq!(t.width, 1);
        assert_eq!(t.height, 1);
        assert_eq!(t.rgba, vec![255, 255, 255, 255]);
    }

    #[test]
    fn set_texture_attaches() {
        let mut mesh = Mesh::new(
            None,
            vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            vec![0, 1, 2],
        )
        .expect("valid");
        assert!(mesh.texture().is_none());
        mesh.set_texture(MeshTexture::white_1x1());
        assert!(mesh.texture().is_some());
    }

    #[test]
    fn bbox_uncached_matches_cached() {
        let mut mesh = Mesh::new(
            None,
            vec![v(-1.0, -2.0, 0.0), v(3.0, 4.0, 0.0), v(0.0, 0.0, 0.0)],
            vec![0, 1, 2],
        )
        .expect("valid");
        let uncached = mesh.bbox_uncached();
        let cached = mesh.bbox();
        assert_eq!(uncached, cached);
    }
}
