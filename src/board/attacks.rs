//! Attack tables and sliding-attack generation.
//!
//! Phase 2 uses precomputed knight/king/pawn tables and direction-walking
//! sliders. The tables are compile-time constants; the slider walk is easy to
//! audit and is fast enough for correctness work. A later performance phase
//! can replace it with magics or PEXT without changing the board API.

use crate::types::color::Color;
use crate::types::piece::{BISHOP, KING, KNIGHT, PAWN, QUEEN, ROOK};
use crate::types::square::{Square, file, from_file_rank, rank};

const KNIGHT_DELTAS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

const KING_DELTAS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Builds knight attacks for every square.
const fn build_knight_table() -> [u64; 64] {
    let mut table = [0_u64; 64];
    let mut square = 0_usize;
    while square < 64 {
        let f = (square % 8) as i8;
        let r = (square / 8) as i8;
        let mut delta = 0_usize;
        while delta < KNIGHT_DELTAS.len() {
            let (df, dr) = KNIGHT_DELTAS[delta];
            let nf = f + df;
            let nr = r + dr;
            if nf >= 0 && nf < 8 && nr >= 0 && nr < 8 {
                let target = (nr * 8 + nf) as u32;
                table[square] |= 1_u64 << target;
            }
            delta += 1;
        }
        square += 1;
    }
    table
}

/// Builds king attacks for every square.
const fn build_king_table() -> [u64; 64] {
    let mut table = [0_u64; 64];
    let mut square = 0_usize;
    while square < 64 {
        let f = (square % 8) as i8;
        let r = (square / 8) as i8;
        let mut delta = 0_usize;
        while delta < KING_DELTAS.len() {
            let (df, dr) = KING_DELTAS[delta];
            let nf = f + df;
            let nr = r + dr;
            if nf >= 0 && nf < 8 && nr >= 0 && nr < 8 {
                let target = (nr * 8 + nf) as u32;
                table[square] |= 1_u64 << target;
            }
            delta += 1;
        }
        square += 1;
    }
    table
}

/// Builds pawn attacks for both colours.
const fn build_pawn_table() -> [[u64; 64]; 2] {
    let mut table = [[0_u64; 64]; 2];
    let mut color = 0_usize;
    while color < 2 {
        let dr: i8 = if color == 0 { 1 } else { -1 };
        let mut square = 0_usize;
        while square < 64 {
            let f = (square % 8) as i8;
            let r = (square / 8) as i8;
            let nr = r + dr;
            if nr >= 0 && nr < 8 {
                if f > 0 {
                    let target = (nr * 8 + f - 1) as u32;
                    table[color][square] |= 1_u64 << target;
                }
                if f < 7 {
                    let target = (nr * 8 + f + 1) as u32;
                    table[color][square] |= 1_u64 << target;
                }
            }
            square += 1;
        }
        color += 1;
    }
    table
}

static KNIGHT_TABLE: [u64; 64] = build_knight_table();
static KING_TABLE: [u64; 64] = build_king_table();
static PAWN_TABLE: [[u64; 64]; 2] = build_pawn_table();

/// Returns the knight attacks from `square`.
#[inline(always)]
pub fn knight_attacks(square: Square) -> u64 {
    KNIGHT_TABLE[square as usize]
}

/// Returns the king attacks from `square`.
#[inline(always)]
pub fn king_attacks(square: Square) -> u64 {
    KING_TABLE[square as usize]
}

/// Returns the pawn attacks of `color` from `square`.
#[inline(always)]
pub fn pawn_attacks(color: Color, square: Square) -> u64 {
    PAWN_TABLE[color as usize][square as usize]
}

/// Walks one direction until the board edge or a blocker.
#[inline]
fn ray(square: Square, df: i8, dr: i8, occupancy: u64) -> u64 {
    let mut attacks = 0_u64;
    let mut f = file(square) as i8 + df;
    let mut r = rank(square) as i8 + dr;
    while (0..8).contains(&f) && (0..8).contains(&r) {
        let target = from_file_rank(f as u8, r as u8);
        attacks |= 1_u64 << target;
        if occupancy & (1_u64 << target) != 0 {
            break;
        }
        f += df;
        r += dr;
    }
    attacks
}

/// Returns rook attacks from `square` with the given occupancy.
pub fn rook_attacks(square: Square, occupancy: u64) -> u64 {
    ray(square, 1, 0, occupancy)
        | ray(square, -1, 0, occupancy)
        | ray(square, 0, 1, occupancy)
        | ray(square, 0, -1, occupancy)
}

/// Returns bishop attacks from `square` with the given occupancy.
pub fn bishop_attacks(square: Square, occupancy: u64) -> u64 {
    ray(square, 1, 1, occupancy)
        | ray(square, 1, -1, occupancy)
        | ray(square, -1, 1, occupancy)
        | ray(square, -1, -1, occupancy)
}

/// Returns queen attacks from `square` with the given occupancy.
pub fn queen_attacks(square: Square, occupancy: u64) -> u64 {
    rook_attacks(square, occupancy) | bishop_attacks(square, occupancy)
}

/// Returns the attacks of one piece type from a square.
#[inline]
pub fn attacks(piece_type: usize, color: Color, square: Square, occupancy: u64) -> u64 {
    match piece_type {
        PAWN => pawn_attacks(color, square),
        KNIGHT => knight_attacks(square),
        BISHOP => bishop_attacks(square, occupancy),
        ROOK => rook_attacks(square, occupancy),
        QUEEN => queen_attacks(square, occupancy),
        KING => king_attacks(square),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bishop_attacks, king_attacks, knight_attacks, pawn_attacks, queen_attacks, rook_attacks,
    };
    use crate::types::color::{BLACK, WHITE};
    use crate::types::square::from_algebraic;

    #[test]
    fn knight_on_a1_has_two_targets() {
        assert_eq!(
            knight_attacks(from_algebraic("a1").unwrap()).count_ones(),
            2
        );
    }

    #[test]
    fn king_in_the_corner_has_three_targets() {
        assert_eq!(king_attacks(from_algebraic("a1").unwrap()).count_ones(), 3);
    }

    #[test]
    fn pawn_attacks_point_forward() {
        let e4 = from_algebraic("e4").unwrap();
        let white = pawn_attacks(WHITE, e4);
        let black = pawn_attacks(BLACK, e4);
        assert_eq!(white.count_ones(), 2);
        assert_eq!(black.count_ones(), 2);
        assert_eq!(
            white,
            (1_u64 << from_algebraic("d5").unwrap()) | (1_u64 << from_algebraic("f5").unwrap())
        );
        assert_eq!(
            black,
            (1_u64 << from_algebraic("d3").unwrap()) | (1_u64 << from_algebraic("f3").unwrap())
        );
    }

    #[test]
    fn rook_on_empty_board_reaches_the_edge() {
        let d4 = from_algebraic("d4").unwrap();
        assert_eq!(rook_attacks(d4, 0).count_ones(), 14);
        assert_eq!(bishop_attacks(d4, 0).count_ones(), 13);
        assert_eq!(queen_attacks(d4, 0).count_ones(), 27);
    }

    #[test]
    fn blockers_stop_a_ray() {
        let d4 = from_algebraic("d4").unwrap();
        let blocker = 1_u64 << from_algebraic("d6").unwrap();
        let attacks = rook_attacks(d4, blocker);
        assert!(attacks & blocker != 0);
        assert_eq!(attacks & (1_u64 << from_algebraic("d7").unwrap()), 0);
    }
}
