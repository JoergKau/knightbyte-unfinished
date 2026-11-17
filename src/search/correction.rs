//! Correction history: a learned offset for the static evaluation.
//!
//! The neural network's static evaluation is systematically optimistic or
//! pessimistic for whole classes of positions. Correction history learns that
//! bias: whenever a completed search disagrees with the corrected static
//! evaluation, the difference is folded into tables keyed by pawn structure,
//! each side's non-pawn material and the opponent's last move.
//!
//! Four tables contribute, each a depth-weighted running average of
//! `score - eval`:
//!
//! | Term | Key | Weight |
//! | :--- | :--- | ---: |
//! | pawn | `pawn_key & 16383`, per side to move | 99 % |
//! | white non-pawn | white material key, per side to move | 100 % |
//! | black non-pawn | black material key, per side to move | 100 % |
//! | countermove | previous mover piece and target, per side to move | 96 % |
//!
//! The weighted sum divided by 64 is added to the raw evaluation, clamped to
//! `±29999`. Cell values are stored in the reference's unit: `diff * 64` with a
//! blend weight of `min(depth + 1, 16)`, clamped to `±2048`, which bounds the
//! total correction to roughly ±127 centipawns.
//!
//! Only the corrected value drives pruning. The transposition table must keep
//! caching the raw evaluation so a cached value stays reusable across positions
//! that share a key.

use crate::board::position::Position;
use crate::board::zobrist;
use crate::search::history::Previous;
use crate::types::piece::piece_type;

/// Number of key slots per side in the hash tables.
const TABLE_SIZE: usize = 16_384;

/// Mask applied to a 64-bit key to index a table.
const TABLE_MASK: u64 = (TABLE_SIZE - 1) as u64;

/// Blend denominator of the running average.
const SCALE: i32 = 64;

/// Highest absolute value a table cell may hold.
const MAX_VALUE: i32 = 2_048;

/// Largest correction the four terms together may produce.
const MAX_CORRECTION: i32 = 29_999;

/// The four correction tables.
pub struct CorrectionHistory {
    pawn: Box<[i16]>,
    non_pawn: Box<[i16]>,
    counter: Box<[i16]>,
}

impl Default for CorrectionHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl CorrectionHistory {
    /// Allocates empty tables.
    pub fn new() -> Self {
        Self {
            pawn: vec![0; 2 * TABLE_SIZE].into_boxed_slice(),
            non_pawn: vec![0; 2 * 2 * TABLE_SIZE].into_boxed_slice(),
            counter: vec![0; 2 * 6 * 64].into_boxed_slice(),
        }
    }

    /// Empties every table.
    pub fn clear(&mut self) {
        self.pawn.fill(0);
        self.non_pawn.fill(0);
        self.counter.fill(0);
    }

    /// Returns the correction for `position`.
    ///
    /// `previous` is the move that produced the position; it selects the
    /// countermove term.
    pub fn correction(&self, position: &Position, previous: Option<Previous>) -> i32 {
        let keys = zobrist::correction_keys(position);
        let side = position.side() as usize;

        let pawn =
            i32::from(self.pawn[side * TABLE_SIZE + (keys.pawn & TABLE_MASK) as usize]) * 99 / 100;
        let white = i32::from(
            self.non_pawn[side * TABLE_SIZE + (keys.white_non_pawn & TABLE_MASK) as usize],
        );
        let black = i32::from(
            self.non_pawn[(2 + side) * TABLE_SIZE + (keys.black_non_pawn & TABLE_MASK) as usize],
        );
        let counter = previous.map_or(0, |(piece, _, target)| {
            i32::from(self.counter[(side * 6 + piece_type(piece)) * 64 + target as usize]) * 96
                / 100
        });

        (pawn + white + black + counter) / SCALE
    }

