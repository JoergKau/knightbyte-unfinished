//! Scheme-2 feature indexing: 768 slots, king-region buckets and file mirror.
//!
//! The frame flip depends on the network perspective, not on the side to move:
//! the white perspective sees the real board, the black perspective sees a
//! rank-flipped board. This is what makes the accumulator incrementally
//! updatable when a king does not move.

use crate::board::bitboard::bits;
use crate::board::position::Position;
use crate::eval::nnue::net::{Net, SLOTS_PER_REGION};
use crate::types::color::Color;
use crate::types::piece::{is_white, piece_type};
use crate::types::square::Square;

/// Maximum number of active features in one perspective.
pub const MAX_ACTIVE_FEATURES: usize = 32;

/// Returns the frame flip for a perspective (`0` white, `56` black).
#[inline(always)]
pub const fn frame_flip(perspective: Color) -> Square {
    if perspective == 0 { 0 } else { 56 }
}

/// Returns the king square expressed in a perspective's own frame.
#[inline(always)]
pub fn king_square_in_frame(position: &Position, perspective: Color) -> Square {
    position.king_square(perspective) ^ frame_flip(perspective)
}

/// Returns the king region for a perspective.
#[inline(always)]
pub fn region(net: &Net, position: &Position, perspective: Color) -> usize {
    let king = king_square_in_frame(position, perspective);
    net.region_table()[king as usize] as usize
}

/// Returns the horizontal mirror mask for a perspective.
#[inline(always)]
pub fn mirror(position: &Position, perspective: Color) -> usize {
    let king = king_square_in_frame(position, perspective);
    if king & 7 > 3 { 7 } else { 0 }
}

/// Returns the weight row for one piece in one perspective.
#[inline]
pub fn feature_index(
    net: &Net,
    position: &Position,
    perspective: Color,
    square: Square,
    piece: u8,
) -> usize {
    let own = (perspective == 0) == is_white(piece);
    let color_offset = if own { 0 } else { 384 };
    let local_square = square ^ frame_flip(perspective);
    let slot = color_offset + piece_type(piece) * 64 + local_square as usize;
    region(net, position, perspective) * SLOTS_PER_REGION + (slot ^ mirror(position, perspective))
}

/// Fills `out` with every active feature row of one perspective.
///
/// Returns the number of rows written. The caller supplies space for at least
/// [`MAX_ACTIVE_FEATURES`] entries.
pub fn active_features(
    net: &Net,
    position: &Position,
    perspective: Color,
    out: &mut [usize; MAX_ACTIVE_FEATURES],
) -> usize {
    let base = region(net, position, perspective) * SLOTS_PER_REGION;
    let mirror = mirror(position, perspective);
    let flip = frame_flip(perspective);
    let mut count = 0_usize;

    for color in 0..2_usize {
        for piece_type_index in 0..6_usize {
            let mut board = position.pieces(color as Color, piece_type_index);
            while board != 0 {
                let square = crate::board::bitboard::lsb(board);
                board &= board.wrapping_sub(1);
                let piece = position.piece_at(square);
                let own = (perspective == 0) == is_white(piece);
                let color_offset = if own { 0 } else { 384 };
                let local_square = square ^ flip;
                let slot = color_offset + piece_type_index * 64 + local_square as usize;
                out[count] = base + (slot ^ mirror);
                count += 1;
            }
        }
    }

    debug_assert!(count <= MAX_ACTIVE_FEATURES);
    count
}

/// Returns the active feature rows as a vector; intended for tests and tools.
pub fn active_features_vec(net: &Net, position: &Position, perspective: Color) -> Vec<usize> {
    let mut buffer = [0_usize; MAX_ACTIVE_FEATURES];
    let count = active_features(net, position, perspective, &mut buffer);
    buffer[..count].to_vec()
}

/// Returns a compact feature description for diagnostics.
pub fn describe(net: &Net, position: &Position) -> String {
    let mut parts = Vec::new();
    for perspective in 0..2 {
        let rows = active_features_vec(net, position, perspective);
        parts.push(format!(
            "{}:{}:{}",
            region(net, position, perspective),
            mirror(position, perspective),
            rows.len()
        ));
    }
    parts.join("|")
}

/// Iterates the pieces of one colour as `(square, piece)` pairs.
pub fn pieces_of_color(
    position: &Position,
    color: Color,
) -> impl Iterator<Item = (Square, u8)> + '_ {
    (0..6).flat_map(move |piece_type_index| {
        bits(position.pieces(color, piece_type_index))
            .map(move |square| (square, position.piece_at(square)))
    })
}
