//! KnightByte UCI entry point.

use std::process::ExitCode;

/// Runs the UCI event loop and maps I/O failures to a non-zero exit code.
///
/// The process arguments reproduce the reference's startup surface:
/// `param:Name=Value` entries preload validated search-parameter overrides, and
/// any other argument becomes a one-shot UCI command line that runs once and
/// exits. With no arguments the engine reads standard input interactively.
fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match knightbyte::uci::run_with_argv(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fatal: {error}");
            ExitCode::FAILURE
        }
    }
}
