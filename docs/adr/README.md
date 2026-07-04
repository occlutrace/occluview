# Architecture Decision Records (ADR)

An ADR is a short, immutable document that records **why** a decision was made.
We use ADRs so that future contributors (human and AI) understand the reasoning
behind the architecture and cannot silently reverse it — reversing an ADR
requires writing a new ADR that supersedes it.

## Format

Each ADR is a single Markdown file `NNNN-kebab-case-title.md`, numbered
sequentially. Structure:

- **Title** — `ADR-NNNN: <decision>`
- **Status** — Proposed | Accepted | Superseded by ADR-XXXX | Deprecated
- **Date** — ISO date
- **Context** — the problem, the forces, the options considered
- **Decision** — what we chose, in one paragraph
- **Consequences** — positive, negative, neutral; what we now must do / can't do
- **Alternatives considered** — one paragraph each, with the reason for rejection

## When to write an ADR

You must write (or update) an ADR when you:

- Add, remove, or replace a workspace dependency.
- Change the layering / dependency graph in `docs/ARCHITECTURE.md`.
- Introduce or remove a public API surface that other crates depend on.
- Change a non-functional target (perf budget, MSRV, supported platforms).
- Make a security-relevant choice (FFI boundary, parsing strategy, IPC).
- Decide a dental-domain default (units, camera, coordinate frame).

You do **not** need an ADR for a bug fix, a refactor that preserves behavior, or
an internal implementation detail with no cross-crate impact.

## Index

- [ADR-0001: Rust as the single implementation language](0001-rust-as-single-language.md)
- [ADR-0002: wgpu as the GPU abstraction](0002-wgpu-as-gpu-abstraction.md)
- [ADR-0003: egui for the v1 GUI](0003-egui-for-v1-gui.md)
- [ADR-0004: Per-format loaders, not assimp](0004-per-format-loaders-not-assimp.md)
- [ADR-0005: Out-of-process Rust COM thumbnail provider](0005-out-of-process-rust-com-thumbnail-provider.md)
- [ADR-0006: Cargo workspace as the build system](0006-cargo-workspace-build-system.md)
- [ADR-0007: Mesh-only — no volumetric CBCT/DICOM in v1](0007-mesh-only-no-volumetric-cbct.md)
- [ADR-0008: Apache-2.0 license, open-core model](0008-apache-2.0-open-core.md)
- [ADR-0009: Dental defaults — mm units, occlusal camera, Y-up](0009-dental-defaults.md)
- [ADR-0010: glTF reader via serde_json, not cgltf](0010-gltf-via-serde-json-not-cgltf.md)
