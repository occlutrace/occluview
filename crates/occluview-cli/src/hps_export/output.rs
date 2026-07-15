use super::error::CliError;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const RESERVED_ARTIFACTS: &[&str] = &["surface.glb", "surface.ply", "manifest.json"];

pub(crate) fn write_artifacts(
    output_dir: &Path,
    geometry: &[u8],
    preview: Option<&[u8]>,
    manifest: &[u8],
) -> Result<(), CliError> {
    fs::create_dir_all(output_dir).map_err(|_| CliError::OutputDirectoryFailed)?;
    if !output_dir.is_dir() {
        return Err(CliError::OutputDirectoryFailed);
    }
    for reserved in RESERVED_ARTIFACTS {
        if output_dir
            .join(reserved)
            .try_exists()
            .map_err(|_| CliError::OutputDirectoryFailed)?
        {
            return Err(CliError::OutputExists);
        }
    }

    let geometry_path = output_dir.join("surface.ply");
    write_new(&geometry_path, geometry)?;
    let preview_path = output_dir.join("surface.glb");
    if let Some(preview) = preview {
        if let Err(error) = write_new(&preview_path, preview) {
            let _ = fs::remove_file(&geometry_path);
            return Err(error);
        }
    }
    if let Err(error) = write_new(&output_dir.join("manifest.json"), manifest) {
        let _ = fs::remove_file(geometry_path);
        if preview.is_some() {
            let _ = fs::remove_file(preview_path);
        }
        return Err(error);
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CliError::OutputExists
            } else {
                CliError::OutputWriteFailed
            }
        })?;
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(CliError::OutputWriteFailed);
    }
    Ok(())
}
