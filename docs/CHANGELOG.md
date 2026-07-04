# Changelog

All notable, user-visible changes to OccluView are recorded here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once we reach 1.0.

Categories: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`,
`Performance`, `Documentation`, `Build`, and `BREAKING` (for breaking changes).

## [Unreleased]

### Added
- Project foundation: `AGENTS.md` (constitution + 7-stage workflow),
  `CONTRIBUTING.md`, `SECURITY.md`, `TRADEMARK.md`, `CODE_OF_CONDUCT.md`.
- Architecture documentation: `docs/ARCHITECTURE.md` and the foundational ADR set
  (ADR-0001 … ADR-0009), recording decisions on language (Rust), GPU layer
  (wgpu), GUI (egui), loaders (per-format), shell integration (out-of-process
  COM), build (Cargo), mesh-only scope, Apache-2.0 licensing, and dental defaults.
- Engineering docs: `docs/ENGINEERING.md`, `docs/TESTING.md`,
  `docs/SHELL_INTEGRATION.md`, `docs/ANTI_SLOP.md`, `docs/GLOSSARY.md`,
  `docs/FORMAT_SUPPORT.md`, `docs/ROADMAP.md`.
- Workspace skeleton: `crates/{core,formats,render,shell,app,cli}` with crate
  manifests and stub `lib.rs`/`main.rs` files (no functional behavior yet).
- Tooling: pinned `rust-toolchain.toml`, workspace `Cargo.toml`, `deny.toml`,
  `clippy.toml`, `.gitignore`, `.gitattributes`.
- CI: GitHub Actions workflow enforcing fmt / clippy / test / deny / docs / perf.
- Community: issue templates, PR template with the Definition-of-Done checklist,
  `CODEOWNERS`.

### Notes
- Implementation begins after this foundational commit. No binary is released
  yet; the first shipped binary will be `v0.1.0`.

[Unreleased]: https://github.com/occlutrace/occluview/compare/HEAD
