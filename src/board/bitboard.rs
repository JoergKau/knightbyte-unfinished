//! Small bitboard helpers.
//!
//! The board uses plain `u64` bitboards; these helpers keep the hot loops
//! explicit and avoid debug-build overflow panics when clearing a zero mask.

use crate::types::square::Square;

/// Returns the least-significant set-bit index.
///
/// # Panics
///
/// Panics in debug builds when `mask` is zero.
#[inline(always)]
pub fn lsb(mask: u64) -> Square {
    debug_assert!(mask != 0);
    mask.trailing_zeros() as Square
}

/// Clears the least-significant set bit.
#[inline(always)]
pub fn clear_lsb(mask: &mut u64) {
    *mask &= mask.wrapping_sub(1);
}

/// Iterates the set bits of a mask from least to most significant.
pub struct BitIterator(u64);

impl Iterator for BitIterator {
    type Item = Square;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let square = lsb(self.0);
        clear_lsb(&mut self.0);
        Some(square)
    }
}

/// Returns an iterator over the set bits of `mask`.
#[inline(always)]
pub fn bits(mask: u64) -> BitIterator {
    BitIterator(mask)
}

#[cfg(test)]
mod tests {
    use super::{bits, clear_lsb, lsb};

    #[test]
    fn lsb_and_clear() {
        let mut mask = (1_u64 << 3) | (1_u64 << 17);
        assert_eq!(lsb(mask), 3);
        clear_lsb(&mut mask);
        assert_eq!(lsb(mask), 17);
        clear_lsb(&mut mask);
        assert_eq!(mask, 0);
    }

    #[test]
    fn iterator_visits_every_set_bit() {
        let squares: Vec<_> = bits((1_u64 << 0) | (1_u64 << 9) | (1_u64 << 63)).collect();
        assert_eq!(squares, vec![0, 9, 63]);
    }
}
