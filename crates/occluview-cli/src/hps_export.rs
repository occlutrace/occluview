//! PHI-safe dental HPS conversion command-line tool.
//!
//! The binary accepts `--input <FILE|->` and a mandatory
//! `--output-dir <DIR>`. It always emits geometry as `surface.ply`; textured
//! surfaces additionally emit a self-contained `surface.glb` preview. A
//! schema-v2 `manifest.json` hashes both artifacts when present.
//! Runtime HPS keys come only from the environment; command-line key material
//! is intentionally unsupported. Failures are sanitized JSON on stderr with
//! stable exit codes.

mod args;
mod convert;
mod error;
mod manifest;
mod output;

use self::args::ParseOutcome;
use self::error::CliError;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::process::ExitCode;

/// Run the standalone HPS export command and return its process exit code.
pub fn entrypoint() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    match run(arguments, &mut stdin, &mut stdout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error.write_json(&mut io::stderr().lock());
            ExitCode::from(error.exit_code())
        }
    }
}

fn run(
    arguments: Vec<OsString>,
    stdin: &mut impl Read,
    stdout: &mut impl Write,
) -> Result<(), CliError> {
    match args::parse(arguments)? {
        ParseOutcome::Convert(arguments) => convert::convert(arguments, stdin, stdout),
        ParseOutcome::Help => stdout
            .write_all(args::HELP.as_bytes())
            .map_err(|_| CliError::StdoutWriteFailed),
        ParseOutcome::Version => {
            writeln!(stdout, "occluview-hps-export {}", env!("CARGO_PKG_VERSION"))
                .map_err(|_| CliError::StdoutWriteFailed)
        }
    }
}
