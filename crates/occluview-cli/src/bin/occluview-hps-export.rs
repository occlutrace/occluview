//! Standalone HPS package export binary backed by the shared CLI package.

fn main() -> std::process::ExitCode {
    occluview_cli::hps_export::entrypoint()
}
