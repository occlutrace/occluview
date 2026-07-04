# Glossary

Domain terms used across OccluView docs and code. Read this before touching
anything dental-specific. Names marked 🔧 are types in `occluview-core`.

## Dental / anatomical

- **Arch** — the crescent-shaped dental ridge. An adult has an **upper (maxillary)**
  arch and a **lower (mandibular)** arch. OccluView treats an upper+lower pair as
  a first-class two-mesh scene.
- **Occlusal** — the chewing surface of the teeth; the direction you look down
  onto that surface. OccluView's **default camera** is the occlusal view (ADR-0009).
- **Occlusion / occlusal alignment** — how the upper and lower arches meet when
  the jaw closes. This is what OccluTrace (the cloud service) computes.
- **Buccal / Labial / Lingual** — cheek-side / lip-side / tongue-side surfaces.
  Useful for naming view presets.
- **Mesial / Distal** — toward / away from the midline of the arch. The
  mesial-distal axis is roughly horizontal in the occlusal view.
- **Intraoral scan (IOS)** — a 3D surface captured by an intraoral scanner
  (3Shape TRIOS, Medit, iTero, Carestream, Planmeca, Shining 3D, etc.). Usually
  exported as STL/PLY/OBJ; native formats are often proprietary.
- **CBCT** — Cone-Beam Computed Tomography. A volumetric (voxel) dental X-ray.
  **Out of scope for OccluView v1** (ADR-0007).
- **Die** — the prepared tooth shape scanned for a crown. A small, precise mesh.
- **Margin line** — the boundary line where a crown preparation ends. Often
  annotated in CAD; OccluView may *display* it if present, not compute it.
- **Articulator** — a mechanical (or virtual) device simulating jaw movement.
  OccluView shows static meshes, not articulation, in v1.
- **NIR / near-infrared imaging** — some scanners capture NIR images for caries
  detection; may appear as vertex/texture data on a mesh.

## Units & coordinate systems

- **`Millimeters(f32)`** 🔧 — OccluView's length unit. Dental meshes are in mm
  (sometimes unit-less numbers that are in fact mm). ADR-0009.
- **Right-handed, Y-up** — OccluView's internal coordinate frame. Each format's
  native frame is converted on load; see `docs/FORMAT_SUPPORT.md`.
- **Scale bar** — an on-screen ruler in mm, on by default.

## Regulatory scope (important — read once)

OccluView is a **CAD preview / inspection tool**. It is **not** a medical device
and makes no diagnostic claims. Concretely:

- It does **not** interpret images for clinical diagnosis (no FDA "Software as a
  Medical Device" / SaMD function).
- It does **not** perform measurements used in treatment decisions (a scale bar
  is a visual aid, not a calibrated measurement).
- It does **not** process volumetric DICOM (ADR-0007).

Keeping this boundary clear keeps the project out of FDA / EU MDR scope. Any
feature that blurs it (auto-diagnosis, calibrated measurement used in treatment,
CBCT interpretation) must be escalated to maintainers and may be rejected.

## Graphics / rendering

- **wgpu** — OccluView's GPU abstraction (ADR-0002); Rust implementation of the
  WebGPU spec, D3D12 backend on Windows.
- **WGSL** — WebGPU Shading Language. OccluView's shaders are written in WGSL.
- **Offscreen / headless rendering** — rendering to a texture instead of a
  window. Used by the thumbnail provider.
- **WARP** — Windows Advanced Rasterization Platform; Microsoft's software
  rasterizer. Our no-GPU fallback for thumbnails.
- **PBR** — Physically Based Rendering. OccluView uses light PBR (mostly flat /
  Lambertian with optional vertex color); full PBR for textured glTF.
- **Bindless / mesh shaders** — advanced GPU features for very large scenes;
  deferred to v2 (open Q2 in `ARCHITECTURE.md`).
- **Golden-image test** — render a fixed scene and compare to a stored PNG;
  how we regression-test the renderer (`docs/TESTING.md`).

## File formats

- **STL** — stereolithography; triangle-only, no color. The dental workhorse.
- **PLY** — Polygon File Format; flexible properties, supports vertex colors —
  used for color/NIR scans.
- **OBJ + MTL** — Wavefront; geometry + simple materials (color, texture map).
- **glTF / GLB** — modern interchange; PBR materials, animation; GLB is the
  binary single-file variant.
- **3MF** — XML-in-ZIP; rich metadata, materials; used increasingly for 3D
  printing handoff. Loaded via lib3mf (ADR-0004).

## Dental scanning (extended)

- **NIRI** — Near-Infrared Imaging. Some intraoral scanners (3Shape TRIOS,
  iTero, Medit) capture near-infrared channels for caries/proximity detection
  alongside surface color. Delivered as separate grayscale textures or as
  vertex/texture color; STL cannot carry it, only PLY/OBJ+MTL/glTF.
- **Mucosal shade** — surface color of the gums/teeth captured for shade
  matching. Carried as vertex color (PLY) or texture (OBJ+MTL, glTF).
- **PCA** — Principal Component Analysis. The standard pose-invariant method for
  auto-orienting a dental mesh for thumbnails: covariance SVD on the vertex
  cloud gives arch-length / buccal-lingual / occlusal-gingival axes.
  See `docs/RESEARCH.md` §4.6.

## Windows shell

- **MOTW** — Mark-of-the-Web. An alternate-data-stream flag Windows attaches to
  files downloaded from the Internet Zone. Win11 suppresses preview handlers for
  MOTW files by default; thumbnails are unaffected. Common in dental labs
  receiving scans by email.
- **WARP** — Windows Advanced Rasterization Platform. Microsoft's software
  D3D rasterizer. Our no-GPU fallback for the thumbnail worker (ADR-0005).
- **Surrogate** — the host process (`dllhost.exe` for thumbnails, `prevhost.exe`
  for preview handlers) Windows uses to run shell extensions out-of-process.
- **Job Object** — a Windows kernel object for grouping/limiting processes. The
  thumbnail worker runs under one with a RAM cap and kill-on-hang watchdog.

## Regulatory

- **SaMD** — Software as a Medical Device (FDA). Software with a medical
  intended use. OccluView is **not** SaMD: it is display-only CAD preview.
- **MDDS** — Medical Device Data System. A removed-from-regulation FDA category
  for software that only displays/stores/transfers device data without
  interpretation. OccluView's display-only scope aligns here.
- **MDR** — Medical Device Regulation (EU). Rule 11 means most standalone
  medical-device software is Class IIa+. OccluView is outside MDR Art. 2(1) (no
  medical purpose). Intended-use statement must be confirmed by regulatory
  counsel before any public release.

## Process / governance

- **ADR** — Architecture Decision Record (`docs/adr/`). The "why" of a decision.
- **DoD** — Definition of Done (`AGENTS.md` §7).
- **DCO** — Developer Certificate of Origin (`CONTRIBUTING.md`); `git commit -s`.
- **Open-core** — Apache-2.0 viewer (this repo) + proprietary OccluTrace cloud.
- **Slop** — plausible-but-unearned code. See `docs/ANTI_SLOP.md`.
- **Conventional Commit** — `<type>(<scope>): <subject>` (`AGENTS.md` §5).
- **Assisted-by** — commit trailer for AI attribution, in place of
  `Co-authored-by` (Linux kernel convention, `AGENTS.md` §5).
