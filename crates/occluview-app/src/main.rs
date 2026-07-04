//! `occluview-app` - the desktop viewer binary.
//!
//! Windows-only (ADR-0001: Windows-first). On other hosts this binary prints a
//! clear message and exits; the live GUI requires winit + eframe which we only
//! resolve on Windows targets (see Cargo.toml `target.cfg(windows)` deps).
//!
//! ## Status (this commit)
//!
//! Opens a file from the CLI arg via `occluview-formats::dispatch`, renders it
//! offscreen through `occluview-render::Offscreen`, and displays the result as
//! an egui image with a stats overlay. The live wgpu surface integration
//! (orbit/zoom) lands next; this commit gets a real image on screen.

#![cfg_attr(not(windows), allow(dead_code, clippy::needless_main))]

use anyhow::{Context, Result};
use occluview_core::Mesh;
use occluview_formats::dispatch_by_extension;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[cfg(not(windows))]
fn main() -> Result<()> {
    eprintln!("occluview: the GUI is Windows-only (ADR-0001). On this host, use:");
    eprintln!("  cargo run -p occluview-cli -- thumbnail <file>");
    Ok(())
}

#[cfg(windows)]
fn main() -> Result<()> {
    use eframe::egui;
    use glam::{Mat4, Vec3};
    use occluview_core::Camera;
    use occluview_render::{GpuCamera, Offscreen, ThumbnailSpec};
    use std::sync::Arc;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let args = parse_args();
    tracing::info!(?args.file, "OccluView starting");

    let mesh = match args.file.as_ref() {
        Some(path) => Some(load_mesh(path).with_context(|| format!("loading {}", path.display()))?),
        None => None,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("OccluView"),
        ..Default::default()
    };

    eframe::run_native(
        "OccluView",
        native_options,
        Box::new(move |cc| Ok(Box::new(OccluViewApp::new(mesh)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e:?}"))?;

    Ok(())
}

#[cfg(windows)]
mod app_impl {
    use super::*;
    use eframe::egui;
    use glam::{Mat4, Vec3};
    use occluview_core::{Camera, Mesh};
    use occluview_render::{GpuCamera, Offscreen, ThumbnailSpec};
    use std::sync::Arc;

    pub(super) struct Args {
        pub file: Option<PathBuf>,
    }

    pub(super) fn parse_args() -> Args {
        let mut args = std::env::args().skip(1);
        let file = args.next().map(PathBuf::from);
        Args { file }
    }

    /// Load and parse a mesh file by extension (magic-first dispatch).
    pub(super) fn load_mesh(path: &std::path::Path) -> Result<Mesh> {
        let bytes = std::fs::read(path).context("reading file")?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .context("file has no extension")?;
        let mesh = dispatch_by_extension(&ext, &bytes).context("parsing mesh")?;
        Ok(mesh)
    }

    pub(super) struct OccluViewApp {
        mesh: Option<Arc<Mesh>>,
        rendered: Option<RenderedFrame>,
        needs_render: bool,
    }

    struct RenderedFrame {
        texture: egui::TextureHandle,
        size_px: [usize; 2],
        stats: MeshStats,
    }

    #[derive(Clone, Copy)]
    struct MeshStats {
        triangles: usize,
        vertices: usize,
        has_colors: bool,
        bbox_mm: [f32; 3],
    }

    impl OccluViewApp {
        pub(super) fn new(mesh: Option<Mesh>) -> Self {
            Self {
                mesh: mesh.map(Arc::new),
                rendered: None,
                needs_render: true,
            }
        }

        fn render_now(&mut self, ctx: &egui::Context) {
            use occluview_core::Camera;
            let Some(mesh) = self.mesh.clone() else {
                return;
            };
            let mut mesh_mut = (*mesh).clone();
            let bbox = mesh_mut.bbox();
            let cam = Camera::default().frame_occlusal(bbox, 45.0_f32.to_radians());
            let view = build_view_matrix(&cam);
            let proj = build_proj_matrix(&cam, 1.0);
            let gpu_cam = GpuCamera::new(view, proj, Vec3::new(0.4, 0.8, 0.5), cam.eye());

            let offscreen = match pollster::block_on(Offscreen::new()) {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!(error = ?e, "offscreen init failed");
                    return;
                }
            };
            let spec = ThumbnailSpec {
                size_px: 512,
                ..Default::default()
            };
            let pixels = match pollster::block_on(offscreen.render(&mesh, &gpu_cam, spec)) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(error = ?e, "offscreen render failed");
                    return;
                }
            };

            let stats = MeshStats {
                triangles: mesh.triangle_count(),
                vertices: mesh.vertices().len(),
                has_colors: mesh.has_vertex_colors(),
                bbox_mm: {
                    let [w, h, d] = bbox.dimensions_mm();
                    [w.as_mm(), h.as_mm(), d.as_mm()]
                },
            };

            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [usize::from(spec.size_px), usize::from(spec.size_px)],
                &pixels,
            );
            let texture =
                ctx.load_texture("occluview-mesh", color_image, egui::TextureOptions::LINEAR);
            self.rendered = Some(RenderedFrame {
                texture,
                size_px: [usize::from(spec.size_px), usize::from(spec.size_px)],
                stats,
            });
            self.needs_render = false;
        }
    }

    impl eframe::App for OccluViewApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            ctx.input(|i| {
                if let Some(file) = i.raw.dropped_files.first().cloned() {
                    if let Some(path) = file.path.clone() {
                        match load_mesh(&path) {
                            Ok(m) => {
                                self.mesh = Some(Arc::new(m));
                                self.needs_render = true;
                                self.rendered = None;
                            }
                            Err(e) => tracing::error!(error = ?e, "drop load failed"),
                        }
                    }
                }
            });

            if self.needs_render {
                self.render_now(ctx);
                ctx.request_repaint();
            }

            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Open...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("3D meshes", &["stl", "ply", "obj", "gltf", "glb"])
                            .pick_file()
                        {
                            match load_mesh(&path) {
                                Ok(m) => {
                                    self.mesh = Some(Arc::new(m));
                                    self.needs_render = true;
                                    self.rendered = None;
                                }
                                Err(e) => tracing::error!(error = ?e, "open failed"),
                            }
                        }
                    }
                    if let Some(m) = &self.mesh {
                        ui.label(format!(
                            "{} verts, {} tris{}",
                            m.vertices().len(),
                            m.triangle_count(),
                            if m.has_vertex_colors() {
                                " - colors"
                            } else {
                                ""
                            }
                        ));
                    } else {
                        ui.label("Drop a .stl/.ply/.obj/.glb file or click Open");
                    }
                });
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(r) = &self.rendered {
                    let available = ui.available_size();
                    let scale = (available.x / r.size_px[0] as f32)
                        .min(available.y / r.size_px[1] as f32)
                        .min(1.5);
                    let size =
                        egui::Vec2::new(r.size_px[0] as f32 * scale, r.size_px[1] as f32 * scale);
                    ui.allocate_ui_at_rect(
                        egui::Rect::from_center_size(ui.max_rect().center(), size),
                        |ui| {
                            ui.image((egui::TextureId::Managed(r.texture.id()), size));
                        },
                    );
                } else if self.mesh.is_none() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(120.0);
                        ui.heading("OccluView");
                        ui.label("Drop a 3D mesh file to open it.");
                    });
                } else {
                    ui.spinner();
                }
            });

            if let Some(r) = &self.rendered {
                egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let s = r.stats;
                        ui.label(format!(
                            "{} tris - {} verts - bbox {:.1}x{:.1}x{:.1} mm{}",
                            s.triangles,
                            s.vertices,
                            s.bbox_mm[0],
                            s.bbox_mm[1],
                            s.bbox_mm[2],
                            if s.has_colors { " - colors" } else { "" }
                        ));
                    });
                });
            }
        }
    }

    fn build_view_matrix(cam: &Camera) -> Mat4 {
        Mat4::look_at_rh(cam.eye(), cam.target, Vec3::Y)
    }

    fn build_proj_matrix(cam: &Camera, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(cam.fovy, aspect, cam.near, cam.far)
    }
}

#[cfg(windows)]
use app_impl::{load_mesh, parse_args, OccluViewApp};
