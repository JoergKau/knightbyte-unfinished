//! Static-exchange and move-ordering gates.
//!
//! Every FEN used here is checked by the external verifier when the test runs,
//! as required by the project rules. The cases are deliberately small: one
//! exchange idea per position, plus the standard perft positions for the
//! "every move exactly once" property.

mod common;

use knightbyte::board::movegen::legal;
use knightbyte::board::position::Position;
use knightbyte::search::history::{Continuations, History, Previous};
use knightbyte::search::killers::Killers;
use knightbyte::search::move_picker::MovePicker;
use knightbyte::search::see::{see_approximate, see_value};
use knightbyte::types::color::WHITE;
use knightbyte::types::r#move::Move;

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Parses `fen` after checking it against the external verifier.
fn verified(fen: &str) -> Position {
    common::verify_fen(fen);
    Position::from_fen(fen).unwrap_or_else(|error| panic!("cannot parse {fen}: {error}"))
}

/// Returns the legal move spelled `uci`, or panics.
fn legal_move(position: &Position, uci: &str) -> Move {
    legal(position)
        .as_slice()
        .iter()
        .copied()
        .find(|mv| mv.to_uci() == uci)
        .unwrap_or_else(|| panic!("{uci} is not legal in {}", position.to_fen()))
}

/// Returns every move in picker order.
fn ordered(
    position: &Position,
    history: &History,
    tt_move: Move,
    killers: &Killers,
    previous: Option<Previous>,
) -> Vec<Move> {
    let moves = legal(position);
    let cont: Continuations = [None; 4];
    MovePicker::new(
        position,
        moves.as_slice(),
        tt_move,
        killers,
        history,
        0,
        previous,
        &cont,
    )
    .collect()
}

#[test]
fn every_move_is_returned_exactly_once() {
    let fens = [
        STARTPOS,
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    ];
    for fen in fens {
        let position = verified(fen);
        let expected = legal(&position);
        let history = History::new();
        let killers = Killers::new();
        let picked = ordered(&position, &history, Move::NONE, &killers, None);

        assert_eq!(picked.len(), expected.len(), "move count differs in {fen}");
        let mut from_picker: Vec<String> = picked.iter().map(|mv| mv.to_uci()).collect();
        let mut from_generator: Vec<String> =
            expected.as_slice().iter().map(|mv| mv.to_uci()).collect();
        from_picker.sort();
        from_generator.sort();
        from_picker.dedup();
        assert_eq!(
            from_picker, from_generator,
            "picker lost or duplicated moves"
        );
    }
}

#[test]
fn see_approximation_is_a_cheap_material_difference() {
    let position = verified(STARTPOS);
    // An empty target reads as a pawn in the reference convention.
    assert_eq!(see_approximate(&position, legal_move(&position, "e2e4")), 0);
    assert_eq!(
        see_approximate(&position, legal_move(&position, "g1f3")),
        109 - 403
    );
}

#[test]
fn see_scores_free_pawn_and_defended_pawn() {
    let free = verified("4k3/8/8/3p4/8/8/8/3QK3 w - - 0 1");
    assert_eq!(see_value(&free, legal_move(&free, "d1d5")), 109);

    let defended = verified("4k3/8/2p5/3p4/8/8/8/3QK3 w - - 0 1");
    assert_eq!(
        see_value(&defended, legal_move(&defended, "d1d5")),
        -1_133,
        "queen for pawn against a defended pawn must lose material"
    );
}

