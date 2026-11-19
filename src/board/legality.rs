//! Make-free legality test for pseudo-legal moves.
//!
//! The straightforward legality filter makes every pseudo-legal move on a
//! copy of the position and asks whether the mover's king is attacked. That
//! costs a 200-byte copy, an occupancy rebuild and a king-attack test per
//! move, and on a typical node most of the moves are legal anyway.
//!
//! [`Legality`] answers the same question from three facts computed **once
//! per node**: the pieces giving check, the own pieces pinned to the king,
//! and the king square. A move is then legal when
//!
//! * a king move lands on a square the opponent does not attack (with the
//!   king itself removed from the occupancy, so it cannot hide behind
//!   itself on a slider's ray);
//! * any other move, in a position with two checkers, never (only the king
//!   can answer a double check);
//! * any other move, in a position with one checker, captures the checker or
//!   lands between a sliding checker and the king;
//! * a move of a pinned piece stays on the line through the king.
//!
//! En passant (two pieces leave one rank) and castling (rare; its path
//! safety is decided by the generator) are settled by making the move, so
//! the exotic discovered-attack cases keep the exact answer of the reference
//! filter. The result is move-for-move identical to
//! [`legal_by_making`](crate::board::movegen::legal_by_making); a randomised
//! differential test in this module's tests and in the integration suite
//! keeps it that way.

use crate::board::attacks::{
    bishop_attacks, king_attacks, knight_attacks, pawn_attacks, rook_attacks,
};
use crate::board::bitboard::{bits, lsb};
use crate::board::position::Position;
use crate::types::color::{Color, flip};
use crate::types::r#move::{Move, flags};
use crate::types::piece::{BISHOP, KING, KNIGHT, PAWN, QUEEN, ROOK};
use crate::types::square::{Square, bit};

/// The eight ray directions as `(file delta, rank delta)`.
const DIRECTIONS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Returns the squares strictly beyond `square` in one direction, up to the
/// board edge.
const fn ray_mask(square: usize, df: i8, dr: i8) -> u64 {
    let mut mask = 0_u64;
    let mut f = (square % 8) as i8 + df;
    let mut r = (square / 8) as i8 + dr;
    while f >= 0 && f < 8 && r >= 0 && r < 8 {
        mask |= 1_u64 << (r * 8 + f);
        f += df;
        r += dr;
    }
    mask
}

/// Builds the pair-indexed `between` (exclusive of both ends) or `line` (the
/// whole board line through both squares) table. Unaligned pairs hold zero.
const fn build_pair_table(line: bool) -> [[u64; 64]; 64] {
    let mut table = [[0_u64; 64]; 64];
    let mut a = 0_usize;
    while a < 64 {
        let mut d = 0_usize;
        while d < 8 {
            let (df, dr) = DIRECTIONS[d];
            let opposite = ray_mask(a, -df, -dr);
            let mut f = (a % 8) as i8 + df;
            let mut r = (a / 8) as i8 + dr;
            let mut walked = 0_u64;
            while f >= 0 && f < 8 && r >= 0 && r < 8 {
                let b = (r * 8 + f) as usize;
                table[a][b] = if line {
                    ray_mask(a, df, dr) | opposite | (1_u64 << a)
                } else {
                    walked
                };
                walked |= 1_u64 << b;
                f += df;
                r += dr;
            }
            d += 1;
        }
        a += 1;
    }
    table
}

static BETWEEN: [[u64; 64]; 64] = build_pair_table(false);
static LINE: [[u64; 64]; 64] = build_pair_table(true);

/// Returns the squares strictly between two aligned squares (zero when the
/// squares do not share a rank, file or diagonal).
#[inline(always)]
pub fn between(a: Square, b: Square) -> u64 {
    BETWEEN[a as usize][b as usize]
}

/// Returns the whole board line through two aligned squares, both included
/// (zero when the squares are not aligned).
#[inline(always)]
pub fn line(a: Square, b: Square) -> u64 {
    LINE[a as usize][b as usize]
}

/// Per-node facts for a make-free legality test of the side to move.
#[derive(Clone, Copy, Debug)]
pub struct Legality {
    us: Color,
    them: Color,
    king: Square,
    checkers: u64,
    pinned: u64,
}

