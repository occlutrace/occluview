use super::{
    AppErrorDialog, LayerContextAction, LayerContextRequest, OccluViewApp, PathBuf, Scene,
};
use anyhow::{bail, Context, Result};
use occluview_formats::write::{
    write_mesh_overwrite, MeshWriteFormat, MeshWriteOptions, MeshWriteReport,
};
use std::path::Path;

/// How an interactive save-edited-layers pass ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SaveEditedLayersOutcome {
    /// Every layer with unsaved edits was exported.
    AllSaved,
    /// The operator cancelled a dialog or an export failed; unsaved edits
    /// remain.
    Aborted,
    /// Nothing carried unsaved edits.
    NothingToSave,
}

impl OccluViewApp {
    pub(super) fn save_layer_export_dialog(
        &mut self,
        scene: &Scene,
        paths: &[PathBuf],
        request: LayerContextRequest,
    ) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PLY mesh", &["ply"])
            .add_filter("STL mesh", &["stl"])
            .add_filter("OBJ mesh", &["obj"])
            .set_file_name(default_layer_export_name(paths, scene, request.index))
            .save_file()
        else {
            return false;
        };

        match write_layer_export_to_path(scene, request, &path) {
            Ok(report) => {
                // The layer on disk now matches the scene: it no longer
                // counts toward the unsaved-edits close guard.
                self.unsaved_edit_layer_ids.remove(&request.layer_id);
                if self.unsaved_edit_layer_ids.is_empty() {
                    self.has_unsaved_mesh_edits = false;
                }
                self.status_message = Some(format!(
                    "Exported layer as {}: {}",
                    mesh_export_format_label(report.format),
                    path.display()
                ));
                true
            }
            Err(error) => {
                let summary = format!("Could not export layer: {error}");
                self.status_message = Some(summary.clone());
                self.app_error = Some(AppErrorDialog {
                    title: "Could not export layer".to_string(),
                    summary,
                    details: format!(
                        "Layer export failed\n\nPath:\n{}\n\nError:\n{error:#}",
                        path.display()
                    ),
                });
                false
            }
        }
    }

    /// Walk every layer with unsaved edits through the export dialog, one at
    /// a time. Stops at the first cancelled dialog or failed write so the
    /// operator is never told edits were saved when they were not.
    pub(super) fn save_edited_layers_flow(&mut self) -> SaveEditedLayersOutcome {
        let Some(scene) = self.scene.clone() else {
            return SaveEditedLayersOutcome::NothingToSave;
        };
        let paths = self.current_paths.clone();
        let pending: Vec<(usize, occluview_core::SceneMeshId)> = scene
            .meshes()
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.unsaved_edit_layer_ids.contains(&entry.id()))
            .map(|(index, entry)| (index, entry.id()))
            .collect();
        if pending.is_empty() {
            // Edited layers may have been removed from the scene since; the
            // guard has nothing actionable left.
            self.clear_unsaved_mesh_edits();
            return SaveEditedLayersOutcome::NothingToSave;
        }
        for (index, layer_id) in pending {
            let request = LayerContextRequest {
                index,
                layer_id,
                action: LayerContextAction::ExportLayer,
            };
            if !self.save_layer_export_dialog(scene.as_ref(), &paths, request) {
                return SaveEditedLayersOutcome::Aborted;
            }
        }
        if self.unsaved_edit_layer_ids.is_empty() {
            SaveEditedLayersOutcome::AllSaved
        } else {
            SaveEditedLayersOutcome::Aborted
        }
    }
}

fn write_layer_export_to_path(
    scene: &Scene,
    request: LayerContextRequest,
    path: &Path,
) -> Result<MeshWriteReport> {
    if request.action != LayerContextAction::ExportLayer {
        bail!("layer export received a non-export action");
    }

    let Some(entry) = scene.meshes().get(request.index) else {
        bail!("layer index {} is no longer available", request.index + 1);
    };
    if entry.id() != request.layer_id {
        bail!("layer identity changed before export");
    }

    let format = mesh_export_format_from_path(path)?;
    write_mesh_overwrite(path, &entry.mesh, format, MeshWriteOptions::default())
        .with_context(|| format!("writing {}", path.display()))
}

fn mesh_export_format_from_path(path: &Path) -> Result<MeshWriteFormat> {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        bail!("unsupported export format; choose an output file ending in .ply, .stl, or .obj");
    };

    match extension.to_ascii_lowercase().as_str() {
        "ply" => Ok(MeshWriteFormat::PlyBinaryLittleEndian),
        "stl" => Ok(MeshWriteFormat::StlBinary),
        "obj" => Ok(MeshWriteFormat::Obj),
        other => bail!("unsupported export format .{other}; use .ply, .stl, or .obj"),
    }
}

fn mesh_export_format_label(format: MeshWriteFormat) -> &'static str {
    match format {
        MeshWriteFormat::StlBinary => "STL",
        MeshWriteFormat::PlyBinaryLittleEndian => "PLY",
        MeshWriteFormat::Obj => "OBJ",
    }
}

