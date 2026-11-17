//! Side-to-move colour.
//!
//! The numeric values match the bitboard and network perspective indices:
//! `0` is White, `1` is Black. They are deliberately not an enum so bit
//! operations (`side ^ 1`) stay cheap and allocation-free.

/// A side to move: `WHITE` or `BLACK`.
pub type Color = u8;

/// White side-to-move value.
pub const WHITE: Color = 0;

/// Black side-to-move value.
pub const BLACK: Color = 1;

/// Returns the opposite colour.
#[inline(always)]
pub const fn flip(color: Color) -> Color {
    color ^ 1
}
