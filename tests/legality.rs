//! Differential gate for the make-free legality test.
//!
//! `movegen::legal` filters the pseudo-legal list with `Legality` (pins and
//! checkers computed once per node). The reference filter makes every move
//! and tests the king (`legal_by_making`). Both must agree move for move, in
//! the same order, on every position a long seeded random walk reaches —
//! including check, double check, pinned pieces, en passant and castling.

mod common;

use knightbyte::board::legality::Legality;
use knightbyte::board::movegen::{
    has_exactly_one_pseudo_move, legal, legal_by_making, pseudo_legal,
};
use knightbyte::board::position::Position;

/// The perft suite's positions: they cover every special case (castling both
/// ways, en passant, promotions, pins, discovered and double checks).
const FENS: [&str; 5] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
];

/// Deterministic xorshift so the walk replays identically.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn the_make_free_test_matches_the_making_filter_on_random_walks() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut checked = 0_u64;
    let mut in_check = 0_u64;
    let mut double_check = 0_u64;
    let mut with_pins = 0_u64;
    let mut rejected = 0_u64;
    for fen in FENS {
        common::verify_fen(fen);
        for _ in 0..200 {
            let mut position = Position::from_fen(fen).expect("the FEN must parse");
            for _ in 0..150 {
                let context = Legality::new(&position);
                let expected = legal_by_making(&position);
                let pseudo = pseudo_legal(&position);
                let filtered: Vec<_> = pseudo
                    .as_slice()
                    .iter()
                    .copied()
                    .filter(|&mv| context.is_legal(&position, mv))
                    .collect();
                assert_eq!(
                    filtered.as_slice(),
                    expected.as_slice(),
                    "legality mismatch from {fen}"
                );
                assert_eq!(
                    legal(&position).as_slice(),
                    expected.as_slice(),
                    "legal() mismatch from {fen}"
                );
                checked += 1;
                in_check += u64::from(context.checkers() != 0);
                double_check += u64::from(context.checkers().count_ones() > 1);
                with_pins += u64::from(context.pinned() != 0);
                rejected += (pseudo.len() - expected.len()) as u64;
                if expected.is_empty() {
                    break;
                }
                let pick = (rng.next() % expected.len() as u64) as usize;
                position = position.make_move(expected.as_slice()[pick]);
            }
        }
    }
    // The walk must actually visit the interesting classes.
    assert!(checked > 50_000, "walks too short: {checked}");
    assert!(in_check > 1_000, "too few positions in check: {in_check}");
    assert!(double_check > 0, "no double check visited");
    assert!(with_pins > 1_000, "too few pinned positions: {with_pins}");
    assert!(rejected > 10_000, "too few illegal moves: {rejected}");
}

/// Positions with few pieces, where a side often has very few pseudo-legal
/// moves: the forced-reply count must agree with the full generator there too.
const SPARSE_FENS: [&str; 5] = [
    "8/8/8/8/8/8/6k1/K7 w - - 0 1",
    "8/8/2k5/8/8/8/1P6/K7 w - - 0 1",
    "7k/8/5QK1/8/8/8/8/8 b - - 0 1",
    "8/8/8/4k3/8/8/PP6/KB6 w - - 0 1",
    "6k1/5ppp/8/8/8/8/5PPP/6K1 w - - 0 1",
];

/// Positions where the side to move has exactly one pseudo-legal move (its
/// king is walled in by its own pieces). A random walk never reaches such a
/// position, so they are listed; each was found by a search and is checked
/// below, and the verifier confirms every one is a legal position.
const ONE_MOVE_FENS: [&str; 4] = [
    "5n2/b6n/2q1n3/q1P1k3/PbPb4/RP2p3/PP1p4/KRr3n1 w - - 0 1",
    "3b4/8/1p1kn3/1p2q1p1/1P2r3/BP1b4/PP6/KRr2n1r w - - 0 1",
    "8/p7/1p2pbp1/1r2kb2/1P3r2/BP6/PP1q2bn/KRq4q w - - 0 1",
    "8/8/1ppk1p1p/1b6/pP3p2/PP1n4/PP1P4/KRBr4 w - - 0 1",
];

#[test]
fn the_early_exit_forced_reply_count_matches_the_full_generator() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut checked = 0_u64;
    for fen in ONE_MOVE_FENS {
        common::verify_fen(fen);
        let position = Position::from_fen(fen).expect("the FEN must parse");
        assert_eq!(
            pseudo_legal(&position).len(),
            1,
            "fixture lost its property: {fen}"
        );
        assert!(
            has_exactly_one_pseudo_move(&position),
            "missed a forced reply: {fen}"
        );
    }
    for fen in FENS
        .iter()
        .chain(SPARSE_FENS.iter())
        .chain(ONE_MOVE_FENS.iter())
    {
        common::verify_fen(fen);
        for _ in 0..300 {
            let mut position = Position::from_fen(fen).expect("the FEN must parse");
            for _ in 0..120 {
                let full = pseudo_legal(&position);
                assert_eq!(
                    has_exactly_one_pseudo_move(&position),
                    full.len() == 1,
                    "forced-reply count mismatch from {fen} ({} moves)",
                    full.len()
                );
                checked += 1;
                let legal_moves = legal(&position);
                if legal_moves.is_empty() {
                    break;
                }
                let pick = (rng.next() % legal_moves.len() as u64) as usize;
                position = position.make_move(legal_moves.as_slice()[pick]);
            }
        }
    }
    assert!(checked > 50_000, "walks too short: {checked}");
}
