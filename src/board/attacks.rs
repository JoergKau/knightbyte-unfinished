//! Attack tables and sliding-attack generation.
//!
//! Knight/king/pawn attacks are precomputed tables. Slider attacks use BMI2
//! PEXT tables on hosts with fast BMI2 (Intel Haswell+, AMD Zen 3+), falling
//! back to the direction-walking rays on hosts without BMI2 or with microcoded
//! `pext` (Zen 1/2, Excavator) — selected once per process, with
//! `KNIGHTBYTE_FORCE_PEXT=1` and `KNIGHTBYTE_DISABLE_PEXT=1` as A/B overrides.
//!
//! The PEXT tables are **filled from the ray-walking implementation**, so both
//! backends produce bit-identical attack sets by construction: the choice
//! changes only speed, never the attack sets, the move generation or the
//! search tree (the migration-plan node-identity gates are unaffected).

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

/// The relevant-occupancy mask of `square` for one slider kind: the reachable
/// squares up to (excluding) the board edge.
fn relevant_mask(square: Square, diagonal: bool) -> u64 {
    let directions: [(i8, i8); 4] = if diagonal {
        [(1, 1), (1, -1), (-1, 1), (-1, -1)]
    } else {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
    };
    let f0 = file(square) as i8;
    let r0 = rank(square) as i8;
    let mut mask = 0_u64;
    for (df, dr) in directions {
        let mut f = f0 + df;
        let mut r = r0 + dr;
        // Walk while the NEXT square in this direction stays on the board:
        // the edge square is excluded from the relevant occupancy.
        while (0..8).contains(&(f + df)) && (0..8).contains(&(r + dr)) {
            mask |= 1_u64 << from_file_rank(f as u8, r as u8);
            f += df;
            r += dr;
        }
    }
    mask
}

/// BMI2 PEXT sliding-attack tables, mirroring the predecessor's
/// `PextBitboards`: relevant-occupancy masks and per-square offsets, with the
/// tables **filled from the ray-walking implementation** — bit-identical to it
/// by construction, so the dispatch changes only speed, never the attack sets.
///
/// Selected once per process: BMI2 present, not disabled through
/// `KNIGHTBYTE_DISABLE_PEXT=1`, and not a slow-BMI2 CPU (Zen 1/2, Excavator),
/// with `KNIGHTBYTE_FORCE_PEXT=1` as the A/B override.
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
struct PextTables {
    rook_masks: [u64; 64],
    bishop_masks: [u64; 64],
    rook_offsets: [usize; 64],
    bishop_offsets: [usize; 64],
    rook_table: Vec<u64>,
    bishop_table: Vec<u64>,
}

#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
impl PextTables {
    /// Fills the tables from the ray-walking attacks (bit-identical to the
    /// magic-style reference construction).
    fn build() -> Self {
        let mut rook_masks = [0_u64; 64];
        let mut bishop_masks = [0_u64; 64];
        let mut rook_offsets = [0_usize; 64];
        let mut bishop_offsets = [0_usize; 64];
        let mut rook_total = 0_usize;
        let mut bishop_total = 0_usize;

        for square in 0..64_usize {
            let slider = square as Square;
            rook_masks[square] = relevant_mask(slider, false);
            bishop_masks[square] = relevant_mask(slider, true);
            rook_offsets[square] = rook_total;
            bishop_offsets[square] = bishop_total;
            rook_total += 1_usize << rook_masks[square].count_ones();
            bishop_total += 1_usize << bishop_masks[square].count_ones();
        }

        let mut rook_table = vec![0_u64; rook_total];
        let mut bishop_table = vec![0_u64; bishop_total];

        // The Carry-Rippler trick enumerates every subset of the mask.
        for square in 0..64_usize {
            let slider = square as Square;
            for (diagonal, mask, offsets, table) in [
                (
                    false,
                    rook_masks[square],
                    rook_offsets[square],
                    &mut rook_table,
                ),
                (
                    true,
                    bishop_masks[square],
                    bishop_offsets[square],
                    &mut bishop_table,
                ),
            ] {
                let mut subset = 0_u64;
                let mut index = 0_usize;
                loop {
                    // The fill uses the ray walks directly — NOT the public
                    // `rook_attacks`/`bishop_attacks`, which re-enter the
                    // `pext_tables()` initialisation and deadlock on it.
                    table[offsets + index] = if diagonal {
                        ray(slider, 1, 1, subset)
                            | ray(slider, 1, -1, subset)
                            | ray(slider, -1, 1, subset)
                            | ray(slider, -1, -1, subset)
                    } else {
                        ray(slider, 1, 0, subset)
                            | ray(slider, -1, 0, subset)
                            | ray(slider, 0, 1, subset)
                            | ray(slider, 0, -1, subset)
                    };
                    index += 1;
                    subset = subset.wrapping_sub(mask) & mask;
                    if subset == 0 {
                        break;
                    }
                }
            }
        }

        Self {
            rook_masks,
            bishop_masks,
            rook_offsets,
            bishop_offsets,
            rook_table,
            bishop_table,
        }
    }

