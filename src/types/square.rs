//! Square indices and algebraic conversion.
//!
//! Squares use the little-endian rank-file mapping: `a1 = 0`, `h1 = 7`,
//! `a8 = 56`, `h8 = 63`. The value `64` is the "no square" sentinel.

/// A board square in `0..64`, or [`NO_SQUARE`].
pub type Square = u8;

/// Sentinel for "no square" (for example, no en-passant target).
pub const NO_SQUARE: Square = 64;

/// Returns the file (`0..7`, a-file is 0).
#[inline(always)]
pub const fn file(square: Square) -> u8 {
    square & 7
}

/// Returns the rank (`0..7`, rank 1 is 0).
#[inline(always)]
pub const fn rank(square: Square) -> u8 {
    square >> 3
}

/// Builds a square from a file and rank.
#[inline(always)]
pub const fn from_file_rank(file: u8, rank: u8) -> Square {
    rank * 8 + file
}

/// Returns the single-bit mask of a square.
#[inline(always)]
pub const fn bit(square: Square) -> u64 {
    1_u64 << square
}

/// Parses a two-character algebraic square such as `e4`.
pub fn from_algebraic(text: &str) -> Option<Square> {
    let bytes = text.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let file = bytes[0].checked_sub(b'a')?;
    let rank = bytes[1].checked_sub(b'1')?;
    if file > 7 || rank > 7 {
        return None;
    }
    Some(from_file_rank(file, rank))
}

/// Formats a square as lowercase algebraic notation.
pub fn to_algebraic(square: Square) -> String {
    debug_assert!(square < 64);
    let file = (b'a' + file(square)) as char;
    let rank = (b'1' + rank(square)) as char;
    format!("{file}{rank}")
}

#[cfg(test)]
mod tests {
    use super::{NO_SQUARE, bit, file, from_algebraic, from_file_rank, rank, to_algebraic};

    #[test]
    fn algebraic_round_trip() {
        assert_eq!(from_algebraic("a1"), Some(0));
        assert_eq!(from_algebraic("h8"), Some(63));
        assert_eq!(from_algebraic("e4"), Some(from_file_rank(4, 3)));
        assert_eq!(to_algebraic(0), "a1");
        assert_eq!(to_algebraic(63), "h8");
        assert_eq!(to_algebraic(from_file_rank(4, 3)), "e4");
    }

    #[test]
    fn rejects_invalid_algebraic_input() {
        assert_eq!(from_algebraic(""), None);
        assert_eq!(from_algebraic("a"), None);
        assert_eq!(from_algebraic("i1"), None);
        assert_eq!(from_algebraic("a9"), None);
    }

    #[test]
    fn file_rank_and_bit_are_consistent() {
        let square = from_file_rank(4, 3);
        assert_eq!(file(square), 4);
        assert_eq!(rank(square), 3);
        assert_eq!(bit(square), 1_u64 << square);
        assert_eq!(NO_SQUARE, 64);
    }
}
