# ADR-0009: Dental defaults — mm units, occlusal camera, Y-up

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

Generic 3D viewers assume game-engine conventions: meter units, Y-up,
left-handed, "fit to view" from a default iso angle. None of these fit dental
practice:

- Dental scans are in **millimeters** (sometimes unit-less, but the numbers are
  mm). Showing "0.0008 m" instead of "0.08 mm" erodes trust instantly.
- The diagnostically useful orientation is the **occlusal view** — looking down
  onto the chewing surface — not a 3/4 iso angle.
- Dental scanners emit a mix of handedness and up-vectors; we must normalize.
- Upper and lower arches are a **pair**, conceptually and in OccluTrace's
  alignment workflow.

## Decision

Encode dental defaults in `occluview-core`, applied identically by the app and
the thumbnail renderer:

- **Length unit:** millimeter. Convert on load if the format declares units;
  otherwise assume mm and surface it in the UI ("assumed mm").
- **Internal coordinate frame:** right-handed, **Y-up**. Convert on load per
  format; never assume. Document each format's convention in
  `docs/FORMAT_SUPPORT.md`.
- **Default camera:** **occlusal view** — view direction along the arch's occlusal
  normal, fit-to-bbox, with the mesial-distal axis horizontal. Used by both the
  app's initial framing and the thumbnail.
- **Multi-mesh scenes:** first-class. Upper/lower arches loaded as two meshes
  with independent transforms and colors (e.g. cool/warm tint by default).
- **Scale bar:** on by default, in mm; toggleable.
- **Background:** neutral dark (`#0a0a0a`, OccluTrace brand) for the app;
  thumbnails render on a neutral background suitable for Explorer (light/dark
  aware where feasible).

## Consequences

**Positive**
- The viewer feels native to dental users from the first frame.
- One framing code path serves app + thumbnail — consistency by construction.
- Brand coherence with OccluTrace.

**Negative**
- Non-dental users (e.g. a 3D-printing hobbyist opening an STL) get a dental
  default they may not expect. Mitigation: a "generic" view preset is one click
  away, and remembered per file type.
- We must implement occlusal-view framing, which requires a robust up/right
  heuristic on arbitrary meshes (PCA on the largest planar feature). This is a
  small but real piece of work; tracked as an issue.

**We must now**
- Implement the units type (`Millimeters(f32)`) in `occluview-core`.
- Implement occlusal-view framing (heuristic + manual override).
- Per-format coordinate conversion table in `docs/FORMAT_SUPPORT.md`.

## Alternatives considered

- **Generic iso-view default.** Rejected — actively unhelpful for dental.
- **Detect modality and pick a default.** Too fragile for v1; we go dental-first
  with a one-click generic preset.
