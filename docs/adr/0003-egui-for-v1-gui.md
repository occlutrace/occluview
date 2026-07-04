# ADR-0003: egui for the v1 GUI

- **Status:** Accepted
- **Date:** 2026-07-04

## Context

OccluView is a **viewer**, not an editor. The GUI chrome is minimal: a 3D
viewport filling the window, a thin toolbar (open, recent, units, view presets,
transparency, screenshot), a status bar (counts, units, bbox dimensions), and a
few overlays (scale bar, axis gizmo). We need a GUI approach that:

- Starts instantly (no retained-mode widget tree to build).
- Uses little RAM (no embedded browser, no large framework).
- Embeds naturally over a wgpu surface.
- Is easy to keep consistent under AI-assisted development.

Candidates:

- **egui** (immediate mode, wgpu backend) — tiny, fast, stateless, idiomatic
  Rust, MIT. Limited "native" look.
- **iced** — retained, Elm-like, wgpu backend, GPL-3.0 / Apache dual — strong but
  heavier and the immediate-mode fit for a viewport-overlay GUI is worse.
- **Slint** — declarative, custom DSL, wgpu backend — good, but a DSL adds a
  learning/build step and the permissive vs. GPL licensing choice needs care.
- **WinUI 3 / WPF** — native Windows look, but requires C#/.NET (rejected by
  ADR-0001) or awkward WinRT interop from Rust.
- **Qt** — mature, native-feeling, but LGPL/commercial licensing is a friction
  for a permissively-licensed open-core project, and it's heavy for a viewer.
- **Dear ImGui** — similar to egui, more C++-flavored; egui's Rust-first story is
  cleaner.

## Decision

Use **egui** (with `winit` + raw `wgpu`) for the v1 GUI. The 3D viewport owns the
window; egui renders chrome as an overlay.

## Consequences

**Positive**
- Cold start and idle RAM targets are easy to hit — egui has no persistent widget
  tree, no layout solver warmup, no signal/slot machinery.
- Stateless immediate mode is very easy to reason about under AI-assisted
  development (no hidden state to drift).
- Native wgpu integration; no second rendering stack.
- MIT license is fully compatible with our Apache-2.0 core.

**Negative**
- egui's default look is not "native Windows 11." For a dental lab tool this is
  acceptable for v1; we theme it to the OccluTrace dark palette. A native-look
  v2 is an open question.
- Complex widgets (rich tables, dock-style UI) are weaker; we don't need them
  for v1.
- No built-in accessibility (automation) to the level WinUI gives. Track for v2.

**We must now**
- Own a small theming layer so the app reads as "OccluView", not stock egui.
- Keep egui version pinned; it evolves quickly.

## Alternatives considered

- **iced.** A close second; preferred if we ever need a richer retained UI.
- **Slint.** Strong if we want a declarative DSL; rejected for v1 to avoid the
  extra build complexity.
- **WinUI 3 via C#.** Conflicts with ADR-0001.
- **Qt.** Licensing + weight disqualified it for a minimalist viewer.
