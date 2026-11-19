//! Syzygy placement indexing and encoding.
//!
//! A tablebase file stores one value per legal placement of its material.
//! Placements are canonically reduced (mirror, diagonal flip, piece order) and
//! mapped to a dense index by combinatorial number systems: the two-king
//! triangle index for king-only positions, the "leading piece" decomposition
//! for piece positions, and the file/rank pawn twists for pawn positions.
//!
//! These constants are format data, not tuning values. They are reproduced
//! here so the reader can map an arbitrary position to its file index without
//! external tables.

use std::sync::OnceLock;

use super::pairs::Bytes;
use super::pos::TB_PIECES;

/// Encoding kind: a piece (king-only) table.
pub(crate) const PIECE_ENC: usize = 0;
/// Encoding kind: pawns sorted by file.
pub(crate) const FILE_ENC: usize = 1;

/// One side of a rank or diagonal occupancy differs; drives the triangle flip.
#[rustfmt::skip]
const OFF_DIAG: [i8; 64] = [
    0,-1,-1,-1,-1,-1,-1,-1, 1, 0,-1,-1,-1,-1,-1,-1, 1, 1, 0,-1,-1,-1,-1,-1,
    1, 1, 1, 0,-1,-1,-1,-1, 1, 1, 1, 1, 0,-1,-1,-1, 1, 1, 1, 1, 1, 0,-1,-1,
    1, 1, 1, 1, 1, 1, 0,-1, 1, 1, 1, 1, 1, 1, 1, 0,
];

/// Triangle index of a square (0..9) for the two-king encoding.
#[rustfmt::skip]
const TRIANGLE: [u8; 64] = [
    6, 0, 1, 2, 2, 1, 0, 6, 0, 7, 3, 4, 4, 3, 7, 0, 1, 3, 8, 5, 5, 8, 3, 1,
    2, 4, 5, 9, 9, 5, 4, 2, 2, 4, 5, 9, 9, 5, 4, 2, 1, 3, 8, 5, 5, 8, 3, 1,
    0, 7, 3, 4, 4, 3, 7, 0, 6, 0, 1, 2, 2, 1, 0, 6,
];

/// Mirrors a square across the a1-h8 diagonal.
#[rustfmt::skip]
const FLIP_DIAG: [u8; 64] = [
    0, 8,16,24,32,40,48,56, 1, 9,17,25,33,41,49,57, 2,10,18,26,34,42,50,58,
    3,11,19,27,35,43,51,59, 4,12,20,28,36,44,52,60, 5,13,21,29,37,45,53,61,
    6,14,22,30,38,46,54,62, 7,15,23,31,39,47,55,63,
];

/// Lower-triangle index of a square.
#[rustfmt::skip]
const LOWER: [u8; 64] = [
    28, 0, 1, 2, 3, 4, 5, 6, 0,29, 7, 8, 9,10,11,12, 1, 7,30,13,14,15,16,17,
     2, 8,13,31,18,19,20,21, 3, 9,14,18,32,22,23,24, 4,10,15,19,22,33,25,26,
     5,11,16,20,23,25,34,27, 6,12,17,21,24,26,27,35,
];

/// Diagonal index of a square.
#[rustfmt::skip]
const DIAG: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 8, 0, 1, 0, 0, 0, 0, 9, 0, 0, 0, 2, 0, 0,10, 0, 0,
    0, 0, 0, 3,11, 0, 0, 0, 0, 0, 0,12, 4, 0, 0, 0, 0, 0,13, 0, 0, 5, 0, 0,
    0,14, 0, 0, 0, 0, 6, 0,15, 0, 0, 0, 0, 0, 0, 7,
];

/// Pawn twist order by file and by rank.
#[rustfmt::skip]
const FLAP: [[u8; 64]; 2] = [
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 6,12,18,18,12, 6, 0, 1, 7,13,19,19,13, 7, 1,
        2, 8,14,20,20,14, 8, 2, 3, 9,15,21,21,15, 9, 3, 4,10,16,22,22,16,10, 4,
        5,11,17,23,23,17,11, 5, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 3, 2, 1, 0, 4, 5, 6, 7, 7, 6, 5, 4,
        8, 9,10,11,11,10, 9, 8,12,13,14,15,15,14,13,12,16,17,18,19,19,18,17,16,
        20,21,22,23,23,22,21,20, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
];