    /// Folds the difference between the search result and the corrected
    /// evaluation into every table.
    ///
    /// `corrected_eval` is the evaluation the search actually started from,
    /// including any earlier correction. Callers gate this on the reference's
    /// conditions: not in check, no mate score, a quiet or absent best move, and
    /// an untrusted bound.
    pub fn update(
        &mut self,
        position: &Position,
        previous: Option<Previous>,
        best_score: i32,
        corrected_eval: i32,
        depth: i32,
    ) {
        let diff = best_score - corrected_eval;
        let new_value = diff * SCALE;
        let new_weight = (depth + 1).min(16);
        let old_weight = SCALE - new_weight;

        let keys = zobrist::correction_keys(position);
        let side = position.side() as usize;

        blend(
            &mut self.pawn[side * TABLE_SIZE + (keys.pawn & TABLE_MASK) as usize],
            old_weight,
            new_value,
            new_weight,
        );
        blend(
            &mut self.non_pawn[side * TABLE_SIZE + (keys.white_non_pawn & TABLE_MASK) as usize],
            old_weight,
            new_value,
            new_weight,
        );
        blend(
            &mut self.non_pawn
                [(2 + side) * TABLE_SIZE + (keys.black_non_pawn & TABLE_MASK) as usize],
            old_weight,
            new_value,
            new_weight,
        );
        if let Some((piece, _, target)) = previous {
            blend(
                &mut self.counter[(side * 6 + piece_type(piece)) * 64 + target as usize],
                old_weight,
                new_value,
                new_weight,
            );
        }
    }

    /// Clamps `eval + correction` into the range the search expects.
    #[inline]
    pub fn corrected_eval(&self, eval: i32, correction: i32) -> i32 {
        (eval + correction).clamp(-MAX_CORRECTION, MAX_CORRECTION)
    }
}

/// Blends one cell towards `new_value` and clamps it.
fn blend(cell: &mut i16, old_weight: i32, new_value: i32, new_weight: i32) {
    let old = i32::from(*cell);
    let blended = (old * old_weight + new_value * new_weight) / SCALE;
    *cell = blended.clamp(-MAX_VALUE, MAX_VALUE) as i16;
}

#[cfg(test)]
mod tests {
    use super::{CorrectionHistory, MAX_CORRECTION, MAX_VALUE, SCALE};
    use crate::board::position::Position;
    use crate::types::piece::WP;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn empty_tables_correct_by_zero() {
        let history = CorrectionHistory::new();
        let position = Position::from_fen(START).unwrap();
        assert_eq!(history.correction(&position, None), 0);
    }

    #[test]
    fn update_learns_from_the_diff_and_is_reversible_by_sign() {
        let mut history = CorrectionHistory::new();
        let position = Position::from_fen(START).unwrap();
        history.update(&position, None, 40, 0, 15);
        let learned = history.correction(&position, None);
        assert!(learned > 0, "a positive diff must raise the correction");

        history.clear();
        history.update(&position, None, -40, 0, 15);
        assert!(
            history.correction(&position, None) < 0,
            "a negative diff must lower the correction"
        );
    }

    #[test]
    fn correction_is_bounded() {
        let mut history = CorrectionHistory::new();
        let position = Position::from_fen(START).unwrap();
        for _ in 0..200 {
            history.update(&position, None, 29_000, 0, 15);
        }
        let learned = history.correction(&position, None);
        assert!(
            learned.abs() <= MAX_VALUE,
            "cell clamp not applied: {learned}"
        );
        assert_eq!(
            history.corrected_eval(30_000, learned),
            MAX_CORRECTION,
            "the combined correction must clamp"
        );
    }

    #[test]
    fn counter_term_depends_on_the_previous_move() {
        let mut history = CorrectionHistory::new();
        let position = Position::from_fen(START).unwrap();
        let previous = Some((WP, 0, 8));
        history.update(&position, previous, 500, 0, 10);
        assert_ne!(history.correction(&position, previous), 0);
        assert_ne!(
            history.correction(&position, previous),
            history.correction(&position, Some((WP, 0, 9))),
            "a different previous move must miss the countermove cell"
        );
    }

    #[test]
    fn clear_and_scale_are_consistent() {
        assert_eq!(SCALE, 64);
        let mut history = CorrectionHistory::new();
        let position = Position::from_fen(START).unwrap();
        history.update(&position, None, 64, 0, 0);
        // depth 0 -> weight 1: cell = (0 * 63 + 64*64 * 1) / 64 = 64
        let side_cells = history.correction(&position, None);
        assert!(side_cells > 0);
        history.clear();
        assert_eq!(history.correction(&position, None), 0);
    }
}
