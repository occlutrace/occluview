<!--
  Thank you for contributing to OccluView!
  Read AGENTS.md first — it is binding for every contributor (human or AI).
  Fill in every section. A PR missing sections will be returned for revision.
-->

## Linked issue

<!-- `brainstorm` + `plan` notes live in the issue; link it here. -->
Closes #

## What & why

<!-- One paragraph: what does this change do, and why? Cite the plan task. -->

## Verification (evidence, not assertion — AGENTS.md §0.1)

<!-- Paste the command and the tail of the output. Do not claim done without proof. -->

```
$ cargo test --workspace
<paste here>
```

## Checklist — Definition of Done (AGENTS.md §7)

- [ ] Linked issue with `brainstorm` + `plan`.
- [ ] One concern per PR; target < 600 diff lines; rebased on `main`.
- [ ] `cargo fmt --all --check` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo test --workspace` green (output pasted above).
- [ ] `cargo doc --workspace --no-deps` builds without warnings.
- [ ] No new `unwrap`/`expect`/`panic!` outside permitted zones.
- [ ] Public API documented; CHANGELOG updated if user-visible.
- [ ] File/complexity limits respected (≤ 500 LOC/file, ≤ 60 LOC/fn, ≤ 15 complexity).
- [ ] Renderer change → golden-image baseline updated (with an ADR if so).
- [ ] New dependency → ADR filed; `cargo deny` green.
- [ ] Perf path touched → before/after numbers in this PR.

## If you are an AI agent (AGENTS.md §9)

- [ ] Quoted the plan task at the top of this PR.
- [ ] No `todo!()`/`unimplemented!()`/`// TODO` committed.
- [ ] Did not delete tests or baselines to go green.
- [ ] Did not refactor outside task scope.
- [ ] Did not invent APIs (every symbol verified to exist).
- [ ] Did not disable a CI check or lint to pass.
