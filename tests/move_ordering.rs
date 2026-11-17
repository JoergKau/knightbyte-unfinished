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
use knightbyte::search::move_picker::{PickerState, debug_dump_picker_order};
use knightbyte::search::see::{see_approximate, see_value};
use knightbyte::types::color::WHITE;
use knightbyte::types::r#move::{Move, flags};
use knightbyte::types::piece::PAWN;

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
    debug_dump_picker_order(
        position,
        moves.as_slice(),
        tt_move,
        killers,
        history,
        0,
        previous,
        &cont,
    )
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
    // The reference adds the promotion delta twice (once in the move value,
    // once in the exchange walk); reproduced faithfully for parity.
    assert_eq!(see_value(&position, legal_move(&position, "a7a8q")), 2_375);
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
    let previous: Previous = (1, 8, 16);
    let reply = legal_move(&position, "b1c3");
    history.update_counter(Some(previous), reply);
    let picked = ordered(&position, &history, Move::NONE, &killers, Some(previous));
    assert_eq!(picked[0], reply);
}

/// Rook on g2 with the king far away: `g2g4` is an ordinary rook move here.
const ROOK_ON_G2: &str = "4k3/8/8/8/8/8/6R1/4K3 w - - 0 1";

#[test]
fn killer_recorded_as_a_pawn_double_push_matches_the_same_squares_of_another_piece() {
    // A killer travels between positions. Recorded as a pawn's g2-g4 double
    // push, it must still be recognised when a rook can play g2-g4, and the
    // picker must hand back the *generated* rook move, not the remembered one
    // (which carries the double-push marker and would corrupt the position).
    let position = verified(ROOK_ON_G2);
    let history = History::new();
    let mut killers = Killers::new();
    killers.record(0, Move::new(14, 30, flags::DOUBLE_PUSH));
    let picked = ordered(&position, &history, Move::NONE, &killers, None);
    assert_eq!(picked[0].to_uci(), "g2g4");
    assert_eq!(picked[0].flag(), flags::NORMAL);
    assert!(legal(&position).as_slice().contains(&picked[0]));
    assert!(killers.contains(0, legal_move(&position, "g2g4")));
}

#[test]
fn countermove_recorded_as_a_pawn_double_push_matches_the_same_squares_of_another_piece() {
    let position = verified(ROOK_ON_G2);
    let mut history = History::new();
    let killers = Killers::new();
    let previous: Previous = (1, 52, 36);
    history.update_counter(Some(previous), Move::new(14, 30, flags::DOUBLE_PUSH));
    let picked = ordered(&position, &history, Move::NONE, &killers, Some(previous));
    assert_eq!(picked[0].to_uci(), "g2g4");
    assert_eq!(picked[0].flag(), flags::NORMAL);
}

#[test]
fn black_pawn_captures_are_generated_east_before_west() {
    // d6 can capture on e5 (the -7 direction) and on c5 (-9). The reference
    // generator visits -7 first, and the picker's tie-break between equally
    // scored captures follows generation order, so this order decides which
    // subtree is searched first.
    let position = verified("rnbqkbnr/pp2pppp/3p4/2P1P3/8/8/PPP2PPP/RNBQKBNR b KQkq - 0 4");
    let order: Vec<String> = legal(&position)
        .as_slice()
        .iter()
        .map(|mv| mv.to_uci())
        .collect();
    let east = order.iter().position(|mv| mv == "d6e5").expect("d6e5");
    let west = order.iter().position(|mv| mv == "d6c5").expect("d6c5");
    assert!(east < west, "expected d6e5 before d6c5 in {order:?}");
}

