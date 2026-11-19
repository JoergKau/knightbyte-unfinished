//! Differential gate for the make-free legality test.
//!
//! `movegen::legal` filters the pseudo-legal list with `Legality` (pins and
//! checkers computed once per node). The reference filter makes every move
//! and tests the king (`legal_by_making`). Both must agree move for move, in
//! the same order, on every position a long seeded random walk reaches —
//! including check, double check, pinned pieces, en passant and castling.

mod common;

use knightbyte::board::legality::Legality;
use knightbyte::board::movegen::{legal, legal_by_making, pseudo_legal};
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
