use crate::error::FormatError;
use glam::Vec3;
use occluview_core::{Mesh, MeshTexture, Vertex};
use occluview_hps::{DecodedSurface, DecodedSurfaceParts};

pub(super) fn build_mesh(surface: DecodedSurface) -> Result<Mesh, FormatError> {
    let DecodedSurfaceParts {
        positions,
        indices,
        colors,
        uvs,
        normals,
        texture,
    } = surface.into_parts();
    let positions: Vec<Vec3> = positions.into_iter().map(Vec3::from_array).collect();
    let normals = normals.map_or_else(
        || smooth_normals(&positions, &indices),
        |normals| normals.into_iter().map(Vec3::from_array).collect(),
    );
    let mut vertices = Vec::with_capacity(positions.len());

    for (index, position) in positions.into_iter().enumerate() {
        let mut vertex = Vertex::at(position).with_normal(normals[index]);
        if let Some(colors) = &colors {
            vertex = vertex.with_color(colors[index]);
        }
        if let Some(uvs) = &uvs {
            vertex = vertex.with_uv(uvs[index]);
        }
        vertices.push(vertex);
    }

    let mut mesh =
        Mesh::new(Some("HPS".to_string()), vertices, indices).map_err(FormatError::Core)?;
    if let Some(texture) = texture {
        let (width, height, rgba) = texture.into_parts();
        mesh.set_texture(MeshTexture::new(width, height, rgba));
    }
    Ok(mesh)
}

fn smooth_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let index_a = triangle[0] as usize;
        let index_b = triangle[1] as usize;
        let index_c = triangle[2] as usize;
        let (Some(&a), Some(&b), Some(&c)) = (
            positions.get(index_a),
            positions.get(index_b),
            positions.get(index_c),
        ) else {
            continue;
        };
        let face_normal = (b - a).cross(c - a);
        if face_normal.is_finite() && face_normal.length_squared() > f32::EPSILON {
            normals[index_a] += face_normal;
            normals[index_b] += face_normal;
            normals[index_c] += face_normal;
        }
    }
    for normal in &mut normals {
        *normal = if normal.length_squared() > f32::EPSILON {
            normal.normalize()
        } else {
            Vec3::Z
        };
    }
    normals
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::build_mesh;
    use occluview_hps::{DecodedSurface, DecodedTexture};

    #[test]
    fn neutral_surface_adapter_preserves_geometry_attributes_and_texture() {
        let texture = DecodedTexture::new(1, 1, vec![9, 8, 7, 255]).expect("valid texture");
        let surface = DecodedSurface::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![0, 1, 2],
            Some(vec![[1, 2, 3, 255], [4, 5, 6, 255], [7, 8, 9, 255]]),
            Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
            Some(texture),
        )
        .expect("valid neutral surface")
        .with_normals(vec![[1.0, 0.0, 0.0]; 3])
        .expect("valid neutral normals");

        let mesh = build_mesh(surface).expect("surface should adapt");

        assert_eq!(mesh.indices(), &[0, 1, 2]);
        assert_eq!(mesh.vertices()[1].position, [1.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices()[2].color, [7, 8, 9, 255]);
        assert_eq!(mesh.vertices()[2].uv, [0.0, 1.0]);
        assert_eq!(mesh.vertices()[0].normal, [1.0, 0.0, 0.0]);
        let texture = mesh.texture().expect("mesh texture should be present");
        assert_eq!((texture.width, texture.height), (1, 1));
        assert_eq!(texture.rgba, [9, 8, 7, 255]);
    }
}
