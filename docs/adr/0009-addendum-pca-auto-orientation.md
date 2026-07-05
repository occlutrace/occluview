# ADR-0009 Addendum: PCA auto-orientation

- **Status:** Accepted
- **Date:** 2026-07-04
- **Supersedes:** none (extends ADR-0009's occlusal-view framing consequence)

## Context

ADR-0009's "Negative consequences" called out the open work: occlusal framing
requires a robust up/right heuristic on arbitrary meshes, and named PCA on the
largest planar feature as the approach. Scanners export meshes in whatever frame
their sensor used — the thin (occlusal-normal) axis is not reliably `+Y`. The
naive `frame_occlusal` (assume Y-up, fit to bbox) produced wrong-angle thumbnails
for any file not already in canonical orientation, and — compounded with a
separate index-parsing bug — entirely empty renders for real GLB files.

## Decision

Implement PCA-based auto-orientation in `occluview-core::pca`, folded into the
thumbnail (and, later, the app's initial framing):

1. **Solver.** `symmetric_eig` — cyclic Jacobi eigendecomposition of a symmetric
   3×3. Chosen over pulling in a linear-algebra crate (nalgebra/ndarray) because
   the 3×3 case is tiny, the algorithm is ~80 lines, deterministic, robust to
   repeated eigenvalues (planar meshes, point clouds, collinear inputs), and
   keeps the dependency surface flat. No `unsafe`.
2. **Orientation.** `principal_axes` computes the vertex covariance, decomposes
   it, and returns a right-handed rotation `R` into OccluView's canonical dental
   frame:
   - `+Y` ← smallest-variance eigenvector (the thinnest direction = occlusal
     normal),
   - `+X` ← largest-variance eigenvector (the arch's long / mesial-distal axis),
   - `+Z` ← middle eigenvector (buccal-lingual).
3. **Application.** The thumbnail composes `R` into the **camera view matrix**
   (`view = view_local * R`) rather than rewriting every vertex — a mesh-wide
   rotation costs one matrix multiply, vs. touching every position/normal. The
   camera is framed against the bbox rotated into the canonical frame (8-corner
   re-enclosure) so distance/near/far fit the rotated extents.
4. **Sampling.** PCA is a global second-moment statistic; a uniform-stride sample
   of up to 4096 vertices is statistically sufficient and bounds cost for
   million-vertex scans.

## Consequences

**Positive**
- Thumbnails are correctly occlusal regardless of source orientation — the
  single most visible "this is a dental tool" signal from ADR-0009 now holds for
  every file, not just pre-aligned ones.
- No per-vertex rewrite; the rotation lives in the camera, so load cost is
  unchanged.
- Pure-math module, fully unit-tested on Linux with no GPU or Windows needed —
  fits the "Linux-testable progress" constraint while the COM/app layers wait on
  a Windows host.

**Negative**
- PCA is a second-moment method: it orients to the *dominant* thin direction,
  which for a single arch is the occlusal normal, but for unusual geometries
  (e.g. a scan with a long attached handle) the dominant axes may not match
  dental anatomy. Mitigation: this is an *initial* framing; the app exposes
  manual orbit (ADR-0003) and the thumbnail only needs to be recognizable, not
  diagnostic-grade.
- Sign ambiguity: eigenvectors are defined up to sign, so a mesh may render
  mirrored. For thumbnails this is immaterial; for the app, a later "flip" control
  can address it if real files need it.

**We must now**
- Expose `principal_axes` to the app's initial framing (currently only the
  thumbnail uses it — the app still assumes Y-up on load).
- Revisit if a dental-specific heuristic (e.g. fit an arch curve, pick the normal
  of its best-fit plane) beats raw PCA for the hard cases. Tracked as a follow-up
  if real corpus files show poor PCA framing.

## Alternatives considered

- **Pull in nalgebra / ndarray for SVD.** Rejected — 3×3 Jacobi is small enough
  to vendor, avoids a heavy dep, and SVD's extra generality isn't needed for a
  symmetric covariance matrix.
- **Rewrite vertex positions on load.** Rejected — strictly more work than a
  camera-frame rotation, and it would diverge the in-memory mesh from the file's
  native frame (bad for any future "export transformed" feature).
- **Per-format up-vector conventions.** Rejected as the primary mechanism — file
  headers are unreliable across the corpus (mislabeled, missing). PCA on geometry
  is format-agnostic. Per-format hints remain a possible *fallback* for files
  where PCA fails.
