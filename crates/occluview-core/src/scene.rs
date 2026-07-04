//! The scene graph.
//!
//! A [`Scene`] holds one or more meshes and the per-mesh transforms/materials
//! that position and present them. The dental use case is **upper + lower arch**
//! as a two-mesh scene (ADR-0009), so multi-mesh is first-class, not an
//! afterthought.
//!
//! The scene is the bridge between file loading and rendering: loaders produce
//! meshes, the app composes a scene, the renderer consumes it.

use crate::mesh::Mesh;
use glam::{Affine3A, Vec3};

/// Per-instance mesh entry in a scene.
#[derive(Clone, Debug)]
pub struct SceneMesh {
    /// The underlying mesh. Shared via `Arc` once we add cross-thread sharing;
    /// owned for now to keep `core` dependency-free.
    pub mesh: Mesh,
    /// Per-instance transform (placement of this mesh in the scene).
    pub transform: Affine3A,
    /// Display tint in linear sRGB (0..1). Default white.
    pub tint: [f32; 4],
    /// Opacity 0..1 — used for transparency / "ghost" arches.
    pub opacity: f32,
    /// Whether this mesh is visible.
    pub visible: bool,
}

impl SceneMesh {
    /// Construct an entry from a mesh, identity transform, white tint, opaque,
    /// visible.
    #[inline]
    #[must_use]
    pub fn new(mesh: Mesh) -> Self {
        Self {
            mesh,
            transform: Affine3A::IDENTITY,
            tint: [1.0, 1.0, 1.0, 1.0],
            opacity: 1.0,
            visible: true,
        }
    }

    /// Set the per-instance transform.
    #[inline]
    #[must_use]
    pub fn with_transform(mut self, t: Affine3A) -> Self {
        self.transform = t;
        self
    }

    /// Set the linear-sRGB tint.
    #[inline]
    #[must_use]
    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    /// Set opacity 0..1.
    #[inline]
    #[must_use]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

/// A scene: a collection of positioned, styled meshes plus a default lighting
/// and background. The dental "upper+lower" case is a 2-mesh scene.
#[derive(Clone, Debug)]
pub struct Scene {
    meshes: Vec<SceneMesh>,
    /// Scene-wide background color (linear sRGB). OccluTrace dark by default.
    pub background: [f32; 4],
    /// Ambient light intensity (0..1). Affects flat-shaded meshes.
    pub ambient: f32,
    /// Key light direction in scene space (unit length expected).
    pub key_light_dir: Vec3,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            meshes: Vec::new(),
            // OccluTrace brand dark: #0a0a0a in sRGB.
            background: linear_srgb_from_srgb([0.039, 0.039, 0.039, 1.0]),
            ambient: 0.35,
            key_light_dir: Vec3::new(0.4, 0.8, 0.5).normalize_or_zero(),
        }
    }
}

impl Scene {
    /// Construct an empty scene with OccluTrace defaults.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mesh entry; returns its index in the scene.
    #[inline]
    pub fn add(&mut self, entry: SceneMesh) -> usize {
        let i = self.meshes.len();
        self.meshes.push(entry);
        i
    }

    /// All mesh entries.
    #[inline]
    #[must_use]
    pub fn meshes(&self) -> &[SceneMesh] {
        &self.meshes
    }

    /// All mesh entries, mutable.
    #[inline]
    pub fn meshes_mut(&mut self) -> &mut [SceneMesh] {
        &mut self.meshes
    }

    /// Number of visible meshes.
    #[inline]
    #[must_use]
    pub fn visible_count(&self) -> usize {
        self.meshes.iter().filter(|m| m.visible).count()
    }
}

impl Default for SceneMesh {
    fn default() -> Self {
        Self::new(Mesh::empty())
    }
}

/// Approximate sRGB→linear conversion (no external color crate in `core`).
///
/// Used only for the default background tint; precise color management is an
/// explicit v2 concern (open Q3 in `ARCHITECTURE.md`).
fn linear_srgb_from_srgb(srgb: [f32; 4]) -> [f32; 4] {
    let f = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(srgb[0]), f(srgb[1]), f(srgb[2]), srgb[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Vertex;
    use glam::Vec3;

    fn tri() -> Mesh {
        Mesh::new(
            None,
            vec![
                Vertex::at(Vec3::ZERO),
                Vertex::at(Vec3::new(1.0, 0.0, 0.0)),
                Vertex::at(Vec3::new(0.0, 1.0, 0.0)),
            ],
            vec![0, 1, 2],
        )
        .expect("valid mesh")
    }

    #[test]
    fn empty_scene_has_no_meshes() {
        let s = Scene::new();
        assert_eq!(s.meshes().len(), 0);
        assert_eq!(s.visible_count(), 0);
    }

    #[test]
    fn add_two_meshes_for_upper_lower() {
        let mut s = Scene::new();
        s.add(SceneMesh::new(tri()).with_tint([0.6, 0.8, 1.0, 1.0]));
        s.add(SceneMesh::new(tri()).with_tint([1.0, 0.7, 0.6, 1.0]));
        assert_eq!(s.meshes().len(), 2);
        assert_eq!(s.visible_count(), 2);
    }

    #[test]
    fn opacity_is_clamped() {
        let e = SceneMesh::new(tri()).with_opacity(5.0);
        assert_eq!(e.opacity, 1.0);
        let e = SceneMesh::new(tri()).with_opacity(-1.0);
        assert_eq!(e.opacity, 0.0);
    }

    #[test]
    fn hide_affects_visible_count() {
        let mut s = Scene::new();
        let i = s.add(SceneMesh::new(tri()));
        s.add(SceneMesh::new(tri()));
        s.meshes_mut()[i].visible = false;
        assert_eq!(s.visible_count(), 1);
    }
}