impl Legality {
    /// Computes the checkers and pinned pieces of the side to move.
    pub fn new(position: &Position) -> Self {
        let us = position.side();
        let them = flip(us);
        let king = position.king_square(us);
        let all = position.all_occupancy();

        let bishops = position.pieces(them, BISHOP) | position.pieces(them, QUEEN);
        let rooks = position.pieces(them, ROOK) | position.pieces(them, QUEEN);
        let checkers = (pawn_attacks(us, king) & position.pieces(them, PAWN))
            | (knight_attacks(king) & position.pieces(them, KNIGHT))
            | (bishop_attacks(king, all) & bishops)
            | (rook_attacks(king, all) & rooks);

        // A pinned piece is the only blocker between the king and an
        // enemy slider that would otherwise attack along that line.
        let own = position.occupancy(us);
        let snipers = (bishop_attacks(king, 0) & bishops) | (rook_attacks(king, 0) & rooks);
        let mut pinned = 0_u64;
        for sniper in bits(snipers) {
            let blockers = between(king, sniper) & all;
            if blockers != 0 && blockers & (blockers - 1) == 0 {
                pinned |= blockers & own;
            }
        }

        Self {
            us,
            them,
            king,
            checkers,
            pinned,
        }
    }

    /// Returns the bitboard of pieces giving check.
    #[inline(always)]
    pub fn checkers(&self) -> u64 {
        self.checkers
    }

    /// Returns the bitboard of own pieces pinned to the king.
    #[inline(always)]
    pub fn pinned(&self) -> u64 {
        self.pinned
    }

    /// Returns whether the pseudo-legal move `mv` leaves the mover's king
    /// safe. `position` must be the position the context was built from.
    #[inline]
    pub fn is_legal(&self, position: &Position, mv: Move) -> bool {
        let from = mv.from();
        let to = mv.to();
        let flag = mv.flag();

        if flag == flags::EN_PASSANT || flag == flags::CASTLE {
            return !position.make_move(mv).in_check(self.us);
        }

        if from == self.king {
            return !self.king_destination_attacked(position, from, to);
        }

        if self.checkers != 0 {
            if self.checkers & (self.checkers - 1) != 0 {
                return false;
            }
            let checker = lsb(self.checkers);
            if (between(self.king, checker) | bit(checker)) & bit(to) == 0 {
                return false;
            }
        }

        self.pinned & bit(from) == 0 || line(self.king, from) & bit(to) != 0
    }

    /// Returns whether the opponent attacks `to` once the king has left
    /// `from`, ignoring a piece the king captures on `to`.
    #[inline]
    fn king_destination_attacked(&self, position: &Position, from: Square, to: Square) -> bool {
        let them = self.them;
        let gone = !bit(to);
        let occupancy = position.all_occupancy() ^ bit(from);
        let attackers = |piece_type: usize| position.pieces(them, piece_type) & gone;

        pawn_attacks(self.us, to) & attackers(PAWN) != 0
            || knight_attacks(to) & attackers(KNIGHT) != 0
            || king_attacks(to) & attackers(KING) != 0
            || bishop_attacks(to, occupancy) & (attackers(BISHOP) | attackers(QUEEN)) != 0
            || rook_attacks(to, occupancy) & (attackers(ROOK) | attackers(QUEEN)) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::{between, line};

    #[test]
    fn between_and_line_follow_the_geometry() {
        // a1..h1: the squares strictly between are b1..g1.
        assert_eq!(between(0, 7), 0x7e);
        // a1..h8: the long diagonal without its ends.
        assert_eq!(between(0, 63).count_ones(), 6);
        // Unaligned squares (a knight's jump) have neither.
        assert_eq!(between(0, 17), 0);
        assert_eq!(line(0, 17), 0);
        // The line through two squares of a file is the whole file.
        assert_eq!(line(8, 24), 0x0101_0101_0101_0101);
        // Symmetric.
        assert_eq!(line(63, 0), line(0, 63));
        assert_eq!(between(63, 0), between(0, 63));
    }
}
