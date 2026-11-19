//! Opening-book signature and probe tests.
//!
//! The `.ctg` fixtures are synthesised in memory, so the tests never depend on
//! a third-party book being present. The signature vectors are format values
//! checked against real book files; the probe tests use positions built from
//! the start position.

mod common;

use knightbyte::board::position::Position;
use knightbyte::tb::book::{BookRng, CtgBook, compute_signature};
use knightbyte::types::r#move::{Move, flags};

/// Parses `fen` after checking it against the external verifier.
fn verified(fen: &str) -> Position {
    common::verify_fen(fen);
    Position::from_fen(fen).unwrap_or_else(|error| panic!("cannot parse {fen}: {error}"))
}

fn signature_hex(position: &Position) -> String {
    let (buffer, length) = compute_signature(position);
    buffer[..length]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

/// Builds one book entry.
fn entry(
    signature: &[u8],
    moves: &[(u8, u8)],
    wins: u32,
    draws: u32,
    losses: u32,
    recommendation: u8,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(signature);
    bytes.push((1 + 2 * moves.len()) as u8);
    for &(mv, annotation) in moves {
        bytes.push(mv);
        bytes.push(annotation);
    }
    let total = wins + draws + losses;
    for value in [total, losses, wins, draws] {
        bytes.extend_from_slice(&value.to_be_bytes()[1..]);
    }
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.push(recommendation);
    bytes.extend_from_slice(&[0; 2]);
    bytes
}

/// Assembles a `.ctg` file with one position entry per page.
fn book_bytes(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0_u8; 4096];
    for entry in entries {
        let mut page = vec![0_u8; 4096];
        page[0] = 0;
        page[1] = 1; // one position
        page[4..4 + entry.len()].copy_from_slice(entry);
        bytes.extend_from_slice(&page);
    }
    bytes
}

