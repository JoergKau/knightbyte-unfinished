//! Search correctness gates.
//!
//! These tests exercise the negamax driver, quiescence and the real `bench`
//! command. Every FEN is checked by the external verifier at test time, as the
//! project rules require. The mate lines are short enough to confirm by hand:
//! `a1a8+` forces `h8h7`, then `b2b7#`.

mod common;

use knightbyte::board::movegen::legal;
use knightbyte::board::position::Position;
use knightbyte::board::zobrist;
use knightbyte::search::limits::Limits;
use knightbyte::search::searcher::{MATE_LIMIT, SearchResult, Searcher};
use knightbyte::tools::bench;
use std::time::{Duration, Instant};

const SCHOLARS_MATE: &str = "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4";
const MATE_IN_TWO: &str = "7k/8/8/6K1/8/8/1R6/R7 w - - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Parses `fen` after checking it against the external verifier.
fn verified(fen: &str) -> Position {
    common::verify_fen(fen);
    Position::from_fen(fen).unwrap_or_else(|error| panic!("cannot parse {fen}: {error}"))
}

/// Searches `position` to `depth` with a private search state.
fn search(position: &Position, depth: u32) -> SearchResult {
    let mut searcher = Searcher::new(16);
    searcher.search(position, Limits::depth(depth), &mut |_| {})
}

#[test]
fn finds_mate_in_one() {
    let position = verified(SCHOLARS_MATE);
    let result = search(&position, 2);
    assert_eq!(result.best_move.to_uci(), "f3f7");
    assert!(
        result.score >= MATE_LIMIT,
        "mate must be reported as a mate score, got {}",
        result.score
    );
    assert_eq!(result.pv.first().copied(), Some(result.best_move));
}

#[test]
fn finds_mate_in_two() {
    // Six plies: the reference implementation also reports this mate only from
    // depth 5, so the test must not force a shallower depth.
    let position = verified(MATE_IN_TWO);
    let result = search(&position, 8);
    assert!(
        result.score >= MATE_LIMIT,
        "score {} is not a mate",
        result.score
    );
    assert!(!result.pv.is_empty(), "a mate line must have a PV");
}

#[test]
fn principal_variation_is_playable() {
    let position = verified(KIWIPETE);
    let result = search(&position, 4);
    assert!(!result.pv.is_empty());
    let mut current = position;
    for mv in &result.pv {
        let legal_moves = legal(&current);
        assert!(
            legal_moves.as_slice().contains(mv),
            "{} is not legal in {}",
            mv.to_uci(),
            current.to_fen()
        );
        current = current.make_move(*mv);
    }
}

#[test]
fn search_is_deterministic() {
    let position = verified(STARTPOS);
    let first = search(&position, 4);
    let second = search(&position, 4);
    assert_eq!(first.best_move, second.best_move);
    assert_eq!(first.score, second.score);
    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.pv, second.pv);
    assert!(first.nodes > 0);
}

#[test]
fn node_limit_stops_the_search() {
    let position = verified(STARTPOS);
    let mut searcher = Searcher::new(8);
    let result = searcher.search(&position, Limits::nodes(500), &mut |_| {});
    // The stop flag is observed at node entry, so the final node may push the
    // counter one or two past the limit.
    assert!(result.nodes <= 502, "node limit exceeded: {}", result.nodes);
    assert!(result.nodes > 0);
    assert!(
        legal(&position).as_slice().contains(&result.best_move),
        "a move-limited search must still return a legal move"
    );
}

#[test]
fn null_move_preserves_the_key() {
    let position = verified("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3");
    let null = position.make_null();
    assert_ne!(null.side(), position.side());
    assert_eq!(null.en_passant(), knightbyte::types::square::NO_SQUARE);
    assert_eq!(null.key(), zobrist::compute(&null));
    assert_ne!(null.key(), position.key());
}

#[test]
fn bench_positions_return_legal_moves() {
    for fen in bench::BENCH_POSITIONS {
        let position = verified(fen);
        let result = search(&position, 3);
        assert!(
            legal(&position).as_slice().contains(&result.best_move),
            "bench position {} returned an illegal best move",
            position.to_fen()
        );
    }
}

/// Recorded depth-6 bench total (Hash 256 MB, one thread).
///
/// This value is the node-identity total of the predecessor engine — the
/// bug-fixed build of 2026-11-18 (predecessor commits bffe98a, f274b39; see
/// ADR-0008) — and it must be identical on every host and in debug and release
/// builds. If a search change alters it, the change is a search change: update
/// this constant together with `docs/baselines.md` and the golden files, and
/// record the reason.
const BENCH_DEPTH_6_NODES: u64 = 15_528;

#[test]
fn bench_node_totals_match_the_recorded_baseline() {
    assert_eq!(bench::total_nodes(6), BENCH_DEPTH_6_NODES);
}

