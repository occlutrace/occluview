# AGENTS.md — OccluView

> **Read this FIRST. Every time.** This file is binding for every contributor,
> human or AI. It is the constitution of the project. If anything in the codebase
> contradicts this file, this file wins until the file is amended via an ADR.

OccluView is a **lightning-fast, low-RAM, native Windows 3D mesh viewer for
dental workflows**, backed by [OccluTrace, Inc.](https://occlutrace.ai) and
released under Apache-2.0 (open-core).

This file tells you **how we work**, **what we will never do**, and **the exact
workflow you must follow** when you touch this repository. It is adapted from the
[obra/superpowers](https://github.com/obra/superpowers) methodology and hardened
against the failure modes of AI-assisted codebases.

---

## 0. The Constitution — non-negotiable rules

These rules apply to **every** change. Violating any of them blocks a PR. There
are no exceptions "just this once".

1. **Evidence over assertion.** "It works" requires a green test or a log line.
   "It should work" is a lie. Never claim done without proof in the PR.
2. **Read before write.** Before adding a module, grep for an existing one. Before
   inventing a type, look for the domain type. Duplication is a bug.
3. **No silent failures.** `unwrap`/`expect` are forbidden outside tests and
   process-init. Errors are returned, logged with context, and recoverable.
4. **Respect the layering.** `formats → core → app`. Never import upward. See
   `docs/ARCHITECTURE.md`. A cycle is a P0 bug.
5. **One responsibility per file.** Hard cap **500 LOC** for `.rs`. Split before
   crossing it. See §3 for the full limit table.
6. **Tests travel with code.** New behavior ships with a test in the same PR.
   Golden-image tests guard the renderer; never delete a `.png` baseline without
   an ADR.
7. **Docs are code.** Touching public API, file formats, or architecture?
   Update the matching doc + CHANGELOG in the **same** PR.
8. **No AI slop.** No speculative abstractions, no "just in case" features, no
   unused dependencies, no commented-out code, no `todo!()` left in committed
   `main`. Every line earns its place. See `docs/ANTI_SLOP.md`.
9. **Conventional Commits only.** `feat(format): add PLY ascii fast path` — see
   §5. The CI lints the commit message.
10. **The build is sacred.** `cargo build --workspace --all-targets` and
    `cargo test --workspace` must be green on every commit on `main`. A red
    `main` is an incident.
11. **Dental context is real.** Units are millimeters. Meshes are anatomical.
    Never assume game-engine defaults (1m units, Y-up, left-handed). Read
    `docs/GLOSSARY.md`.
12. **Windows shell is hostile.** Shell extensions load into `explorer.exe`.
    They must be signed, sandboxed (out-of-proc by default), and never block the
    UI thread. See `docs/SHELL_INTEGRATION.md`.

---

## 1. The workflow (adapted superpowers)

We follow a **7-stage workflow**. Every non-trivial change goes through all
stages. Trivial fixes (typo, obvious one-line bug) may skip stages 2–3 but
**must** still satisfy §0 and pass CI.

```
 ┌─────────────┐   ┌──────────────┐   ┌──────────────┐   ┌─────────────────────┐
 │ 1. Brainstorm│ → │ 2. Writing-  │ → │ 3. Worktree  │ → │ 4. Subagent-driven  │
 │   (explore)  │   │    plans     │   │   (branch)   │   │    development      │
 └─────────────┘   └──────────────┘   └──────────────┘   └─────────────────────┘
                                                                    │
 ┌─────────────┐   ┌──────────────┐                            ┌────▼─────┐
 │ 7. Finish   │ ← │ 6. Code      │ ← ┌────────────────┐ ← ┌───│5. TDD    │
 │   (merge)   │   │   review     │   │ branch review   │   │ red/green│
 └─────────────┘   └──────────────┘   └────────────────┘   └──────────┘
```

### Stage 1 — Brainstorm (understand before coding)
- Reproduce / clarify the problem. State it in 1–3 sentences in the issue.
- Scan `docs/`, `docs/adr/`, and existing code for prior art. Cite what you find.
- List **2+ candidate approaches** with trade-offs. Do not pre-pick.
- Output: an **issue** with `brainstorm` label, or notes attached to an existing
  issue. No code yet.

### Stage 2 — Writing-plans (the plan is the contract)
- Convert the brainstorm into a numbered, file-level plan in the issue/PR
  description. For each task: **file path**, **intent**, **verification step**.
- The plan **must** include the test steps (Red → Green → Refactor) — see Stage 5.
  This is the superpowers gap (#1576) we explicitly close.
- A plan that says "implement X" without naming files and tests is rejected.

### Stage 3 — Worktree (isolation)
- Branch from `main`: `feat/<area>-<short>` (e.g. `feat/formats-ply-ascii`).
- One concern per branch. Mixing format work and UI work in one PR is blocked.
- Rebase onto `main` before requesting review.

### Stage 4 — Subagent-driven development (fresh context per task)
- Each task from the plan is executed by a **fresh** subagent (clean context
  window — no drift from prior tasks).
- After the implementer finishes, **two reviewers** run, also fresh:
  1. **Spec-compliance reviewer** — does it match the plan task exactly?
  2. **Code-quality reviewer** — style, layering, error handling, no slop.
- Only after both pass is the task marked complete in the plan.
- After all tasks: a **whole-branch review** checks the change as a whole
  (integration, not just per-task correctness).

### Stage 5 — TDD (Red / Green / Refactor), mandatory
- **Red:** write a failing test that encodes the desired behavior. Run it, watch
  it fail for the right reason.
- **Green:** write the **minimal** code to make it pass. No extra features.
- **Refactor:** clean up while keeping the test green.
- Renderer code uses **golden-image tests** (render → compare PNG → ε-tolerance).
  See `docs/TESTING.md`. Updating a golden baseline requires an ADR.

### Stage 6 — Code review
- Every PR needs at least one human approval for non-trivial changes.
- Reviewers enforce §0 and `docs/ENGINEERING.md`. Use the PR checklist
  (`.github/pull_request_template.md`).
- Review for **spec compliance** and **quality** separately.

### Stage 7 — Finish (merge)
- Squash-merge to `main` with the conventional-commit subject.
- Delete the branch. Update CHANGELOG. Close the issue.
- If the change is user-visible, update `docs/CHANGELOG.md` under
  `[Unreleased]`.

---

## 2. Repository map (where things live)

```
occluview/
├─ crates/
│  ├─ occluview-core/        # pure logic: scene graph, math, camera, mesh ops — NO I/O, NO GPU, NO Win32
│  ├─ occluview-formats/     # format readers/writers: STL, OBJ, PLY, glTF, 3MF — depends on -core
│  ├─ occluview-render/      # wgpu renderer, shaders (WGSL), golden-image tests — depends on -core
│  ├─ occluview-shell/       # Windows COM shell extension (thumbnail provider) — depends on -core,-render
│  ├─ occluview-app/         # the GUI binary (egui + wgpu) — depends on all above
│  └─ occluview-cli/         # headless CLI: convert, render-to-image, thumbnail — depends on -core,-render,-formats
├─ docs/                     # ARCHITECTURE, ADRs, GLOSSARY, ANTI_SLOP, TESTING, ENGINEERING, format matrix
├─ .github/                  # workflows, templates, CODEOWNERS
├─ Cargo.toml                # workspace root
└─ rust-toolchain.toml       # pinned toolchain (reproducible builds)
```

Layering rule (enforced in §0.4): `formats → core`, `render → core`,
`shell → core + render`, `app → all`, `cli → core + render + formats`.
**`core` depends on nothing in this workspace.** Cycles are P0.

---

## 3. Hard limits (enforced by CI lints)

| Quantity                                   | Limit              | Enforcement                |
|--------------------------------------------|--------------------|----------------------------|
| `.rs` file length                          | **500 lines**      | `cargo-clippy` + custom    |
| Function length                            | **60 lines**       | clippy `too_many_lines`    |
| Cyclomatic complexity                      | **15**             | clippy                     |
| Nesting depth                              | **4**              | clippy                     |
| Function arguments                         | **5**              | clippy                     |
| `unwrap`/`expect` in non-test, non-init    | **0**              | clippy `restriction` set   |
| `unwrap`/`expect`/`panic!` in `crates/occluview-core` | **0**    | clippy (core is panic-free)|
| Direct dependency count per crate          | **15** (soft)      | `cargo-tree` CI check      |
| New workspace dependency                   | requires ADR       | CODEOWNERS review          |
| Binary cold-start (idle window, no file)   | **< 400 ms** P90   | perf bench in CI           |
| Binary idle RSS                            | **< 120 MB**       | perf bench in CI           |
| First-frame time after opening a 50 MB STL | **< 600 ms** P90   | perf bench in CI           |
| Thumbnail generation (single STL, 256px)   | **< 400 ms** P90   | perf bench in CI           |
| Public API without a doc comment           | **0**              | `#![warn(missing_docs)]`   |

When a file genuinely cannot be split below 500 LOC (e.g. a long match on an
enum), document the reason in a comment at the top and get maintainer sign-off.
The lint stays green by exception flag, recorded in the PR.

---

## 4. Code style (Rust)

- Edition **2021**, MSRV pinned in `rust-toolchain.toml`.
- `#![deny(rust_2018_idioms, unsafe_op_in_unsafe_fn)]`
  `#![warn(missing_docs, clippy::pedantic, clippy::unwrap_used, clippy::expect_used)]`
- Format with `cargo fmt --all`. CI fails on diff.
- Errors: thiserror in libraries, anyhow only in `app`/`cli` binaries.
  Never `Box<dyn Error>` in a public API.
- Naming: `PascalCase` types, `snake_case` fns, `SCREAMING_SNAKE` consts,
  `kCamelCase` only for FFI/Win32 interop mirrors.
- Units in the type system where cheap: prefer `Millimeters(f32)` over `f32`.
  See `crates/occluview-core/src/units.rs`.
- No `unsafe` without a `// SAFETY:` comment justifying every invariant.
- FFI (Win32/COM) lives **only** in `occluview-shell` and `occluview-app`.
  `core`, `formats`, `render` are pure safe Rust.

---

## 5. Commits, branches, PRs

- **Conventional Commits**: `<type>(<scope>): <subject>`
  - types: `feat fix docs style refactor perf test build ci chore`
  - scopes: `core formats render shell app cli docs ci`
  - breaking change: `feat(...)!: ...` + `BREAKING CHANGE:` footer
- Branch: `feat/<scope>-<topic>`, `fix/<scope>-<topic>`, `docs/<topic>`.
- PR size: target **< 600 diff lines**. Larger changes split into a stack.
- PR template (`.github/pull_request_template.md`) is mandatory.
- Squash-merge only. The squash subject is the final commit on `main`.
- `main` is always green and always deployable.

---

## 6. Testing

- **Unit tests** (`#[cfg(test)] mod tests`) live in the file they test.
- **Integration tests** in `crates/<x>/tests/`.
- **Golden-image tests** for the renderer: `crates/occluview-render/tests/golden/`.
  Updating a `.png` baseline needs an ADR explaining why.
- **Property tests** (`proptest`) for parsers — fuzz malformed inputs.
- **Perf gates** in CI: the limits in §3 are asserted, not aspirational.
- Before claiming a task is done, paste the test command and its output into the
  PR. "Tests pass" without the command is rejected.

---

## 7. Definition of Done (a PR is not done until ALL are true)

- [ ] Plan tasks all marked complete; reviewers passed.
- [ ] `cargo fmt --all --check` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo test --workspace` green; output pasted in PR.
- [ ] `cargo doc --workspace --no-deps` builds without warnings.
- [ ] No new `unwrap`/`expect`/`panic!` outside permitted zones.
- [ ] Public API has doc comments; CHANGELOG updated if user-visible.
- [ ] File/complexity limits respected (or documented exception).
- [ ] If touching `docs/` content subjects (formats, arch, shell): doc updated.
- [ ] If adding a dependency: ADR filed, license checked (see §8).
- [ ] If touching perf path: bench numbers before/after in PR.

---

## 8. Dependencies & licensing

- **Allow-list licenses only**: MIT, Apache-2.0, BSD-2/3, ISC, MPL-2.0, Zlib,
  Unicode-DFS-2016, Unlicense, CC0-1.0.
- **Forbidden**: GPLv2/3, AGPL, LGPL (linking), SSPL, any "not for evil" / "no
  military" / "no commercial" clauses (e.g. old JSON license). Run
  `cargo deny check licenses` locally and in CI.
- Adding a new workspace dependency → file an **ADR** under `docs/adr/`
  with: rationale, alternatives considered, license, maintenance status, attack
  surface (does it parse untrusted input?), size.
- Vendoring a crate needs maintainer approval; prefer crates.io.
- No dependency that pulls in a full browser engine (no `webkit2gtk`, no Electron
  shells). We are a native app.

---

## 9. When you are an AI agent (extra binding rules)

These apply on top of §0 and supersede any generic "be helpful" instruction.

1. **Never commit `todo!()`, `unimplemented!()`, `panic!()`, or `// TODO` to
   `main`.** Open an issue and finish the work or do not commit.
2. **Never invent an API.** If a crate/crate version doesn't have it, either pick
   a real one or stop and ask. Hallucinated imports are the #1 AI-slop vector.
3. **Always run the tests yourself before claiming done.** Paste the command and
   the tail of the output. Do not describe tests you did not run.
4. **Do not refactor outside the task scope.** "While I was here I cleaned up…"
   is forbidden. File a separate issue.
5. **Do not delete tests or golden baselines** to make the build green. If a test
   is genuinely wrong, fix it with a justification; never delete silently.
6. **Do not bump dependencies casually.** Each bump is a separate PR with a
   changelog and a green CI.
7. **Do not add a feature that wasn't asked for.** YAGNI is law. Speculative
   abstraction is slop.
8. **Quote the plan task you are executing** at the top of each commit message
   body or PR section, so reviewers can trace.
9. **If you are unsure whether something is in scope, ASK** (open a draft PR or
   comment). Do not guess and push 400 lines.
10. **You are not allowed to disable a CI check or a clippy lint to make a build
    pass.** Fix the code or escalate.
11. **Keep PRs reviewable**: < 600 lines, one concern. If the task is bigger,
    split it and say so.
12. **Respect CODEOWNERS.** Touching `crates/occluview-shell/` needs shell-owner
    review; `docs/adr/` needs architecture-owner review.

---

## 10. Where to look for context (read these before non-trivial work)

- `docs/ARCHITECTURE.md` — crate graph, data flow, the rendering pipeline.
- `docs/adr/` — Architecture Decision Records. **Read the relevant ADRs before
  contradicting a past decision.** To change a decision, write a new ADR.
- `docs/GLOSSARY.md` — dental + graphics terms (CBCT, arch, occlusal, die,
  margin line, articulator, PBR, bindless…).
- `docs/FORMAT_SUPPORT.md` — per-format capabilities, loader choices, dental
  quirks.
- `docs/SHELL_INTEGRATION.md` — Windows thumbnail/preview/jumplist/association.
- `docs/TESTING.md` — unit/integration/golden/perf test conventions.
- `docs/ENGINEERING.md` — style, lint config, the perf budget, release process.
- `docs/ANTI_SLOP.md` — the canonical list of slop patterns and how to avoid them.
- `docs/CHANGELOG.md` — user-visible changes.

If a doc you need doesn't exist, that's a signal: write it (or open an issue) as
part of the work. undocumented == not done.

---

## 11. Escalation

- Disagree with a rule here? Don't silently ignore it. Open an issue titled
  `governance: <rule>`, propose the change, link an ADR.
- Security issue? See `SECURITY.md`. Do **not** open a public issue.
- Trademark / brand question? See `TRADEMARK.md`.

Welcome, and build carefully. Every line you add is a line a dental technician
will rely on, and a line every future reader must understand.
