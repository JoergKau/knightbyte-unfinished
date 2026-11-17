//! Zobrist hashing for incremental position keys.
//!
//! All randomised values are generated at compile time from a fixed seed, so
//! every machine that builds the same source produces bit-identical keys. That
//! is a hard requirement for reproducible node counts: the transposition table
//! decides which subtrees are cut, so a key that differed between hosts would
//! make `bench` output host-dependent.
//!
//! The full key covers piece placement, castling rights, the en-passant file
//! and the side to move. Move counters are deliberately excluded, because two
//! paths to the same board must hash identically.
//!
//! Pawn and non-pawn keys are subsets of the same piece tables. They exist for
//! caches keyed on structure rather than on the exact position (pawn-structure
//! evaluation, material signatures) and are cheap to recompute on demand.

use crate::board::position::Position;
use crate::types::color::BLACK;
use crate::types::piece::{PAWN, Piece, piece_type};
use crate::types::square::{NO_SQUARE, Square, file};

/// Seed of the key generator.
///
/// This value is part of the contract: the transposition table, the correction
/// history and the evaluation cache all index by key material, so changing the
/// seed changes which entries collide and therefore the benchmark node counts.
const SEED: u64 = 0x243F_6A88_85A3_08D3;

/// A small xorshift64 generator.
///
/// The search-parity gate compares node counts with the reference
/// implementation, whose tables come from this generator. Reproducing it keeps
/// the hash-based tables aligned index for index.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    /// Seeds the generator exactly like the reference implementation.
    const fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Returns the next value.
    const fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

/// Builds the 13x64 piece-square table.
///
/// Row 0 is the empty-square row. It is drawn even though it is never used for
/// a key, because the reference draws the whole table in piece order and every
/// later value depends on that order.
const fn build_pieces() -> [[u64; 64]; 13] {
    let mut random = XorShift64::new(SEED);
    let mut table = [[0_u64; 64]; 13];
    let mut piece = 0;
    while piece < 13 {
        let mut square = 0;
        while square < 64 {
            table[piece][square] = random.next();
            square += 1;
        }
        piece += 1;
    }
    table
}

/// Builds the 16-entry castling table from the four right keys.
///
/// Index 0 is zero so "no rights" costs nothing to hash and cannot introduce a
/// key mismatch. Every other index is the XOR of the held rights, which is what
/// an incremental update produces.
const fn build_castling() -> [u64; 16] {
    let mut random = XorShift64::new(SEED);
    let mut draw = 0;
    while draw < 13 * 64 + 8 {
        random.next();
        draw += 1;
    }
    let mut rights = [0_u64; 4];
    let mut index = 0;
    while index < 4 {
        rights[index] = random.next();
        index += 1;
    }
    let mut table = [0_u64; 16];
    let mut mask = 1;
    while mask < 16 {
        let mut combined = 0;
        let mut bit = 0;
        while bit < 4 {
            if mask & (1 << bit) != 0 {
                combined ^= rights[bit];
            }
            bit += 1;
        }
        table[mask] = combined;
        mask += 1;
    }
    table
}

/// Builds the per-file en-passant table.
const fn build_en_passant() -> [u64; 8] {
    let mut random = XorShift64::new(SEED);
    let mut draw = 0;
    while draw < 13 * 64 {
        random.next();
        draw += 1;
    }
    let mut table = [0_u64; 8];
    let mut file = 0;
    while file < 8 {
        table[file] = random.next();
        file += 1;
    }
    table
}

/// Builds the side-to-move key.
const fn build_side() -> u64 {
    let mut random = XorShift64::new(SEED);
    let mut draw = 0;
    while draw < 13 * 64 + 4 + 8 {
        random.next();
        draw += 1;
    }
    random.next()
}

const PIECES: [[u64; 64]; 13] = build_pieces();
const CASTLING: [u64; 16] = build_castling();
const EN_PASSANT: [u64; 8] = build_en_passant();
const SIDE_KEY: u64 = build_side();

/// Returns the key contribution of a piece standing on a square.
///
/// `piece` uses the colour-coded layout `WP..=BK` (`1..=12`); `EMPTY` is not a
/// valid argument.
#[inline(always)]
pub const fn piece_key(piece: Piece, square: Square) -> u64 {
    debug_assert!(piece != 0 && piece <= 12);
    PIECES[piece as usize][square as usize]
}

