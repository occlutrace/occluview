//! `occluview-app` - the desktop viewer binary.
//! file-size-exempt: Windows GUI bootstrap stays together until live viewport state is extracted.
//!
//! Windows-only (ADR-0001: Windows-first). On other hosts this binary exits
//! with failure; the live GUI requires winit + eframe which we only resolve on
//! Windows targets (see Cargo.toml `target.cfg(windows)` deps).
//!
//! ## Status (this commit)
//!
//! Opens a file from the CLI arg via `occluview-formats::dispatch`, renders it
//! offscreen through `occluview-render::Offscreen`, and displays the result as
//! an egui image with a stats overlay. The live wgpu surface integration
//! (orbit/zoom) lands next; this commit gets a real image on screen.

#[cfg(windows)]
use anyhow::{Context, Result};
#[cfg(windows)]
use occluview_formats::dispatch_by_extension;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use tracing_subscriber::EnvFilter;

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> Result<()> {
    use eframe::egui;

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
        Box::new(move |_cc| Ok(Box::new(OccluViewApp::new(mesh)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e:?}"))?;

    Ok(())
}

#[cfg(windows)]
mod app_impl {
    use super::{dispatch_by_extension, Context, PathBuf, Result};
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
        // --- Cut View state ---
        show_cut_view: bool,
        cut_state: CutState,
        cut_texture: Option<egui::TextureHandle>,
        cut_needs_render: bool,
    }

    /// The cut-view controls: plane preset, offset, cap settings.
    #[derive(Clone)]
    struct CutState {
        preset: CutPreset,
        offset_mm: f32,
        yaw_deg: f32,
        pitch_deg: f32,
        show_hollow: bool,
        cap_color: [f32; 4],
    }

    impl Default for CutState {
        fn default() -> Self {
            Self {
                preset: CutPreset::Axial,
                offset_mm: 0.0,
                yaw_deg: 0.0,
                pitch_deg: 0.0,
                show_hollow: false,
                // Gingiva-warm pink #E84C4B in linear.
                cap_color: [0.776, 0.182, 0.175, 1.0],
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum CutPreset {
        Axial,
        Coronal,
        Sagittal,
        Custom,
    }

    impl CutState {
        /// Build the `ClipPlane` from the current preset + offset.
        fn clip_plane(&self) -> occluview_render::ClipPlane {
            match self.preset {
                CutPreset::Axial => occluview_render::ClipPlane::axial(self.offset_mm),
                CutPreset::Coronal => occluview_render::ClipPlane::coronal(self.offset_mm),
                CutPreset::Sagittal => occluview_render::ClipPlane::sagittal(self.offset_mm),
                CutPreset::Custom => occluview_render::ClipPlane::custom(
                    self.yaw_deg.to_radians(),
                    self.pitch_deg.to_radians(),
                    self.offset_mm,
                ),
            }
        }

        fn cut_view_spec(&self) -> occluview_render::CutViewSpec {
            occluview_render::CutViewSpec {
                plane: self.clip_plane(),
                cap_color: self.cap_color,
                show_hollow: self.show_hollow,
            }
        }
    }

    struct RenderedFrame {
        texture: egui::TextureHandle,
        size_px: [u16; 2],
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
                show_cut_view: false,
                cut_state: CutState::default(),
                cut_texture: None,
                cut_needs_render: false,
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
                size_px: [spec.size_px, spec.size_px],
                stats,
            });
            self.needs_render = false;
        }

        /// Re-render only the cut-view image (cheaper than the full render).
        fn render_cut_now(&mut self, ctx: &egui::Context) {
            let Some(mesh) = self.mesh.clone() else {
                return;
            };
            let cut = self.cut_state.cut_view_spec();
            let offscreen = match pollster::block_on(Offscreen::new()) {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!(error = ?e, "cut-view offscreen init failed");
                    return;
                }
            };
            let spec = ThumbnailSpec {
                size_px: 256,
                ..Default::default()
            };
            let pixels = match pollster::block_on(offscreen.render_cut_view(&mesh, &cut, spec)) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(error = ?e, "cut-view render failed");
                    return;
                }
            };
            let color_image = egui::ColorImage::from_rgba_unmultiplied([256, 256], &pixels);
            let texture =
                ctx.load_texture("occluview-cut", color_image, egui::TextureOptions::LINEAR);
            self.cut_texture = Some(texture);
            self.cut_needs_render = false;
        }

        fn set_mesh(&mut self, mesh: Mesh) {
            self.mesh = Some(Arc::new(mesh));
            self.needs_render = true;
            self.rendered = None;
            self.cut_needs_render = self.show_cut_view;
        }

        fn load_path(&mut self, path: &std::path::Path, source: &'static str) {
            match load_mesh(path) {
                Ok(mesh) => self.set_mesh(mesh),
                Err(e) => tracing::error!(error = ?e, source, "mesh load failed"),
            }
        }

        fn handle_dropped_files(&mut self, ctx: &egui::Context) {
            ctx.input(|i| {
                if let Some(file) = i.raw.dropped_files.first().cloned() {
                    if let Some(path) = file.path.clone() {
                        self.load_path(&path, "drop");
                    }
                }
            });
        }

        fn show_toolbar(&mut self, ctx: &egui::Context) {
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Open...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("3D meshes", &["stl", "ply", "obj", "gltf", "glb"])
                            .pick_file()
                        {
                            self.load_path(&path, "open");
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
                        ui.separator();
                        let prev = self.show_cut_view;
                        ui.checkbox(&mut self.show_cut_view, "Cut View");
                        if self.show_cut_view != prev {
                            self.cut_needs_render = true;
                        }
                    } else {
                        ui.label("Drop a .stl/.ply/.obj/.glb file or click Open");
                    }
                });
            });
        }

        fn maybe_render_cut_view(&mut self, ctx: &egui::Context) {
            if self.cut_needs_render && self.show_cut_view && self.mesh.is_some() {
                self.render_cut_now(ctx);
                ctx.request_repaint();
            }
        }

        fn show_central_panel(&mut self, ctx: &egui::Context) {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(r) = &self.rendered {
                    let available = ui.available_size();
                    let width = f32::from(r.size_px[0]);
                    let height = f32::from(r.size_px[1]);
                    let scale = (available.x / width).min(available.y / height).min(1.5);
                    let size = egui::Vec2::new(width * scale, height * scale);
                    ui.allocate_new_ui(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_center_size(ui.max_rect().center(), size)),
                        |ui| {
                            ui.image((r.texture.id(), size));
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
        }

        fn show_status_panel(&self, ctx: &egui::Context) {
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

        fn show_cut_window(&mut self, ctx: &egui::Context) {
            if self.show_cut_view {
                let prev_state = self.cut_state.clone();
                egui::Window::new("Cut View")
                    .open(&mut self.show_cut_view)
                    .resizable(true)
                    .default_pos([16.0, 100.0])
                    .default_size([300.0, 420.0])
                    .show(ctx, |ui| {
                        // The cut-view render.
                        if let Some(tex) = &self.cut_texture {
                            let size = egui::Vec2::new(256.0, 256.0);
                            ui.horizontal(|ui| {
                                ui.image((tex.id(), size));
                            });
                            ui.separator();
                        } else if self.mesh.is_some() {
                            ui.label("Rendering cut view...");
                        } else {
                            ui.label("Load a mesh first.");
                            return;
                        }

                        // Plane preset dropdown.
                        ui.horizontal(|ui| {
                            ui.label("Plane:");
                            let presets = [
                                (CutPreset::Axial, "Axial"),
                                (CutPreset::Coronal, "Coronal"),
                                (CutPreset::Sagittal, "Sagittal"),
                                (CutPreset::Custom, "Custom"),
                            ];
                            egui::ComboBox::from_label("")
                                .selected_text(
                                    presets
                                        .iter()
                                        .find(|(p, _)| *p == self.cut_state.preset)
                                        .map_or("Axial", |(_, n)| *n),
                                )
                                .show_ui(ui, |ui| {
                                    for (p, name) in presets {
                                        ui.selectable_value(&mut self.cut_state.preset, p, name);
                                    }
                                });
                        });

                        // Offset slider: -50..+50 mm.
                        ui.horizontal(|ui| {
                            ui.label("Offset:");
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.cut_state.offset_mm, -50.0..=50.0)
                                        .suffix(" mm")
                                        .fixed_decimals(1),
                                )
                                .changed()
                            {
                                self.cut_needs_render = true;
                            }
                        });

                        // Custom yaw/pitch (only when Custom preset).
                        if self.cut_state.preset == CutPreset::Custom {
                            ui.horizontal(|ui| {
                                ui.label("Yaw:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.cut_state.yaw_deg,
                                            -90.0..=90.0,
                                        )
                                        .suffix(" deg"),
                                    )
                                    .changed()
                                {
                                    self.cut_needs_render = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Pitch:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.cut_state.pitch_deg,
                                            -90.0..=90.0,
                                        )
                                        .suffix(" deg"),
                                    )
                                    .changed()
                                {
                                    self.cut_needs_render = true;
                                }
                            });
                        }

                        // Cap settings.
                        ui.separator();
                        ui.checkbox(&mut self.cut_state.show_hollow, "Show hollow (no cap)");
                        if self.cut_state.show_hollow {
                            self.cut_needs_render = true;
                        }
                    });
                if self.cut_state_changed(&prev_state) {
                    self.cut_needs_render = true;
                }
            }
        }

        fn cut_state_changed(&self, prev: &CutState) -> bool {
            self.cut_state.offset_mm.to_bits() != prev.offset_mm.to_bits()
                || self.cut_state.preset != prev.preset
                || self.cut_state.yaw_deg.to_bits() != prev.yaw_deg.to_bits()
                || self.cut_state.pitch_deg.to_bits() != prev.pitch_deg.to_bits()
                || self.cut_state.show_hollow != prev.show_hollow
        }
    }

    impl eframe::App for OccluViewApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.handle_dropped_files(ctx);
            if self.needs_render {
                self.render_now(ctx);
                ctx.request_repaint();
            }
            self.show_toolbar(ctx);
            self.maybe_render_cut_view(ctx);
            self.show_central_panel(ctx);
            self.show_status_panel(ctx);
            self.show_cut_window(ctx);
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
