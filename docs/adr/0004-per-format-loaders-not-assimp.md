# ADR-0004: Per-format loaders, not assimp

- **Status:** Accepted (with an open item on 3MF)
- **Date:** 2026-07-04

## Context

OccluView opens untrusted files from disk. The loader is the primary attack
surface and the primary correctness surface. We need loaders for STL, PLY, OBJ,
glTF/GLB, 3MF. Two strategies:

- **assimp** — one C++ library that loads "everything." BSD-3 license is fine, but
  assimp has a long history of CVEs in rare-format parsers, its API encourages
  loading the whole file into a generic `aiScene`, and we'd carry code for dozens
  of formats we don't need.
- **Per-format best-in-class loaders** — pick the strongest library (or write a
  small focused parser) for each format we actually ship. Smaller attack surface,
  clearer ownership, easier to fuzz.

Dental-scanner specifics that matter:

- **STL** — trivial binary/ASCII; dental scanners emit binary STL almost always.
- **PLY** — dental color scans (NIR/RGB) rely on vertex color properties; must
  support both binary and ASCII and a wide range of property layouts.
- **OBJ + MTL** — common export from intraoral scanners; needs robust handling of
  malformed MTL.
- **glTF/GLB** — modern interchange; the future-proof choice for textured/PBR
  meshes.
- **3MF** — XML-in-ZIP; the official library is **lib3mf** (C++, MIT-like), which
  is comprehensive but heavy. A native Rust reader is possible but less complete.

## Decision

Use **per-format, best-in-class loaders**:

- **STL** — custom Rust reader in `occluview-formats::stl` (binary + ASCII, mmap
  streaming for large files). STL is small enough that a focused parser is safer
  and faster than a dependency.
- **PLY** — custom Rust reader handling full property flexibility (vertex colors,
  normals, custom properties), binary big/little-endian and ASCII.
- **OBJ** — `fast_obj` or a focused custom reader; robust against malformed MTL.
- **glTF/GLB** — **cgltf** (single-file C, MIT) as the primary, with **fastgltf**
  (C++17, MIT, SIMD) considered where parse speed dominates. glb's binary buffer
  and embedded images must be handled without zip-slip or arbitrary writes.
- **3MF** — **lib3mf via FFI** for v1 (correctness over purity), isolated behind a
  safe Rust wrapper in `occluview-formats::threemf`. Open item: a native Rust 3MF
  reader to drop the C++ dependency is a future ADR.

## Consequences

**Positive**
- Minimal attack surface: each format has one focused, fuzzed parser.
- No monolithic dependency with rare-format CVE tail.
- Clear per-format ownership and per-format tests (including dental-scanner
  sample files in `crates/occluview-formats/tests/fixtures`).

**Negative**
- More code to write/maintain than "just call assimp" — but the code is small,
  focused, and tested, which is exactly what keeps AI-assisted codebases honest.
- 3MF drags in a C++ dependency via lib3mf FFI. We accept it for correctness now
  and plan a native reader.

**We must now**
- Add `cargo fuzz` targets for every parser.
- Keep a `fixtures/` set of real (anonymized) dental-scanner files per format.
- Validate zip-slip / path-traversal protection for glTF-embedded and 3MF
  archives.

## Alternatives considered

- **assimp as universal loader.** Rejected on CVE history and attack surface.
- **Write everything from scratch.** Rejected for glTF (cgltf/fastgltf are
  excellent) and 3MF (lib3mf is comprehensive); accepted only for STL and PLY
  where the formats are simple and the dental specifics are sharp.