/// Returns the key contribution of a castling-right bitmask.
#[inline(always)]
pub const fn castling_key(rights: u8) -> u64 {
    CASTLING[(rights & 15) as usize]
}

/// Returns the key contribution of an en-passant target square.
///
/// The key depends on the file only: the rank is implied by the side to move.
/// [`NO_SQUARE`] contributes nothing.
#[inline(always)]
pub const fn en_passant_key(square: Square) -> u64 {
    if square == NO_SQUARE {
        0
    } else {
        EN_PASSANT[file(square) as usize]
    }
}

/// Returns the key contribution of a black side to move.
#[inline(always)]
pub const fn side_key() -> u64 {
    SIDE_KEY
}

/// Recomputes the full position key from scratch.
///
/// [`Position::make_move`] maintains the key incrementally; this function is
/// the ground truth that the incremental path is tested against.
pub fn compute(position: &Position) -> u64 {
    let mut key = 0;
    for (square, piece) in position.pieces_on_board() {
        key ^= piece_key(piece, square);
    }
    key ^= castling_key(position.castling_rights());
    key ^= en_passant_key(position.en_passant());
    if position.side() == BLACK {
        key ^= side_key();
    }
    key
}

/// Recomputes the pawn-only key from scratch.
///
/// Pawn structures change slowly, so caches keyed on this value survive
/// piece-rerouting above the pawns. The key includes pawn colour, so a white
/// and a black pawn on mirrored squares hash differently.
pub fn pawn_key(position: &Position) -> u64 {
    let mut key = 0;
    for (square, piece) in position.pieces_on_board() {
        if piece_type(piece) == PAWN {
            key ^= piece_key(piece, square);
        }
    }
    key
}

/// Recomputes the key covering every piece except pawns.
///
/// Together with [`pawn_key`] this splits the full piece contribution in two,
/// which is what evaluation caches use to tolerate non-structural changes.
pub fn non_pawn_key(position: &Position) -> u64 {
    let mut key = 0;
    for (square, piece) in position.pieces_on_board() {
        if piece_type(piece) != PAWN {
            key ^= piece_key(piece, square);
        }
    }
    key
}

/// The three keys the correction tables index by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CorrectionKeys {
    /// Key over both sides' pawns.
    pub pawn: u64,
    /// Key over White's non-pawn pieces.
    pub white_non_pawn: u64,
    /// Key over Black's non-pawn pieces.
    pub black_non_pawn: u64,
}

/// Computes the correction-history keys in one board pass.
pub fn correction_keys(position: &Position) -> CorrectionKeys {
    let mut keys = CorrectionKeys {
        pawn: 0,
        white_non_pawn: 0,
        black_non_pawn: 0,
    };
    for (square, piece) in position.pieces_on_board() {
        let key = piece_key(piece, square);
        if piece_type(piece) == PAWN {
            keys.pawn ^= key;
        } else if crate::types::piece::color_of(piece) == crate::types::color::WHITE {
            keys.white_non_pawn ^= key;
        } else {
            keys.black_non_pawn ^= key;
        }
    }
    keys
}

