# OccluView — Architecture

> Status: **Foundational ADR set.** This document records the architecture
> chosen at project inception (2026-07) and the reasoning behind it. Changes go
> through new ADRs in `docs/adr/`, not silent edits here.

This is the canonical architecture reference. Read it before touching anything
in `crates/`. Cross-references: `AGENTS.md` (rules), `docs/adr/` (decisions),
`docs/SHELL_INTEGRATION.md` (Windows specifics).

---

## 1. Goals & non-goals

### Goals
- **Open lightning-fast.** Cold start to an interactive window in < 400 ms P90
  on a 5-year-old office PC. Idle RSS < 120 MB.
- **Low RAM.** Dental lab PCs are often 8–16 GB with integrated graphics. A 50 MB
  STL must open comfortably alongside the practice-management software.
- **Native Windows citizen.** 3D thumbnails in Explorer, default "Open with",
  jumplist/recent files — the things that make it feel built-in.
- **Format-agnostic.** STL, OBJ, PLY, glTF/GLB, 3MF at v1; extensible to more.
- **Dental-aware.** Millimeter units, occlusal default camera, upper/lower arch
  pairing, vertex-color (NIR/texture) scans — not generic game meshes.
- **Open-core OSS.** Apache-2.0 core; OccluTrace runs the cloud connector /
  alignment service as a separate proprietary product.

### Non-goals (at least for v1)
- Volumetric CBCT / DICOM rendering. We show **meshes** only. A CBCT slice viewer
  is a different product. See ADR-0007.
- Mesh editing / sculpting. OccluView is a **viewer**. Light transforms (move,
  align two arches, transparency) yes; CAD modeling no.
- Cross-platform GUI. We are **Windows-first**. The core is portable; the GUI and
  shell extension are not, on purpose, until v2.
- Medical-device claims. This is a CAD preview / inspection tool, deliberately
  outside FDA SaMD / EU MDR scope. See `docs/GLOSSARY.md` → "Regulatory scope".

---

## 2. The stack (decided; rationale in ADRs)

| Layer             | Choice                  | Why                                                | ADR       |
|-------------------|-------------------------|----------------------------------------------------|-----------|
| Language          | **Rust (edition 2021)** | One language across viewer + shell DLL; native start; memory safety in parsers | ADR-0001 |
| GPU abstraction   | **wgpu** (D3D12 backend on Windows) | Modern, safe, maps to WebGPU; lets us ship a headless render path for thumbnails | ADR-0002 |
| Windowing / GUI   | **egui** (+ `winit` + raw `wgpu`) for v1 | Immediate mode = tiny binary, fast start, easy to embed; sufficient for a viewer's minimal chrome | ADR-0003 |
| Mesh loading      | **cgltf** (glTF), custom STL, custom PLY, `happly`-style PLY, `fastgltf` opt., **lib3mf** via FFI for 3MF | Per-format best-in-class; avoid assimp's CVE surface | ADR-0004 |
| Shell extension   | **Rust COM DLL** via `windows-rs`, `IThumbnailProvider`, **out-of-process** by default | Doesn't risk `explorer.exe`; GPU render is safe in surrogate; proven by stl-thumb / win-svg-thumbs-rust | ADR-0005 |
| Build             | **Cargo workspace**     | Reproducible, no CMake hell, lockfile in VCS       | ADR-0006 |
| License           | **Apache-2.0**          | Permissive + patent grant; open-core compatible    | ADR-0008 |

### Rejected alternatives (one-liners; full reasoning in the ADRs)
- **C++ + bgfx/VTK (à la F3D).** Excellent but two languages if we want a Rust
  shell DLL, and CMake/build-repro fragility raises AI-slop risk. Revisit if we
  need VTK's scientific viz.
- **C# / .NET 8 NativeAOT + WinUI 3.** Great Windows story but slower cold start
  and .NET-inside-explorer is historically painful for shell extensions.
- **Electron / WebView2 + Three.js.** Disqualified on cold-start + RAM goals.
- **bgfx / Filament over wgpu.** More mature PBR today, but wgpu gives us one
  safe abstraction across the live app **and** the headless thumbnail path with
  no extra dependency, which matters more for a viewer.
- **assimp** as universal loader. BSD-3 license is fine, but its CVE history and
  "load everything" surface are wrong for a tool that opens untrusted files.

---

## 3. Workspace & dependency graph

