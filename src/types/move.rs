//! Bit-packed moves and a fixed-capacity move list.
//!
//! A move is a `u16`: six bits for the source, six for the target and four
//! flag bits. The list lives on the stack; the search must never allocate for
//! move generation.

use crate::types::square::{Square, to_algebraic};

/// Move flag values packed into the high four bits.
pub mod flags {
    /// A normal move (including captures and non-castling king moves).
    pub const NORMAL: u8 = 0;
    /// A pawn double push.
    pub const DOUBLE_PUSH: u8 = 1;
    /// A castling move. The rook is implied by the source and target.
    pub const CASTLE: u8 = 2;
    /// An en-passant capture.
    pub const EN_PASSANT: u8 = 3;
    /// Promotion to a knight.
    pub const PROMO_KNIGHT: u8 = 4;
    /// Promotion to a bishop.
    pub const PROMO_BISHOP: u8 = 5;
    /// Promotion to a rook.
    pub const PROMO_ROOK: u8 = 6;
    /// Promotion to a queen.
    pub const PROMO_QUEEN: u8 = 7;
}

/// A bit-packed chess move.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Move(u16);

const _: () = assert!(core::mem::size_of::<Move>() == 2);

impl Move {
    /// The invalid "no move" sentinel (`a1a1`).
    pub const NONE: Move = Move(0);

    /// Builds a move from its components.
    #[inline(always)]
    pub const fn new(from: Square, to: Square, flag: u8) -> Self {
        debug_assert!(from < 64 && to < 64 && flag < 8);
        Self((from as u16) | ((to as u16) << 6) | ((flag as u16) << 12))
    }

    /// Returns the source square.
    #[inline(always)]
    pub const fn from(self) -> Square {
        (self.0 & 0x3f) as Square
    }

    /// Returns the target square.
    #[inline(always)]
    pub const fn to(self) -> Square {
        ((self.0 >> 6) & 0x3f) as Square
    }

    /// Returns the raw packed bits.
    ///
    /// Used by compact storage such as transposition-table entries. The
    /// encoding is `from | to << 6 | flag << 12`.
    #[inline(always)]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Rebuilds a move from raw packed bits.
    ///
    /// Only bits produced by [`Move::bits`] are meaningful; arbitrary values
    /// can describe encodings that never occur in a real game.
    #[inline(always)]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// Returns the move flag.
    #[inline(always)]
    pub const fn flag(self) -> u8 {
        (self.0 >> 12) as u8
    }

    /// Returns `true` for the no-move sentinel.
    #[inline(always)]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the move is a promotion.
    #[inline(always)]
    pub const fn is_promotion(self) -> bool {
        self.flag() >= flags::PROMO_KNIGHT
    }

    /// Formats the move in UCI notation.
    pub fn to_uci(self) -> String {
        let mut text = format!("{}{}", to_algebraic(self.from()), to_algebraic(self.to()));
        if self.is_promotion() {
            text.push(match self.flag() {
                flags::PROMO_KNIGHT => 'n',
                flags::PROMO_BISHOP => 'b',
                flags::PROMO_ROOK => 'r',
                _ => 'q',
            });
        }
        text
    }
}

/// A fixed-capacity move list (no heap allocation).
pub struct MoveList {
    moves: [Move; 256],
    len: u16,
}

impl MoveList {
    /// Creates an empty list.
    pub const fn new() -> Self {
        Self {
            moves: [Move::NONE; 256],
            len: 0,
        }
    }

    /// Appends a move.
    #[inline(always)]
    pub fn push(&mut self, mv: Move) {
        debug_assert!((self.len as usize) < self.moves.len());
        self.moves[self.len as usize] = mv;
        self.len += 1;
    }

    /// Returns the number of moves.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns `true` when the list is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the moves as a slice.
    #[inline(always)]
    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..self.len as usize]
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Move, MoveList, flags};

    #[test]
    fn move_round_trip() {
        let mv = Move::new(12, 28, flags::DOUBLE_PUSH);
        assert_eq!(mv.from(), 12);
        assert_eq!(mv.to(), 28);
        assert_eq!(mv.flag(), flags::DOUBLE_PUSH);
        assert!(!mv.is_none());
        assert!(!mv.is_promotion());
        assert_eq!(mv.to_uci(), "e2e4");
    }

    #[test]
    fn promotion_uci_suffix() {
        assert_eq!(Move::new(52, 60, flags::PROMO_QUEEN).to_uci(), "e7e8q");
        assert_eq!(Move::new(52, 60, flags::PROMO_KNIGHT).to_uci(), "e7e8n");
        assert!(Move::new(52, 60, flags::PROMO_ROOK).is_promotion());
    }

    #[test]
    fn no_move_sentinel() {
        assert!(Move::NONE.is_none());
        assert_eq!(Move::NONE.to_uci(), "a1a1");
    }

    #[test]
    fn raw_bits_round_trip() {
        let mv = Move::new(52, 60, flags::PROMO_QUEEN);
        assert_eq!(Move::from_bits(mv.bits()), mv);
        assert_eq!(Move::from_bits(0), Move::NONE);
        assert_eq!(mv.bits() >> 12, flags::PROMO_QUEEN as u16);
    }

    #[test]
    fn move_list_is_fixed_capacity() {
        let mut list = MoveList::new();
        assert!(list.is_empty());
        list.push(Move::new(0, 1, flags::NORMAL));
        assert_eq!(list.len(), 1);
        assert_eq!(list.as_slice()[0].to(), 1);
    }
}