/// Pawn square twist per encoding.
#[rustfmt::skip]
const PAWN_TWIST: [[u8; 64]; 2] = [
    [
        0, 0, 0, 0, 0, 0, 0, 0,47,35,23,11,10,22,34,46,45,33,21, 9, 8,20,32,44,
        43,31,19, 7, 6,18,30,42,41,29,17, 5, 4,16,28,40,39,27,15, 3, 2,14,26,38,
        37,25,13, 1, 0,12,24,36, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0,47,45,43,41,40,42,44,46,39,37,35,33,32,34,36,38,
        31,29,27,25,24,26,28,30,23,21,19,17,16,18,20,22,15,13,11, 9, 8,10,12,14,
         7, 5, 3, 1, 0, 2, 4, 6, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
];

/// Two-king placement index, indexed by the first king's triangle and the
/// second king's square.
#[rustfmt::skip]
const KK_IDX: [[i16; 64]; 10] = [
    [-1,-1,-1, 0, 1, 2, 3, 4,-1,-1,-1, 5, 6, 7, 8, 9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57],
    [58,-1,-1,-1,59,60,61,62,63,-1,-1,-1,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115],
    [116,117,-1,-1,-1,118,119,120,121,122,-1,-1,-1,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173],
    [174,-1,-1,-1,175,176,177,178,179,-1,-1,-1,180,181,182,183,184,-1,-1,-1,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228],
    [229,230,-1,-1,-1,231,232,233,234,235,-1,-1,-1,236,237,238,239,240,-1,-1,-1,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255,256,257,258,259,260,261,262,263,264,265,266,267,268,269,270,271,272,273,274,275,276,277,278,279,280,281,282,283],
    [284,285,286,287,288,289,290,291,292,293,-1,-1,-1,294,295,296,297,298,-1,-1,-1,299,300,301,302,303,-1,-1,-1,304,305,306,307,308,309,310,311,312,313,314,315,316,317,318,319,320,321,322,323,324,325,326,327,328,329,330,331,332,333,334,335,336,337,338],
    [-1,-1,339,340,341,342,343,344,-1,-1,345,346,347,348,349,350,-1,-1,441,351,352,353,354,355,-1,-1,-1,442,356,357,358,359,-1,-1,-1,-1,443,360,361,362,-1,-1,-1,-1,-1,444,363,364,-1,-1,-1,-1,-1,-1,445,365,-1,-1,-1,-1,-1,-1,-1,446],
    [-1,-1,-1,366,367,368,369,370,-1,-1,-1,371,372,373,374,375,-1,-1,-1,376,377,378,379,380,-1,-1,-1,447,381,382,383,384,-1,-1,-1,-1,448,385,386,387,-1,-1,-1,-1,-1,449,388,389,-1,-1,-1,-1,-1,-1,450,390,-1,-1,-1,-1,-1,-1,-1,451],
    [452,391,392,393,394,395,396,397,-1,-1,-1,-1,398,399,400,401,-1,-1,-1,-1,402,403,404,405,-1,-1,-1,-1,406,407,408,409,-1,-1,-1,-1,453,410,411,412,-1,-1,-1,-1,-1,454,413,414,-1,-1,-1,-1,-1,-1,455,415,-1,-1,-1,-1,-1,-1,-1,456],
    [457,416,417,418,419,420,421,422,-1,458,423,424,425,426,427,428,-1,-1,-1,-1,-1,429,430,431,-1,-1,-1,-1,-1,432,433,434,-1,-1,-1,-1,-1,435,436,437,-1,-1,-1,-1,-1,459,438,439,-1,-1,-1,-1,-1,-1,460,440,-1,-1,-1,-1,-1,-1,-1,461],
];

/// File mirror table used by the pawn file encoding.
const FILE_TO_FILE: [u8; 8] = [0, 1, 2, 3, 3, 2, 1, 0];

/// Maps a WDL value (-2..=2, offset by two) to a DTZ map slot.
pub(crate) const WDL_TO_MAP: [usize; 5] = [1, 3, 0, 2, 0];
/// Pawn-table flags by WDL slot (whether the value is stored doubled).
pub(crate) const PA_FLAGS: [u8; 5] = [8, 0, 0, 0, 4];
/// Curated DTZ base value per WDL slot.
pub(crate) const WDL_TO_DTZ: [i32; 5] = [-1, -101, 0, 101, 1];

/// Material-independent indexing tables, built once per process.
pub(crate) struct IndexTables {
    /// Binomial coefficients `C(i, j)`.
    pub(crate) binomial_: [[u64; 64]; TB_PIECES],
    /// Pawn base index per `[encoding][pawn count - 1][flap]`.
    pub(crate) pawn_idx: [[[u64; 24]; 6]; 2],
    /// Pawn factor per file.
    pub(crate) pawn_factor_file: [[u64; 4]; 6],
}

/// Returns the shared indexing tables. The first call initialises them.
pub(crate) fn tables() -> &'static IndexTables {
    static TABLES: OnceLock<IndexTables> = OnceLock::new();
    TABLES.get_or_init(build_tables)
}

