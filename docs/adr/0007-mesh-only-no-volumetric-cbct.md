# ADR-0007: Mesh-only — no volumetric CBCT/DICOM in v1

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

Dental imaging includes both **surface meshes** (intraoral scans, exported
CBCT surfaces, CAD models — STL/PLY/OBJ/glTF/3MF) and **volumetric data**
(CBCT DICOM series — dense voxel volumes used for diagnosis and implant
planning). A viewer that handles both is a much larger product: volumetric
rendering, MPR slicing, windowing/leveling, and—critically—**medical-device
regulatory exposure** (FDA SaMD, EU MDR).

## Decision

OccluView v1 is **mesh-only**. We do not load volumetric DICOM. If a DICOM file
is a *mesh-exported* file (some scanners export surface models as `.dcm`), we may
detect and surface that as an "unsupported, please export to STL/PLY" message,
but we do not implement DICOM parsing for v1.

## Consequences

**Positive**
- Keeps OccluView clearly on the **non-medical / CAD-preview** side of the
  regulatory line (see `docs/GLOSSARY.md` → "Regulatory scope"). This is a
  product-survival decision, not a laziness one.
- Focuses v1 on what OccluTrace does best: surface meshes for occlusal alignment.
- Avoids the heavy dependencies (DCMTK / DICOM parsers) and their CVE surface.

**Negative**
- Some dental users will ask for CBCT. We say no for v1, with a clear path: they
  export a surface model from their CBCT software and open that in OccluView.
- We must communicate this scope clearly in the UI (friendly "unsupported"
  message) so users don't think it's a bug.

**We must now**
- Add an explicit non-goal in `docs/ARCHITECTURE.md` (done).
- Provide a clean error path for unsupported / volumetric DICOM files.
- Revisit as a separate ADR if/when volumetric support becomes a product goal
  (it would imply a regulatory workstream).

## Alternatives considered

- **Support DICOM mesh-export only.** Still needs DICOM parsing; rejected for v1
  on complexity + regulatory grounds. Detect-and-instruct is the v1 behavior.
- **Full volumetric support.** Wrong product for now.