    /// Returns the rook attacks for `occupancy` via one BMI2 `pext` lookup.
    fn rook_attacks(&self, square: usize, occupancy: u64) -> u64 {
        let index = pext_index(occupancy, self.rook_masks[square]);
        self.rook_table[self.rook_offsets[square] + index]
    }

    /// Returns the bishop attacks for `occupancy` via one BMI2 `pext` lookup.
    fn bishop_attacks(&self, square: usize, occupancy: u64) -> u64 {
        let index = pext_index(occupancy, self.bishop_masks[square]);
        self.bishop_table[self.bishop_offsets[square] + index]
    }
}

/// Extracts the relevant occupancy bits into a compact index: the k-th set
/// bit of the mask maps to the k-th bit of the index, matching the
/// Carry-Rippler subset enumeration order of the table build.
///
/// On x86-64 this is the single BMI2 `pext` instruction; the PEXT tables are
/// only ever built after the CPU feature check, which is what makes the
/// instruction safe to issue. Elsewhere (and for the `#[cfg(test)]` oracle)
/// [`pext_index_software`] computes the same index.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn pext_index(occupancy: u64, mask: u64) -> usize {
    let index: u64;
    // SAFETY: `pext` is a BMI2 instruction. `pext_index` is only reached
    // through `PextTables`, and `pext_tables()` builds those tables only when
    // `is_x86_feature_detected!("bmi2")` is true. The instruction reads two
    // registers and writes one: no memory, no stack, no flags.
    unsafe {
        core::arch::asm!(
            "pext {index}, {occupancy}, {mask}",
            index = lateout(reg) index,
            occupancy = in(reg) occupancy,
            mask = in(reg) mask,
            options(pure, nomem, nostack, preserves_flags),
        );
    }
    index as usize
}

/// Portable form of [`pext_index`] for targets without BMI2.
#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
fn pext_index(occupancy: u64, mask: u64) -> usize {
    pext_index_software(occupancy, mask)
}

/// Software `pext`: the portable fallback and the test oracle.
#[cfg(any(test, not(target_arch = "x86_64")))]
#[inline]
fn pext_index_software(occupancy: u64, mask: u64) -> usize {
    let mut index = 0_usize;
    let mut bit = 0_usize;
    let mut rest = mask;
    while rest != 0 {
        let lowest = rest.trailing_zeros();
        if occupancy & (1_u64 << lowest) != 0 {
            index |= 1_usize << bit;
        }
        rest &= rest - 1;
        bit += 1;
    }
    index
}

#[cfg(target_arch = "x86_64")]
static PEXT_TABLES: std::sync::OnceLock<Option<PextTables>> = std::sync::OnceLock::new();

/// Returns the PEXT tables when the host can run them, or `None` when the
/// ray-walking fallback stays.
#[cfg(not(target_arch = "x86_64"))]
fn pext_tables() -> Option<&'static PextTables> {
    None
}

/// Returns the PEXT tables when the host can run them, or `None` when the
/// ray-walking fallback stays.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn pext_tables() -> Option<&'static PextTables> {
    PEXT_TABLES
        .get_or_init(|| {
            let bmi2 = std::arch::is_x86_feature_detected!("bmi2");
            let force = std::env::var("KNIGHTBYTE_FORCE_PEXT").as_deref() == Ok("1");
            let disable = std::env::var("KNIGHTBYTE_DISABLE_PEXT").as_deref() == Ok("1");
            // PEXT needs BMI2 hardware; the ADR-0001 rule keeps it off CPUs where
            // `pext` is microcoded (Zen 1/2, Excavator).
            if bmi2 && !disable && (force || !crate::platform::cpu::has_slow_bmi2()) {
                Some(PextTables::build())
            } else {
                None
            }
        })
        .as_ref()
}

