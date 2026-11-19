//! Every advertised lever must change the search.
//!
//! The tuning campaign sets each of the sixty levers one at a time and plays
//! matches; a lever that is advertised but silently does nothing would burn days
//! of games on a null result (the predecessor logged exactly that class of bug).
//! This test sets every lever to a non-default value and requires the search of
//! the four bench positions to differ from the default search in its node count
//! or in a best move (depth 9, then 12, then 15). A lever that needs a companion setting to be observable
//! (a quiescence ply needs narrowing, ProbCut's margin needs ProbCut) gets it in
//! both the baseline and the variant.
//!
//! `UseStableTimeManagement` only changes how a clock is spent, never a fixed
//! depth search; `search::time_manager::tests` compares its allocations.

use knightbyte::board::position::Position;
use knightbyte::search::lever_table::LEVERS;
use knightbyte::search::limits::Limits;
use knightbyte::search::options::{OptionKind, set};
use knightbyte::search::params::SearchParams;
use knightbyte::search::searcher::Searcher;
use knightbyte::tools::bench::BENCH_POSITIONS;
use knightbyte::types::r#move::Move;

/// Levers that never change a fixed-depth search, with the reason.
const TIME_ONLY: [&str; 1] = ["UseStableTimeManagement"];

/// The companion settings a lever needs before its effect is observable.
fn companions(name: &str) -> &'static [(&'static str, i64)] {
    match name {
        "QsRecaptureOnlyPly" => &[("UseQsNarrowing", 1)],
        "ProbCutMargin" | "ProbCutMinDepth" => &[("UseProbCut", 1)],
        "TtExtensionMaxDepth" => &[("UseTtExtension", 1)],
        "BnpDepth" => &[("UseBadNoisyPruning", 1)],
        _ => &[],
    }
}

/// Returns the value that differs most from the default.
fn probe_value(name: &str) -> i64 {
    let spec = LEVERS
        .iter()
        .find(|spec| spec.name == name)
        .expect("the lever exists");
    if spec.kind == OptionKind::Check {
        1 - spec.default
    } else if (spec.max - spec.default) > (spec.default - spec.min) {
        spec.max
    } else {
        spec.min
    }
}

/// Searches the bench positions and returns the total nodes and every best move.
fn run(searcher: &mut Searcher, positions: &[Position], depth: u32) -> (u64, Vec<Move>) {
    let mut nodes = 0;
    let mut moves = Vec::new();
    for position in positions {
        searcher.clear();
        let result = searcher.search(position, Limits::depth(depth), &mut |_| {});
        nodes += result.nodes;
        moves.push(result.best_move);
    }
    (nodes, moves)
}

/// Builds a parameter set: the defaults, the companions, then (optionally) the
/// lever at its probe value.
fn params_for(name: &str, probe: bool) -> SearchParams {
    let mut params = SearchParams::default();
    for &(companion, value) in companions(name) {
        assert!(set(&mut params, companion, value), "{companion}");
    }
    if probe {
        assert!(set(&mut params, name, probe_value(name)), "{name}");
    }
    params
}

/// Release-mode gate: the deeper probes are far too slow for a debug build.
/// Run it with `cargo test --release --test lever_effect`.
#[test]
#[cfg_attr(debug_assertions, ignore = "run with --release")]
fn every_lever_changes_the_search() {
    let positions: Vec<Position> = BENCH_POSITIONS
        .iter()
        .map(|fen| Position::from_fen(fen).expect("a bench FEN must parse"))
        .collect();
    let mut searcher = Searcher::new(32);
    let mut inert = Vec::new();

    for spec in LEVERS {
        if TIME_ONLY.contains(&spec.name) {
            continue;
        }
        // A shallow search first; a lever that only acts deep gets a second,
        // deeper chance before it is declared inert.
        let mut changed = false;
        for depth in [9, 12, 15] {
            *searcher.params_mut() = params_for(spec.name, false);
            let baseline = run(&mut searcher, &positions, depth);
            *searcher.params_mut() = params_for(spec.name, true);
            let variant = run(&mut searcher, &positions, depth);
            if variant != baseline {
                println!(
                    "{:<32} {:>10} -> {:>10} nodes (depth {depth})",
                    spec.name, baseline.0, variant.0
                );
                changed = true;
                break;
            }
        }
        if !changed {
            inert.push(spec.name);
        }
    }

    assert!(
        inert.is_empty(),
        "these levers did not change the search at depth 15: {inert:?}"
    );
}
