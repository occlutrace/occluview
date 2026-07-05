//! `occluview-cli` - headless CLI.
//!
//! Subcommands:
//!   - `thumbnail <file> [-o out.png] [--size N]` - render a thumbnail via
//!     the same offscreen path the Explorer shell extension uses. The exact
//!     same code path (`render_thumb::render_thumbnail`), so a correct PNG here
//!     means a correct thumbnail in Explorer.
//!   - `info <file> [file...]` - print format, vertex/triangle counts, bbox,
//!     colors, UVs, texture. Multiple files print per-file stats + an
//!     aggregate scene bbox (upper+lower arch case).
//!   - `help` - show usage.

// CLI tool: stdout/stderr is the entire point.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::{anyhow, Context, Result};
use occluview_formats::dispatch::{read_file, read_files};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let subcommand = args.next().unwrap_or_else(|| {
        print_usage();
        "help".to_string()
    });

    match subcommand.as_str() {
        "thumbnail" => cmd_thumbnail(&mut args),
        "info" => cmd_info(&mut args),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => {
            print_usage();
            Err(anyhow!("unknown subcommand: {other}"))
        }
    }
}

/// `thumbnail <file> [-o out.png] [--size N]`
fn cmd_thumbnail(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let file: PathBuf = args
        .next()
        .ok_or_else(|| anyhow!("thumbnail: missing <file> argument"))?
        .into();
    let mut output: Option<PathBuf> = None;
    let mut size: u16 = 256;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output = Some(PathBuf::from(
                    args.next().ok_or_else(|| anyhow!("-o requires a path"))?,
                ));
            }
            "--size" => {
                size = args
                    .next()
                    .ok_or_else(|| anyhow!("--size requires a number"))?
                    .parse()
                    .context("--size must be a number")?;
            }
            other => return Err(anyhow!("unknown flag: {other}")),
        }
    }

    let out_path = output.unwrap_or_else(|| {
        let mut p = file.clone();
        p.set_extension("png");
        p
    });

    eprintln!("Loading {}...", file.display());
    let mesh = read_file(&file).with_context(|| format!("loading {}", file.display()))?;
    eprintln!(
        "  {} vertices, {} triangles, {}",
        mesh.vertices().len(),
        mesh.triangle_count(),
        if mesh.is_point_cloud() {
            "point cloud"
        } else {
            "triangle mesh"
        }
    );

    eprintln!("Rendering {size}x{size} thumbnail...");
    let pixels = occluview_shell::render_thumbnail(
        file.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| anyhow!("file has no extension"))?,
        &std::fs::read(&file)?,
        occluview_render::ThumbnailSpec {
            size_px: size,
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("render failed: {e}"))?;

    eprintln!("Writing {}...", out_path.display());
    let img = image::RgbaImage::from_raw(u32::from(size), u32::from(size), pixels)
        .ok_or_else(|| anyhow!("failed to create image buffer"))?;
    img.save(&out_path)
        .with_context(|| format!("writing {}", out_path.display()))?;

    eprintln!("Done: {}", out_path.display());
    Ok(())
}

/// `info <file> [file...]` - print mesh statistics. When multiple files are
/// given, prints per-file stats plus an aggregate scene bbox.
fn cmd_info(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let files: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if files.is_empty() {
        return Err(anyhow!("info: missing <file> argument"));
    }

    // Single-file fast path keeps the existing output format unchanged.
    if files.len() == 1 {
        return cmd_info_one(&files[0]);
    }

    // Multi-file: per-file summary + scene aggregate.
    let scene = read_files(&files).map_err(|(path, e)| anyhow!("{}: {}", path.display(), e))?;

    for (i, entry) in scene.meshes().iter().enumerate() {
        let m = &entry.mesh;
        let bbox = m.bbox_uncached();
        let [w, h, d] = bbox.dimensions_mm();
        println!(
            "[{}/{}] {}  verts={} tris={} kind={} bbox={:.1}x{:.1}x{:.1}mm",
            i + 1,
            scene.meshes().len(),
            files
                .get(i)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            m.vertices().len(),
            m.triangle_count(),
            if m.is_point_cloud() { "cloud" } else { "mesh" },
            w.as_mm(),
            h.as_mm(),
            d.as_mm(),
        );
    }

    let scene_bbox = scene.bbox();
    if !scene_bbox.is_empty() {
        let [w, h, d] = scene_bbox.dimensions_mm();
        println!(
            "Scene bbox: {:.2} x {:.2} x {:.2} mm  ({})",
            w.as_mm(),
            h.as_mm(),
            d.as_mm(),
            files.len()
        );
    }
    Ok(())
}

/// Single-file info (the original output format).
fn cmd_info_one(file: &Path) -> Result<()> {
    let mut mesh = read_file(file).with_context(|| format!("loading {}", file.display()))?;

    let bbox = mesh.bbox();
    let [w, h, d] = bbox.dimensions_mm();

    println!("File:       {}", file.display());
    println!(
        "Format:     {}",
        file.extension().and_then(|e| e.to_str()).unwrap_or("?")
    );
    println!(
        "Kind:       {}",
        if mesh.is_point_cloud() {
            "point cloud"
        } else {
            "triangle mesh"
        }
    );
    println!("Vertices:   {}", mesh.vertices().len());
    println!("Triangles:  {}", mesh.triangle_count());
    println!(
        "Colors:     {}",
        if mesh.has_vertex_colors() {
            "yes"
        } else {
            "no"
        }
    );
    println!("UVs:        {}", if mesh.has_uvs() { "yes" } else { "no" });
    println!(
        "Texture:    {}",
        if mesh.texture().is_some() {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "Bbox:       {:.2} x {:.2} x {:.2} mm",
        w.as_mm(),
        h.as_mm(),
        d.as_mm()
    );
    println!(
        "Bbox range: [{:.2}, {:.2}, {:.2}] .. [{:.2}, {:.2}, {:.2}]",
        bbox.min.x, bbox.min.y, bbox.min.z, bbox.max.x, bbox.max.y, bbox.max.z,
    );
    Ok(())
}

fn print_usage() {
    eprintln!(
        "occluview-cli - headless OccluView\n\
         \n\
         USAGE:\n    \
         occluview-cli <SUBCOMMAND> [ARGS]\n\
         \n\
         SUBCOMMANDS:\n    \
         thumbnail <file> [-o out.png] [--size N]   Render a thumbnail (same path as the Explorer extension)\n    \
         info      <file>                          Print format / counts / bbox\n    \
         help                                       Show this message"
    );
}