/// Returns rook attacks from `square` with the given occupancy.
pub fn rook_attacks(square: Square, occupancy: u64) -> u64 {
    if let Some(tables) = pext_tables() {
        return tables.rook_attacks(square as usize, occupancy);
    }
    ray(square, 1, 0, occupancy)
        | ray(square, -1, 0, occupancy)
        | ray(square, 0, 1, occupancy)
        | ray(square, 0, -1, occupancy)
}

/// Returns bishop attacks from `square` with the given occupancy.
pub fn bishop_attacks(square: Square, occupancy: u64) -> u64 {
    if let Some(tables) = pext_tables() {
        return tables.bishop_attacks(square as usize, occupancy);
    }
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
    #[cfg(target_arch = "x86_64")]
    use crate::types::square::Square;
    use crate::types::square::from_algebraic;

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn the_hardware_pext_index_matches_the_software_index() {
        if !std::arch::is_x86_feature_detected!("bmi2") {
            return;
        }
        // A fixed xorshift stream of occupancies against every real mask.
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        for square in 0..64 {
            for diagonal in [false, true] {
                let mask = super::relevant_mask(square, diagonal);
                for _ in 0..200 {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    assert_eq!(
                        super::pext_index(state, mask),
                        super::pext_index_software(state, mask),
                        "square {square} diagonal {diagonal}"
                    );
                }
            }
        }
    }

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

    /// Every PEXT lookup must match the ray-walking attacks for the same
    /// subset occupancy: the fill and the lookup are pinned together.
    /// PEXT tables only exist on x86-64; AArch64 always uses the ray walk.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn pext_lookups_match_the_ray_walk_for_every_relevant_subset() {
        let tables = super::pext_tables().expect("this host must expose BMI2 for the test");
        for square in 0..64_usize {
            for (diagonal, masks, offsets, table) in [
                (
                    false,
                    tables.rook_masks[square],
                    tables.rook_offsets[square],
                    &tables.rook_table,
                ),
                (
                    true,
                    tables.bishop_masks[square],
                    tables.bishop_offsets[square],
                    &tables.bishop_table,
                ),
            ] {
                let bits = masks.count_ones();
                for index in 0..(1_usize << bits) {
                    // The subset occupancy for this compact index: the k-th
                    // set index bit sits on the k-th set mask bit.
                    let mut occupancy = 0_u64;
                    let mut rest = masks;
                    let mut bit = 0_usize;
                    while rest != 0 {
                        let lowest = rest.trailing_zeros();
                        if index & (1 << bit) != 0 {
                            occupancy |= 1_u64 << lowest;
                        }
                        rest &= rest - 1;
                        bit += 1;
                    }
                    let expected = if diagonal {
                        bishop_ray(square as Square, occupancy)
                    } else {
                        rook_ray(square as Square, occupancy)
                    };
                    let actual = table[offsets + index];
                    assert_eq!(
                        actual, expected,
                        "square {square} subset {index}: lookup diverges"
                    );
                }
            }
        }
    }

    /// The ray-walking attacks (the table-fill oracle, same file).
    #[cfg(target_arch = "x86_64")]
    fn rook_ray(square: Square, occupancy: u64) -> u64 {
        super::ray(square, 1, 0, occupancy)
            | super::ray(square, -1, 0, occupancy)
            | super::ray(square, 0, 1, occupancy)
            | super::ray(square, 0, -1, occupancy)
    }

    /// The ray-walking bishop attacks (the table-fill oracle, same file).
    #[cfg(target_arch = "x86_64")]
    fn bishop_ray(square: Square, occupancy: u64) -> u64 {
        super::ray(square, 1, 1, occupancy)
            | super::ray(square, 1, -1, occupancy)
            | super::ray(square, -1, 1, occupancy)
            | super::ray(square, -1, -1, occupancy)
    }
}
