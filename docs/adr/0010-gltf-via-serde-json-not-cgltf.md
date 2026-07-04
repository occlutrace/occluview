# ADR-0010: glTF reader via serde_json, not cgltf

- **Status:** Accepted
- **Date:** 2026-07-04
- **Supersedes:** part of ADR-0004 (glTF loader choice)

## Context

ADR-0004 named `cgltf` (single-file C, MIT) as the glTF loader. Reconsidering
under the actual constraints of the project:

- The dental-viewer glTF subset is **small**: `POSITION`, `indices`, optionally
  `NORMAL`, `COLOR_0`, simple `materials`. We do not need animations, skinning,
  morph targets, KHR_materials_* extensions, or Draco in v1.
- Pulling `cgltf` (C) in via the `cc` crate means an `unsafe` FFI seam in the
  parsers, which `SECURITY.md` flags as the primary attack surface. The parser
  would handle untrusted input through C memory unsafety.
- glTF's structure is JSON + binary slices. Rust's `serde_json` (MIT/Apache-2.0)
  is a memory-safe, audited parser that handles the JSON half natively; the
  binary half is just byte ranges from a buffer view, fully safe Rust.

## Decision

Implement the glTF/GLB reader in **native Rust**, parsing the JSON chunk with
`serde_json` and reading binary buffer views as byte slices. Cover only the v1
mesh subset (POSITION, indices, NORMAL, COLOR_0, mode 4 triangles). Reject
unsupported features with a typed `FormatError`, do not crash.

This adds `serde` + `serde_json` as workspace dependencies (both MIT/Apache-2.0,
allow-listed in `deny.toml`).

## Consequences

**Positive**
- No `unsafe` and no C dependency in the format layer. The whole
  `occluview-formats` crate stays `#![forbid(unsafe_code)]`.
- Memory-safe JSON parsing of untrusted input (the documented glTF attack
  surface shrinks to our buffer-slice arithmetic, which is bounds-checked).
- The reader is unit-testable without a C toolchain.

**Negative**
- We re-implement the small subset of the glTF spec we need, rather than reusing
  a spec-complete library. Acceptable: the subset is stable and small, and
  avoiding C FFI is worth more than feature breadth we will not use.
- If we later need Draco, morph targets, or full PBR material round-tripping,
  we re-evaluate (likely adding `gltf-rs`, which is also pure Rust, not cgltf).

**We must now**
- Add `serde` + `serde_json` to `deny.toml` allow-list (already permitted — MIT
  and Apache-2.0 are in the allow-list).
- Maintain zip-slip / path-traversal protection for external `.gltf` buffer and
  texture URIs (only `GLB` single-file is the in-corpus case, but `.gltf` with
  external URIs must be handled safely).

## Alternatives considered

- **cgltf (C, MIT).** Original ADR-0004 choice. Rejected on the `unsafe`-in-
  parser concern.
- **gltf-rs (Rust, MIT/Apache-2.0).** Spec-complete, pure Rust. Considered as
  the loader instead of writing our own. Rejected for v1 because it pulls a
  large dependency tree for features we don't need yet; revisitable for v2 when
  we add material/animation support. Our hand-rolled subset stays under 500 LOC.
- **fastgltf (C++17, MIT).** Same FFI concern as cgltf.