/// Per-position node counts of the predecessor engine for depths 1 to 8, in
/// `bench::BENCH_POSITIONS` order. A fixed-depth search to depth N visits
/// exactly the nodes of the first N iterations of a deeper search, so one
/// depth-8 search per position can be checked at every depth. Captured from
/// the bug-fixed reference build (ADR-0008): the fixes for the former quirks
/// Q2-Q4 (KnightByte bugs B35-B37) change the tree from depth 2 on.
const REFERENCE_NODES: [[u64; 4]; 8] = [
    [47, 110, 33, 371],
    [213, 331, 140, 911],
    [439, 551, 272, 1_252],
    [1_210, 965, 1_078, 1_748],
    [4_001, 1_660, 1_308, 2_970],
    [5_741, 2_683, 2_019, 5_085],
    [12_666, 4_234, 2_978, 7_495],
    [29_604, 7_832, 4_237, 9_686],
];

#[test]
fn bench_node_counts_are_identical_to_the_reference_per_position_and_depth() {
    for (index, fen) in bench::BENCH_POSITIONS.iter().enumerate() {
        let position = verified(fen);
        let mut searcher = Searcher::new(bench::BENCH_TT_MEGABYTES);
        searcher.clear();
        let mut seen = Vec::new();
        searcher.search(&position, Limits::depth(8), &mut |info| {
            seen.push((info.depth, info.nodes));
        });
        assert_eq!(seen.len(), REFERENCE_NODES.len(), "{fen}: iterations");
        for (depth_index, expected) in REFERENCE_NODES.iter().enumerate() {
            let (depth, nodes) = seen[depth_index];
            assert_eq!(depth as usize, depth_index + 1);
            assert_eq!(
                nodes,
                expected[index],
                "position {} differs from the reference at depth {depth}",
                index + 1
            );
        }
    }
}

#[test]
fn singular_verification_lever_is_off_by_default_and_changes_the_search_when_on() {
    // Off (the default) reproduces the reference: depth 9 of the start position
    // is 47,178 nodes there (bug-fixed build, ADR-0008), and singular
    // extensions never fire. The KnightByte-only lever makes the verification
    // search the alternatives, which must change the tree.
    let position = verified(STARTPOS);

    let mut reference = Searcher::new(bench::BENCH_TT_MEGABYTES);
    reference.clear();
    let off = reference.search(&position, Limits::depth(9), &mut |_| {});
    assert_eq!(
        off.nodes, 47_178,
        "default behaviour must match the reference"
    );

    let mut research = Searcher::new(bench::BENCH_TT_MEGABYTES);
    research.clear();
    research.params_mut().singular_exclude_tt_cutoff = true;
    let on = research.search(&position, Limits::depth(9), &mut |_| {});
    assert_ne!(on.nodes, off.nodes, "the lever had no effect on the search");
}

#[test]
fn a_timed_search_is_measured_from_the_go_command_not_from_an_earlier_search() {
    // Regression: the search clock used to be restarted inside `search()`,
    // after the time manager had stored its absolute limits on the old clock.
    // Every timed search then overran by the time since the previous search
    // began (about a second between two moves of a game), which lost games on
    // time. Here the previous search began more than 400 ms before the `go`.
    let position = verified(STARTPOS);
    let mut searcher = Searcher::new(16);
    searcher.search(&position, Limits::depth(3), &mut |_| {});
    std::thread::sleep(Duration::from_millis(400));

    searcher.init_movetime(150);
    let started = Instant::now();
    let result = searcher.search(&position, Limits::depth(64), &mut |_| {});
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(330),
        "a 150 ms search ran for {elapsed:?}"
    );
    assert!(result.depth >= 1, "the first iteration always completes");
    assert!(legal(&position).as_slice().contains(&result.best_move));
}

#[test]
fn an_earlier_search_does_not_leak_into_the_next_one() {
    // Everything that describes an earlier search's path (the per-ply stacks
    // and the countermove table) is reset at the start of a search, and
    // `clear()` forgets the learned tables. A used searcher must therefore
    // behave exactly like a fresh one. This is a determinism guard: it
    // protects the whole reset, but it does not isolate any single stack.
    let first = verified(KIWIPETE);
    let second = verified(STARTPOS);

    let mut used = Searcher::new(16);
    used.search(&first, Limits::depth(7), &mut |_| {});
    used.clear();
    let after_use = used.search(&second, Limits::depth(7), &mut |_| {});

    let mut fresh = Searcher::new(16);
    let from_fresh = fresh.search(&second, Limits::depth(7), &mut |_| {});

    assert_eq!(after_use.nodes, from_fresh.nodes);
    assert_eq!(after_use.best_move, from_fresh.best_move);
    assert_eq!(after_use.score, from_fresh.score);
}
