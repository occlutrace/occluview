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