/// Builds the binomial and pawn-factor tables.
fn build_tables() -> IndexTables {
    let mut binomial_ = [[0_u64; 64]; TB_PIECES];
    for (index, row) in binomial_.iter_mut().enumerate() {
        for (column, slot) in row.iter_mut().enumerate() {
            if index == 0 {
                *slot = 1;
                continue;
            }
            if column < index {
                // Never addressed: every use has at least `index` squares left.
                continue;
            }
            let mut numerator = 1_u64;
            let mut denominator = 1_u64;
            for step in 0..index {
                numerator = numerator.wrapping_mul((column - step) as u64);
                denominator = denominator.wrapping_mul((step + 1) as u64);
            }
            *slot = numerator / denominator;
        }
    }

    let mut pawn_idx = [[[0_u64; 24]; 6]; 2];
    let mut pawn_factor_file = [[0_u64; 4]; 6];

    for i in 0..6 {
        let mut sum = 0_u64;
        for j in 0..24 {
            pawn_idx[0][i][j] = sum;
            sum = sum
                .wrapping_add(binomial_[i][PAWN_TWIST[0][((1 + (j % 6)) * 8) + (j / 6)] as usize]);
            if (j + 1) % 6 == 0 {
                pawn_factor_file[i][j / 6] = sum;
                sum = 0;
            }
        }
    }

    IndexTables {
        binomial_,
        pawn_idx,
        pawn_factor_file,
    }
}

/// Everything `encode` needs to know about the table's material.
#[derive(Clone, Copy)]
pub(crate) struct EncShape {
    /// Number of pieces on the board.
    pub(crate) num: usize,
    /// Whether the table uses the compact two-king encoding.
    pub(crate) kk_enc: bool,
    /// Pawns of the leading colour.
    pub(crate) pawns0: usize,
    /// Pawns of the other colour.
    pub(crate) pawns1: usize,
}

/// Per-table encoding factors and the piece groups of one side.
#[derive(Clone)]
pub(crate) struct EncInfo {
    /// Decompression table (set after the header is read).
    pub(crate) precomp: Option<super::pairs::PairsData>,
    /// Placement factor per piece group.
    pub(crate) factor: [u64; TB_PIECES],
    /// Piece codes in the table's canonical order.
    pub(crate) pieces: [u8; TB_PIECES],
    /// Size of the piece group starting at each slot.
    pub(crate) norm: [u8; TB_PIECES],
}

impl Default for EncInfo {
    fn default() -> Self {
        Self {
            precomp: None,
            factor: [0; TB_PIECES],
            pieces: [0; TB_PIECES],
            norm: [0; TB_PIECES],
        }
    }
}

