# ADR-0002: wgpu as the GPU abstraction

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

We need a GPU layer that (a) runs the live viewer, (b) runs **headless/offscreen**
to generate identical thumbnails inside the shell extension, (c) is safe to use
from native Rust, and (d) has a permissive license. The dental meshes are
triangle soups, often 1–5M triangles each, multiple per scene, sometimes with
vertex colors.

Candidates:

- **wgpu** — Rust-native implementation of the WebGPU spec; D3D12/Vulkan/Metal
  backends; first-class offscreen rendering; MIT/Apache-2.0.
- **bgfx** — cross-backend C++ library, popular, BSD-2. Would need Rust bindings
  (`bgfx-rs`); offscreen rendering works but the binding layer is thinner.
- **Google Filament** — high-quality PBR, C++/Java/Kotlin; heavier; GPL-style
  concerns are absent (Apache-2.0) but the C++ integration cost is real.
- **Direct3D 11/12 directly** — maximal Windows control, but locks us to Windows
  (we want `core`/`render` portable for future cross-platform) and duplicates the
  offscreen path by hand.
- **OpenGL / Vulkan via ash** — legacy (GL) or very low-level (ash); more code to
  maintain and get wrong.

## Decision

Adopt **wgpu** as the GPU abstraction for `occluview-render`, used by both the
live app and the headless thumbnail renderer. On Windows the backend is D3D12.

## Consequences

**Positive**
- One render code path serves the app and the thumbnailer — the thumbnail is the
  app frame at lower resolution, by construction.
- Safe Rust API; no hand-rolled `unsafe` Vulkan/D3D in the hot path.
- Portable: if we later support Linux/macOS for a CLI or a viewer, the renderer
  doesn't change.
- Mature ecosystem integration with egui (ADR-0003), which uses wgpu natively.

**Negative**
- A thin layer of overhead vs. raw D3D12 — acceptable for a viewer (we are not
  pushing 240 fps AAA workloads), and wgpu's overhead is small and shrinking.
- PBR materials require our own work; wgpu gives us primitives, not a renderer.
  This is fine — dental meshes are mostly flat-shaded with optional vertex color.
- Bindless / mesh shaders are not yet first-class; revisit for v2 if very large
  multi-mesh scenes demand it (open Q2 in `ARCHITECTURE.md`).

**We must now**
- Pin a wgpu version per release; wgpu API has been evolving — track upstream.
- Define WGSL shaders as the source of truth (not SPIR-V blobs).
- Implement an offscreen render path early; it is on the critical path for v1.

## Alternatives considered

- **bgfx.** Excellent and battle-tested; rejected mainly because wgpu's
  Rust-native, safe API and first-class offscreen path reduce risk for a small
  team and an AI-assisted codebase.
- **Filament.** Best PBR quality out of the box; rejected because its C++ nature
  reintroduces the two-language problem ADR-0001 avoided, and its full feature
  set is more than a dental viewer needs.
- **Raw D3D12.** Rejected for portability of `core`/`render` and the cost of
  hand-writing the offscreen path.
