# ADR-0008: Apache-2.0 license, open-core model

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

OccluView is backed by OccluTrace, Inc., a company that operates a proprietary
cloud alignment service. We want the viewer to be **widely adopted** (dental
labs, clinics, hobbyists, other vendors) while letting OccluTrace keep a
commercial offering. We also want to attract outside contributors.

Candidate licenses:

- **MIT** — maximally permissive, no explicit patent grant.
- **Apache-2.0** — permissive + explicit patent grant + trademark carve-out.
- **BSD-3** — permissive, similar to MIT.
- **MPL-2.0** — file-level copyleft; modifications to our files must stay open,
  but the license doesn't extend to combining code.
- **LGPL** — linking-library copyleft; awkward for a desktop app.
- **GPLv3 / AGPLv3** — strong copyleft; would prevent many dental companies from
  embedding/using OccluView, and conflicts with some distribution channels.

## Decision

License OccluView under **Apache-2.0**. Run an **open-core** model: the viewer
and its shell integration are open source; OccluTrace's cloud alignment service
and any proprietary connectors are separate products that depend on the public
`occluview-core` API.

## Consequences

**Positive**
- Permissive license removes adoption friction (vendors, labs, researchers).
- Explicit patent grant protects contributors and users.
- Trademark is reserved (`TRADEMARK.md`) so "OccluView" still means the official
  build — the standard open-core governance pattern.
- Compatible with our dependency choices (wgpu/egui/cgltf/fastgltf are MIT or
  Apache-2.0).

**Negative**
- A competitor could fork and host OccluView without contributing back. We accept
  this: network effects, the OccluTrace cloud service, and trademark are the moat,
  not license lock-in.
- Patent litigation termination clause (standard Apache) is a minor friction for
  some enterprises; it's the price of the grant.

**We must now**
- Keep `LICENSE` + `NOTICE` accurate; run `cargo deny check licenses` in CI.
- Use **DCO** (not CLA) for contributions — lower friction, sufficient for the
  open-core model. See `CONTRIBUTING.md`.
- Mark OccluTrace-proprietary crates (none in this repo) clearly if added later.

## Alternatives considered

- **MIT.** Fine, but Apache-2.0's patent grant is worth the negligible extra
  text.
- **MPL-2.0.** Strong contender; rejected only because we want maximum adoption
  for the viewer itself.
- **AGPLv3.** Would protect against SaaS re-hosting, but at the cost of scaring
  off the dental-industry users who are our primary audience.