/// Recomputes the non-pawn key of one colour.
///
/// The correction history keys its white and black material terms separately,
/// so the split has to follow the piece colour, not just the piece type.
pub fn non_pawn_key_for(position: &Position, color: crate::types::color::Color) -> u64 {
    let keys = correction_keys(position);
    if color == crate::types::color::WHITE {
        keys.white_non_pawn
    } else {
        keys.black_non_pawn
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        CASTLING, EN_PASSANT, PIECES, castling_key, compute, en_passant_key, non_pawn_key,
        pawn_key, piece_key, side_key,
    };
    use crate::board::movegen::legal;
    use crate::board::position::Position;
    use crate::types::piece::{BP, WN, WP};
    use crate::types::square::{NO_SQUARE, from_file_rank};

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

    /// Collects every random value in the tables.
    fn all_values() -> HashSet<u64> {
        let mut values = HashSet::new();
        for row in &PIECES {
            for &value in row {
                values.insert(value);
            }
        }
        for &value in CASTLING.iter().skip(1) {
            values.insert(value);
        }
        for &value in &EN_PASSANT {
            values.insert(value);
        }
        values.insert(side_key());
        values
    }

    #[test]
    fn tables_contain_no_duplicates_or_zeroes() {
        let values = all_values();
        assert_eq!(values.len(), 13 * 64 + 15 + 8 + 1);
        assert!(!values.contains(&0));
        assert_eq!(castling_key(0), 0);
        assert_eq!(en_passant_key(NO_SQUARE), 0);
    }

    #[test]
    fn startpos_key_is_stable() {
        // Golden value: locks the seed so a table change cannot silently
        // invalidate stored keys or benchmark captures.
        let position = Position::from_fen(START).unwrap();
        assert_eq!(position.key(), 0x48e8_6881_dc3d_fd16);
        assert_eq!(position.key(), compute(&position));
    }

    #[test]
    fn side_and_castling_rights_change_the_key() {
        let white = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let black = Position::from_fen("4k3/8/8/8/8/8/8/4K3 b - - 0 1").unwrap();
        assert_ne!(white.key(), black.key());
        assert_eq!(white.key() ^ black.key(), side_key());

        let castling = Position::from_fen("4k3/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
        let no_rights = Position::from_fen("4k3/8/8/8/8/8/8/4K2R w - - 0 1").unwrap();
        assert_ne!(castling.key(), white.key());
        assert_eq!(
            castling.key() ^ no_rights.key(),
            castling_key(1),
            "with the same pieces only the kingside right may change the key"
        );
    }

    #[test]
    fn en_passant_key_uses_the_file_only() {
        let position = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        assert_eq!(position.en_passant(), from_file_rank(3, 5));
        assert_eq!(
            en_passant_key(position.en_passant()),
            EN_PASSANT[3],
            "d6 must hash through the d-file entry"
        );
        let without = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(
            position.key() ^ without.key(),
            en_passant_key(position.en_passant())
        );
    }

    #[test]
    fn move_counters_do_not_affect_the_key() {
        let first = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let second = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 41 88").unwrap();
        assert_eq!(first.key(), second.key());
    }

    #[test]
    fn pawn_and_non_pawn_keys_split_the_piece_contribution() {
        let position = Position::from_fen(KIWIPETE).unwrap();
        let mut pieces = 0;
        for (square, piece) in position.pieces_on_board() {
            pieces ^= piece_key(piece, square);
        }
        assert_eq!(pawn_key(&position) ^ non_pawn_key(&position), pieces);
        assert_ne!(pawn_key(&position), 0);
        assert_ne!(non_pawn_key(&position), 0);
    }

    #[test]
    fn pawn_key_hashes_both_colours() {
        let white = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let black = Position::from_fen("4k3/4p3/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(pawn_key(&white), piece_key(WP, from_file_rank(4, 1)));
        assert_eq!(pawn_key(&black), piece_key(BP, from_file_rank(4, 6)));
        assert_ne!(pawn_key(&white), pawn_key(&black));
    }

    #[test]
    fn knight_pawn_colour_pairs_are_distinct() {
        for square in [8_u8, 48] {
            assert_ne!(piece_key(WP, square), piece_key(BP, square));
            assert_ne!(piece_key(WN, square), piece_key(WP, square));
        }
    }

    #[test]
    fn incremental_key_matches_a_full_recompute_over_perft() {
        for fen in [START, KIWIPETE] {
            let position = Position::from_fen(fen).unwrap();
            walk(&position, 3);
        }
    }

    /// Recomputes the key of every node in a small perft tree.
    fn walk(position: &Position, depth: u32) {
        assert_eq!(
            position.key(),
            compute(position),
            "key drift at {}",
            position.to_fen()
        );
        if depth == 0 {
            return;
        }
        for &mv in legal(position).as_slice() {
            let next = position.make_move(mv);
            assert_eq!(
                next.key(),
                compute(&next),
                "incremental key wrong after {} in {}",
                mv.to_uci(),
                position.to_fen()
            );
            walk(&next, depth - 1);
        }
    }
}
