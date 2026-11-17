//! Deterministic benchmark.
//!
//! The benchmark searches a fixed position set to a fixed depth with a freshly
//! cleared table for every position. Nodes are counted, not timed, so the total
//! is reproducible across runs and machines — which is what makes it usable as
//! a regression gate. Wall-clock time and nodes per second are printed for
//! information only and never compared between hosts.
//!
//! Note on the search-parity gate: the recorded reference totals came from the
//! predecessor engine, and matching them exactly requires porting its remaining
//! search formulas. Until that is done, this command prints the current totals
//! and the parity check stays open.

use std::io;
use std::time::Instant;

use crate::board::position::Position;
use crate::output::Output;
use crate::search::limits::Limits;
use crate::search::searcher::Searcher;

/// The four fixed positions used by the deterministic benchmark.
///
/// They span the opening, a tactical middlegame, an endgame and a second
/// tactical middlegame so a single run exercises different tree shapes.
pub const BENCH_POSITIONS: [&str; 4] = [
    // Start position.
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    // Dense middlegame with castling rights on both sides.
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    // Rook-and-pawn endgame.
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    // Tactical middlegame with an exposed king.
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
];

/// Depth used when the command does not name one.
pub const DEFAULT_BENCH_DEPTH: u32 = 12;

/// Transposition-table size used by the benchmark, in megabytes.
///
/// The recorded identity protocol uses a 256 MB hash with one thread, so the
/// benchmark must allocate the same table size to be comparable at all.
pub const BENCH_TT_MEGABYTES: usize = 256;

/// Runs `bench [depth]`.
///
/// # Errors
///
/// Returns an I/O error if the output stream cannot be written.
pub fn run(command: &str, output: &mut Output) -> io::Result<()> {
    let depth = command
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_BENCH_DEPTH);

    let mut searcher = Searcher::new(BENCH_TT_MEGABYTES);
    let started = Instant::now();
    let mut total_nodes = 0_u64;

    output.line(&format!(
        "info string bench positions {} depth {depth}",
        BENCH_POSITIONS.len()
    ))?;

    for (index, fen) in BENCH_POSITIONS.iter().enumerate() {
        let position = Position::from_fen(fen)
            .map_err(|error| io::Error::other(format!("bench position {}: {error}", index + 1)))?;
        searcher.clear();
        let result = searcher.search(&position, Limits::depth(depth), &mut |_| {});
        total_nodes += result.nodes;
        output.line(&format!(
            "info string bench position {} nodes {} bestmove {}",
            index + 1,
            result.nodes,
            result.best_move.to_uci()
        ))?;
    }

    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis().max(1);
    let nps = total_nodes.saturating_mul(1_000) / elapsed_ms as u64;

    output.line(&format!("Nodes: {total_nodes}"))?;
    output.line(&format!("Time: {elapsed_ms} ms"))?;
    output.line(&format!("NPS: {nps}"))?;
    Ok(())
}

/// Returns the total node count of a benchmark run at `depth`.
///
/// This is the value the regression gate compares; it never depends on the
/// clock, the host or the table size after [`Searcher::clear`].
pub fn total_nodes(depth: u32) -> u64 {
    let mut searcher = Searcher::new(BENCH_TT_MEGABYTES);
    let mut total = 0;
    for fen in BENCH_POSITIONS {
        let Ok(position) = Position::from_fen(fen) else {
            return total;
        };
        searcher.clear();
        total += searcher
            .search(&position, Limits::depth(depth), &mut |_| {})
            .nodes;
    }
    total
}
