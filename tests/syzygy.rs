//! Syzygy probing and its search integration.
//!
//! The tests use the three committed 3-man tables (`tests/data/tb`), which are
//! part of the public Syzygy set and small enough to ship as test data. Every
//! FEN is verified by the external verifier at test time. The prober state is
//! process-wide, so the tests in this binary run one at a time behind a lock.

mod common;

use std::sync::{Mutex, MutexGuard, OnceLock};

use knightbyte::board::position::Position;
use knightbyte::search::limits::Limits;
use knightbyte::search::searcher::{MATE_LIMIT, SearchInfo, Searcher};
use knightbyte::tb::syzygy::{self, TB_WIN_SCORE, TB_WIN_SCORE_MIN};

/// Serialises the tests that reconfigure the process-wide table set.
fn tb_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    syzygy::set_probing_enabled(true);
    syzygy::set_use_rule50(true);
    guard
}

/// Directory of the committed 3-man tables.
fn table_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb").to_owned()
}

/// Parses `fen` after checking it against the external verifier.
fn verified(fen: &str) -> Position {
    common::verify_fen(fen);
    Position::from_fen(fen).unwrap_or_else(|error| panic!("cannot parse {fen}: {error}"))
}

#[test]
fn wdl_probe_matches_known_values() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let (tables, max_pieces) = syzygy::status();
    assert!(
        tables >= 3,
        "expected at least three WDL tables, got {tables}"
    );
    assert_eq!(max_pieces, 3);

    let cases = [
        ("8/8/8/8/8/1Q6/8/K6k w - - 0 1", 2),
        ("8/8/8/8/8/1R6/8/K6k w - - 0 1", 2),
        ("k7/8/8/8/8/8/P7/K7 w - - 0 1", 0),
        ("4k3/4P3/8/8/8/8/8/6K1 w - - 0 1", 0),
        ("8/8/8/8/8/8/8/K6k w - - 0 1", 0),
        ("8/8/8/8/8/1Q6/8/K6k b - - 0 1", -2),
        ("6k1/8/6K1/8/8/8/8/1R6 b - - 0 1", -2),
    ];
    let tables = syzygy::current().expect("tables were loaded");
    for (fen, expected) in cases {
        let position = verified(fen);
        assert_eq!(
            tables.probe_wdl(&position),
            Some(expected),
            "WDL mismatch for {fen}"
        );
    }
}

#[test]
fn root_probe_returns_a_legal_winning_move() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let position = verified("8/8/8/8/8/1Q6/8/K6k w - - 0 1");
    let tables = syzygy::current().expect("tables were loaded");
    let result = tables.probe_root(&position);
    assert!(!result.failed, "root probe failed");
    assert!(!result.mv.is_none(), "root probe returned no move");
    assert_eq!(result.wdl, 2);
    assert!(result.dtz > 0, "winning root must have a positive DTZ");
    assert!(
        knightbyte::board::movegen::legal(&position)
            .as_slice()
            .contains(&result.mv),
        "root move {} is not legal",
        result.mv.to_uci()
    );
}

#[test]
fn root_probe_preserves_a_draw() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let position = verified("k7/8/8/8/8/8/P7/K7 w - - 0 1");
    let tables = syzygy::current().expect("tables were loaded");
    let result = tables.probe_root(&position);
    assert!(!result.failed, "root probe failed");
    assert_eq!(result.wdl, 0);
    assert_eq!(result.dtz, 0);
    assert!(
        !result.mv.is_none(),
        "a draw position must still have a move"
    );
}

#[test]
fn root_probe_detects_checkmate_and_stalemate() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let tables = syzygy::current().expect("tables were loaded");

    // Both terminal positions are derived from one verified parent by playing
    // a legal move: the verifier cannot answer a position without legal moves.
    let parent = verified("k7/8/2K5/8/8/8/8/1Q6 w - - 0 1");
    let mate = parent.make_move(knightbyte::types::r#move::Move::new(
        1,
        49,
        knightbyte::types::r#move::flags::NORMAL,
    ));
    let result = tables.probe_root(&mate);
    assert!(!result.failed, "mate root probe failed: {result:?}");
    assert!(result.is_checkmate, "expected checkmate, got {result:?}");

    let stale = parent.make_move(knightbyte::types::r#move::Move::new(
        1,
        41,
        knightbyte::types::r#move::flags::NORMAL,
    ));
    let result = tables.probe_root(&stale);
    assert!(!result.failed, "stalemate root probe failed: {result:?}");
    assert!(result.is_stalemate, "expected stalemate, got {result:?}");
}

