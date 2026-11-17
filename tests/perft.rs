//! Golden perft gates and external verifier cross-checks.

mod common;

use knightbyte::board::perft::perft;
use knightbyte::board::position::Position;

/// The standard positions and the exact counts recorded from the reference
/// implementation. The external verifier checks the same counts again when the
/// test runs.
const CASES: [(&str, &str, u32, u64); 5] = [
    (
        "startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        4,
        197_281,
    ),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        3,
        97_862,
    ),
    (
        "position3",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        4,
        43_238,
    ),
    (
        "position4",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        3,
        9_467,
    ),
    (
        "position5",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        3,
        62_379,
    ),
];

#[test]
fn perft_matches_golden_and_external_verifier() {
    for (name, fen, depth, expected) in CASES {
        common::verify_fen(fen);
        let position = Position::from_fen(fen).unwrap_or_else(|error| {
            panic!("{name}: cannot parse FEN: {error}");
        });
        assert_eq!(
            perft(&position, depth),
            expected,
            "{name}: perft mismatch at depth {depth}"
        );
        assert_eq!(
            common::verifier_perft(fen, depth),
            expected,
            "{name}: external verifier disagrees at depth {depth}"
        );
    }
}
