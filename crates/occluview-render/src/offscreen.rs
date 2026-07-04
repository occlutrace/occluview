//! Offscreen (headless) rendering — used by the thumbnail provider.
//!
//! The thumbnail path renders a mesh to a texture at a requested size using the
//! same shader pipeline as the live renderer, then returns pixels the shell
//! extension hands to Windows as an `HBITMAP`/`IWICBitmap`. A watchdog bounds
//! the render time; on timeout or any error the caller falls back to a branded
//! placeholder (`docs/SHELL_INTEGRATION.md`).

use crate::RenderError;

/// Parameters for an offscreen thumbnail render.
#[derive(Copy, Clone, Debug)]
pub struct ThumbnailSpec {
    /// Square output dimension in pixels (Explorer requests 32/96/256/1024).
    pub size_px: u16,
    /// Render-time budget in milliseconds before the watchdog fires.
    pub timeout_ms: u32,
    /// Allow software (WARP) fallback when no GPU is available.
    pub allow_software_fallback: bool,
}

impl Default for ThumbnailSpec {
    fn default() -> Self {
        Self {
            size_px: 256,
            timeout_ms: 400,
            allow_software_fallback: true,
        }
    }
}

/// Offscreen renderer. Real implementation lives in a follow-up PR.
#[derive(Debug)]
pub struct Offscreen;

impl Offscreen {
    /// Render a mesh to an RGBA8 buffer per `spec`.
    ///
    /// # Errors
    /// - [`RenderError::Timeout`] if the watchdog fires.
    /// - Other variants for adapter/shader/surface failures.
    ///
    /// Stub returns `RenderError::Timeout` so callers wire the fallback path
    /// first; the implementation replaces this with the real pipeline.
    pub fn render(
        &self,
        _mesh: &occluview_core::Mesh,
        spec: ThumbnailSpec,
    ) -> Result<Vec<u8>, RenderError> {
        let _ = spec; // used by the real implementation.
        Err(RenderError::Timeout { ms: 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spec_matches_explorer_expectations() {
        let s = ThumbnailSpec::default();
        assert_eq!(s.size_px, 256);
        assert!(s.allow_software_fallback);
    }

    #[test]
    fn stub_render_returns_timeout_so_fallback_wires_first() {
        // Per AGENTS.md §9.5, we never silently fake success. The stub returns a
        // typed error until the real implementation lands.
        use occluview_core::{Mesh, MeshBuilder};
        let mesh = MeshBuilder::new().build().unwrap_or(
            Mesh::new(None, Vec::new(), Vec::new()).unwrap(),
        );
        let res = Offscreen.render(&mesh, ThumbnailSpec::default());
        assert!(matches!(res, Err(RenderError::Timeout { .. })));
    }
}
