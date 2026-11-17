//! Shared integration-test helpers.
//!
//! The external verifier is a local UCI engine supplied through
//! `KNIGHTBYTE_FEN_VERIFIER`. It is used at test time to verify every FEN and
//! to cross-check perft counts.

#![allow(dead_code)] // Each integration-test target compiles the whole helper module.

use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Returns the configured external verifier binary.
pub fn verifier() -> PathBuf {
    env::var_os("KNIGHTBYTE_FEN_VERIFIER")
        .map(PathBuf::from)
        .expect(
            "set KNIGHTBYTE_FEN_VERIFIER to a UCI verifier binary before running the tests \
             (see project-rules.md)",
        )
}

fn run(commands: &str, fen: &str) -> String {
    let mut child = Command::new(verifier())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot start verifier for {fen}: {error}"));

    child
        .stdin
        .as_mut()
        .expect("verifier stdin")
        .write_all(commands.as_bytes())
        .expect("write verifier commands");

    let output = child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("verifier failed for {fen}: {error}"));
    assert!(
        output.status.success(),
        "verifier rejected {fen}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Verifies that a FEN is legal and round-trips through the external verifier.
pub fn verify_fen(fen: &str) {
    let stdout = run(
        &format!("uci\nisready\nposition fen {fen}\nd\ngo depth 1\nquit\n"),
        fen,
    );
    assert!(
        !stdout.contains("Illegal"),
        "verifier reported an illegal position for {fen}"
    );
    assert!(
        stdout.contains("bestmove"),
        "verifier produced no bestmove for {fen}"
    );
    assert!(
        !stdout.contains("bestmove (none)"),
        "verifier found no legal move for {fen}"
    );

    let reported = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Fen: "))
        .unwrap_or_else(|| panic!("verifier did not report a FEN for {fen}"));
    let input_fields: Vec<&str> = fen.split_whitespace().take(4).collect();
    let reported_fields: Vec<&str> = reported.split_whitespace().take(4).collect();
    assert_eq!(
        input_fields, reported_fields,
        "FEN did not round-trip through the verifier: {fen}"
    );
}

/// Returns the external verifier's perft count for a FEN and depth.
pub fn verifier_perft(fen: &str, depth: u32) -> u64 {
    let stdout = run(
        &format!("position fen {fen}\ngo perft {depth}\nquit\n"),
        fen,
    );
    let value = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Nodes searched:"))
        .unwrap_or_else(|| panic!("verifier produced no perft count for {fen}"));
    value
        .trim()
        .parse()
        .unwrap_or_else(|error| panic!("invalid verifier perft count '{value}' for {fen}: {error}"))
}