/// Sorts the leading pawn into first place and returns its flap value.
pub(crate) fn leading_pawn(p: &mut [i32; TB_PIECES], pawns0: usize, enc: usize) -> usize {
    for i in 1..pawns0 {
        if FLAP[enc - 1][p[0] as usize] > FLAP[enc - 1][p[i] as usize] {
            p.swap(0, i);
        }
    }
    if enc == FILE_ENC {
        FILE_TO_FILE[p[0] as usize & 7] as usize
    } else {
        (p[0] as usize - 8) >> 3
    }
}

/// Computes the dense placement index of `p` for one table.
pub(crate) fn encode(p: &mut [i32; TB_PIECES], ei: &EncInfo, shape: &EncShape, enc: usize) -> u64 {
    let binary = tables();
    let n = shape.num;
    let mut index;

    if p[0] & 0x04 != 0 {
        for slot in p.iter_mut().take(n) {
            *slot ^= 0x07;
        }
    }

    if enc == PIECE_ENC {
        if p[0] & 0x20 != 0 {
            for slot in p.iter_mut().take(n) {
                *slot ^= 0x38;
            }
        }
        for i in 0..n {
            if OFF_DIAG[p[i] as usize] != 0 {
                if OFF_DIAG[p[i] as usize] > 0 && i < if shape.kk_enc { 2 } else { 3 } {
                    for slot in p.iter_mut().take(n) {
                        *slot = FLIP_DIAG[*slot as usize] as i32;
                    }
                }
                break;
            }
        }

        if shape.kk_enc {
            let kk = KK_IDX[TRIANGLE[p[0] as usize] as usize][p[1] as usize];
            if kk < 0 {
                return 0;
            }
            index = kk as u64;
        } else {
            let s1 = i32::from(p[1] > p[0]);
            let s2 = i32::from(p[2] > p[0]) + i32::from(p[2] > p[1]);
            if OFF_DIAG[p[0] as usize] != 0 {
                index = TRIANGLE[p[0] as usize] as u64 * 63 * 62
                    + (p[1] - s1) as u64 * 62
                    + (p[2] - s2) as u64;
            } else if OFF_DIAG[p[1] as usize] != 0 {
                index = 6 * 63 * 62
                    + DIAG[p[0] as usize] as u64 * 28 * 62
                    + LOWER[p[1] as usize] as u64 * 62
                    + (p[2] - s2) as u64;
            } else if OFF_DIAG[p[2] as usize] != 0 {
                index = 6 * 63 * 62
                    + 4 * 28 * 62
                    + DIAG[p[0] as usize] as u64 * 7 * 28
                    + (DIAG[p[1] as usize] as i32 - s1) as u64 * 28
                    + LOWER[p[2] as usize] as u64;
            } else {
                index = 6 * 63 * 62
                    + 4 * 28 * 62
                    + 4 * 7 * 28
                    + DIAG[p[0] as usize] as u64 * 7 * 6
                    + (DIAG[p[1] as usize] as i32 - s1) as u64 * 6
                    + (DIAG[p[2] as usize] as i32 - s2) as u64;
            }
        }
        index * ei.factor[0]
    } else {
        for i in 1..shape.pawns0 {
            for j in (i + 1)..shape.pawns0 {
                if PAWN_TWIST[enc - 1][p[i] as usize] < PAWN_TWIST[enc - 1][p[j] as usize] {
                    p.swap(i, j);
                }
            }
        }

        let k = shape.pawns0;
        index = binary.pawn_idx[enc - 1][k - 1][FLAP[enc - 1][p[0] as usize] as usize];
        for i in 1..k {
            index = index
                .wrapping_add(binary.binomial_[k - i][PAWN_TWIST[enc - 1][p[i] as usize] as usize]);
        }
        index *= ei.factor[0];

        let mut group_end = k;
        if shape.pawns1 != 0 {
            let end = k + shape.pawns1;
            for i in k..end {
                for j in (i + 1)..end {
                    if p[i] > p[j] {
                        p.swap(i, j);
                    }
                }
            }
            let mut sum = 0_u64;
            for i in k..end {
                let square = p[i] as usize;
                let mut skips = 0_usize;
                for &other in p.iter().take(k) {
                    skips += usize::from(square > other as usize);
                }
                sum = sum.wrapping_add(
                    binary.binomial_[i - k + 1][square.wrapping_sub(skips).wrapping_sub(8)],
                );
            }
            index = index.wrapping_add(sum.wrapping_mul(ei.factor[k]));
            group_end = end;
        }

        let mut k = group_end;
        while k < n {
            let end = k + ei.norm[k] as usize;
            for i in k..end {
                for j in (i + 1)..end {
                    if p[i] > p[j] {
                        p.swap(i, j);
                    }
                }
            }
            let mut sum = 0_u64;
            for i in k..end {
                let square = p[i] as usize;
                let mut skips = 0_usize;
                for &other in p.iter().take(k) {
                    skips += usize::from(square > other as usize);
                }
                sum = sum.wrapping_add(binary.binomial_[i - k + 1][square.wrapping_sub(skips)]);
            }
            index = index.wrapping_add(sum.wrapping_mul(ei.factor[k]));
            k = end;
        }
        index
    }
}

