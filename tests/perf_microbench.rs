//! Performance decomposition microbench (Phase 7 / performance plan P0).
//!
//! Measures the raw building blocks in isolation so the optimisation order
//! follows data, not guesswork. The blocks are reported **separately**:
//!
//! * a full NNUE refresh (reusable evaluator, no per-call allocation),
//! * pseudo-legal move generation,
//! * legal move generation (pseudo generation plus the per-move filter),
//! * one copy-make on a precomputed move list (no move generation in the loop),
//! * the incremental evaluation path (accumulator push plus the output head),
//! * a real depth-12 search over the four bench positions.
//!
//! The difference between the legal and the make rows is the marginal cost of a
//! single `make_move`; the legal row is the cost of the filter itself. Earlier
//! versions of this bench measured `legal() + make` in the "copy-make" row,
//! which made the make look far more expensive than it is.
//!
//! Run it in release mode:
//! `cargo test --release --test perf_microbench -- --nocapture`

use std::hint::black_box;
use std::time::Instant;

use knightbyte::board::movegen::{is_capture, legal, pseudo_legal};
use knightbyte::board::position::Position;
use knightbyte::eval::nnue::evaluate::{Evaluator, embedded_net};
use knightbyte::search::limits::Limits;
use knightbyte::search::searcher::Searcher;
use knightbyte::search::see::see_ge;
use knightbyte::tools::bench::{BENCH_POSITIONS, BENCH_TT_MEGABYTES};
use knightbyte::types::r#move::Move;

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

    // 1. Full-refresh NNUE evaluation with the reusable evaluator (the
    //    search itself no longer pays this per node; it is the upper bound).
    let mut evaluator = Evaluator::new(embedded_net());
    let eval_per_second = throughput(2, |i| {
        black_box(evaluator.evaluate(black_box(&positions[i % positions.len()]))) as u64
    });

    // 2. Pseudo-legal move generation alone.
    let pseudo_per_second = throughput(2, |i| {
        pseudo_legal(black_box(&positions[i % positions.len()]))
            .as_slice()
            .len() as u64
    });

    // 3. Legal move generation (pseudo generation plus the make/check filter).
    let movegen_per_second = throughput(2, |i| {
        legal(black_box(&positions[i % positions.len()]))
            .as_slice()
            .len() as u64
    });

    // 4. One copy-make over a precomputed move list: no `legal()` in the
    //    loop, so the row is the make alone. The move is legal in its
    //    position because it came from the legal list.
    let first_moves: Vec<Option<Move>> = positions
        .iter()
        .map(|position| legal(position).as_slice().first().copied())
        .collect();
    let make_per_second = throughput(2, |i| {
        let index = i % positions.len();
        match first_moves[index] {
            Some(mv) => black_box(&positions[index]).make_move(mv).key(),
            None => 0,
        }
    });

    // 5. SEE pruning form on captures. The node-stats run shows about one
    //    SEE call per search node, so this rate is part of the composition.
    let see_moves: Vec<Vec<Move>> = positions
        .iter()
        .map(|position| {
            let legal_moves = legal(position);
            let captures: Vec<Move> = legal_moves
                .as_slice()
                .iter()
                .copied()
                .filter(|mv| is_capture(position, *mv))
                .collect();
            if captures.is_empty() {
                legal_moves.as_slice().to_vec()
            } else {
                captures
            }
        })
        .collect();
    let see_per_second = throughput(2, |i| {
        let index = i % positions.len();
        let moves = &see_moves[index];
        let mv = moves[i % moves.len()];
        black_box(see_ge(&positions[index], mv, 0)) as u64
    });

    // 6. The incremental evaluation path: push the child accumulator pair
    //    from a prepared parent, then run the output head. One searcher per
    //    position keeps the parent pair valid without a full refresh in the
    //    timed loop.
    let mut incremental: Vec<Searcher> = positions.iter().map(|_| Searcher::new(1)).collect();
    for (searcher, position) in incremental.iter_mut().zip(positions.iter()) {
        searcher.search(position, Limits::depth(1), &mut |_| {});
    }
    let incremental_per_second = throughput(2, |i| {
        let index = i % positions.len();
        let Some(mv) = first_moves[index] else {
            return 0;
        };
        black_box(incremental[index].bench_incremental_eval(&positions[index], mv)) as u64
    });

    // 7. A real depth-12 search per bench position (the bench convention:
    //    freshly cleared table per position). This is the honest nodes-per-
    //    second base for the eval-share figure below.
    let mut bench_searcher = Searcher::new(BENCH_TT_MEGABYTES);
    let search_started = Instant::now();
    let mut search_nodes = 0_u64;
    for fen in BENCH_POSITIONS {
        let position = Position::from_fen(fen).expect("a bench FEN must parse");
        bench_searcher.clear();
        let result = bench_searcher.search(&position, Limits::depth(12), &mut |_| {});
        search_nodes += result.nodes;
    }
    let search_per_second = search_nodes as f64 / search_started.elapsed().as_secs_f64();

    println!("=== performance decomposition (release) ===");
    println!("evaluate (full refresh):     {:>12.0} /s", eval_per_second);
    println!(
        "pseudo-legal generation:     {:>12.0} /s",
        pseudo_per_second
    );
    println!(
        "legal move generation:       {:>12.0} /s",
        movegen_per_second
    );
    println!("copy-make (one move):        {:>12.0} /s", make_per_second);
    println!("see_ge (capture):            {:>12.0} /s", see_per_second);
    println!(
        "incremental push + head:     {:>12.0} /s",
        incremental_per_second
    );
    println!(
        "search (bench 12):           {:>12.0} nodes/s",
        search_per_second
    );
    println!(
        "one copy-make:               {:>12.1} ns",
        1e9 / make_per_second
    );
    println!(
        "one legal move generation:   {:>12.1} ns",
        1e9 / movegen_per_second
    );
    println!(
        "eval share of the search budget (full-refresh upper bound): ~{:.0}%",
        eval_per_second / search_per_second * 100.0
    );
}
