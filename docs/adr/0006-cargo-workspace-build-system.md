# ADR-0006: Cargo workspace as the build system

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

We need a build system that is reproducible, minimal, and hard to get wrong under
AI-assisted development. Candidates: Cargo (Rust-native), CMake, Meson, Bazel,
xmake, Buck2.

## Decision

Use a single **Cargo workspace** at the repo root. All crates live under
`crates/`. `Cargo.lock` is committed for reproducible builds. The toolchain is
pinned in `rust-toolchain.toml`.

## Consequences

**Positive**
- One tool, one mental model, one lockfile. Reproducible builds by default.
- Workspace-level `cargo test` / `cargo clippy` / `cargo bench` cover everything.
- No generator-step / configure-step drift (the CMake class of "works on my
  machine"). This is a meaningful anti-slop property.
- Native dependency resolution via crates.io; `cargo deny` gates licenses and
  advisories.
- FFI to C/C++ (lib3mf, Windows SDK) handled with `cc`/`windows-rs` build scripts
  inside the relevant crate — locality of concerns.

**Negative**
- Rust-only — if we ever need a C++ component outside FFI, we'd need a secondary
  build, but we explicitly chose single-language (ADR-0001).
- Cross-compilation for Windows from Linux/macOS for developer convenience is
  possible but not a v1 priority; we develop on Windows for Windows.

**We must now**
- Keep `Cargo.lock` in VCS.
- Keep one workspace `rust-toolchain.toml`, one `clippy.toml`, one `deny.toml`.
- Run `cargo update` in a dedicated PR (not bundled with feature work).

## Alternatives considered

- **CMake.** Powerful and universal in C++ 3D, but unnecessary in a Rust-only
  workspace and a known source of build-repro fragility.
- **Bazel / Buck2.** Excellent for very large monorepos; overkill here.
- **Meson / xmake.** Good, but offer no advantage over Cargo for a Rust project.
