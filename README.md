<p align="center">
  <img src="assets/occluview-logo.png" width="88" height="88" alt="OccluView logo">
</p>

<h1 align="center">OccluView</h1>

<p align="center">
  Fast native viewer and mesh editor for dental scans and common 3D files.<br>
  Rust · egui · wgpu · Windows and Linux
</p>

<p align="center">
  <img src="assets/screenshot1.png" alt="OccluView showing a dental arch scan" width="820">
</p>

Opening a scan should not require a full CAD project. OccluView is built around
the everyday operator path: double-click a mesh, inspect the surface, clean it
up when needed, and keep the file manager useful while browsing case folders.

OccluView is the open-source desktop viewer of the
[OccluTrace](https://occlutrace.ai) platform for dental labs.

The current stable release is [v1.0.0](https://github.com/occlutrace/OccluView/releases/tag/v1.0.0).

## Features

- **Viewer** — full-window 3D viewport with CAD-style navigation (right-drag
  orbit, middle-click retarget, cut view, axis gizmo, mm scale bar).
- **Layers** — keep multiple scans in one scene with per-layer visibility,
  opacity, tint, and wireframe.
- **Mesh editor** — exocad-style cleanup tuned for dental scans and
  prostheses, reachable by right-clicking any mesh or layer: rectangle,
  outline lasso, and whole-object selection (surface or through-mesh),
  delete/crop/cut/separate, close holes with interpolated caps, keep-largest,
  flip normals, mesh repair, shape-preserving Taubin smoothing, undo/redo.
  Edited layers export to PLY, STL, or OBJ.
- **Cut View** — interactive cross-section: drag the cut plane in the
  viewport and read the live section panel, with an in-slice ruler for
  point-to-point distance and a one-click wall-thickness probe.
- **Explorer integration (Windows)** — thumbnails, an interactive live Preview
  Pane, file associations, and an "Edit in OccluView" context entry, all
  installed by the MSI.
- **Desktop integration (Linux)** — launcher, MIME registration, and a
  GNOME-compatible thumbnailer in the `.deb`.
- **Auto-update** — the app checks the signed release manifest at launch and
  offers new versions; nothing installs silently. Every artifact is
  sha256-pinned and minisign-verified against a key baked into the binary.

## Performance

Measured end to end (file read, parse, weld, normals) on a plain desktop
Ryzen 5 3600, warm file cache, best of 5:

| File | Size | Open time |
| --- | --- | --- |
| Intraoral scan, 300 k triangles | 15 MB binary STL | ~0.1 s |
| 1 M triangles | 50 MB binary STL | ~0.4 s |
| 5 M triangles | 250 MB binary STL | ~1.8 s |

STL decoding and normal generation are parallel; numbers scale with cores.

## Distribution

The current stable release is [OccluView v1.0.0](https://github.com/occlutrace/OccluView/releases/tag/v1.0.0).
Windows and Linux packages, checksums, signatures, and release notes are
published on the GitHub release page.

The release package layout is:

- **MSI** — the normal Windows install: app, Explorer thumbnails, live Preview
  Pane, file associations, shared 3D file icon.
- **Portable ZIP** — manual Windows launch only, no Explorer integration.
- **`.deb`** — Debian, Ubuntu, and GNOME-family desktops:

```bash
sudo apt install ./occluview_<version>_amd64.deb
```

Installed builds update themselves in place from the signed manifest when a
release channel is available.

## Formats

| Format | Notes |
| --- | --- |
| `.stl` | binary and ASCII |
| `.ply` | binary and ASCII, vertex colors |
| `.obj` | geometry and vertex colors |
| `.glb` | embedded textures for the viewer subset |
| `.hps` | native HPS mesh support in release packages |

HPS files open natively and can be re-exported as PLY, STL, or OBJ after
editing.

## Windows integration details

- Double-click opens supported 3D files in the viewer; additional opens are
  handed to the already-running window instead of spawning a new process.
- File type names stay neutral (`STL File`, `PLY File`, …) with one shared 3D
  object icon — mesh files are not rebranded.

<p align="center">
  <img src="assets/animation.gif" alt="Interactive Windows Explorer preview pane" width="820">
</p>

## Build from source

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo run -p occluview-app --release -- path/to/scan.stl
```

Headless thumbnail from the CLI:

```bash
cargo run -p occluview-cli --release -- thumbnail scan.stl -o thumb.png --size 256
```

Headless HPS conversion uses a second machine-facing binary in the CLI package
and an explicit output directory:

```bash
cargo run -p occluview-cli --bin occluview-hps-export --release -- \
  --input scan.hps \
  --output-dir ./converted

cat scan.hps | cargo run -p occluview-cli --bin occluview-hps-export --release -- \
  --input - \
  --output-dir ./converted
```

The converter always writes geometry as `surface.ply`. When the decoded surface
has a texture, it additionally writes a self-contained `surface.glb` preview.
It also writes `manifest.json` and emits the same JSON to stdout. Manifest
schema v2 contains the parser version, a mandatory `geometry` artifact, and an
optional `preview` artifact; each entry has a relative format/path and lowercase
SHA-256. These fixed names do not derive from the input filename.

Encrypted HPS input uses `RuntimeHpsKeyProvider`. Public converter builds read
key material only from `OCCLUVIEW_HPS_ENCRYPTION_KEY` or its supported legacy
environment aliases (`OCCLUVIEW_HPS_KEY`, `OCCLUTRACE_HPS_ENCRYPTION_KEY`, and
`HPS_ENCRYPTION_KEY`). Key material is not accepted in command arguments or
written to output. Medical DICOM files with a `DICM` preamble are not dental
HPS containers and are rejected.

Failures are one JSON object on stderr and use stable exit-code classes: `2`
for arguments, `3` for input I/O, `4` for key configuration, `5` for parsing,
`6` for output I/O, and `7` for surface/artifact encoding. Error output omits
input paths, filenames, parser details, and key material.

## Status

The public release baseline is `1.0.0`. The viewer, mesh loaders and editor,
renderer, Windows shell extension, MSI, Debian packaging, and signed update
channel are maintained as one release-tested product. The platform-neutral
thumbnail pipeline is shared by the Linux thumbnailer, the Windows shell
extension, and the headless CLI; the Windows COM layer contains only Explorer
integration.

The update manifest and downloadable update artifacts are minisign-verified.
Windows Authenticode signing is not enabled in the public CI configuration yet;
the MSI remains a normal open-source installer and its SHA-256 is published
with every release.

## License

Apache-2.0. See `LICENSE`.
