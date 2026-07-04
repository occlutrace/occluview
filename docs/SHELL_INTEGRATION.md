# Windows Shell Integration

How OccluView becomes a native Windows citizen: thumbnails in Explorer, the
default "Open with", and a jumplist of recent cases. The technical decisions are
in [ADR-0005](adr/0005-out-of-process-rust-com-thumbnail-provider.md); this
document is the implementation reference.

## 1. What we integrate (v1 scope)

| Surface                          | v1 | Mechanism                                                |
|----------------------------------|----|----------------------------------------------------------|
| 3D thumbnails in Explorer        | ✅ | `IThumbnailProvider` COM DLL (out-of-process)            |
| Default "Open with" / ProgID     | ✅ | File association via ProgID registry entries             |
| Jumplist / Recent files          | ✅ | `ICustomDestinationList`                                 |
| Preview Handler (Reading Pane)   | ⏳ v2 | `IPreviewHandler`                                      |
| Custom Properties tab            | ⏳ v2 | `IShellPropSheetExt` / PropertyStore                    |
| Context-menu verbs               | ⏳ v2 | `IExplorerCommand`                                     |

## 2. Thumbnail provider — the mechanism

- Implement `IThumbnailProvider` (+ `IInitializeWithStream` or
  `IInitializeWithFile`) and a COM class factory in `occluview-shell`.
- Register the CLSID under each extension's `ShellEx` key, e.g.
  `HKCR\.stl\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` (the
  `IThumbnailProvider` category). Do the same for `.obj .ply .gltf .glb .3mf`.
- Run **out-of-process** by default (Windows hosts the DLL in `dllhost.exe`).
  Never set `DisableProcessIsolation` — an explorer.exe crash is unacceptable.
- The render uses `occluview-render`'s offscreen path (ADR-0002), so the
  thumbnail is pixel-identical to the in-app view (same occlusal framing, ADR-0009).

### Performance & robustness rules (enforced in code)

- **Watchdog:** each thumbnail is rendered under a time budget. On timeout,
  return a branded placeholder and log. Never hang Explorer's thumbnail worker.
- **Software fallback:** if no GPU is available (server, locked-down account),
  fall back to **WARP** (Windows Advanced Rasterization Platform). Still bounded
  by the watchdog.
- **Right-sized:** render at the requested size (32/96/256/1024 typical). No
  supersampling.
- **No crash propagation:** any panic/error → placeholder. The surrogate may die,
  Windows restarts it; Explorer itself is unaffected (out-of-process).
- **Caching:** we rely on the Windows thumbnail cache. To force a refresh we bump
  the registry `TypeOverlay`/cache key or set a `ThumbnailCutoff` if needed.

## 3. File associations ("Open with" + default app)

- Register a ProgID per family (e.g. `OccluView.stl`) with the open verb pointing
  to the app EXE and our icon.
- Add the ProgID to each extension's `OpenWithList` / `OpenWithProgids`.
- On Windows 11, setting the **UserChoice** default is protected (hash-based); we
  direct users to the Settings → Default apps flow rather than forcing it
  silently. The installer can set our ProgID as a recommended association.

## 4. Jumplist / Recent

- `ICustomDestinationList` to surface recent files and a "Recent cases" category.
- Backed by the app's own recent-files list (persisted in a small local DB /
  JSON under `%APPDATA%\OccluView`).

## 5. Packaging (open item — ADR-0005)

Shell extensions have friction with MSIX (packaged apps can't always register
arbitrary COM). Two viable paths:

- **MSI (unpackaged) + per-machine COM registration** — simplest, most reliable
  for shell extensions. v1 default.
- **MSIX / Sparse Package** — cleaner install/uninstall; requires the
  "unpackaged COM" / sparse-manifest pattern. Considered for v2.

The decision is left as open Q4 in `ARCHITECTURE.md` until we validate the
MSIX COM story on Win11.

## 6. Signing

- The shell DLL and the installer **must be signed**. Unsigned, SmartScreen may
  flag the install and Explorer may refuse to load the provider in some
  configurations.
- Releases are signed with an EV cert where possible; releases publish the cert
  chain and a checksums file.

## 7. Debugging shell extensions

- Explorer caches loaded extensions; restart Explorer or use `dllhost.exe`
  attaching via the debugger after the surrogate starts.
- The `occluview-cli thumbnail` subcommand exercises the **exact same** render
  path as the shell DLL, so most debugging happens outside Explorer entirely.
- Logs go to `%LOCALAPPDATA%\OccluView\logs\` (rotated).

## 8. References (foundational reading)

- Microsoft Learn — Thumbnail Handlers: <https://learn.microsoft.com/en-us/windows/win32/shell/thumbnail-providers>
- Microsoft Learn — Building Thumbnail Providers: <https://learn.microsoft.com/en-us/windows/win32/shell/building-thumbnail-providers>
- Microsoft Learn — Shell Extension Handlers: <https://learn.microsoft.com/en-us/windows/win32/shell/handlers>
- Raymond Chen ("The Old New Thing") — extensive writing on shell-extension pitfalls.
- Reference Rust implementations: [stl-thumb](https://github.com/unlimitedbacon/stl-thumb),
  [win-svg-thumbs-rust](https://github.com/ThioJoe/win-svg-thumbs-rust).
- The Windows built-in 3D Viewer / 3MF handler (Microsoft) — the default behavior
  we replace.
