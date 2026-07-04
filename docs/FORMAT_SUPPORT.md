# Format Support

Per-format capabilities, loader choices, and the dental-scanner quirks we handle.
Loader decisions are in [ADR-0004](adr/0004-per-format-loaders-not-assimp.md).

## v1 support matrix

| Format | Extension(s)        | Binary | ASCII | Vertex color | Materials | Units declared | Loader (v1)            | Priority |
|--------|---------------------|:------:|:-----:|:------------:|:---------:|:--------------:|------------------------|:--------:|
| STL    | `.stl`              | ✅     | ✅     | —            | —         | no             | custom Rust reader     | P0       |
| PLY    | `.ply`              | ✅     | ✅     | ✅            | —         | sometimes      | custom Rust reader     | P0       |
| OBJ    | `.obj` (+`.mtl`)    | —      | ✅     | via mtl/vertex| ✅        | no             | `fast_obj` or custom   | P0       |
| glTF   | `.gltf` `.glb`      | ✅     | ✅     | ✅            | ✅ (PBR)  | no             | `cgltf`                | P0       |
| 3MF    | `.3mf`              | ✅     | —     | ✅            | ✅        | yes            | lib3mf via FFI         | P1       |

Legend: P0 = ships in v1.0; P1 = ships in v1.x; — = not applicable.

## STL

- **Spec:** binary (80-byte header + triangle count + per-triangle: normal +
  3 verts + attribute, 50 bytes) or ASCII (`solid … endsolid`).
- **Dental reality:** almost always binary. File sizes 1–50 MB typical; a full
  arch 0.5–3 M triangles.
- **Quirks to tolerate:**
  - 80-byte header sometimes contains non-ASCII; never assume it's text.
  - Triangle count field occasionally wrong (some scanners write a 4-byte size
    then fewer triangles). Detect by EOF, not by count alone.
  - ASCII sometimes mis-declared as binary (no header signature). Probe by trying
    binary, falling back to ASCII.
- **Units:** STL declares none. We assume mm and surface it in the UI.
- **Vertex colors:** rare/non-standard; some "color STL" variants exist but are
  not interoperable. Out of scope for v1.
- **Coordinate frame:** scanner-dependent; we load as-is and normalize to our
  internal Y-up right-handed frame.

## PLY

- **Spec:** header declares properties; both ASCII and binary (LE/BE).
- **Dental reality:** the format for **color/NIR scans**. Vertex properties often
  include `red green blue` (and sometimes `alpha`, `confidence`, `nx ny nz`).
- **Quirks to tolerate:**
  - Property order/format varies wildly across scanners — parse from header,
    don't hard-code.
  - Mixed endianness; some scanners emit big-endian binary.
  - `list` properties for faces: triangles and quads.
- **Units:** sometimes declared in a `comment` line; otherwise assume mm.
- **Vertex colors:** first-class; render via vertex-color material path.

## OBJ (+ MTL)

- **Spec:** Wavefront; geometry in `.obj`, materials in `.mtl` referenced by
  `usemtl`.
- **Dental reality:** a common export from intraoral scanners (e.g. Medit).
- **Quirks to tolerate:**
  - Malformed MTL (missing newlines, vendor-specific keys) — be lenient.
  - 1-based indexing, negative (relative) indexing per the spec.
  - Faces with > 3 verts → fan-triangulate on load.
  - External texture paths — resolve relative to the file; never load textures
    from arbitrary absolute paths (security).
- **Units:** none declared; assume mm.

## glTF / GLB

- **Spec:** glTF 2.0; `.gltf` (JSON + external buffers) or `.glb` (binary
  container with embedded buffers/images).
- **Dental reality:** increasingly common as a modern interchange; supports PBR
  materials, vertex colors, and morph targets.
- **Quirks to tolerate / security:**
  - **Zip-slip / path traversal:** external buffer/texture URIs in `.gltf` must
    be constrained to the file's directory; reject `..` and absolute paths.
  - Embedded images in GLB — decode safely; bound dimensions.
  - KHR_materials_* extensions — load the common ones; ignore unknown extensions
    with a warning, don't crash.
- **Units:** meters per spec (rarely honored); convert mm↔m on load.
- **Loader:** `cgltf` (MIT), single-file C, embedded via `cc`.

## 3MF

- **Spec:** XML-in-ZIP; defined by the 3MF Consortium. Declares units
  (thou/in/ft/mm/micron).
- **Dental reality:** growing as a 3D-printing handoff format; rich metadata.
- **Quirks:** official library is comprehensive.
- **Loader:** lib3mf via FFI (C++, MIT-like), wrapped in a safe Rust API. Open
  item: native Rust reader to drop the C++ dependency (future ADR).
- **Security:** zip-slip protection on extraction; bounded XML depth.

## Coordinate-frame conversion table (per format, to internal Y-up RH)

Maintained as we validate each format against real dental files. Filled in per
loader implementation. (Placeholder rows; populate when each loader lands.)

| Format | Native frame (typical)       | Conversion to Y-up RH      |
|--------|------------------------------|----------------------------|
| STL    | varies (scanner-dependent)   | passthrough; user-rotatable|
| PLY    | varies                       | passthrough; user-rotatable|
| OBJ    | RHS, often Y-up              | none                       |
| glTF   | RHS, Y-up                    | none                       |
| 3MF    | RHS, Z-up, units declared    | swap Y↔Z on load           |

## Dental scanner native formats (reference; not supported in v1)

These are proprietary; OccluView shows a friendly "export to STL/PLY" message if
it detects them. Listed for awareness, not as a roadmap commitment.

- 3Shape TRIOS: `.3oxz`, `.dcm` (export-oriented `.obj`/`.stl` are interoperable).
- Medit: `.dcm`, `.obj`+`.mtl` (OBJ is interoperable).
- iTero: `.dcm`, `.ply`.
- Carestream: proprietary; STL export standard.
- Planmeca: STL/PLY export standard.
- Shining 3D: STL/PLY export standard.

The practical v1 path is: export from the scanner software to STL/PLY/OBJ and
open in OccluView.