fn default_layer_export_name(paths: &[PathBuf], scene: &Scene, index: usize) -> String {
    let stem = paths
        .get(index)
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .or_else(|| {
            scene
                .meshes()
                .get(index)
                .and_then(|entry| entry.mesh.name())
        })
        .map(sanitize_filename_stem)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| format!("layer-{}", index + 1));

    format!("{stem}-edited.ply")
}

fn sanitize_filename_stem(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .trim_matches(['.', ' '])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer_actions::{LayerContextAction, LayerContextRequest};
    use glam::Vec3;
    use occluview_core::{
        delete_selected_faces_in_mesh, FaceSelection, Mesh, MeshEditOptions, Scene, SceneMesh,
        Vertex,
    };
    use occluview_formats::write::MeshWriteFormat;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn v(x: f32, y: f32, z: f32) -> Vertex {
        Vertex::at(Vec3::new(x, y, z))
    }

    fn exportable_scene() -> Result<Scene> {
        let mesh = Mesh::new(
            Some("scan".into()),
            vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            vec![0, 1, 2],
        )?;
        let mut scene = Scene::new();
        scene.add(SceneMesh::new(mesh));
        Ok(scene)
    }

    #[test]
    fn layer_export_format_is_selected_from_output_extension() -> Result<()> {
        assert_eq!(
            mesh_export_format_from_path(Path::new("edited.stl")).ok(),
            Some(MeshWriteFormat::StlBinary)
        );
        assert_eq!(
            mesh_export_format_from_path(Path::new("edited.ply")).ok(),
            Some(MeshWriteFormat::PlyBinaryLittleEndian)
        );
        assert_eq!(
            mesh_export_format_from_path(Path::new("edited.obj")).ok(),
            Some(MeshWriteFormat::Obj)
        );
        let error = match mesh_export_format_from_path(Path::new("edited.glb")) {
            Ok(format) => {
                return Err(anyhow::anyhow!(
                    "glb should not be an edit export target, got {format:?}"
                ));
            }
            Err(error) => error,
        };
        assert!(error.to_string().contains("unsupported export format"));
        Ok(())
    }

    #[test]
    fn default_layer_export_name_prefers_plain_scan_name_with_ply_extension() -> Result<()> {
        let scene = exportable_scene()?;
        let paths = vec![PathBuf::from("very-long-scan-name.hps")];

        let name = default_layer_export_name(&paths, &scene, 0);

        assert_eq!(name, "very-long-scan-name-edited.ply");
        Ok(())
    }

    #[test]
    fn write_layer_export_writes_requested_layer_without_mutating_scene() -> Result<()> {
        let scene = exportable_scene()?;
        let path = temp_file("ply");
        let request = LayerContextRequest {
            index: 0,
            layer_id: scene.meshes()[0].id(),
            action: LayerContextAction::ExportLayer,
        };

        let report = write_layer_export_to_path(&scene, request, &path)?;

        assert_eq!(report.format, MeshWriteFormat::PlyBinaryLittleEndian);
        assert!(path.exists());
        assert_eq!(scene.meshes()[0].mesh.triangle_count(), 1);
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn write_layer_export_rejects_stale_layer_identity() -> Result<()> {
        let scene = exportable_scene()?;
        let path = temp_file("ply");
        let request = LayerContextRequest {
            index: 0,
            layer_id: SceneMesh::new(Mesh::empty()).id(),
            action: LayerContextAction::ExportLayer,
        };

        let error = match write_layer_export_to_path(&scene, request, &path) {
            Ok(report) => {
                return Err(anyhow::anyhow!(
                    "stale layer export unexpectedly succeeded: {report:?}"
                ));
            }
            Err(error) => error,
        };

        assert!(error
            .to_string()
            .contains("layer identity changed before export"));
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn write_layer_export_uses_current_edited_mesh_snapshot() -> Result<()> {
        let mut scene = exportable_scene()?;
        let layer_id = scene.meshes()[0].id();
        let selection = FaceSelection::new(vec![true]);
        let edit = delete_selected_faces_in_mesh(
            &scene.meshes()[0].mesh,
            &selection,
            MeshEditOptions::default(),
        )?;
        scene.meshes_mut()[0].mesh = edit.mesh;

        let path = temp_file("stl");
        let request = LayerContextRequest {
            index: 0,
            layer_id,
            action: LayerContextAction::ExportLayer,
        };

        let report = write_layer_export_to_path(&scene, request, &path)?;

        assert_eq!(report.format, MeshWriteFormat::StlBinary);
        assert!(path.exists());
        assert_eq!(scene.meshes()[0].mesh.triangle_count(), 0);
        let bytes = std::fs::read(&path)?;
        assert!(bytes.len() >= 84, "binary stl header should exist");
        let Ok(count_bytes) = <[u8; 4]>::try_from(&bytes[80..84]) else {
            return Err(anyhow::anyhow!("could not read binary stl triangle count"));
        };
        let triangle_count = u32::from_le_bytes(count_bytes);
        assert_eq!(triangle_count, 0);
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    fn temp_file(extension: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("occluview-layer-export-{unique}.{extension}"))
    }
}
