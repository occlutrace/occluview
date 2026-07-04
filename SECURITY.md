# Security Policy

OccluView opens **untrusted files** from disk (STL/OBJ/PLY/glTF/3MF from email,
scanners, USB sticks). It also installs a **shell extension that loads into
`explorer.exe`'s surrogate**. Both make security a first-class concern, not an
afterthought.

## Scope

**In scope:**
- Memory-safety and parsing bugs in `occluview-formats` (the parsers are the
  primary attack surface).
- The COM shell extension: crashes, hangs, or code execution triggered by a
  malicious file viewed in Explorer.
- Privilege issues in the installer / file associations.
- Supply-chain issues in our dependencies (`cargo deny` advisories).

**Out of scope** (by design, for v1):
- The OccluTrace cloud service — that's a separate product, separate policy.
- Volumetric DICOM / CBCT — not supported (see `docs/ARCHITECTURE.md` non-goals).

## Reporting a vulnerability

**Do not open a public GitHub issue.**

Email **security@occlutrace.ai** with:
- A description of the issue and its impact.
- A minimal reproducer (a malformed file, a registry state, etc.).
- Affected versions / commits.

We acknowledge within **2 business days** and aim for a fix or mitigation within
**30 days** for high-severity issues. We will coordinate disclosure with you and
credit you in the advisory unless you prefer otherwise.

## Threat model (summary)

| Threat                                              | Mitigation                                                                       |
|-----------------------------------------------------|----------------------------------------------------------------------------------|
| Malformed file crashes the viewer                   | Parsers return `Result`, never panic; property/fuzz tests; panic-free `core`     |
| Malformed file crashes Explorer via the thumbnailer | Thumbnail provider runs **out-of-process** (`dllhost.exe`); watchdog timeout → placeholder |
| Path traversal / symlink tricks in file loading     | Canonicalize, reject symlinks outside the requested file; no shell-out           |
| Dependency CVE                                      | `cargo deny check advisories` in CI; renovate bot; pinned lockfile in VCS        |
| Unsigned DLL loading into Explorer                  | Release artifacts are **signed**; installer enforces signature                  |
| Stack overflow on deeply nested formats (glTF/3MF)  | Bounded recursion; explicit stack limits                                         |
| Zip-slip / arbitrary file write (glTF/3MF archives) | Extract to a constrained temp dir; reject absolute/`..` paths                    |

## Hardening commitments

- `occluview-core` is **panic-free** (clippy-enforced; `AGENTS.md` §3).
- `unsafe` is restricted to `occluview-shell` and `occluview-app`, each block
  justified with a `// SAFETY:` comment.
- The shell extension uses **out-of-process** activation by default (we do **not**
  set `DisableProcessIsolation`), so a bug cannot take down Explorer.
- All parsers run under `cargo fuzz` targets in CI (continuous fuzzing).
- Releases are built reproducibly from the tagged commit; binaries are signed.
