//! Chess position: mailbox, bitboards and the incremental game state.
//!
//! The board is a hybrid of a 64-square mailbox for O(1) piece lookup and
//! per-colour/per-type bitboards for move generation. Phase 2 uses copy-make:
//! [`Position`] is `Copy`, and a move returns the next position by value. That
//! keeps the rules code small and easy to audit; the search can benchmark
//! make/unmake separately in the performance phase.

use crate::board::attacks::{
    bishop_attacks, king_attacks, knight_attacks, pawn_attacks, rook_attacks,
};
use crate::board::bitboard::bits;
use crate::board::zobrist;
use crate::types::color::{BLACK, Color, WHITE, flip};
use crate::types::r#move::{Move, flags};
use crate::types::piece::{
    BISHOP, BK, EMPTY, KING, KNIGHT, PAWN, Piece, QUEEN, ROOK, WK, color_of, is_white, piece_type,
};
use crate::types::square::{NO_SQUARE, Square, bit};

/// Castling-right bit for White kingside.
pub const CASTLE_WK: u8 = 1;
/// Castling-right bit for White queenside.
pub const CASTLE_WQ: u8 = 2;
/// Castling-right bit for Black kingside.
pub const CASTLE_BK: u8 = 4;
/// Castling-right bit for Black queenside.
pub const CASTLE_BQ: u8 = 8;

/// A complete chess position.
///
/// The struct is deliberately small enough to copy per node during the
/// correctness phase. The network accumulator is *not* stored here; it lives
/// beside the evaluator because it is much larger than the board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub(crate) board: [Piece; 64],
    pub(crate) pieces: [[u64; 6]; 2],
    pub(crate) occupancy: [u64; 2],
    pub(crate) all: u64,
    pub(crate) side: Color,
    pub(crate) castling: u8,
    pub(crate) ep: Square,
    pub(crate) halfmove: u16,
    pub(crate) fullmove: u16,
    pub(crate) king_sq: [Square; 2],
    pub(crate) key: u64,
}

const _: () = assert!(core::mem::size_of::<Position>() <= 256);

impl Position {
    /// Returns the side to move.
    #[inline(always)]
    pub const fn side(&self) -> Color {
        self.side
    }

    /// Returns the piece on a square (`EMPTY` if the square is empty).
    #[inline(always)]
    pub const fn piece_at(&self, square: Square) -> Piece {
        self.board[square as usize]
    }

    /// Returns the bitboard for one colour and piece type.
    #[inline(always)]
    pub const fn pieces(&self, color: Color, piece_type: usize) -> u64 {
        self.pieces[color as usize][piece_type]
    }

    /// Returns the occupancy of one colour.
    #[inline(always)]
    pub const fn occupancy(&self, color: Color) -> u64 {
        self.occupancy[color as usize]
    }

    /// Returns the union of both colours' occupancy.
    #[inline(always)]
    pub const fn all_occupancy(&self) -> u64 {
        self.all
    }

    /// Returns the king square of one colour.
    #[inline(always)]
    pub const fn king_square(&self, color: Color) -> Square {
        self.king_sq[color as usize]
    }

    /// Returns the castling-right bitmask.
    #[inline(always)]
    pub const fn castling_rights(&self) -> u8 {
        self.castling
    }

    /// Returns the en-passant target, or [`NO_SQUARE`].
    #[inline(always)]
    pub const fn en_passant(&self) -> Square {
        self.ep
    }

    /// Returns the halfmove clock.
    #[inline(always)]
    pub const fn halfmove_clock(&self) -> u16 {
        self.halfmove
    }

    /// Returns the fullmove number.
    #[inline(always)]
    pub const fn fullmove_number(&self) -> u16 {
        self.fullmove
    }

    /// Returns the incremental Zobrist key of the position.
    ///
    /// The key covers piece placement, castling rights, the en-passant file and
    /// the side to move; move counters are excluded. It is maintained by
    /// [`Position::make_move`] and can be checked against
    /// [`zobrist::compute`](crate::board::zobrist::compute).
    #[inline(always)]
    pub const fn key(&self) -> u64 {
        self.key
    }

