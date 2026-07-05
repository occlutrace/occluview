# AGENTS.md

Build and contribution facts for OccluView — a native Windows 3D mesh viewer
for dental workflows (Rust). Product overview: [README.md](README.md).
Architecture and crate graph: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Build & test

```sh
cargo build --workspace           # Linux/macOS build the cross-platform crates
cargo test --workspace            # unit + integration + golden-image tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check                  # license / advisory audit
```

The Windows-only crates (`occluview-shell`, `occluview-app`) are excluded from
`default-members`; on Linux/macOS the commands above build everything that can
build. Cross-check Windows targets with
`cargo check -p occluview-app -p occluview-shell --target x86_64-pc-windows-msvc`.

## Crate map

| Crate | Role | Depends on |
|---|---|---|
| `occluview-core` | scene, math, camera, mesh ops — no I/O, no GPU, no Win32 | — |
| `occluview-formats` | STL / OBJ / PLY / glTF-GLB / 3MF readers | core |
| `occluview-render` | wgpu renderer, WGSL shaders, offscreen path | core |
| `occluview-shell` | Windows COM thumbnail provider + registration | core, render, formats |
| `occluview-app` | the GUI binary (egui) | all of the above |
| `occluview-cli` | headless convert / render / thumbnail | core, formats, render, shell |

Dependencies point downward only; a cycle is a bug. `occluview-core` depends on
nothing in this workspace and is panic-free by lint.

## Conventions you can't see from one file

- Rust files stay under **500 lines**; split before crossing. A genuine
  exception carries a one-line reason at the top of the file.
- `unwrap`/`expect` are banned outside tests and process init
  (`clippy::unwrap_used`/`expect_used` are warnings promoted by CI).
- Units live in types: prefer `Millimeters(f32)` (`occluview-core/src/units.rs`)
  over bare `f32`. Meshes are millimeters, anatomy-oriented — never assume
  game-engine defaults (meters, Y-up).
- Errors: `thiserror` in libraries, `anyhow` only in the `app`/`cli` binaries.
  No `Box<dyn Error>` in public APIs.
- `unsafe` requires a `// SAFETY:` comment per invariant. FFI (Win32/COM) lives
  only in `occluview-shell` and `occluview-app`.
- New workspace dependency ⇒ an ADR in `docs/adr/` (rationale, license,
  maintenance, attack surface). Licenses are allow-listed via `deny.toml`.

## Testing

- Unit tests live next to the code; integration tests in `crates/<x>/tests/`.
- The renderer is guarded by golden-image tests
  (`crates/occluview-render/tests/`): render → compare PNG within tolerance.
  Updating a baseline requires an ADR — never regenerate one to make CI green.
- Parsers take malformed input from the real world; property tests are welcome
  wherever a parser branch is non-trivial.

## Commits & PRs

- Conventional Commits: `<type>(<scope>): <subject>` — types
  `feat fix docs style refactor perf test build ci chore`, scopes
  `core formats render shell app cli docs ci`.
- ASCII in code, comments, and commit messages (no em-dashes, arrows, emoji).
- No AI attribution trailers of any kind. Commits describe the change, not the
  tooling. `Signed-off-by:` is a human-only act.
- One concern per PR, target under 600 diff lines, squash-merge, green `main`.

## For AI-assisted contributions

AI tools are welcome; the standard is the same as for any contributor — you
must understand and be able to defend every line you submit. In particular:

- Verify an API exists before using it; hallucinated imports are the most
  common failure.
- Run the test commands yourself and include real output; never describe tests
  you did not run.
- Never delete a test or a golden baseline to get to green.
- No drive-by refactors outside the task; file an issue instead.
- Don't disable a lint or CI check to pass — fix the code or raise it in the PR.
- When unsure about scope, open a draft PR and ask.

## Where to look next

`docs/ARCHITECTURE.md` (design), `docs/adr/` (past decisions — read before
revisiting one), `docs/GLOSSARY.md` (dental + graphics terms),
`docs/FORMAT_SUPPORT.md`, `docs/SHELL_INTEGRATION.md`, `docs/TESTING.md`,
`docs/ENGINEERING.md` (lint config, perf budgets, release process),
[CONTRIBUTING.md](CONTRIBUTING.md) (PR process), [SECURITY.md](SECURITY.md).
