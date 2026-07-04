//! Shell-extension error type.

use thiserror::Error;

/// Errors raised by the shell extension. These always result in a **placeholder
/// thumbnail** being returned to Windows, never a propagated crash
/// (`docs/SHELL_INTEGRATION.md` §2).
#[derive(Debug, Error)]
pub enum ShellError {
    /// The file did not match any supported format.
    #[error(transparent)]
    Format(#[from] occluview_formats::FormatError),

    /// The renderer failed (no adapter, shader error, watchdog timeout).
    #[error(transparent)]
    Render(#[from] occluview_render::RenderError),

    /// A Windows API call failed inside the COM layer.
    #[error("win32 error: {0}")]
    Win32(String),
}
