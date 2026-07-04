//! `occluview-cli` - headless CLI.
//!
//! Subcommands (planned):
//!   - `thumbnail <file> -o out.png --size 256`  - render a thumbnail, same path
//!     as the Explorer shell extension (`occluview-shell`). Useful for debugging
//!     the shell path *without* Explorer.
//!   - `info <file>`         - print format, vertex/triangle counts, bbox dims.
//!   - `convert <in> <out>`  - re-encode between supported formats (later).
//!
//! ## Status
//!
//! Foundation only: argument parsing and a friendly "not implemented" for each
//! subcommand. Real implementations land alongside the loaders/renderer.

// CLI tool: stdout/stderr is the entire point.
#![allow(clippy::print_stdout, clippy::print_stderr)]
//!     the shell path *without* Explorer.
//!   - `info <file>`         — print format, vertex/triangle counts, bbox dims.
//!   - `convert <in> <out>`  — re-encode between supported formats (later).
//!
//! ## Status
//!
//! Foundation only: argument parsing and a friendly "not implemented" for each
//! subcommand. Real implementations land alongside the loaders/renderer.

use anyhow::{bail, Result};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_target(false)
        .compact()
        .init();

    let mut args = std::env::args().skip(1);
    let Some(subcommand) = args.next() else {
        print_usage();
        bail!("missing subcommand");
    };

    match subcommand.as_str() {
        "thumbnail" => {
            let file: PathBuf = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("thumbnail: missing <file> argument"))?
                .into();
            tracing::info!(?file, "thumbnail subcommand");
            eprintln!(
                "occluview-cli thumbnail: not yet implemented ({}) — see ROADMAP.md",
                file.display()
            );
        }
        "info" => {
            let file: PathBuf = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("info: missing <file> argument"))?
                .into();
            tracing::info!(?file, "info subcommand");
            eprintln!(
                "occluview-cli info: not yet implemented ({}) — see ROADMAP.md",
                file.display()
            );
        }
        "help" | "--help" | "-h" => print_usage(),
        other => {
            print_usage();
            bail!("unknown subcommand: {other}");
        }
    }
    Ok(())
}

fn print_usage() {
    eprintln!(
        "occluview-cli — headless OccluView\n\
         \n\
         USAGE:\n    \
         occluview-cli <SUBCOMMAND> [ARGS]\n\
         \n\
         SUBCOMMANDS:\n    \
         thumbnail <file> -o <out.png> --size <px>   Render a thumbnail (same path as the Explorer extension)\n    \
         info      <file>                          Print format / counts / bbox\n    \
         help                                       Show this message\n\
         \n\
         See ROADMAP.md for implementation status."
    );
}
