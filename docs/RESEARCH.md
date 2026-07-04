# Research Report — OccluView Foundation

> This document records the research that produced OccluView's foundational
> decisions. It is the evidence base behind `docs/ARCHITECTURE.md` and the
> `docs/adr/` set. Research conducted July 2026 across five parallel
> investigations (Windows shell, render stack, governance, format/dental,
> landscape/naming), each cited inline.
>
> **Status: confirmation + refinement of the original architecture.** Every
> foundational decision held up under deeper research; the refinements are
> captured as ADR addenda below.

---

## 0. Headline findings

1. **Every foundational decision is validated.** Rust + wgpu + egui + Apache-2.0
   + out-of-process Rust COM thumbnail provider + per-format loaders (no assimp)
   + mesh-only scope — all four research streams independently arrived at the
   same conclusions.
2. **A market vacuum opened July 1, 2026:** Microsoft removed 3D Viewer from the
   Store and never supplied STL/PLY thumbnails. There is no first-party answer.
   OccluView lands into that gap. ([Windows Latest 2026-02][ms-3d-deprecated];
   [Dev.to: the STL gap nobody's talking about][stl-gap])
3. **Whitespace in OSS:** there is no open-source, lightweight, *dental-aware*
   mesh viewer. F3D is the closest generic; exocad/3Shape are closed; MeshLab is
   heavy/GPL; Meshmixer is abandoned. OccluView occupies an uncontested niche.
4. **assimp is a security liability** for a tool that opens untrusted files —
   long tail of 2025-2026 CVEs (heap overflows, OOB writes in `read_meshes`,
   `SceneCombiner`, `FBXMeshGeometry`). Confirms ADR-0004. ([OpenCVE assimp])
5. **3MF library correction:** lib3mf is **BSD-3-Clause**, not MIT. NOTICE and
   ADR-0004 updated accordingly.
6. **AGENTS.md is an emerging open standard** (60k+ repos since mid-2025;
   Codex, Cursor, Jules, Claude Code, Copilot). OccluView's AGENTS.md is
   correctly positioned; add CLAUDE.md as a one-line redirect to it (single
   source of truth, no drift). ([agents.md], [InfoQ], [Socket])

[ms-3d-deprecated]: https://www.windowslatest.com/2026/02/07/windows-11-after-paint-3d-microsoft-is-removing-3d-viewer-as-the-creators-update-era-fades-away/
[stl-gap]: https://dev.to/josh_green_dev/microsoft-3d-viewer-dies-july-1-the-stl-gap-nobody-is-talking-about-2580
[OpenCVE assimp]: https://app.opencve.io/cve/?vendor=assimp
[agents.md]: https://agents.md/
[InfoQ]: https://www.infoq.com/news/2025/08/agents-md/
[Socket]: https://socket.dev/blog/agents-md-gains-traction-as-an-open-format-for-ai-coding-agents

---

## 1. Windows shell integration (deep technical reference)

### 1.1 The right surface for v1: `IThumbnailProvider`, out-of-process

The shell's modern thumbnail contract is `IThumbnailProvider` (in `thumbcache.h`):

```cpp
HRESULT GetThumbnail(UINT cx, HBITMAP *phbmp, WTS_ALPHATYPE *pdwAlpha);
```

with `cx` ∈ {16, 32, 48, 96, 256, 1024}. The provider must also implement one of
`IInitializeWithStream` (preferred — gets an `IStream`, never a path, works for
Mark-of-the-Web files), `IInitializeWithItem`, or `IInitializeWithFile`.

Registration is COM-in-proc-server: under
`HKCR\.<ext>\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` (the Thumbnail
Handler *category* GUID, not a per-app CLSID) for each supported extension, plus
the standard `HKCR\CLSID\{OurCLSID}\InProcServer32`. ([Building Thumbnail Handlers],
[Thumbnail Providers])

[Building Thumbnail Handlers]: https://learn.microsoft.com/en-us/windows/win32/shell/building-thumbnail-providers
[Thumbnail Providers]: https://learn.microsoft.com/en-us/windows/win32/shell/thumbnail-providers

### 1.2 The out-of-process worker pattern (refines ADR-0005)

The single highest-leverage architectural refinement from research: **do not
render inside the in-proc stub**. Production pattern (from stl-thumb, Icaros,
FastPictureViewer, and Microsoft's own samples):

```
explorer.exe / dllhost.exe (surrogate)
  ThumbStub.dll  (native, ~50 KB; IThumbnailProvider + IInitializeWithStream)
    ── spawns / connects to ─────────────►  ThumbWorker.exe
                                              (restricted token + Job Object,
                                               RAM cap, kill-on-hang watchdog)
                                              mesh parser + decimator + WARP raster
                                              ───► returns HBITMAP via shared mem
```

**Why this matters concretely for OccluView:**
- A worker crash never reaches `explorer.exe`. The OS imposes a responsiveness
  budget; a black-listed thumbnailer shows up as "thumbnails just stopped
  working" and is very hard to diagnose.
- The worker can be sandboxed with a restricted token + Job Object (RAM cap,
  kill-on-hang) — essential because the mesh parser is a large attack surface
  reading untrusted files.
- WARP (Windows Advanced Rasterization Platform) is fast enough for 256-px
  thumbnails of decimated meshes (<100 ms), deterministic, and immune to
  RDP/headless/no-GPU conditions. **GPU in the worker buys little and costs
  fragility.** Reserve GPU for the live viewer app.
- Multiple `GetThumbnail` calls can be batched in one worker for cache-warming.

Use `IInitializeWithStream` (not `IInitializeWithFile`) — lets the shell hand a
stream for MOTW/Internet-Zone files and OneDrive placeholders, both common in
dental labs receiving scans by email.

### 1.3 Bitness, packaging, signing

- Ship **both x64 and x86** DLLs registered under both registry views
  (`HKCR\WOW6432Node\...`). Covers 32-bit Outlook Reading Pane edge cases.
- **MSI + per-machine COM registration** is the v1 default (simplest for shell
  extensions). MSIX Sparse Package is the v2 path (gives package identity for
  the modern Win11 context menu via `IExplorerCommand`).
- **Authenticode-sign everything** (DLL, worker, installer). Unsigned shell
  extensions are silently rejected on Server SKUs, trigger SmartScreen at
  install, and may be flagged by Smart App Control on recent Win11.
- Relies on the **system thumbnail cache** (`IThumbnailCache`); do not roll our
  own. Cache is content-keyed, auto-invalidates on file change, persists across
  reboots, shared by Explorer + Open File dialog.

### 1.4 Beyond thumbnails (v2 surface area)

| Surface | Interface | v1 | v2 |
|---|---|---|---|
| 3D thumbnails | `IThumbnailProvider` | yes | — |
| Open-with / default app | ProgID + `UserChoice` | yes | — |
| Jumplist / Recent | `ICustomDestinationList` / `SHAddToRecentDocs` | yes | — |
| Reading Pane preview | `IPreviewHandler` (runs in `prevhost.exe`, always OOP) | — | yes |
| Custom Properties tab | `IShellPropSheetExt` (bbox, vert count, units) | — | yes |
| Modern Win11 context menu | `IExplorerCommand` (+ package identity) | — | yes |
| Property store / search | `IPropertyStore` + `.propdesc` ("triangles:>50000") | — | maybe |

**Critical Win11 behavior:** the OS now suppresses preview handlers for files
carrying Mark-of-the-Web by default. For dental labs receiving scans by email,
this is operationally relevant — favor thumbnails over preview handlers for the
"feels instant" experience. ([textslashplain: Windows Shell Previews Restricted])

[textslashplain: Windows Shell Previews Restricted]: https://textslashplain.com/2025/10/20/windows-shell-previews/

### 1.5 .NET in-proc shell extensions: just say no

The canonical warning: Raymond Chen / Jesse Kaplan, [Do not write in-process
shell extensions in managed code][chen-2006] (2006) and the [.NET 4 SxS
follow-up][chen-2013] (2013). Even with .NET 4 side-by-side runtimes, the CLR
cannot be cleanly unloaded from `explorer.exe`. C#/.NET is acceptable **only**
for the out-of-proc worker — never for the in-proc stub. This independently
confirms ADR-0001/0005.

[chen-2006]: https://devblogs.microsoft.com/oldnewthing/20061218-01/?p=28693
[chen-2013]: https://devblogs.microsoft.com/oldnewthing/20130222-01/?p=5163

### 1.6 References (open source)

- [unlimitedbacon/stl-thumb](https://github.com/unlimitedbacon/stl-thumb) — Rust, the best architectural reference. STL/OBJ/3MF (no PLY — our wedge).
- [ThioJoe/win-svg-thumbs-rust](https://github.com/ThioJoe/win-svg-thumbs-rust) — minimal Rust COM provider, no deps.
- [f3d-app/f3d](https://github.com/f3d-app/f3d) — C++/VTK, broadest format support, ships a Windows thumbnail handler.
- [Recipe Thumbnail Provider sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/recipethumbnailprovider/) — Microsoft's canonical C++ sample.
- [com-rs issue #140](https://github.com/microsoft/com-rs/issues/140) — Rust `IThumbnailProvider` plumbing walkthrough.
- [WARP Guide](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp) — software rasterizer reference.

---

## 2. Render stack & language

### 2.1 Language verdict: Rust (validated)

Independent confirmation across cold-start, RAM, ecosystem, type-safety, and
license axes:

| | Rust | C++ | C#/.NET 8 AOT | Electron/WebView2 |
|---|---|---|---|---|
| Cold start | <100 ms native | <100 ms native | Tens of ms (AOT), but WinUI 3 shell is heavy | 100-300 ms + ~80-150 MB |
| Empty-shell RAM | 20-60 MB | 20-60 MB | 80-150 MB (WinUI 3 worse than WPF) | 80-200 MB |
| Reproducible builds | **Cargo + Cargo.lock (hermetic)** | CMake/vcpkg (fragile) | dotnet + NuGet (solid) | npm (mixed) |
| Anti-AI-slop | **Borrow checker rejects LLM slop at compile** | Permissive; LLM C++ → UB | Strong typing; looser runtime | Two languages |
| 3D ecosystem | wgpu (Rust-native, modern) | bgfx/Filament/Vulkan (deepest) | Second-class consumer | Three.js (best lib, wrong shell) |

**Rust wins on the anti-slop reproducibility property** — Cargo's hermetic
builds and the borrow checker are the strongest defenses against the AI-slop
failure mode this project explicitly guards against. ([Rust wins the AI code-gen
race][rust-ai]; [Reversing Labs: AI coding breathes life into Rust][rl-rust])

[rust-ai]: https://medium.com/@chalyi/rust-is-winning-the-ai-code-generation-race-60c65074236c
[rl-rust]: https://www.reversinglabs.com/blog/ai-coding-rust

### 2.2 Renderer verdict: wgpu (validated)

wgpu (MIT/Apache-2.0) is Rust-native, WebGPU-aligned, with a clean offscreen
path that serves both the live app and the thumbnail worker (the key
ADR-0002 rationale). Backends: Vulkan/Metal/**D3D12**/D3D11/OpenGL/WebGL2/WebGPU.

- **Filament** (Apache-2.0) is the runner-up — best PBR out of the box, C++ host.
  Right if we ever want photoreal dental enamel shading without writing PBR.
- **bgfx** (BSD-2) is credible but its build/shader friction and lack of
  Rust-first story make it less attractive than wgpu for a new project.
- **assimp** disqualified as a universal loader on security grounds (see §0.4).
- **bindless / mesh shaders** are not yet first-class in wgpu — irrelevant at
  dental-mesh scale (a few million tris); revisit only past ~10M tris.

### 2.3 Windowing: winit + egui (validated)

- **egui**: immediate mode, renders into the same wgpu surface (zero extra
  compositor), tiny binary, ~13M downloads, the largest Rust GUI ecosystem.
  Perfect for "viewport fills the window; chrome overlays it."
- **iced** is the migration path if the UI outgrows an overlay (powers System76
  COSMIC). Pre-1.0 today; fine, since egui is the v1 UI.
- **WinUI 3 / WPF / Qt / Slint** all disqualified: WinUI 3 is measurably slower
  than WPF and RAM-heavier; Qt LGPL is friction for a permissive OSS project;
  Slint is dual GPL/commercial.
- DPI / multi-monitor: winit exposes per-monitor `scale_factor()`. Solved.
- Touch: winit delivers `Touch` events; egui handles them. Dental offices
  increasingly use touchscreens — covered.

### 2.4 Mesh loaders: per-format best-in-class (validated, with one correction)

| Format | Loader | License | Notes |
|---|---|---|---|
| STL | custom Rust | — | trivial; binary+ASCII, mmap streaming |
| PLY | custom Rust | — | flexible properties, vertex colors (NIR/RGB) |
| OBJ | `fast_obj` or custom | MIT | lenient MTL, fan-triangulation |
| glTF/GLB | **cgltf** (C, single-file) | MIT | zip-slip protection on URIs |
| 3MF | **lib3mf** via FFI (C++) | **BSD-3-Clause** | the official 3MF Consortium library |

**Correction to NOTICE/ADR-0004:** lib3mf is BSD-3-Clause, not MIT. Updated in
NOTICE. The allow-list in `deny.toml` already permits BSD-3-Clause, so no policy
change — just the NOTICE attribution.

For streaming large dental scans: `memmap2` (Rust) + binary parsers → zero-copy
GPU buffer upload. Pattern keeps a 10M-tri CBCT-derived surface at ~100 MB
resident instead of 3-5x.

### 2.5 Reference projects to study (not ship inside)

- **F3D** ([f3d.app]) — the closest spiritual analogue. KISS, CLI-first, shell
  integration, permissive BSD-3. Our wedge: *dental-aware* + *lighter weight*.
- **Bevy** — wgpu rendering architecture reference (don't ship: ECS overhead).
- **wgpu-example** ([matthewjberger/wgpu-example]) — minimal wgpu skeleton.
- **Open3D** — reference for offscreen render and ICP (compare-two-meshes v2).

[f3d.app]: https://f3d.app/
[matthewjberger/wgpu-example]: https://github.com/matthewjberger/wgpu-example

### 2.6 Performance targets (validated, with concrete numbers)

- **Cold start < 400 ms** (we set 400; research shows < 100 ms achievable for a
  minimal Rust+winit+egui+wgpu shell — keep the budget, take the headroom).
- **Empty-shell RAM < 120 MB** (research: 20-60 MB realistic for Rust; 60-120 MB
  with a mid-size mesh).
- **Thumbnail < 400 ms at 256 px** (research: < 100 ms on WARP for decimated mesh).
- **60 fps orbit of multi-M-tri mesh**: indexed draws + VBOs on any integrated
  GPU from the last decade; the camera-uniform upload + draw is trivial.

---

## 3. Project governance & anti-AI-slop

### 3.1 AGENTS.md is correct; one addition

Research confirms the AGENTS.md structure is right and matches the open standard.
**One addition:** ship a one-line `CLAUDE.md` that redirects to AGENTS.md, so
Claude Code (which auto-loads CLAUDE.md) reads the same single source of truth.
This is what rust-analyzer does. Don't maintain divergent copies — divergence is
itself slop.

### 3.2 Three rules to steal from the community

These come from llama.cpp's `AGENTS.md`, the Linux kernel's
`Documentation/process/coding-assistants.rst` (Torvalds-signed, late 2025), and
Ladybird's `CodePolicy.md`. They are worth adopting verbatim:

1. **ASCII-only artifacts.** No em-dashes (`—`), unicode arrows (`→`), ellipsis
   (`…`), or emoji in code, comments, commit messages, or PR text. Em-dashes in
   particular are a tell-tale fingerprint of AI text leaking into human
   artifacts. (llama.cpp)
2. **`Assisted-by:` trailer, never `Co-authored-by:` for AI.** Format:
   `Assisted-by: Claude:claude-5.2-sonnet [tool1 tool2]`. (Linux kernel)
3. **AI must not add `Signed-off-by:` tags.** Only humans can legally certify
   the DCO. The human submitter adds their own. (Linux kernel)

These are added to `AGENTS.md` §5 (commits) and §9 (agent rules).

### 3.3 The Iron Law (steal from obra/superpowers)

`verification-before-completion` is the single best anti-slop rule in any source
I found. Quoted verbatim into `AGENTS.md` §0.1 (already there as "Evidence over
assertion") and §9.3:

> **NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE.** Re-run the
> command. Read the output. *Then* claim done.

### 3.4 Adopt superpowers as a developer option, not a project dependency

Cherry-pick its four load-bearing rules into our AGENTS.md (done); install the
plugin as an opt-in developer convenience (MIT, no contamination). Don't depend
on it project-wide: it's harness-opinionated (worktrees, subagent dispatch) and
pings telemetry to primeradiant.com (unacceptable for a medical-software vendor
without legal review).

### 3.5 Torvalds's stance (a useful framing)

"AI slop" is filtered by the **same review process** used for any low-quality
human patch — no separate, weaker bar for AI. ([r/linux][r-linux]) This matches
our AGENTS.md §9: agent rules are *additive* to §0, not a substitute.

[r-linux]: https://www.reddit.com/r/linux/comments/1q79ueh/linus_torvalds_the_ai_slop_issue_is_not_going_to/

### 3.6 File-size limits: literature supports our 500-LOC cap

Oxlint's `max-lines` default is 300, with the docs noting "recommendations
usually range from 100 to 500 lines." NIST cyclomatic complexity standard is
≤ 10 (we cap at 15 — reasonable for a renderer with match-heavy code). Our
limits (`AGENTS.md` §3) are within the mainstream; no change.

---

## 4. Formats & dental specifics

### 4.1 v1 format matrix (validated)

| Format | Color | Units | Dental adoption | Loader | v1 |
|---|---|---|---|---|---|
| STL | none | none | universal | custom Rust | P0 |
| PLY | **per-vertex RGB** | none | growing, research-backed for archival | custom Rust | P0 |
| OBJ | via .mtl | none | common (Medit) | custom/`fast_obj` | P0 |
| glTF/GLB | PBR + vertex | (meters, rarely honored) | low (export target) | cgltf | P0 |
| 3MF | yes | **declared (mm default)** | negligible (3D-print handoff) | lib3mf FFI | P1 |

**PLY is the strategic format** — recent peer-reviewed work recommends PLY for
long-term archival of intraoral scans: smaller than STL while preserving
geometry **and** color. ([ResearchGate: STL vs PLY for intraoral scans][ply-study])
Our PLY loader already landed (commit `cb337e6`).

[ply-study]: https://www.researchgate.net/publication/397829810_Comparison_of_stereolithography_STL_and_polygon_file_format_PLY_for_intraoral_scans_from_chairside_to_archive

### 4.2 DICOM scope: stay mesh-only (validated, ADR-0007)

Two completely different things share the `.dcm` extension:
- **3Shape/Medit `.dcm`** = a *mesh* format (textured triangle mesh). Not medical.
- **DICOM (medical)** = *volumetric voxel slices* from CBCT. Requires segmentation
  before any mesh exists — a separate, complex pipeline (3D Slicer / InVesalius).

A CBCT DICOM dataset is volumetric; it must be segmented (threshold → mask →
surface extraction) before any mesh exists. OccluView accepts the *output* of
that pipeline (STL produced by Slicer), but does not decode volumetric DICOM.
Confirmed by ADR-0007; no change.

### 4.3 Dental scanner native formats (reference, not v1)

| Vendor | Native | Open export | v1 path |
|---|---|---|---|
| 3Shape TRIOS | `.3oxz`, `.dcm` (mesh, not DICOM) | STL/OBJ | export → STL |
| Medit | `.dcm`, OBJ+MTL+texture | OBJ+MTL | open OBJ |
| iTero | proprietary | STL | export → STL |
| Carestream | proprietary | STL/PLY | export → STL/PLY |
| Planmeca | proprietary | STL/PLY | export → STL/PLY |
| Shining 3D | proprietary | STL/PLY (manual) | export → STL/PLY |
| Dentsply Primescan | proprietary | STL | export → STL |

### 4.4 Vertex color, units, coordinate frame

- **NIRI / color scans** are increasingly important (3Shape TRIOS, iTero, Medit
  capture mucosal shade + near-infrared caries imaging). STL cannot carry this;
  only PLY (per-vertex RGB), OBJ+MTL (texture), and glTF (PBR + vertex color) can.
  Our PLY vertex-color support is the wedge over stl-thumb.
- **Units:** STL/OBJ/PLY are unit-less but author in mm. 3MF declares units
  (mm default). glTF is unit-less. Detection heuristic: bbox diagonal of a full
  arch ~40-80 mm; a crown ~5-15 mm. If diagonal ~1-2, assume meters (×1000); if
  ~10000-20000, microns (÷1000). Always surface the assumption in the UI.
- **Coordinate frame:** scanners commonly export +Z up (occlusal on XY) even
  though STL/OBJ/glTF are right-handed. Loading into our Y-up renderer needs a
  −90° X rotation (`y'=z, z'=−y`), no handedness flip. Already in `core::frame`
  (`SourceFrame::RightHandedZUp`).

### 4.5 Regulatory boundary (critical — read once)

**Intended use governs classification, not the file format.** OccluView stays
non-medical by:
1. **Intended use statement**: "visualization, format conversion, and CAD
   preview of 3D mesh files. **Not intended for diagnostic, treatment-planning,
   or any clinical decision-making. Display-only.**"
2. **Not providing:** clinical measurements, segmentation of clinical structures,
   margin/gap analysis, implant planning, AI pathology detection, shade matching.
3. **Not marketing to** "diagnose / treat / plan" — only "preview, inspect,
   convert, archive."

Under FDA this is MDDS-like (display-only, removed from device regulation by the
21st Century Cures Act). Under EU MDR it is outside Art. 2(1) (no medical
purpose). Both hinge on intended use — **regulatory counsel must confirm the
exact wording** before any public release. ([FDA SaMD], [FDA: functions that are
NOT medical devices], [Johner: MDR Rule 11], [Open Regulatory: viewer
classifications])

[FDA SaMD]: https://www.fda.gov/medical-devices/digital-health-center-excellence/software-medical-device-samd
[FDA: functions that are NOT medical devices]: https://www.fda.gov/medical-devices/device-software-functions-including-mobile-medical-applications/examples-software-functions-are-not-medical-devices
[Johner: MDR Rule 11]: https://blog.johner-institute.com/regulatory-affairs/mdr-rule-11/
[Open Regulatory: viewer classifications]: https://openregulatory.com/questions/are-there-equivalent-classifications-to-fda-class-i-and-ii-viewers-under-eu-mdr

### 4.6 Auto-orientation for thumbnails (the PCA method)

For a good dental thumbnail without manual intervention:
1. **PCA on the vertex cloud** (covariance SVD) gives the three principal axes.
   Largest-variance = arch length (mesial-distal); second = buccal-lingual;
   smallest = occlusal-gingival = the camera direction.
2. **Refine the occlusal plane** by fitting a plane to the upper third of
   vertices (cusp tips); its normal is the viewing direction.
3. **Disambiguate up/down** by signed skew: the gingival side has more surface
   area than the cusp tips.
4. Render the occlusal view + a 10 mm scale bar labeled "10 mm (assumed)".

This is the standard pose-invariant method in dental-arch research. ([SMU
pose-invariant arch extraction][smu-pca], [Pocket Dentistry: object reference
frame][pd-frame])

[smu-pca]: https://ink.library.smu.edu.sg/context/sis_research/article/8941/viewcontent/09412829.pdf
[pd-frame]: https://pocketdentistry.com/new-approach-to-establish-an-object-reference-frame-for-dental-arch-in-computer-aided-surgical-simulation/

---

## 5. Landscape & positioning

### 5.1 The vacuum OccluView fills

Microsoft 3D Viewer was **deprecated Feb 2026 and removed from the Store
July 1, 2026**. It never provided STL/PLY thumbnails. Paint 3D was removed Nov
2024; 3D Builder and Print 3D are already gone. There is no first-party answer
for opening STL on Windows. This is the market opening. ([MS deprecated
features][ms-deprecated])

[ms-deprecated]: https://learn.microsoft.com/en-us/windows/whats-new/deprecated-features-resources

### 5.2 Competitive map

| Project | Stack | License | Dental-aware? | Thumbnails? | Verdict |
|---|---|---|---|---|---|
| **F3D** | C++/VTK | BSD-3 | no | yes | benchmark; we are lighter + dental |
| MeshLab | C++/Qt/VCGLib | GPL-3 | no | no | heavy editor, GPL-contagious |
| CloudCompare | C++/Qt | GPL-2 | no | no | gold standard for compare-2-meshes (v2 feature) |
| Open3D | C++/Python | MIT | no | no | library, not a productized viewer |
| MS 3D Viewer | UWP | proprietary | no | **no** | dead July 2026 |
| stl-thumb | Rust/OpenGL | MIT | no | yes | best reference; STL/OBJ/3MF only (no PLY) |
| glTF Sample Viewer | WebGL | Apache-2 | no | no | rendering reference, glTF-only |
| exocad / 3Shape | closed | — | yes | — | the closed CAD layer we sit beneath |
| Meshmixer | closed | — | yes (legacy) | no | abandoned by Autodesk |

**The wedge:** a free, open, lightweight, *dental-aware* viewer with native
Windows thumbnails including PLY color scans. Nobody does this today.

### 5.3 What dental users need that generic viewers miss

| Need | Why generic viewers fail | OccluView's answer |
|---|---|---|
| Arch pairing (upper + lower) | treat files as unrelated | auto-pair, color-code (pink/blue), articulated |
| Occlusal default view | iso/perspective default | first camera = occlusal (ADR-0009) |
| Color scans (NIRI/RGB) | STL-only tools lose color | first-class PLY vertex color |
| Units in mm | scene units or meters | always mm, visible scale bar |
| Transparency (margin inspection) | rarely casual | per-arch opacity slider |
| Cross-section | buried in menus | one-click clipping plane |
| Compare two meshes | only CloudCompare, research UX | deviation color map (v2) |
| Margin line annotation | nobody does this | v2, ties to "trace" name |

The unifying insight: **dental users don't want a 3D viewer with more buttons —
they want a viewer that already knows it's looking at a mouth.**

### 5.4 Name: OccluView (validated)

Research independently recommended OccluView as the lead candidate ("feels
right; direct sibling of OccluTrace; 'view' signals exactly the viewer role;
strong trademark prospects"). Alternates were ArchLine, BiteScope, ArchLens.
OccluView confirmed; run a USPTO TESS + domain search before lock-in.

---

## 6. Action items from research

### 6.1 Doc updates (this commit)

- [x] This file (`docs/RESEARCH.md`).
- [x] ADR-0005 addendum: out-of-process worker with restricted token + Job
      Object + WARP fallback (refines the original).
- [x] NOTICE: lib3mf is BSD-3-Clause (was incorrectly listed as MIT-like).
- [x] AGENTS.md §5: ASCII-only artifacts + `Assisted-by:` trailer.
- [x] CLAUDE.md: one-line redirect to AGENTS.md (single source of truth).
- [x] GLOSSARY: add PCA, NIRI, MOTW, WARP, MDDS, SaMD.
- [x] README: position against the MS 3D Viewer removal (July 1 2026).

### 6.2 Implementation implications (future PRs)

- The thumbnail worker (`occluview-shell`) should spawn a separate
  `ThumbWorker.exe` (or reuse `occluview-cli thumbnail`) with a Job Object and
  watchdog, not render inside the in-proc stub. (ADR-0005 addendum.)
- `occluview-cli thumbnail` already exists as the debug path; it becomes the
  worker's entry point. Good architectural fit.
- Color-scan support: PLY vertex color is in; OBJ+MTL texture and glTF PBR are
  the v1 follow-ons.
- Auto-orientation via PCA for thumbnails: a `core::orient` module (v1.x).
- Regulatory: the intended-use statement belongs in README + a `DISCLAIMER.md`
  before any public release.

---

## Appendix: research streams (each cited inline above)

1. **Windows shell integration** — `IThumbnailProvider`, out-of-proc worker,
   bitness/packaging/signing, beyond-thumbnails surface.
2. **Render stack & language** — Rust vs C++ vs C# vs Electron; wgpu vs bgfx vs
   Filament; winit+egui vs alternatives; per-format loaders; assimp CVEs.
3. **Governance & anti-AI-slop** — AGENTS.md standard, obra/superpowers, llama.cpp,
   Linux kernel AI policy, Ladybird, file-size limits, ADRs.
4. **Formats & dental specifics** — per-format deep dive, scanner native formats,
   DICOM scope, vertex color/units/coordinate frame, regulatory boundary, PCA
   auto-orientation.
5. **Landscape & naming** — F3D/MeshLab/CloudCompare/Open3D/MS 3D Viewer/stl-thumb
   profiles, dental whitespace, feature ladder, name candidates.
