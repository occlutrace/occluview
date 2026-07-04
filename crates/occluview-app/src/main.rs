//! `occluview-app` — the desktop viewer binary.
//!
//! A thin egui shell around an `occluview-render` viewport. The design (ADR-0003)
//! is "viewport fills the window; chrome overlays it": a toolbar, a status bar,
//! a scale bar, and an axis gizmo. Cold start target < 400 ms; idle < 120 MB
//! (`docs/ENGINEERING.md` §6).
//!
//! ## Status
//!
//! Foundation only. Opening a file from the CLI arg, the egui chrome, and the
//! wgpu viewport are implemented in follow-up PRs. Today this binary parses its
//! args and shows a placeholder window, so the cold-start measurement harness
//! has something to time.

use anyhow::Result;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

/// Parsed command-line arguments.
#[derive(Debug, Clone)]
struct Args {
    /// Optional file to open on startup.
    file: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut args = pico_args::lite::Arguments::from_env_skip_first();
    let file = args.opt_free_from_str::<PathBuf>().ok().flatten();
    Args { file }
}

fn main() -> Result<()> {
    // Initialize logging. `RUST_LOG=occluview=info` for normal use.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let args = parse_args();
    tracing::info!(
        "OccluView starting (target file: {:?})",
        args.file.as_ref().and_then(|p| p.to_str())
    );

    // Foundation: the egui window is wired in a follow-up PR. We exit cleanly
    // today with a clear "not yet implemented" message rather than fake a UI.
    eprintln!(
        "occluview: GUI not yet implemented (see ROADMAP.md). \
         Foundation + governance are in place; the viewer lands next."
    );
    Ok(())
}

/// Minimal argument parsing without an extra dependency at the workspace root.
///
/// We avoid pulling `clap` to keep cold-start binary size small for v1; if the
/// flag set grows beyond a handful, reconsider (and file an ADR).
mod pico_args {
    /// Trivially small `pico-args`-style parser. Replace with the real crate
    /// once we have ≥3 flags (then it's worth the ADR + dependency).
    pub mod lite {
        use std::env;

        /// An iterator over args, skipping argv[0].
        pub struct Arguments {
            inner: Vec<String>,
            cursor: usize,
        }

        impl Arguments {
            /// Construct from `env::args()`, skipping the program name.
            #[must_use]
            pub fn from_env_skip_first() -> Self {
                Self {
                    inner: env::args().skip(1).collect(),
                    cursor: 0,
                }
            }

            /// Take the next positional argument, parsing it as `T`.
            pub fn opt_free_from_str<T>(&mut self) -> std::result::Result<Option<T>, ()>
            where
                T: std::str::FromStr,
            {
                if self.cursor >= self.inner.len() {
                    return Ok(None);
                }
                let s = &self.inner[self.cursor];
                T::from_str(s).map(Some).map_err(|_| ())
            }
        }
    }
}
