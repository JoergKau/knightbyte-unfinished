//! KnightByte UCI entry point.

use std::process::ExitCode;

/// Runs the UCI event loop and maps I/O failures to a non-zero exit code.
fn main() -> ExitCode {
    match knightbyte::uci::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fatal: {error}");
            ExitCode::FAILURE
        }
    }
}