#[test]
fn uncovered_illegal_and_castling_positions_fail() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let tables = syzygy::current().expect("tables were loaded");

    // Kings adjacent: deliberately illegal input for the rejection path, so it
    // is not submitted to the verifier.
    let illegal = Position::from_fen("4k3/5K2/4P3/8/8/8/8/8 w - - 0 1").expect("parses");
    assert_eq!(tables.probe_wdl(&illegal), None);

    // Castling rights present: the prober refuses, like the reference.
    let castling = verified("4k2r/8/8/8/8/8/8/4K2R w Kk - 0 1");
    assert_eq!(tables.probe_wdl(&castling), None);

    // Six pieces: beyond the loaded three-man set.
    let six = verified("k6r/8/1K6/8/8/8/pQQ5/8 w - - 0 1");
    assert_eq!(tables.probe_wdl(&six), None);
}

#[test]
fn wdl_scores_stay_below_the_mate_bound() {
    assert_eq!(syzygy::wdl_to_score(2), TB_WIN_SCORE);
    assert_eq!(syzygy::wdl_to_score(-2), -TB_WIN_SCORE);
    assert_eq!(syzygy::wdl_to_score(1), 1);
    assert_eq!(syzygy::wdl_to_score(-1), -1);
    assert_eq!(syzygy::wdl_to_score(0), 0);
    const _: () = assert!(TB_WIN_SCORE < MATE_LIMIT);
    const _: () = assert!(TB_WIN_SCORE > 10_000);
    assert_eq!(syzygy::wdl_to_score_graded(2, 10), TB_WIN_SCORE - 10);
    assert_eq!(syzygy::wdl_to_score_graded(-2, 10), -TB_WIN_SCORE + 10);
    assert_eq!(syzygy::wdl_to_score_graded(2, 0), TB_WIN_SCORE);
}

/// Runs a search with the process-wide tables installed and returns the info lines.
fn search_with_tables(
    position: &Position,
    depth: u32,
    pv_cutoff: bool,
) -> (Vec<SearchInfo>, Searcher) {
    let mut searcher = Searcher::new(16);
    searcher.params_mut().allow_syzygy_cutoff_on_pv_nodes = pv_cutoff;
    let mut infos = Vec::new();
    let mut observer = |event: knightbyte::search::searcher::SearchEvent| {
        if let knightbyte::search::searcher::SearchEvent::Info(info) = event {
            infos.push(info.clone());
        }
    };
    let result = searcher.search(position, Limits::depth(depth), &mut observer);
    assert!(!result.best_move.is_none(), "search returned no move");
    (infos, searcher)
}

#[test]
fn search_scores_tablebase_wins_exactly() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let position = verified("8/8/8/8/8/1Q6/8/K6k w - - 0 1");
    let (infos, _searcher) = search_with_tables(&position, 4, true);
    assert!(
        infos
            .iter()
            .any(|info| info.score.abs() >= TB_WIN_SCORE_MIN),
        "no tablebase win score in {:?}",
        infos.iter().map(|info| info.score).collect::<Vec<_>>()
    );
}

#[test]
fn search_announces_mate_when_pv_cutoff_is_disabled() {
    let _guard = tb_lock();
    assert!(syzygy::initialize(&table_dir()));
    let position = verified("8/8/8/8/8/8/8/k1KQ4 w - - 0 1");
    let (infos, _searcher) = search_with_tables(&position, 8, false);
    assert!(
        infos.iter().any(|info| info.score >= MATE_LIMIT),
        "mate was not announced: {:?}",
        infos.iter().map(|info| info.score).collect::<Vec<_>>()
    );
}

