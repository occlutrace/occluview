//! `occluview-render` — the wgpu renderer (ADR-0002).
//!
//! Two consumers share this code: the live GUI (`occluview-app`) and the
//! offscreen thumbnail renderer (`occluview-shell`). Keeping one pipeline means
//! the Explorer thumbnail is pixel-identical to the in-app frame.
//!
//! ## Status
//!
//! This is a stub. The pipeline (WGSL shaders, vertex/index upload, occlusal
//! framing, offscreen render-to-texture, WARP fallback) is implemented in
//! dedicated PRs per the roadmap, each guarded by golden-image tests
//! (`docs/TESTING.md`).

#![forbid(unsafe_code)]

pub mod error;
pub mod offscreen;

pub use error::RenderError;
pub use offscreen::ThumbnailSpec;

/// Placeholder for the not-yet-implemented live renderer.
///
/// Exists so dependents (`app`, `shell`) can code against a stable module shape
/// before the GPU code lands. Implementing this is P0 on the roadmap.
#[derive(Debug)]
pub struct Renderer;

impl Renderer {
    /// Construct a placeholder. Real implementation takes a `wgpu::Surface` /
    /// `wgpu::Device` and a config.
    #[must_use]
    #[inline]
    pub const fn new_placeholder() -> Self {
        Self
    }
}
