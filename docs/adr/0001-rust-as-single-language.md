# ADR-0001: Rust as the single implementation language

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

OccluView has two pieces that must share a codebase: a GUI viewer application and
a Windows shell-extension DLL (the thumbnail provider). The shell extension is
performance- and safety-critical (loads into `explorer.exe`'s surrogate and opens
untrusted files). The viewer must cold-start under 400 ms and idle under 120 MB.

Candidate languages:

- **C++** — maximal 3D ecosystem, maximal performance, but a heavier build (CMake,
  ABI care), manual memory management in parsers, and would force a second
  language if we want a safer shell DLL. The closest benchmark, F3D, is C++/VTK.
- **Rust** — native performance, no GC/JIT (instant cold start), strong memory
  safety in the parsing hot path, `windows-rs` for COM, one language across the
  app and the shell DLL, and `cargo` for reproducible builds.
- **C# / .NET 8 NativeAOT** — fastest GUI development on Windows, but slower cold
  start than native, GC pauses, and .NET-inside-explorer is historically painful
  for shell extensions (CLR loading into `explorer.exe`).
- **Zig** — excellent, but a smaller ecosystem for 3D and COM bindings today.

## Decision

Use **Rust (edition 2021)** as the single implementation language for the entire
workspace: core logic, format loaders, renderer, GUI, CLI, and the Windows COM
shell extension.

## Consequences

**Positive**
- One language, one toolchain, one dependency graph — the shell DLL and the app
  literally link the same `occluview-render` code, guaranteeing the thumbnail
  matches the in-app render.
- Memory safety in parsers (the primary attack surface) without a GC.
- `cargo` gives reproducible builds and a lockfile in VCS, which materially
  reduces AI-slop risk (no "works on my machine", no CMake reconfiguration drift).
- Instant cold start (no VM warmup, no JIT).

**Negative**
- The Rust GUI ecosystem is less polished than Qt/WPF. We accept this for v1 by
  keeping the chrome minimal (it's a viewer) and choosing egui (ADR-0003).
- Some dental/CAD libraries are C++ only (notably lib3mf); we bridge via FFI in
  the `formats` crate, isolated behind a safe Rust API.
- Fewer contributors know Rust than C++; mitigated by strict `AGENTS.md` rules
  that make the codebase readable and AI-assisted work safe.

**We must now**
- Pin the toolchain (`rust-toolchain.toml`) and keep the MSRV explicit.
- Restrict `unsafe` to `occluview-shell` and `occluview-app` (FFI boundaries).

## Alternatives considered

- **C++ + VTK (F3D clone).** Strong, but two-language maintenance and CMake
  fragility outweighed the ecosystem advantage for a project that wants to stay
  slop-resistant under heavy AI-assisted development.
- **C# / .NET 8 + WinUI 3.** Best Windows DX, but the cold-start and the
  .NET-in-explorer story conflict with our hard non-functional targets.
- **Hybrid (Rust core + C# UI).** Considered seriously; rejected for v1 to keep
  the toolchain single and the build simple. Revisit if WinUI integration
  becomes a blocker.
