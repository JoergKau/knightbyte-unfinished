//! Performance decomposition microbench (Phase 7 preparation).
//!
//! The engine is at ~0.37 MNPS against the reference's ~1.4 MNPS (single
//! thread, same host). The documented causes are (1) the full-refresh NNUE
//! evaluation per node, (2) copy-make per node, (3) direction-walking slider
//! attacks, (4) a generic (non-`target-cpu`) release build. This bench
//! measures the raw building blocks in isolation — evaluation, move
//! generation, make-move — so the optimisation order follows data, not
//! guesswork.
//!
//! Run it in release mode: `cargo test --release --test perf_microbench -- --nocapture`

use std::hint::black_box;
use std::time::Instant;

use knightbyte::board::movegen::legal;
use knightbyte::board::position::Position;
use knightbyte::eval::nnue::evaluate_position;
use knightbyte::tools::bench::BENCH_POSITIONS;

/// The four benchmark positions, parsed once.
fn positions() -> Vec<Position> {
    BENCH_POSITIONS
        .iter()
        .map(|fen| Position::from_fen(fen).expect("a bench FEN must parse"))
        .collect()
}

/// Runs `f` for at least `seconds` and returns the operations per second.
///
/// `f` must perform ONE operation per call and report a value to sink; every
/// result is fed through [`black_box`] because the measured functions are
/// pure — a discarded result lets LLVM delete the work entirely (the first
/// version of this bench additionally counted 256 operations per call and
/// reported 256x the real rate; both bugs are guarded against now).
fn throughput<F: FnMut(usize) -> u64>(seconds: u64, mut f: F) -> f64 {
    let started = Instant::now();
    let mut operations = 0usize;
    let mut sink = 0u64;
    while started.elapsed().as_secs() < seconds {
        sink = sink.wrapping_add(f(operations));
        black_box(sink);
        operations += 1;
    }
    operations as f64 / started.elapsed().as_secs_f64()
}

#[test]
fn the_building_blocks_throughput() {
    // Debug builds are ~100x slower and would only produce noise; the
    // numbers are only meaningful in release mode.
    if cfg!(debug_assertions) {
        return;
    }
    let positions = positions();

    // 1. Full-refresh NNUE evaluation per node (the current search's cost).
    let eval_per_second = throughput(2, |i| {
        black_box(evaluate_position(black_box(
            &positions[i % positions.len()],
        ))) as u64
    });

    // 2. Legal move generation.
    let movegen_per_second = throughput(2, |i| {
        let moves = legal(black_box(&positions[i % positions.len()]));
        moves.as_slice().len() as u64
    });

    // 3. Copy-make of one move per position.
    let make_per_second = throughput(2, |i| {
        let position = &positions[i % positions.len()];
        let moves = legal(position);
        match moves.as_slice().first() {
            Some(&mv) => {
                let child = position.make_move(mv);
                child.key()
            }
            None => 0,
        }
    });

    println!("=== performance decomposition (release) ===");
    println!("evaluate (full refresh): {:>12.0} /s", eval_per_second);
    println!("legal move generation:   {:>12.0} /s", movegen_per_second);
    println!("copy-make (one move):    {:>12.0} /s", make_per_second);
    println!();
    println!("search runs at ~370,000 nodes/s; one evaluation per node.");
    println!(
        "eval share of the search budget: ~{:.0}%",
        370_000.0 / eval_per_second * 100.0
    );
}
