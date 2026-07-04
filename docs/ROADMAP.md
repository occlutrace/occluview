# Roadmap

This is a living document. Items move between tiers as we learn. Dates are
"targets," not commitments.

## v1.0 — "It just opens" (the MVP)

The smallest honest release that delivers the promise.

- **Core**
  - [ ] `occluview-core`: math, units (`Millimeters`), mesh data model, bbox,
    scene graph (multi-mesh), camera with occlusal default (ADR-0009).
  - [ ] `occluview-formats`: STL (binary+ASCII, mmap streaming), PLY (with
    vertex color), OBJ (+MTL, lenient), glTF/GLB (cgltf, zip-slip safe).
  - [ ] `occluview-render`: wgpu pipeline, flat/vertex-color materials, fit-to-bbox
    framing, offscreen path.
  - [ ] `occluview-app`: egui chrome (open / recent / units / view presets /
    transparency / screenshot), dark OccluTrace theme, scale bar, axis gizmo.
- **Shell**
  - [ ] `occluview-shell`: out-of-process `IThumbnailProvider` for STL/PLY/OBJ/
    glTF/GLB/3MF, watchdog + WARP fallback, signed DLL.
  - [ ] File association ("Open with"), ProgID registration.
  - [ ] Jumplist / Recent files.
- **Dist**
  - [ ] Signed MSI installer; `occluview-cli` for headless thumbnail/convert.
- **Quality**
  - [ ] Property tests + fuzz targets for every parser.
  - [ ] Golden-image renderer tests.
  - [ ] CI perf gates (cold start, idle RSS, open-50MB-STL, thumbnail-256).

## v1.x — Depth

- 3MF support. **Deferred from v1.0**: 3MF is a 3D-print handoff format, not an
  intraoral-scan format (0 files in the OccluTrace corpus). When real user
  demand appears, implement natively in Rust (`zip` + `quick-xml`, both pure
  Rust, MIT/Apache-2.0) rather than via lib3mf FFI, to keep the formats crate
  `#![forbid(unsafe_code)]` (same rationale as ADR-0010 for glTF).
- Measurement tools (point-to-point, in mm — visual aid, not calibrated).
- Cross-section plane.
- Side-by-side / overlay compare of two meshes.
- Preset views (buccal, lingual, mesial, distal, occlusal) and axis gizmo snap.
- Drag-and-drop multiple files into one scene (upper+lower pairing UX).
- Settings UI + persisted user preferences (TOML under `%APPDATA%`).
- winget manifest.

## v2 — Beyond

- Preview Handler (Reading Pane) in Explorer.
- Custom Properties tab (vertex count, bbox dimensions, units) in Explorer.
- Context-menu verbs (open as upper / lower arch, etc.).
- MSIX / Sparse Package distribution (resolves open Q4).
- Color management (ICC) for texture scans (open Q3).
- Bindless / mesh shaders for very large scenes (open Q2).
- Native-look GUI overhaul (Win11 visual style) if egui's defaults age poorly.
- Cross-platform CLI / headless render farm support.

## Explicitly not on the roadmap

- Volumetric CBCT / DICOM rendering (ADR-0007).
- Mesh editing / sculpting / CAD modeling.
- Auto-diagnostic or clinical-decision features (regulatory boundary,
  `docs/GLOSSARY.md`).
- Plugin DLL loading from disk (security; revisit only with strong demand).