#[test]
fn see_handles_en_passant() {
    let position = verified("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    assert_eq!(see_value(&position, legal_move(&position, "e5d6")), 109);
}

#[test]
fn see_handles_promotions() {
    let position = verified("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    // The reference adds the promotion delta twice (once in the move value,\n    // once in the exchange walk); reproduced faithfully for parity.\n    assert_eq!(see_value(&position, legal_move(&position, "a7a8q")), 2_375);
    assert_eq!(see_value(&position, legal_move(&position, "a7a8n")), 697);
}

#[test]
fn see_sees_recaptures_through_a_vacated_square() {
    let position = verified("r3k3/8/r7/8/8/8/R7/R3K3 w - - 0 1");
    assert_eq!(
        see_value(&position, legal_move(&position, "a2a6")),
        679,
        "the rook behind the attacker must be counted after the square is vacated"
    );
}

#[test]
fn tt_move_is_tried_first() {
    let position = verified(STARTPOS);
    let history = History::new();
    let killers = Killers::new();
    let tt_move = legal_move(&position, "g1f3");
    let picked = ordered(&position, &history, tt_move, &killers, None);
    assert_eq!(picked[0], tt_move);
}

#[test]
fn winning_capture_is_tried_first() {
    let position = verified("4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1");
    let capture = legal_move(&position, "e4d5");
    assert!(see_value(&position, capture) > 1_200, "the queen is free");
    let history = History::new();
    let killers = Killers::new();
    let picked = ordered(&position, &history, Move::NONE, &killers, None);
    assert_eq!(picked[0], capture);
}

#[test]
fn most_valuable_victim_is_tried_first() {
    let position = verified("4k3/8/8/3r1n2/4P3/8/8/4K3 w - - 0 1");
    let history = History::new();
    let killers = Killers::new();
    let picked = ordered(&position, &history, Move::NONE, &killers, None);
    assert_eq!(picked[0], legal_move(&position, "e4d5"));
    let knight_capture = picked
        .iter()
        .position(|&mv| mv == legal_move(&position, "e4f5"))
        .expect("knight capture must be ordered too");
    assert!(
        knight_capture > 0,
        "capturing the rook must come before capturing the knight"
    );
}

#[test]
fn losing_capture_is_tried_last() {
    let position = verified("4k3/8/2p5/3p4/8/8/8/3QK2R w - - 0 1");
    let capture = legal_move(&position, "d1d5");
    assert!(see_value(&position, capture) < 0);
    let history = History::new();
    let killers = Killers::new();
    let picked = ordered(&position, &history, Move::NONE, &killers, None);
    assert!(picked.len() > 1);
    assert_eq!(picked.last(), Some(&capture));
}

#[test]
fn killers_are_tried_before_other_quiets() {
    let position = verified(STARTPOS);
    let history = History::new();
    let mut killers = Killers::new();
    let killer = legal_move(&position, "g1f3");
    killers.record(0, killer);
    let picked = ordered(&position, &history, Move::NONE, &killers, None);
    assert_eq!(picked[0], killer);
    assert_eq!(killers.get(0)[0], killer, "one record fills the first slot");
    assert!(killers.get(0)[1..].iter().all(|mv| mv.is_none()));
}

#[test]
fn countermove_is_tried_before_other_quiets() {
    let position = verified(STARTPOS);
    let mut history = History::new();
    let killers = Killers::new();
    let previous: Previous = (1, 8);
    let reply = legal_move(&position, "b1c3");
    history.update_counter(Some(previous), reply);
    let picked = ordered(&position, &history, Move::NONE, &killers, Some(previous));
    assert_eq!(picked[0], reply);
}

#[test]
fn history_promotes_a_quiet_move() {
    let position = verified(STARTPOS);
    let mut history = History::new();
    let killers = Killers::new();
    let quiet = legal_move(&position, "g1f3");
    history.update_quiet(WHITE, quiet, 5, &[], &[None; 4]);
    let picked = ordered(&position, &history, Move::NONE, &killers, None);
    assert_eq!(picked[0], quiet);
}

#[test]
fn ageing_and_clearing_history_change_the_order() {
    let position = verified(STARTPOS);
    let mut history = History::new();
    let quiet = legal_move(&position, "g1f3");
    history.update_quiet(WHITE, quiet, 5, &[], &[None; 4]);
    let before = history.butterfly(WHITE, quiet);
    history.age();
    assert_eq!(
        history.butterfly(WHITE, quiet),
        before * 7 / 8,
        "ageing decays by 7/8"
    );
    history.clear();
    assert_eq!(history.butterfly(WHITE, quiet), 0);
}
