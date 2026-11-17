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
    // The stop flag is observed at node entry, so the final node may push the\n    // counter one or two past the limit.\n    assert!(result.nodes <= 502, "node limit exceeded: {}", result.nodes);
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
/// This value must be identical on every host and in debug and release builds;
/// if a search change alters it on purpose, update this constant together with
/// `docs/baselines.md` and `docs/decisions/ADR-0003-search-parity-baseline.md`.
const BENCH_DEPTH_6_NODES: u64 = 13_118;

#[test]
fn bench_node_totals_match_the_recorded_baseline() {
    assert_eq!(bench::total_nodes(6), BENCH_DEPTH_6_NODES);
}
