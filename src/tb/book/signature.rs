//! CTG `.ctg` position signatures.
//!
//! A `.ctg` book identifies a position by a bit stream: the 64 board squares
//! Huffman-coded in a canonical frame, followed by castling and en-passant
//! flag bits. The first byte holds the signature length in its low five bits
//! plus a castling and an en-passant marker.
//!
//! The canonical frame is "white to move": a black-to-move position is
//! rank-flipped (with the piece colours swapped), and a position without
//! castling rights whose side-to-move king stands on the queenside is
//! file-mirrored. The en-passant square is only recorded when an en-passant
//! capture is actually available, which is stricter than the engine's own FEN
//! convention.

use crate::board::position::Position;
use crate::types::color::{BLACK, WHITE};
use crate::types::piece::{BP, EMPTY, Piece, WP, is_white};

/// Huffman code and bit length per piece value (`0` = empty, `1..=12`).
const HUFFMAN: [(u16, u8); 13] = [
    (0x0, 1),  // Empty
    (0x3, 3),  // White pawn
    (0x9, 5),  // White knight
    (0x5, 5),  // White bishop
    (0xD, 5),  // White rook
    (0x11, 6), // White queen
    (0x1, 6),  // White king
    (0x7, 3),  // Black pawn
    (0x19, 5), // Black knight
    (0x15, 5), // Black bishop
    (0x1D, 5), // Black rook
    (0x31, 6), // Black queen
    (0x21, 6), // Black king
];

/// Returns whether the position must be file-mirrored before encoding.
pub fn is_mirrored(position: &Position) -> bool {
    if position.castling_rights() != 0 {
        return false;
    }
    let king = position.king_square(position.side());
    (king & 7) < 4
}

/// Encodes `position` and returns the signature bytes and their length.
///
/// The returned buffer is always 32 bytes long; only the first `length` bytes
/// are part of the signature. A legal position never needs more than 31.
pub fn compute(position: &Position) -> ([u8; 32], usize) {
    let mut buffer = [0_u8; 32];
    let flip = position.side() == BLACK;
    let mirror = is_mirrored(position);

    let mut bit_position = 8_usize;
    for file in 0..8_u8 {
        for rank in 0..8_u8 {
            let source_file = if mirror { 7 - file } else { file };
            let source_rank = if flip { 7 - rank } else { rank };
            let mut piece = position.piece_at(source_rank * 8 + source_file);
            if flip && piece != EMPTY {
                piece = swap_piece_color(piece);
            }
            let (bits, length) = HUFFMAN[piece as usize];
            append_bits(&mut buffer, &mut bit_position, bits, length);
        }
    }

    // Castling and en-passant flags, expressed from the canonical white side.
    let rights = position.castling_rights();
    let castle: u16 = if flip {
        // Canonical white is the real black side.
        (if rights & 4 != 0 { 4 } else { 0 })
            + (if rights & 8 != 0 { 8 } else { 0 })
            + (if rights & 1 != 0 { 1 } else { 0 })
            + (if rights & 2 != 0 { 2 } else { 0 })
    } else {
        (if rights & 1 != 0 { 4 } else { 0 })
            + (if rights & 2 != 0 { 8 } else { 0 })
            + (if rights & 4 != 0 { 1 } else { 0 })
            + (if rights & 8 != 0 { 2 } else { 0 })
    };

    let mut ep_file: Option<u8> = None;
    if en_passant_capture_possible(position) {
        let target = position.en_passant();
        let mut file = target & 7;
        if mirror {
            file = 7 - file;
        }
        ep_file = Some(file);
    }

    let mut flag_bits = castle;
    let mut flag_length = if castle != 0 { 4 } else { 0 };
    if let Some(file) = ep_file {
        flag_bits = (castle << 3) | u16::from(file);
        flag_length += 3;
    }

    // Pad so the flags end flush at a byte boundary.
    let remainder = bit_position % 8;
    if 8 - remainder < flag_length as usize {
        for _ in 0..8 - remainder {
            append_bits(&mut buffer, &mut bit_position, 0, 1);
        }
    }
    let remainder = bit_position % 8;
    let padding = (8 - remainder).wrapping_sub(flag_length as usize) % 8;
    for _ in 0..padding {
        append_bits(&mut buffer, &mut bit_position, 0, 1);
    }

    for bit in 0..flag_length {
        append_bits(&mut buffer, &mut bit_position, (flag_bits >> bit) & 1, 1);
    }

    let length = bit_position.div_ceil(8);
    let mut header = length as u8;
    if ep_file.is_some() {
        header |= 1 << 5;
    }
    if castle != 0 {
        header |= 1 << 6;
    }
    buffer[0] = header;
    (buffer, length)
}

/// Returns whether the side to move could actually capture en passant.
///
/// The engine keeps the en-passant square after every double push; CTG
/// only records it when a capturing pawn stands next to it.
fn en_passant_capture_possible(position: &Position) -> bool {
    let target = position.en_passant();
    if target >= 64 {
        return false;
    }
    let target_file = target & 7;
    let target_rank = target >> 3;
    let pawn_rank = if position.side() == WHITE {
        target_rank.wrapping_sub(1)
    } else {
        target_rank + 1
    };
    if pawn_rank > 7 {
        return false;
    }
    let capturer = if position.side() == WHITE { BP } else { WP };
    for delta in [-1_i8, 1] {
        let file = target_file as i8 + delta;
        if (0..8).contains(&file) && position.piece_at(pawn_rank * 8 + file as u8) == capturer {
            return true;
        }
    }
    false
}

/// Swaps the colour bits of a colour-coded piece.
fn swap_piece_color(piece: Piece) -> Piece {
    if is_white(piece) {
        piece + 6
    } else {
        piece - 6
    }
}

/// Appends `bits` LSB-first at `bit_position` into a big-endian byte stream.
fn append_bits(buffer: &mut [u8; 32], bit_position: &mut usize, bits: u16, count: u8) {
    for index in 0..count {
        if bits & (1 << index) != 0 {
            buffer[*bit_position >> 3] |= 1 << (7 - (*bit_position & 7));
        }
        *bit_position += 1;
    }
}
