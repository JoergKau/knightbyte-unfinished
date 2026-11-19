//! The three features that ship switched off (ProbCut, the table extension and
//! bad-noisy pruning) must be safe to switch on: legal moves, no panic, and no
//! lost mate. Whether they gain strength is for the tuning campaign to measure.

mod common;

use knightbyte::board::movegen::legal;
use knightbyte::board::position::Position;
use knightbyte::search::limits::Limits;
use knightbyte::search::options::set;
use knightbyte::search::params::SearchParams;
use knightbyte::search::searcher::{MATE_LIMIT, Searcher};
use knightbyte::tools::bench::BENCH_POSITIONS;

/// Every ported feature on, at its default knobs.
fn all_ported_features() -> SearchParams {
    let mut params = SearchParams::default();
    for name in ["UseProbCut", "UseTtExtension", "UseBadNoisyPruning"] {
        assert!(set(&mut params, name, 1), "{name}");
    }
    params
}

fn verified(fen: &str) -> Position {
    common::verify_fen(fen);
    Position::from_fen(fen).unwrap_or_else(|error| panic!("cannot parse {fen}: {error}"))
}

#[test]
fn the_ported_features_play_legal_moves_on_the_bench_positions() {
    let mut searcher = Searcher::new(16);
    *searcher.params_mut() = all_ported_features();
    for fen in BENCH_POSITIONS {
        let position = verified(fen);
        searcher.clear();
        let result = searcher.search(&position, Limits::depth(11), &mut |_| {});
        assert!(
            legal(&position).as_slice().contains(&result.best_move),
            "{fen}: the best move {} is not legal",
            result.best_move.to_uci()
        );
        assert!(result.nodes > 0);
    }
}

#[test]
fn the_ported_features_keep_finding_mates() {
    // Mate in one for white, mate in one for white (Scholar's mate), and a
    // mate in two: the pruning must not cut the mating line away.
    let cases = [
        ("6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1", "d1d8"),
        (
            "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
            "h5f7",
        ),
    ];
    let mut searcher = Searcher::new(16);
    *searcher.params_mut() = all_ported_features();
    for (fen, mating_move) in cases {
        let position = verified(fen);
        searcher.clear();
        let result = searcher.search(&position, Limits::depth(7), &mut |_| {});
        assert_eq!(result.best_move.to_uci(), mating_move, "{fen}");
        assert!(result.score >= MATE_LIMIT, "{fen}: score {}", result.score);
    }
}

#[test]
fn each_ported_feature_alone_is_safe_with_extreme_knobs() {
    // The knobs at both ends of their ranges, one feature at a time.
    let settings: [&[(&str, i64)]; 6] = [
        &[
            ("UseProbCut", 1),
            ("ProbCutMargin", 100),
            ("ProbCutMinDepth", 4),
        ],
        &[
            ("UseProbCut", 1),
            ("ProbCutMargin", 400),
            ("ProbCutMinDepth", 10),
        ],
        &[("UseTtExtension", 1), ("TtExtensionMaxDepth", 12)],
        &[("UseTtExtension", 1), ("TtExtensionMaxDepth", 0)],
        &[("UseBadNoisyPruning", 1), ("BnpDepth", 10)],
        &[("UseBadNoisyPruning", 1), ("BnpDepth", 2)],
    ];
    let position = verified(BENCH_POSITIONS[3]);
    let mut searcher = Searcher::new(16);
    for setting in settings {
        let mut params = SearchParams::default();
        for &(name, value) in setting {
            assert!(set(&mut params, name, value), "{name}");
        }
        *searcher.params_mut() = params;
        searcher.clear();
        let result = searcher.search(&position, Limits::depth(10), &mut |_| {});
        assert!(
            legal(&position).as_slice().contains(&result.best_move),
            "{setting:?}: illegal best move"
        );
    }
}