```
                    ┌────────────────────────────────────────────┐
                    │           occluview-core (pure)            │
                    │  math · units · scene graph · camera ·     │
                    │  mesh data model · bbox · transforms       │
                    │  (NO I/O, NO GPU, NO Win32, panic-free)    │
                    └─────────────────────┬──────────────────────┘
                                          │ depends on (downward)
            ┌─────────────────────────────┼─────────────────────────────┐
            │                             │                             │
   ┌────────▼─────────┐         ┌─────────▼────────┐           ┌────────▼────────┐
   │ occluview-formats│         │ occluview-render │           │  (none, core    │
   │  STL OBJ PLY     │         │  wgpu · WGSL ·   │           │   is leaf)      │
   │  glTF 3MF        │         │  PBR · picking   │           └─────────────────┘
   └────────┬─────────┘         └─────────┬────────┘
            │                             │
            └──────────────┬──────────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
     ┌────────▼────────┐       ┌────────▼─────────┐
     │ occluview-shell │       │ occluview-app    │
     │ (Windows COM    │       │ (GUI binary,     │
     │  thumbnail DLL) │       │  egui + wgpu)    │
     └─────────────────┘       └──────────────────┘
              │
     (also) occluview-cli ── core + render + formats (headless: convert / render-to-png / thumb)
```

**Layering law** (enforced; see `AGENTS.md` §0.4):

- `core` ← depends on **nothing** in this workspace. Pure logic, panic-free,
  fully unit-testable, portable.
- `formats` → `core` only.
- `render` → `core` only.
- `shell` → `core` + `render` (it renders a thumbnail using the same pipeline as
  the app, offscreen).