fn start_position() -> Position {
    verified("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
}

fn move_bytes(position: &Position, moves: &[Move]) -> Vec<(u8, u8)> {
    moves
        .iter()
        .map(|&mv| {
            (
                CtgBook::encode_move_byte(position, mv)
                    .unwrap_or_else(|| panic!("no book byte for {}", mv.to_uci())),
                0,
            )
        })
        .collect()
}

#[test]
fn signature_of_the_start_position_matches_the_format() {
    let position = start_position();
    assert_eq!(
        signature_hex(&position),
        "56B60F7960F3A60F58B078E0C1E1A60F5960F3B60F7F"
    );
}

#[test]
fn signature_flips_ranks_when_black_is_to_move() {
    let position = verified("rnbqkbnr/pppppppp/8/8/8/5N2/PPPPPPPP/RNBQKB1R b KQkq - 1 1");
    assert_eq!(
        signature_hex(&position),
        "56B60F7960F3A60F58B078E0C1E1A613F5960EB60F7F"
    );
}

#[test]
fn en_passant_is_only_recorded_when_a_capture_exists() {
    let start = start_position();
    // After 1.e4 the engine keeps e3, but no black pawn can capture; the book
    // must store the position without an en-passant flag. Stockfish normalises
    // such a FEN away, so the position is derived by playing the move.
    let with_ep = start.make_move(Move::new(12, 28, flags::DOUBLE_PUSH));
    let plain = verified("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
    assert_eq!(signature_hex(&plain), signature_hex(&with_ep));

    // 1.e4 a6 2.e5 d5: black's double push leaves d6, and exd6 is available.
    let with_capture_ep = play(
        &start,
        &[
            Move::new(12, 28, flags::DOUBLE_PUSH),
            Move::new(48, 40, flags::NORMAL),
            Move::new(28, 36, flags::NORMAL),
            Move::new(51, 35, flags::DOUBLE_PUSH),
        ],
    );
    let capture_plain = verified("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
    assert_ne!(
        signature_hex(&capture_plain),
        signature_hex(&with_capture_ep)
    );
}

/// Applies a sequence of known-legal moves.
fn play(position: &Position, moves: &[Move]) -> Position {
    let mut position = *position;
    for &mv in moves {
        position = position.make_move(mv);
    }
    position
}

#[test]
fn synthetic_book_returns_only_book_moves_and_weights_them() {
    let start = start_position();
    let e4 = start.make_move(Move::new(12, 28, flags::DOUBLE_PUSH));
    let d4 = start.make_move(Move::new(11, 27, flags::DOUBLE_PUSH));

    let (start_sig, start_len) = compute_signature(&start);
    let (e4_sig, e4_len) = compute_signature(&e4);
    let (d4_sig, d4_len) = compute_signature(&d4);

    let start_entry = entry(
        &start_sig[..start_len],
        &move_bytes(
            &start,
            &[
                Move::new(12, 28, flags::DOUBLE_PUSH),
                Move::new(11, 27, flags::DOUBLE_PUSH),
            ],
        ),
        0,
        0,
        0,
        0,
    );
    let e4_entry = entry(&e4_sig[..e4_len], &[], 10, 5, 0, 0);
    let d4_entry = entry(&d4_sig[..d4_len], &[], 1, 0, 9, 0);

    let mut book = CtgBook::default();
    assert!(book.load_bytes(book_bytes(&[start_entry, e4_entry, d4_entry])));
    assert!(book.is_loaded());

    let mut rng = BookRng::new(7);
    let mut e4_count = 0;
    for _ in 0..200 {
        let mv = book.pick_book_move(&start, &mut rng);
        assert!(!mv.is_none());
        assert!(
            mv == Move::new(12, 28, flags::DOUBLE_PUSH)
                || mv == Move::new(11, 27, flags::DOUBLE_PUSH)
        );
        if mv.to() == 28 {
            e4_count += 1;
        }
    }
    assert!(e4_count > 100, "the better-scoring move must dominate");
}

#[test]
fn a_recommended_child_restricts_the_choice() {
    let start = start_position();
    let e4 = start.make_move(Move::new(12, 28, flags::DOUBLE_PUSH));
    let d4 = start.make_move(Move::new(11, 27, flags::DOUBLE_PUSH));
    let (start_sig, start_len) = compute_signature(&start);
    let (e4_sig, e4_len) = compute_signature(&e4);
    let (d4_sig, d4_len) = compute_signature(&d4);

    let start_entry = entry(
        &start_sig[..start_len],
        &move_bytes(
            &start,
            &[
                Move::new(12, 28, flags::DOUBLE_PUSH),
                Move::new(11, 27, flags::DOUBLE_PUSH),
            ],
        ),
        0,
        0,
        0,
        0,
    );
    let e4_entry = entry(&e4_sig[..e4_len], &[], 10, 5, 0, 0);
    let d4_entry = entry(&d4_sig[..d4_len], &[], 1, 0, 9, 128);

    let mut book = CtgBook::default();
    assert!(book.load_bytes(book_bytes(&[start_entry, e4_entry, d4_entry])));
    let mut rng = BookRng::new(11);
    for _ in 0..50 {
        let mv = book.pick_book_move(&start, &mut rng);
        assert_eq!(mv, Move::new(11, 27, flags::DOUBLE_PUSH));
    }
}

#[test]
fn a_dubious_annotation_removes_the_move() {
    let start = start_position();
    let e4 = start.make_move(Move::new(12, 28, flags::DOUBLE_PUSH));
    let d4 = start.make_move(Move::new(11, 27, flags::DOUBLE_PUSH));
    let (start_sig, start_len) = compute_signature(&start);
    let (e4_sig, e4_len) = compute_signature(&e4);
    let (d4_sig, d4_len) = compute_signature(&d4);

    let mut moves = move_bytes(
        &start,
        &[
            Move::new(12, 28, flags::DOUBLE_PUSH),
            Move::new(11, 27, flags::DOUBLE_PUSH),
        ],
    );
    moves[0].1 = 0x02; // e2e4 marked dubious
    let start_entry = entry(&start_sig[..start_len], &moves, 0, 0, 0, 0);
    let e4_entry = entry(&e4_sig[..e4_len], &[], 10, 5, 0, 0);
    let d4_entry = entry(&d4_sig[..d4_len], &[], 1, 0, 9, 0);

    let mut book = CtgBook::default();
    assert!(book.load_bytes(book_bytes(&[start_entry, e4_entry, d4_entry])));
    let mut rng = BookRng::new(3);
    for _ in 0..20 {
        assert_eq!(
            book.pick_book_move(&start, &mut rng),
            Move::new(11, 27, flags::DOUBLE_PUSH)
        );
    }
}

#[test]
fn book_moves_decode_for_the_black_side() {
    let start = start_position();
    let after_e4 = start.make_move(Move::new(12, 28, flags::DOUBLE_PUSH));
    let after_e5 = after_e4.make_move(Move::new(52, 36, flags::DOUBLE_PUSH));

    let (start_sig, start_len) = compute_signature(&start);
    let (e4_sig, e4_len) = compute_signature(&after_e4);
    let (e5_sig, e5_len) = compute_signature(&after_e5);

    let start_entry = entry(
        &start_sig[..start_len],
        &move_bytes(&start, &[Move::new(12, 28, flags::DOUBLE_PUSH)]),
        0,
        0,
        0,
        0,
    );
    let e4_entry = entry(
        &e4_sig[..e4_len],
        &move_bytes(&after_e4, &[Move::new(52, 36, flags::DOUBLE_PUSH)]),
        0,
        0,
        0,
        0,
    );
    let e5_entry = entry(&e5_sig[..e5_len], &[], 10, 5, 0, 0);

    let mut book = CtgBook::default();
    assert!(book.load_bytes(book_bytes(&[start_entry, e4_entry, e5_entry])));
    let mut rng = BookRng::new(5);
    assert_eq!(
        book.pick_book_move(&after_e4, &mut rng),
        Move::new(52, 36, flags::DOUBLE_PUSH)
    );
}

#[test]
fn positions_outside_the_book_return_no_move() {
    let start = start_position();
    let (sig, len) = compute_signature(&start);
    let mut book = CtgBook::default();
    assert!(book.load_bytes(book_bytes(&[entry(&sig[..len], &[], 1, 1, 0, 0)])));

    let endgame = verified("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1");
    let mut rng = BookRng::new(1);
    assert!(book.pick_book_move(&endgame, &mut rng).is_none());
}

#[test]
fn missing_and_malformed_books_are_rejected() {
    let mut book = CtgBook::default();
    assert!(!book.load("this-file-does-not-exist.ctg"));
    assert!(!book.is_loaded());
    assert!(!book.load_bytes(vec![0_u8; 4096]));
}