    /// Returns whether `square` is attacked by `by_color`.
    ///
    /// Pawn attacks use the opposite colour's attack table: the set of squares
    /// from which a pawn of `by_color` could attack `square` is exactly the
    /// attack set of an opposite-coloured pawn standing on `square`.
    pub fn is_attacked(&self, square: Square, by_color: Color) -> bool {
        let by = by_color as usize;
        if pawn_attacks(flip(by_color), square) & self.pieces[by][PAWN] != 0 {
            return true;
        }
        if knight_attacks(square) & self.pieces[by][KNIGHT] != 0 {
            return true;
        }
        if king_attacks(square) & self.pieces[by][KING] != 0 {
            return true;
        }
        let bishops = self.pieces[by][BISHOP] | self.pieces[by][QUEEN];
        if bishop_attacks(square, self.all) & bishops != 0 {
            return true;
        }
        let rooks = self.pieces[by][ROOK] | self.pieces[by][QUEEN];
        rook_attacks(square, self.all) & rooks != 0
    }

    /// Returns whether `color`'s king is in check.
    #[inline]
    pub fn in_check(&self, color: Color) -> bool {
        self.is_attacked(self.king_sq[color as usize], flip(color))
    }

    /// Applies a move and returns the next position (copy-make).
    ///
    /// The move must be pseudo-legal for this position; legality is the
    /// caller's responsibility (see `movegen::legal`).
    pub fn make_move(&self, mv: Move) -> Position {
        debug_assert!(!mv.is_none());
        let from = mv.from();
        let to = mv.to();
        let flag = mv.flag();
        let us = self.side;
        let them = flip(us);
        let moving = self.board[from as usize];
        debug_assert!(moving != EMPTY);
        let moving_type = piece_type(moving);

        let mut next = *self;
        next.board[from as usize] = EMPTY;
        next.pieces[us as usize][moving_type] &= !bit(from);

        let captured_square = if flag == flags::EN_PASSANT {
            if us == WHITE { to - 8 } else { to + 8 }
        } else {
            to
        };
        let mut captured = self.board[captured_square as usize];
        if flag == flags::CASTLE || captured_square == to {
            captured = self.board[to as usize];
        }
        if captured != EMPTY {
            let captured_color = color_of(captured) as usize;
            next.pieces[captured_color][piece_type(captured)] &= !bit(captured_square);
            if flag == flags::EN_PASSANT {
                next.board[captured_square as usize] = EMPTY;
            }
        }

        let placed = if mv.is_promotion() {
            promotion_piece(us, flag)
        } else {
            moving
        };
        next.board[to as usize] = placed;
        next.pieces[us as usize][piece_type(placed)] |= bit(to);

        if moving_type == KING {
            next.king_sq[us as usize] = to;
            next.castling &= !castle_mask(us);
        }

        if flag == flags::CASTLE {
            let (rook_from, rook_to) = castle_rook_squares(us, to);
            let rook = self.board[rook_from as usize];
            debug_assert_eq!(piece_type(rook), ROOK);
            next.board[rook_from as usize] = EMPTY;
            next.board[rook_to as usize] = rook;
            next.pieces[us as usize][ROOK] &= !bit(rook_from);
            next.pieces[us as usize][ROOK] |= bit(rook_to);
        }

        next.castling &= !rook_rights_for_square(from);
        next.castling &= !rook_rights_for_square(captured_square);

        next.ep = if flag == flags::DOUBLE_PUSH {
            ((from as u16 + to as u16) / 2) as Square
        } else {
            NO_SQUARE
        };

        next.halfmove = if moving_type == PAWN || captured != EMPTY {
            0
        } else {
            self.halfmove.saturating_add(1)
        };
        next.fullmove = if us == BLACK {
            self.fullmove.saturating_add(1)
        } else {
            self.fullmove
        };
        next.side = them;

        next.occupancy[0] = 0;
        next.occupancy[1] = 0;
        let mut piece_type_index = 0;
        while piece_type_index < 6 {
            next.occupancy[0] |= next.pieces[0][piece_type_index];
            next.occupancy[1] |= next.pieces[1][piece_type_index];
            piece_type_index += 1;
        }
        next.all = next.occupancy[0] | next.occupancy[1];

        // Incremental Zobrist update: remove what left the board, add what
        // arrived, then toggle the state bits that changed. `compute` is the
        // ground truth and is asserted in debug builds.
        let mut key = self.key;
        key ^= zobrist::piece_key(moving, from);
        if captured != EMPTY {
            key ^= zobrist::piece_key(captured, captured_square);
        }
        if flag == flags::CASTLE {
            let (rook_from, rook_to) = castle_rook_squares(us, to);
            key ^= zobrist::piece_key(self.board[rook_from as usize], rook_from);
            key ^= zobrist::piece_key(self.board[rook_from as usize], rook_to);
        }
        key ^= zobrist::piece_key(placed, to);
        if self.castling != next.castling {
            key ^= zobrist::castling_key(self.castling) ^ zobrist::castling_key(next.castling);
        }
        if self.ep != NO_SQUARE {
            key ^= zobrist::en_passant_key(self.ep);
        }
        if next.ep != NO_SQUARE {
            key ^= zobrist::en_passant_key(next.ep);
        }
        key ^= zobrist::side_key();
        next.key = key;
        debug_assert_eq!(key, zobrist::compute(&next));

        debug_assert_eq!(next.all & bit(next.king_sq[0]), bit(next.king_sq[0]));
        debug_assert_eq!(next.all & bit(next.king_sq[1]), bit(next.king_sq[1]));
        next
    }

