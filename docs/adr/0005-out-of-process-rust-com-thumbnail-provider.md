# ADR-0005: Out-of-process Rust COM thumbnail provider

- **Status:** Accepted (with an open item on packaging: MSIX vs MSI)
- **Date:** 2026-07-04

## Context

A core OccluView feature is **native 3D thumbnails in Windows Explorer** for
STL/OBJ/PLY/glTF/3MF. The mechanism is the Windows shell's `IThumbnailProvider`
COM interface (CLSID `{E357FCCD-A995-4576-B01F-234630154E96}` under
`HKCR\<ext>\ShellEx\{...}`). Proven Rust references exist:
[stl-thumb](https://github.com/unlimitedbacon/stl-thumb) and
[win-svg-thumbs-rust](https://github.com/ThioJoe/win-svg-thumbs-rust).

Key constraints:

- Thumbnail providers traditionally load into `explorer.exe`. A bug there takes
  down the user's shell.
- Windows runs them **out-of-process by default** in a surrogate (`dllhost.exe`);
  `DisableProcessIsolation` opts back into in-proc. We never want in-proc.
- Rendering must be fast (Explorer expects a thumbnail quickly) and must not
  block Explorer's UI thread.
- The thumbnail should look like the in-app render (same camera framing, same
  material). Two renderers drifting is a bug.
- We must handle "no GPU available" (e.g. on a server or a locked-down account)
  gracefully with a software fallback.

## Decision

Implement the thumbnail provider as a **Rust COM DLL** in
`occluview-shell`, using `windows-rs`. It implements `IThumbnailProvider` (and
`IInitializeWithStream` / `IInitializeWithFile`) and **runs out-of-process by
default** (we never set `DisableProcessIsolation`). It reuses
`occluview-render`'s offscreen path (ADR-0002) to produce a pixel-identical
preview to the app.

Operational rules baked into the shell crate:

- A watchdog bounds render time; on timeout, return a branded placeholder.
- On any error, return a placeholder and log — never propagate a crash.
- GPU via wgpu with a **WARP** (software) fallback so it works without a GPU.
- Only the requested thumbnail size is rendered (no supersampling).
- The shell crate is the **only** place FFI/`unsafe` lives besides `occluview-app`.

v1 scope (per governance decision): thumbnail provider + "Open with" association +
jumplist/Recent. Preview Handler (Reading Pane) and a custom Properties tab are
deferred to v2.

## Consequences

**Positive**
- A bug in our renderer or a malicious file cannot crash Explorer (it can only
  crash the disposable surrogate, which Windows restarts).
- Pixel-identical thumbnails to the app for free — same `occluview-render` path.
- Proven pattern (stl-thumb, win-svg-thumbs-rust) — no fundamental unknowns.
- Memory-safe parsers (ADR-0001/0004) front the untrusted input.

**Negative**
- Out-of-process activation has a small per-thumbnail startup cost (surrogate
  spin-up). Mitigated by Windows caching thumbnails and by keeping the cold path
  fast (< 400 ms target for a 256px thumbnail).
- MSIX-packaged apps have constraints around shell-extension COM registration; we
  may need to ship the shell extension as an unpackaged DLL registered via the
  installer (open item below).
- Signing: a DLL loaded into Explorer's surrogate must be signed or SmartScreen
  may flag it. Release builds are signed.

**We must now**
- Decide MSIX vs MSI packaging for v1 (open Q4 in `ARCHITECTURE.md`). Leaning
  MSI + per-machine COM registration, with MSIX/Sparse Package as a v2 option.
- Write the COM class factory, `DllRegisterServer`/`DllUnregisterServer`, and the
  registry script that maps each extension to our CLSID.
- Add a watchdog and a WARP fallback; test under "no GPU" conditions.
- Sign release DLLs.

## Alternatives considered

- **In-process thumbnail provider (`DisableProcessIsolation`).** Rejected — risks
  Explorer stability.
- **C++ shell DLL.** Rejected — reintroduces the two-language problem; the Rust
  references prove the path is viable.
- **C# (.NET) shell DLL.** Rejected — CLR-into-Explorer is the classic Windows
  footgun (Raymond Chen documents it extensively); even with .NET 8 it's riskier
  than native.
- **Defer thumbnails to v2.** Rejected — it's a top-3 reason users will adopt.

---

## Addendum (2026-07, post-research refinement)

Deeper research (`docs/RESEARCH.md` §1.2) refined the architecture: **the
in-proc stub must not render**. It spawns / connects to a separate worker
process that does all parsing and rendering. This is the production pattern used
by stl-thumb, Icaros, FastPictureViewer, and Microsoft's own samples.

```
explorer.exe / dllhost.exe (surrogate)
  occluview_shell.dll  (native Rust COM, ~50 KB)
    IThumbnailProvider + IInitializeWithStream
    ── spawns / connects to ─────────────►  ThumbWorker
                                              (= occluview-cli thumbnail today)
                                              restricted token + Job Object
                                              (RAM cap, kill-on-hang watchdog)
                                              mesh parser + decimator + WARP raster
                                              ───► HBITMAP via shared mem
```

**Why a separate worker (not just out-of-proc COM):**

- A worker crash never reaches `explorer.exe` or even the COM surrogate. The OS
  imposes a responsiveness budget; a black-listed thumbnailer is very hard to
  diagnose ("thumbnails just stopped working").
- The worker can be sandboxed with a restricted token + Job Object (RAM cap,
  kill-on-hang) — essential because the mesh parser is a large attack surface
  reading untrusted files.
- Multiple `GetThumbnail` calls can be batched in one worker for cache-warming.

**WARP, not GPU, in the worker.** For 256-px thumbnails of decimated meshes,
WARP (Windows Advanced Rasterization Platform, the documented software fallback)
is fast enough (<100 ms), deterministic, and immune to RDP/headless/no-GPU
conditions. GPU in the worker buys little and costs fragility. Reserve GPU for
the live viewer app (`occluview-app`). ([WARP Guide][warp])

**`IInitializeWithStream`, not `IInitializeWithFile`.** Lets the shell hand a
stream for Mark-of-the-Web files and OneDrive placeholders — both common in
dental labs receiving scans by email. ([textslashplain: Shell Previews
Restricted][motw])

**The worker is `occluview-cli thumbnail`.** The CLI subcommand already exists
as the debug path; it becomes the worker's entry point. This is a good
architectural fit — one render code path, debuggable outside Explorer.

[warp]: https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp
[motw]: https://textslashplain.com/2025/10/20/windows-shell-previews/
