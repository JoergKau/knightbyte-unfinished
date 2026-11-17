//! Test-time verification of every FEN used by the benchmark suite.
//!
//! The project rules require FEN constants to be checked against an external
//! UCI verifier when the tests run. The verifier is supplied through
//! `KNIGHTBYTE_FEN_VERIFIER`; it is a local test tool and is not distributed
//! with the engine.

mod common;

use knightbyte::tools::bench::BENCH_POSITIONS;

#[test]
fn benchmark_fens_are_verified_at_test_time() {
    for fen in BENCH_POSITIONS {
        common::verify_fen(fen);
    }
}