#[test]
fn white_pawn_captures_are_generated_west_before_east() {
    // d3 can capture on c4 (+7) and on e4 (+9), in that order.
    let position = verified("rnbqkbnr/pp1p1ppp/8/8/2p1p3/3P4/PPP2PPP/RNBQKBNR w KQkq - 0 5");
    let order: Vec<String> = legal(&position)
        .as_slice()
        .iter()
        .map(|mv| mv.to_uci())
        .collect();
    let west = order.iter().position(|mv| mv == "d3c4").expect("d3c4");
    let east = order.iter().position(|mv| mv == "d3e4").expect("d3e4");
    assert!(west < east, "expected d3c4 before d3e4 in {order:?}");
}

#[test]
fn en_passant_is_generated_in_capture_direction_order() {
    // The e5 pawn can capture en passant on d6 (+7) and normally on f6 (+9).
    // The reference interleaves the two per direction, so the en-passant
    // capture comes first; the push comes before both.
    let position = verified("rnbqkbnr/ppp1p1pp/5p2/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3");
    let order: Vec<String> = legal(&position)
        .as_slice()
        .iter()
        .map(|mv| mv.to_uci())
        .filter(|mv| mv.starts_with("e5"))
        .collect();
    assert_eq!(order, ["e5e6", "e5d6", "e5f6"]);
}

#[test]
fn castle_table_move_is_neither_tried_first_nor_dropped() {
    // The reference table keeps only the two squares and the promotion piece,
    // so a stored castling move comes back without its castle marker. It then
    // matches no generated move: it is not emitted at the table stage and it
    // does not remove the real castling move from the quiet list.
    let position = verified("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let history = History::new();
    let killers = Killers::new();
    let stripped = Move::new(4, 6, flags::NORMAL);
    let picked = ordered(&position, &history, stripped, &killers, None);
    let castle = legal_move(&position, "e1g1");
    assert_eq!(castle.flag(), flags::CASTLE);
    assert_eq!(picked.len(), legal(&position).len());
    assert_eq!(picked.iter().filter(|&&mv| mv == castle).count(), 1);
    assert_ne!(picked[0], castle, "the stripped table move must not lead");
}

#[test]
fn capture_scores_see_the_history_left_by_the_table_move_subtree() {
    // The capture scores are computed after the table move has been returned
    // (and, in the search, searched), so a capture-history update made in
    // between reorders the captures. e4 can take d5 or f5 with equal material;
    // generation order alone puts d5 first.
    let position = verified("4k3/8/8/3p1p2/4P3/8/8/4K3 w - - 0 1");
    let moves = legal(&position);
    let tt_move = legal_move(&position, "e1e2");
    let take_f5 = legal_move(&position, "e4f5");
    let killers = Killers::new();
    let mut history = History::new();
    let cont: Continuations = [None; 4];

    let mut state = PickerState::new();
    state.reset(tt_move, &killers, &history, 0, None, &cont, false, false);
    assert_eq!(
        state.next(&position, &history, moves.as_slice()),
        Some(tt_move)
    );
    history.update_capture(PAWN, take_f5, PAWN, 8);
    assert_eq!(
        state.next(&position, &history, moves.as_slice()),
        Some(take_f5),
        "the capture-history bonus earned after the table move must count"
    );
}

#[test]
fn quiescence_orders_captures_without_capture_history() {
    // Same position, quiescence shape: the picker has no capture history
    // there, so the bonus changes nothing and generation order decides.
    let position = verified("4k3/8/8/3p1p2/4P3/8/8/4K3 w - - 0 1");
    let moves = legal(&position);
    let take_d5 = legal_move(&position, "e4d5");
    let take_f5 = legal_move(&position, "e4f5");
    let killers = Killers::new();
    let mut history = History::new();
    history.update_capture(PAWN, take_f5, PAWN, 8);
    let cont: Continuations = [None; 4];

    let mut state = PickerState::new();
    state.reset(Move::NONE, &killers, &history, 0, None, &cont, true, true);
    assert_eq!(
        state.next(&position, &history, moves.as_slice()),
        Some(take_d5)
    );
    assert_eq!(
        state.next(&position, &history, moves.as_slice()),
        Some(take_f5)
    );
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