#[test]
fn dtz_root_ranking_prefers_the_fastest_progress() {
    // The full 3-4-5 set is optional: set `KNIGHTBYTE_SYZYGY_PATH` to run the
    // ranking tests. This is the reported endgame where every win-preserving
    // move scores the same flat tablebase value, so only the DTZ ranking can
    // pick the fastest progress (Kd2-e2) over the lazy Kd2-c1.
    let Some(directory) = std::env::var_os("KNIGHTBYTE_SYZYGY_PATH") else {
        return;
    };
    let _guard = tb_lock();
    assert!(syzygy::initialize(&directory.to_string_lossy()));
    let position = verified("8/8/8/8/3kP3/5P2/3K4/8 w - - 0 5");
    let tables = syzygy::current().expect("tables were loaded");
    assert!(
        tables.max_pieces() >= 4,
        "the ranking test needs 4-man tables"
    );

    let result = tables.probe_root(&position);
    assert!(!result.failed, "root probe failed: {result:?}");
    assert_eq!(
        result.mv,
        knightbyte::types::r#move::Move::new(11, 12, knightbyte::types::r#move::flags::NORMAL),
        "the DTZ-optimal move is Kd2-e2"
    );
    assert_eq!(result.wdl, 2);
}

#[test]
fn dtz_root_ranking_drives_the_search_result() {
    let Some(directory) = std::env::var_os("KNIGHTBYTE_SYZYGY_PATH") else {
        return;
    };
    let _guard = tb_lock();
    assert!(syzygy::initialize(&directory.to_string_lossy()));
    let position = verified("8/8/8/8/3kP3/5P2/3K4/8 w - - 0 5");

    let mut ranked = Searcher::new(16);
    let result = ranked.search(&position, Limits::depth(12), &mut |_| {});
    assert_eq!(
        result.best_move,
        knightbyte::types::r#move::Move::new(11, 12, knightbyte::types::r#move::flags::NORMAL),
        "the ranked search must play Kd2-e2"
    );

    // With the ranking off, the pre-fix move ordering reproduces the lazy
    // shuffle (Kd2-c1); this proves the ranking is an isolated tie-break.
    let mut unranked = Searcher::new(16);
    unranked.params_mut().use_syzygy_dtz_root_ranking = false;
    let result = unranked.search(&position, Limits::depth(12), &mut |_| {});
    assert_eq!(
        result.best_move,
        knightbyte::types::r#move::Move::new(11, 2, knightbyte::types::r#move::flags::NORMAL),
        "without ranking the legacy shuffle must return"
    );
}

#[test]
fn dtz_root_ranking_converts_a_won_endgame_to_checkmate() {
    // Endgame conversion regression: with root ranking the winning side must
    // convert to mate, not shuffle into a fifty-move or insufficient-material
    // draw.
    let Some(directory) = std::env::var_os("KNIGHTBYTE_SYZYGY_PATH") else {
        return;
    };
    let _guard = tb_lock();
    assert!(syzygy::initialize(&directory.to_string_lossy()));
    let tables = syzygy::current().expect("tables were loaded");
    if tables.max_pieces() < 6 {
        // The fixture is a 6-man position; without 6-man tables this is a
        // plain search test, not a tablebase conversion test.
        return;
    }
    let mut position = verified("8/3k4/4p3/8/8/3PPP2/3K4/8 b - - 0 1");

    let mut checkmate = false;
    for _ in 0..80 {
        let moves = knightbyte::board::movegen::legal(&position);
        if moves.is_empty() {
            checkmate = position.in_check(position.side());
            break;
        }
        let mut searcher = Searcher::new(4);
        let result = searcher.search(&position, Limits::depth(12), &mut |_| {});
        assert!(!result.best_move.is_none(), "search returned no move");
        assert!(
            moves.as_slice().contains(&result.best_move),
            "search returned an illegal move {}",
            result.best_move.to_uci()
        );
        position = position.make_move(result.best_move);
    }
    assert!(
        checkmate,
        "the tablebase win was not converted within 80 plies"
    );
}

