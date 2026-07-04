# Testing

Tests are not optional. Every behavior ships with a test in the same PR
(`AGENTS.md` §0.6). This document defines the kinds of tests we write and how.

## 1. Layers

| Layer            | Where                                              | Tool              |
|------------------|----------------------------------------------------|-------------------|
| Unit             | `#[cfg(test)] mod tests` in the file under test    | built-in          |
| Integration      | `crates/<x>/tests/*.rs`                            | built-in / nextest|
| Doc tests        | Rust doc examples                                  | built-in          |
| Property / fuzz  | `crates/<x>/tests/proptest.rs`, `fuzz/`            | proptest, cargo-fuzz |
| Golden image     | `crates/occluview-render/tests/golden/*.rs`        | custom harness    |
| Performance      | `benches/` (criterion)                             | criterion         |

CI runs unit + integration + doc tests via `cargo-nextest`, plus property tests
with reduced iteration counts, plus the perf benches as gates.

## 2. Unit tests

- Live in the same file as the code, in `#[cfg(test)] mod tests`.
- One behavior per test; test names read like sentences:
  `binary_stl_with_bad_header_returns_malformed_error`.
- No `unwrap` ban applies inside `#[cfg(test)]` — but prefer meaningful
  assertions.

## 3. Integration tests

- Cover cross-crate behavior: "load PLY → upload to renderer → read back bbox".
- Use the anonymized real-world dental fixtures in
  `crates/occluview-formats/tests/fixtures/` (see §5).

## 4. Property tests & fuzzing

- Every parser has a `proptest` strategy that generates valid **and** malformed
  inputs; the parser must return `Ok` or a typed `Err`, never panic.
- `cargo fuzz` targets live in `fuzz/` and run continuously in CI on a corpus
  that grows over time. Crashes are P0.

## 5. Golden-image tests (renderer)

- For a fixed scene + camera, render to an offscreen wgpu surface and compare the
  PNG to `crates/occluview-render/tests/golden/baselines/<name>.png` with a
  perceptual tolerance (e.g. max per-channel diff ≤ ε, or a small mean).
- **Updating a baseline is an ADR** (`AGENTS.md` §0.6). The PR must explain why
  the rendered output changed and confirm it's intentional.
- Baselines are platform-aware where needed (D3D12 vs WARP can differ at the
  edge of tolerance; we keep separate baselines if necessary).

## 6. Performance tests

- `criterion` benches in `benches/`. The targets in `docs/ENGINEERING.md` §6 are
  asserted in CI: a >10% regression on `main` fails the perf job.
- Bench results are uploaded as CI artifacts for trend tracking.

## 7. Test fixtures

- Real dental-scanner samples, **anonymized**, committed under
  `crates/occluview-formats/tests/fixtures/<format>/`. Keep them small (< 2 MB
  each); large reference meshes live in Git LFS or a separate data repo.
- Each fixture has a `README.md` noting source scanner, anonymization, license
  (the donor's permission), and what the test asserts.

## 8. Running tests locally

```bash
cargo nextest run --workspace                # unit + integration
cargo test --workspace --doc                 # doc tests
cargo test --workspace --proptest            # property (via nextest profile)
cargo bench -p occluview-render              # perf gates
cargo fuzz run <target>                      # fuzz (manual / nightly CI)
```

## 9. What "done" means for a test

A test is done when:
- it fails for the right reason when the code is broken (verify by temporarily
  breaking the code),
- it passes for the right reason when the code is correct,
- its name describes the behavior, not the implementation,
- it does not depend on machine-specific state (paths, GPU vendor) unless
  explicitly platform-gated.