    /// Applies a "null move" and returns the next position.
    ///
    /// A null move passes the turn without moving a piece. It exists only for
    /// the null-move pruning heuristic; the position is not reachable by legal
    /// play, and the halfmove clock advances so the search treats it as a
    /// tempo-losing waiting move. The en-passant target is cleared because a
    /// pass cannot leave a capture right behind.
    pub fn make_null(&self) -> Position {
        let mut next = *self;
        next.side = flip(self.side);
        next.ep = NO_SQUARE;
        next.halfmove = self.halfmove.saturating_add(1);
        next.key = self.key ^ zobrist::en_passant_key(self.ep) ^ zobrist::side_key();
        debug_assert_eq!(next.key, zobrist::compute(&next));
        next
    }

    /// Iterates every piece on the board as `(square, piece)` pairs.
    pub fn pieces_on_board(&self) -> impl Iterator<Item = (Square, Piece)> + '_ {
        bits(self.all).map(|square| (square, self.board[square as usize]))
    }

    /// Returns the side to move in the network's perspective convention.
    #[inline(always)]
    pub const fn side_to_move_is_white(&self) -> bool {
        self.side == WHITE
    }
}

/// Returns the castling mask belonging to one colour.
const fn castle_mask(color: Color) -> u8 {
    if color == WHITE {
        CASTLE_WK | CASTLE_WQ
    } else {
        CASTLE_BK | CASTLE_BQ
    }
}

/// Returns the castling rights invalidated by moving from or capturing on a square.
const fn rook_rights_for_square(square: Square) -> u8 {
    match square {
        0 => CASTLE_WQ,
        7 => CASTLE_WK,
        56 => CASTLE_BQ,
        63 => CASTLE_BK,
        _ => 0,
    }
}

/// Returns the rook source and target for a castling move.
const fn castle_rook_squares(color: Color, king_to: Square) -> (Square, Square) {
    if color == WHITE {
        if king_to == 6 { (7, 5) } else { (0, 3) }
    } else if king_to == 62 {
        (63, 61)
    } else {
        (56, 59)
    }
}

/// Maps a promotion flag to the colour-coded piece.
const fn promotion_piece(color: Color, flag: u8) -> Piece {
    let base = if color == WHITE { 0 } else { 6 };
    base + match flag {
        flags::PROMO_KNIGHT => 2,
        flags::PROMO_BISHOP => 3,
        flags::PROMO_ROOK => 4,
        _ => 5,
    }
}

/// Returns whether a piece is a white king or black king.
#[inline(always)]
pub const fn is_king(piece: Piece) -> bool {
    piece == WK || piece == BK
}

/// Returns whether a piece belongs to the given colour.
#[inline(always)]
pub const fn is_color(piece: Piece, color: Color) -> bool {
    is_white(piece) == (color == WHITE)
}