#[test]
fn search_without_tables_has_no_tablebase_scores() {
    let _guard = tb_lock();
    syzygy::release();
    let position = verified("8/8/8/8/8/1Q6/8/K6k w - - 0 1");
    let (infos, _searcher) = search_with_tables(&position, 4, true);
    assert!(
        infos.iter().all(|info| info.score.abs() < TB_WIN_SCORE_MIN),
        "a tablebase score appeared without tables: {:?}",
        infos.iter().map(|info| info.score).collect::<Vec<_>>()
    );
}

/// Loads the full table set named by `KNIGHTBYTE_SYZYGY_PATH`, or returns
/// `None` when it is not configured (the test then skips).
fn full_set() -> Option<std::sync::Arc<syzygy::Syzygy>> {
    let directory = std::env::var_os("KNIGHTBYTE_SYZYGY_PATH")?;
    assert!(syzygy::initialize(&directory.to_string_lossy()));
    syzygy::current()
}

#[test]
fn four_and_five_piece_values_match_an_independent_reader() {
    // Values agreed by an independent tablebase reader and the reference
    // engine. These positions need the capture sign, the trailing placement
    // groups and the DTZ value decode of a pawnless 4/5-piece table: all
    // three were once wrong while the three-piece tests still passed.
    let _guard = tb_lock();
    let Some(tables) = full_set() else { return };
    assert!(tables.max_pieces() >= 5, "this test needs the 3-4-5 set");

    // (fen, wdl, |dtz|)
    let cases = [
        ("4k3/8/7N/8/6K1/B7/8/8 w - - 0 1", 2, 54),   // KBNvK
        ("3R4/8/8/8/8/6k1/2K1R3/8 b - - 0 1", -2, 6), // KRRvK, black to move
        ("1B6/p7/8/8/8/4N2K/3k4/8 w - - 0 1", 2, 1),  // winning capture
        ("k7/6N1/8/1B6/2K5/8/3B4/8 w - - 0 1", 2, 7), // KBBNvK
    ];
    for (fen, wdl, dtz) in cases {
        let position = verified(fen);
        let result = tables.probe_root(&position);
        assert!(!result.failed, "root probe failed for {fen}");
        assert_eq!(result.wdl, wdl, "WDL mismatch for {fen}");
        assert_eq!(result.dtz, dtz, "DTZ mismatch for {fen}");
    }
}

#[test]
fn six_piece_three_versus_three_material_is_covered() {
    // KQN vs KPN: a 3-v-3 material. The name scan once omitted the whole
    // family, so every such position reported "not covered".
    let _guard = tb_lock();
    let Some(tables) = full_set() else { return };
    if tables.max_pieces() < 6 {
        return;
    }
    let position = verified("6K1/8/6k1/5p1n/2N5/8/4Q3/8 w - - 0 1");
    let result = tables.probe_root(&position);
    assert!(!result.failed, "a 3-v-3 position must be covered");
    assert_eq!(result.wdl, 2);
    assert_eq!(result.dtz, 5);
}
#[test]
fn missing_and_corrupt_tables_are_harmless() {
    let _guard = tb_lock();
    let position = verified("8/8/8/8/8/1Q6/8/K6k w - - 0 1");

    // No path: probing simply reports "not covered".
    syzygy::release();
    assert_eq!(syzygy::status(), (0, 0));
    assert!(syzygy::current().is_none());

    // A directory with a truncated file must be rejected without panicking.
    let directory =
        std::env::temp_dir().join(format!("knightbyte-tb-corrupt-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create temp table directory");
    let source = format!("{}/KQvK.rtbw", table_dir());
    let mut bytes = std::fs::read(&source).expect("read the committed table");
    bytes.truncate(16);
    std::fs::write(directory.join("KQvK.rtbw"), &bytes).expect("write truncated table");

    assert!(syzygy::initialize(&directory.to_string_lossy()));
    if let Some(tables) = syzygy::current() {
        assert_eq!(tables.probe_wdl(&position), None);
    }
    let _ = std::fs::remove_dir_all(&directory);

    // Restore the good tables for any later test in this binary.
    assert!(syzygy::initialize(&table_dir()));
}
