//! Bit-exact network evaluation parity against the captured reference values.

mod common;

use knightbyte::board::position::Position;
use knightbyte::eval::nnue::{Evaluator, embedded_net};

/// Shared FEN list and the reference centipawn evaluation for each position.
///
/// The list varies side to move, king square/region, mirror state and material
/// count. Every FEN is verified by the external UCI verifier at test time.
const CASES: [(&str, &str, i32); 15] = [
    (
        "startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        27,
    ),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        -104,
    ),
    (
        "position3",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        146,
    ),
    (
        "position4",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        -51,
    ),
    (
        "position5",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        499,
    ),
    (
        "position6",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        52,
    ),
    (
        "kiwipete_black",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R b KQkq - 0 1",
        536,
    ),
    ("kings_deep", "4k3/8/8/8/8/8/4P3/4K3 b - - 0 1", -189),
    ("king_a1_white", "8/8/8/8/8/8/8/K6k w - - 0 1", -28),
    ("king_a1_black", "8/8/8/8/8/8/8/K6k b - - 0 1", -64),
    ("king_a8_black", "K6k/8/8/8/8/8/8/8 b - - 0 1", -28),
    ("castling_rook", "4k3/8/8/8/8/8/8/4K2R w K - 0 1", 465),
    (
        "castling_rook_no_rights",
        "8/8/8/8/8/8/6k1/4K2R w K - 0 1",
        520,
    ),
    (
        "uho_opening",
        "r2q1rk1/pp1bppbp/2np1np1/2p5/P1B1PB2/3P1N2/1PP2PPP/RN1Q1RK1 w - - 1 9",
        -13,
    ),
    (
        "sharp_castled",
        "r1bq1rk1/p1pnbppp/1p3n2/3p2B1/3P4/2N1PN2/PPQ2PPP/R3KB1R w KQ - 0 9",
        80,
    ),
];

#[test]
fn embedded_network_metadata_is_valid() {
    let net = embedded_net();
    assert_eq!(net.l1(), 512);
    assert_eq!(net.region_count(), 8);
    assert!(net.description().starts_with("v5_"));
}

#[test]
fn evaluation_matches_reference_values() {
    let net = embedded_net();
    let mut evaluator = Evaluator::new(net);
    for (name, fen, expected) in CASES {
        common::verify_fen(fen);
        let position = Position::from_fen(fen).unwrap_or_else(|error| {
            panic!("{name}: cannot parse FEN: {error}");
        });
        assert_eq!(
            evaluator.evaluate(&position),
            expected,
            "{name}: evaluation mismatch"
        );
    }
}
