# Engineering Guide

The practical companion to `AGENTS.md`: lint config, the perf budget, error
handling conventions, and the release process.

## 1. Toolchain & environment

- Toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml).
- Develop on Windows 10/11 with the Windows SDK installed.
- Recommended cargo subcommands:
  - `cargo-deny` — license + advisory gate (`deny.toml`).
  - `cargo-nextest` — faster, better test runner.
  - `cargo-binstall` — install binaries without compiling.
  - `cargo-machete` / `cargo-udeps` — find unused dependencies.

## 2. Lint configuration

Workspace `Cargo.toml` sets, for every crate:

```rust
#![deny(rust_2018_idioms, unsafe_op_in_unsafe_fn, missing_docs)]
#![warn(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::dbg_macro,
    clippy::print_stdout,
)]
```

Per-crate `lib.rs`/`main.rs` may `#![allow]` a specific pedantic lint with a
justifying comment — never globally in the workspace.

`clippy.toml` (repo root) sets the numeric limits from `AGENTS.md` §3:

```toml
cognitive-complexity-threshold = 15
too-many-arguments-threshold = 5
# file-length and function-length are enforced by a custom CI check
```

## 3. Error handling

- **Library crates** (`core`, `formats`, `render`, `shell`): return
  `Result<T, ThisCrateError>` via `thiserror`. Error types are part of the public
  API — document them.
- **Binaries** (`app`, `cli`): may use `anyhow` at the top level for aggregation
  and pretty reporting. Never leak `anyhow::Error` across a crate boundary into a
  public function signature.
- **No panics in libraries.** `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!`
  are clippy-denied in library code. `core` is panic-free by policy.
- **Error context:** errors carry enough context to diagnose without a debugger
  — file path, byte offset, format-specific detail. Use `#[from]` and `.map_err`
  with context, not bare `?` on opaque errors.

## 4. Units & types (dental-aware)

- Use the unit newtypes in `occluview-core::units`: `Millimeters`, `Radians`,
  `Degrees`. Prefer them over raw floats in public APIs.
- Coordinate frame is right-handed Y-up internally (ADR-0009). Per-format
  conversions live in the format reader, not in the renderer.

## 5. Logging

- `tracing` (not `log`) across all crates. `clippy::dbg_macro` and
  `clippy::print_stdout` are denied — no `println!` in libraries.
- Spans carry structure: path, format, vertex count, timings.
- In the shell extension, logs go to a separate channel (not Explorer's console
  — there isn't one) and are best-effort (never block).

## 6. Performance budget (P90, asserted in CI)

| Metric                                              | Target          | Bench                       |
|-----------------------------------------------------|-----------------|-----------------------------|
| Cold start to interactive window (no file)          | < 400 ms        | `benches/cold_start`        |
| Idle RSS after opening empty window                 | < 120 MB        | `benches/idle_rss`          |
| First frame after opening a 50 MB binary STL        | < 600 ms        | `benches/open_stl_50mb`     |
| Thumbnail generation, single STL, 256px             | < 400 ms        | `benches/thumb_256`         |
| Sustained frame rate, orbiting 5M-tri mesh          | ≥ 60 fps        | `benches/orbit_5m`          |

A regression beyond 10% on `main` fails the CI perf job and blocks the PR until
explained or fixed. See `docs/TESTING.md` for how benches are run.

## 7. File-size and complexity limits

Repeated from `AGENTS.md` §3, enforced in CI:

- `.rs` file: ≤ 500 lines (custom check).
- Function: ≤ 60 lines (clippy `too_many_lines`).
- Cognitive complexity: ≤ 15 (clippy).
- Nesting depth: ≤ 4 (clippy).
- Function arguments: ≤ 5 (clippy).
- `unwrap`/`expect`/`panic!` outside tests/init: 0 (clippy).

## 8. Cross-compile check (catch Windows bugs on Linux)

The shell and app crates are Windows-only (ADR-0001). On a Linux dev host,
type-check them by cross-compiling to `x86_64-pc-windows-gnu`:

```bash
rustup target add x86_64-pc-windows-gnu
cargo check -p occluview-core -p occluview-formats -p occluview-render \
    --target x86_64-pc-windows-gnu
```

This catches type errors in `windows-rs` bindings and COM code without a
Windows VM. The cross-compile job in `.github/workflows/ci.yml` runs this
on every push. Real execution (DLL registration, Explorer thumbnail) still
needs a Windows host (see CONTRIBUTING.md "Windows testing").

## 9. Releases

1. Ensure `main` is green and the perf bench is within budget.
2. Update `docs/CHANGELOG.md`: move `[Unreleased]` to `vX.Y.Z (YYYY-MM-DD)`.
3. Tag `vX.Y.Z`. CI builds the MSI + symbols; the shell DLL and the installer
   are **signed** (a shell extension loaded into Explorer must be signed).
4. Publish: GitHub Release with the signed installer + checksums + SBOM
   (`cargo cyclonedx`) + release notes from the CHANGELOG section.
5. Update winget manifest (maintainer-only) if applicable.

Semver: 0.x means "anything may change"; from 1.0 we follow Cargo semver and
record breaking changes in CHANGELOG with `BREAKING:` prefixes.