/// `C(k, n)` with the same shape as the reference helper.
fn subfactor(k: usize, n: usize) -> u64 {
    let mut numerator = n as u64;
    let mut denominator = 1_u64;
    for i in 1..k {
        numerator = numerator.wrapping_mul((n - i) as u64);
        denominator = denominator.wrapping_mul((i + 1) as u64);
    }
    numerator / denominator
}

/// Reads one side's header and returns its placement factor product.
pub(crate) fn init_enc_info(
    file: &Bytes,
    base: u64,
    shift: u32,
    t: usize,
    enc: usize,
    shape: &EncShape,
) -> Option<(u64, EncInfo)> {
    let more_pawns = enc != PIECE_ENC && shape.pawns1 > 0;
    let mut ei = EncInfo::default();

    for i in 0..shape.num {
        let byte = file.u8(base + i as u64 + 1 + u64::from(more_pawns))?;
        ei.pieces[i] = (byte >> shift) & 0x0f;
        ei.norm[i] = 0;
    }

    let order = ((file.u8(base)? >> shift) & 0x0f) as usize;
    let order2 = if more_pawns {
        ((file.u8(base + 1)? >> shift) & 0x0f) as usize
    } else {
        0x0f
    };

    let mut k = if enc != PIECE_ENC {
        shape.pawns0
    } else if shape.kk_enc {
        2
    } else {
        3
    };
    ei.norm[0] = k as u8;

    if more_pawns {
        ei.norm[k] = shape.pawns1 as u8;
        k += ei.norm[k] as usize;
    }

    let mut i = k;
    while i < shape.num {
        let mut j = i;
        while j < shape.num && ei.pieces[j] == ei.pieces[i] {
            ei.norm[i] = ei.norm[i].saturating_add(1);
            j += 1;
        }
        if ei.norm[i] == 0 {
            return None;
        }
        i += ei.norm[i] as usize;
    }

    let mut n = 64 - k;
    let mut f = 1_u64;
    let mut i = 0_usize;
    while k < shape.num || i == order || i == order2 {
        if i == order {
            ei.factor[0] = f;
            let multiplier = if enc == FILE_ENC {
                tables().pawn_factor_file[ei.norm[0] as usize - 1][t]
            } else if shape.kk_enc {
                462
            } else {
                31332
            };
            f = f.wrapping_mul(multiplier);
        } else if i == order2 {
            let group = ei.norm[0] as usize;
            ei.factor[group] = f;
            f = f.wrapping_mul(subfactor(ei.norm[group] as usize, 48 - group));
        } else {
            ei.factor[k] = f;
            f = f.wrapping_mul(subfactor(ei.norm[k] as usize, n));
            n -= ei.norm[k] as usize;
            k += ei.norm[k] as usize;
        }
        i += 1;
        if i > 64 {
            return None;
        }
    }

    Some((f, ei))
}