- `app` → everything (it's the integration layer).
- `cli` → `core` + `render` + `formats`.

**No upward imports. No cycles.** A cycle is a P0 bug and fails CI.

The reason the **shell extension reuses `render`** rather than having its own
mini-renderer is a hard requirement: the thumbnail must look identical to the
in-app view (same camera framing, same material), and we maintain one shader
pipeline. This is why wgpu's offscreen/headless capability was a deciding factor
(ADR-0002).

---

## 4. Runtime data flow

### 4.1 Opening a file in the app

```
User drops file.stl
   │
   ▼
app: detect format by extension + magic bytes   (occluview-formats::probe)
   │
   ▼
formats::stl::read(stream) -> core::Mesh        (off-main-thread, rayon)
   │  - mmap for large files; stream-decode; never load whole file if avoidable
   │  - return vertex/normals/colors/indices + a Metadata{units, format, counts}
   ▼
core::Scene { meshes: Vec<MeshHandle>, camera, lights }
   │  - compute bbox, frame camera (occlusal default for dental — ADR-0009)
   │  - normalize units to mm
   ▼
render::GpuMesh::upload(scene)                  (wgpu, on render thread)
   │  - vertex/index buffers, bind groups, materials
   ▼
render::frame() each vsync → wgpu surface       (60 fps orbit of multi-M-tri mesh)
```

### 4.2 Generating a thumbnail (shell extension, Explorer)

```
Explorer requests thumbnail for file.stl at size 256
   │  (IThumbnailProvider::GetThumbnail, runs in dllhost.exe surrogate — OOP by default)
   ▼
shell DLL: read file via occluview-formats       (same code path as the app)
   │
   ▼
render::offscreen::render_to_image(mesh, 256, camera=Occlusal)   (wgpu headless surface)
   │  - bounded time budget; on timeout fall back to a silhouette icon
   │  - WARP (software) fallback if no GPU or if in a restricted surrogate
   ▼
return HBITMAP / IWICBitmap to Windows           (cached by the OS thumbnail cache)
```

Key point: **the thumbnail and the app use the same `core` + `render` code.** One
mesh loader, one shader, one framing logic. Drift between them is a bug.

---

## 5. The dental defaults (cross-cutting)

These are architecture-level decisions, not preferences — they're encoded in
`core` and applied by both `app` and `shell`:

| Concern                 | Default                                                                       |
|-------------------------|-------------------------------------------------------------------------------|
| Length unit             | Millimeter. If a format declares units, honor them; else assume mm and surface it in UI. |
| Coordinate frame        | Right-handed, Y-up internally. Convert on load per format; never assume.     |
| Default camera          | **Occlusal view** (looking down onto the occlusal table), fit-to-bbox. ADR-0009. |
| Multi-mesh scene        | First-class. "Upper + lower arch" loaded as two meshes; each gets its own transform + color. |
| Color scans             | Vertex colors (PLY/OBJ+mtl/glTF) rendered by default; NIR/texture supported via material. |
| Scale bar                | On by default, in mm; toggleable.                                             |
| Background              | Neutral dark (`#0a0a0a`, matches OccluTrace brand) for app; transparency-aware for thumbnails. |

---

## 6. Performance budget (target P90, enforced in CI)

See `AGENTS.md` §3 for the limit table. The architecture is shaped to meet it:

- **Cold start < 400 ms.** No VM, no JIT, no GC. The app binary is one native
  EXE. egui has no widget tree to build; first paint is one frame. We defer all
  non-essential initialization (telemetry, update check) to after first frame.
- **Idle RSS < 120 MB.** No retained-mode UI framework holding widget state; no
  embedded browser. Meshes are uploaded to GPU and the CPU copy can be dropped
  for static scenes (keep-or-reload trade-off configurable).
- **First frame < 600 ms for a 50 MB STL.** Loading is off the main thread
  (rayon); the window shows a skeleton immediately and the mesh fades in. We
  stream-decode large files instead of reading them whole.
- **Thumbnail < 400 ms at 256px.** Offscreen render at the requested size; no
  supersampling; WARP fallback bounded by a watchdog.

These are **asserted in CI** by the perf bench, not hoped for.

---

## 7. Threading model

- **Main thread:** windowing (winit) + egui input + UI. Never blocks on I/O.
- **Worker pool (rayon):** file parsing, mesh post-processing (bbox, normal
  recomputation, decimation for the thumbnail LOD).
- **Render thread:** wgpu command encoding, submit, present. Decoupled from the
  UI thread; can run at the display's refresh rate.
- **Shell extension:** runs in the OS surrogate (`dllhost.exe`); uses a **short**
  worker (often just synchronous, since it's already off Explorer's thread) with
  a hard timeout; on timeout, return a placeholder.

`core` is `Send + Sync` and lock-free in the hot path. Cross-thread
communication is via channels; we do not hold locks across wgpu calls.

---

## 8. Error handling strategy

- **`core`, `formats`, `render`:** library crates return
  `Result<T, ThisCrateError>` (thiserror). No panics. `unwrap`/`expect` are
  clippy-denied.
- **`app`, `cli`:** may use `anyhow` for top-level error aggregation. A fatal
  error shows a native dialog (never a panic backtrace to the user).
- **Malformed files** are the common case, not an exception. Parsers return a
  `FormatError::Malformed { offset, reason }` and we try to recover (skip bad
  triangle, warn) rather than abort the whole file when feasible. Property tests
  fuzz every parser.
- **Shell extension:** never crash the surrogate. Any error → placeholder
  thumbnail + structured log; the OS caches the placeholder.

---

## 9. Extensibility points

- **New format:** add a module in `occluview-formats`, implement the
  `FormatReader` trait, register in the dispatcher, add golden parse tests. No
  changes to `core`/`render`/`app`.
- **New shell surface** (e.g. Preview Handler in v2): new COM class in
  `occluview-shell`, register separately. Reuses `render::offscreen`.
- **New tool** (measure, section plane): lives in `core` as a pure op, surfaced
  in `app` UI. Tools never live only in the GUI — they must be CLI-reachable.
- **OccluTrace connector:** separate proprietary crate (not in this repo) that
  depends on the public `occluview-core` API. The OSS boundary stays clean.

---

## 10. What this architecture deliberately does NOT do

- No embedded scripting language. Config is TOML; user presets are TOML.
- No plugin DLL loading from disk. Plugins are compile-time only. (Reduces attack
  surface; revisit if there's real demand.)
- No telemetry by default. If ever added: off by default, disclosed, minimal.
- No auto-update of the binary from the internet inside the app. Updates come via
  the chosen distribution channel (MSI / MSIX / winget) and are user-initiated.

---

## 11. Open architectural questions (tracked as issues, resolved by ADR)

- Q1 — 3MF support: FFI to lib3mf (C++, heavier) vs a native Rust reader (less
  complete). See ADR-0004 (open item).
- Q2 — Bindless / mesh shaders for very large multi-mesh scenes: needed for v2?
- Q3 — Color management / ICC for texture scans: how much for v1?
- Q4 — MSIX vs MSI for distribution, given shell-extension COM registration needs.
  See ADR-0005 (open item).

When you resolve one of these, write the ADR and update this section.
