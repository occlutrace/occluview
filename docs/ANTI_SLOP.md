# The Anti-Slop Playbook

> "AI slop" is the slow accumulation of plausible-looking code that has no
> reason to exist: speculative abstractions, duplicated logic, hallucinated
> APIs, ignored conventions, dead branches, and untested claims. Left
> unchecked it rots a codebase faster than any bug. This document names the
> patterns so we can refuse them.

This is the operational companion to `AGENTS.md` §0.8 and §9. Every pattern here
is block-on-PR.

## The canonical slop patterns

### 1. The hallucinated API
The agent invents a function or crate feature that does not exist.
- Symptom: code "should compile" but references symbols nobody defined.
- Rule: **never trust a symbol you didn't verify exists.** If unsure, run
  `cargo check` before committing, or open the crate docs.
- Counter-example done right: "I used `cgltf_load` because the
  [cgltf README](https://github.com/jkuhlmann/cgltf) shows it on line 12."

### 2. The unrun test
"It works" with no evidence.
- Rule: **paste the command and the output tail.** A green CI later is not a
  substitute; the agent must have run it locally.
- Forbidden phrases: "should pass", "I believe this works", "tests would pass".

### 3. The speculative abstraction
A trait, interface, or plugin system built "for the future" with one
implementation.
- Rule: **YAGNI.** Add the abstraction when there's a **second** concrete need.
  Until then, write the direct code.

### 4. The "while I was here" refactor
Untouched-but-changed code that wasn't in the task scope.
- Rule: **one concern per PR** (`AGENTS.md` §5). File a separate issue.

### 5. The dead branch
Commented-out code, `if false { }`, `// TODO: enable later`, unused feature
flags.
- Rule: **delete it.** Git remembers. `cargo fmt` + clippy `dead_code` enforced.

### 6. The silent failure
`.unwrap()`, `.expect("never")`, `let _ = result;`, `if let Ok(_) = ... {}`
that swallows an error.
- Rule: errors are returned with context; `unwrap`/`expect` are clippy-denied
  outside tests and process init (`AGENTS.md` §3).

### 7. The copy-paste twin
Two functions that differ only slightly, born because grepping was harder than
retyping.
- Rule: **grep before you write.** Duplication is a bug; extract or reuse.

### 8. The unused import / unused dep
`use foo::Bar;` left after a refactor; a crate pulled in for one constant.
- Rule: clippy `unused_imports` + `cargo machete` / `cargo udeps` in CI.

### 9. The growing file
A 900-line module "because it was easier."
- Rule: **500 LOC hard cap** (`AGENTS.md` §3). Split before crossing it.

### 10. The convention drift
The agent invents its own style instead of matching the file it's editing.
- Rule: **read the surrounding 100 lines before writing.** Match naming,
  error style, module layout, comment density.

### 11. The cargo-cult dependency
`Cargo.toml` accumulates crates "in case we need them."
- Rule: every new dependency requires an ADR (`AGENTS.md` §8). If it has zero
  `use` statements at the end of the PR, it gets removed.

### 12. The fake-progress commit
"Implemented feature X" — but the test is missing, the doc is missing, and the
feature isn't wired into the UI.
- Rule: **Definition of Done** (`AGENTS.md` §7) is a gate, not a wish list.

### 13. The over-eager "fix"
Tests fail → agent deletes the test, or disables the lint, or comments out the
  failing case.
- Rule: **never delete a test or baseline to go green.** Fix the code or
  escalate. (`AGENTS.md` §9.5)

### 14. The undocumented public surface
A new public type with no doc comment.
- Rule: `#![warn(missing_docs)]` is on; CI fails on undocumented public items.

### 15. The hand-wavy ADR-less decision
"We picked X" with no record of why, so nobody can audit or reverse it later.
- Rule: decisions in `AGENTS.md` §8 / `docs/adr/README.md` get an ADR.

## How to review for slop

When reviewing (yourself or another agent), walk this list explicitly. A good PR
review names which slop patterns were checked and found absent. A great review
cites the file:line that proves it.

## The deeper discipline

Slop is a **culture** problem, not a lint problem. The lints catch the symptoms;
the culture is: **evidence over assertion, less code over more, reading over
writing, scope over ambition.** When in doubt, do less and document why.
