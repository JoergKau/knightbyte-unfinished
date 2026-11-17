//! Colour-coded piece constants and small conversion helpers.
//!
//! Layout: `EMPTY = 0`, white pieces `1..=6`, black pieces `7..=12`. The
//! ordering is shared with the network feature scheme, so changing it would
//! silently change every evaluation.

use crate::types::color::{BLACK, Color, WHITE};

/// An empty square.
pub const EMPTY: u8 = 0;
/// White pawn.
pub const WP: u8 = 1;
/// White knight.
pub const WN: u8 = 2;
/// White bishop.
pub const WB: u8 = 3;
/// White rook.
pub const WR: u8 = 4;
/// White queen.
pub const WQ: u8 = 5;
/// White king.
pub const WK: u8 = 6;
/// Black pawn.
pub const BP: u8 = 7;
/// Black knight.
pub const BN: u8 = 8;
/// Black bishop.
pub const BB: u8 = 9;
/// Black rook.
pub const BR: u8 = 10;
/// Black queen.
pub const BQ: u8 = 11;
/// Black king.
pub const BK: u8 = 12;

/// Piece-type indices used by bitboard arrays and the network feature scheme.
pub const PAWN: usize = 0;
/// Knight piece-type index.
pub const KNIGHT: usize = 1;
/// Bishop piece-type index.
pub const BISHOP: usize = 2;
/// Rook piece-type index.
pub const ROOK: usize = 3;
/// Queen piece-type index.
pub const QUEEN: usize = 4;
/// King piece-type index.
pub const KING: usize = 5;

/// A colour-coded piece.
pub type Piece = u8;

/// Returns the piece type (`PAWN..=KING`) of a non-empty piece.
#[inline(always)]
pub const fn piece_type(piece: Piece) -> usize {
    debug_assert!(piece != EMPTY);
    ((piece - 1) % 6) as usize
}

/// Returns the colour of a non-empty piece.
#[inline(always)]
pub const fn color_of(piece: Piece) -> Color {
    debug_assert!(piece != EMPTY);
    if piece <= WK { WHITE } else { BLACK }
}

/// Returns `true` for a white piece.
#[inline(always)]
pub const fn is_white(piece: Piece) -> bool {
    piece >= WP && piece <= WK
}

/// Converts a FEN character into a piece, or `None` for an unknown character.
pub const fn from_char(character: char) -> Option<Piece> {
    match character {
        'P' => Some(WP),
        'N' => Some(WN),
        'B' => Some(WB),
        'R' => Some(WR),
        'Q' => Some(WQ),
        'K' => Some(WK),
        'p' => Some(BP),
        'n' => Some(BN),
        'b' => Some(BB),
        'r' => Some(BR),
        'q' => Some(BQ),
        'k' => Some(BK),
        _ => None,
    }
}

/// Converts a piece into its FEN character, or `.` for an empty square.
pub const fn to_char(piece: Piece) -> char {
    match piece {
        WP => 'P',
        WN => 'N',
        WB => 'B',
        WR => 'R',
        WQ => 'Q',
        WK => 'K',
        BP => 'p',
        BN => 'n',
        BB => 'b',
        BR => 'r',
        BQ => 'q',
        BK => 'k',
        _ => '.',
    }
}
