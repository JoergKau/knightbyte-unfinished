//! Move-ordering histories.
//!
//! Three table families cooperate:
//!
//! * **Butterfly history** — one score per `(colour, from, to)`. It remembers
//!   how often a quiet move caused a cutoff anywhere in the tree.
//! * **Capture history** — one score per `(attacker type, target, victim
//!   type)`, colour-agnostic. It refines capture ordering beyond material
//!   values.
//! * **Continuation history** — four tables, one per ply distance (1, 2, 4
//!   and 6), each a `(colour, previous target, from, to)` cell. A quiet move
//!   is conditioned on the destination square of the move played at that
//!   distance earlier; a cell is present only when a move was actually played
//!   that long ago on the current path.
//!
//! Every update uses the reference's gravity formula
//! `h += u - h*|u|/MAX`, which keeps a cell inside its table-specific bound
//! without a hard clamp: the asymptote is the bound. Bonuses and maluses are
//! linear in depth, and the cutoff move earns a bonus while the quiet moves
//! that already failed low earn a malus — in the butterfly table and in every
//! present continuation table. Capture cutoffs update the capture history
//! only: captures never write the butterfly or continuation tables.
//!
//! Tables are aged by 7/8 after each completed iteration from depth 4 on. That
//! keeps the steady-state magnitude near +/-4,100, which is the scale the
//! late-move-reduction history term assumes.
//!
//! The tables are flat boxed slices with index helpers. The four continuation
//! tables together are a few megabytes; building them as nested array literals
//! would put a multi-megabyte temporary on the stack, which is not acceptable
//! on platforms with a 1 MiB main-thread stack.

use crate::types::color::Color;
use crate::types::r#move::Move;
use crate::types::piece::Piece;
use crate::types::square::Square;

/// Bound of the butterfly table.
pub const BUTTERFLY_MAX: i32 = 8_033;
/// Bound of every continuation table.
pub const CONTINUATION_MAX: i32 = 8_003;
/// Bound of the capture table.
pub const CAPTURE_MAX: i32 = 8_488;

/// Fixed-point unit of the late-move-reduction history term, in 1024ths of a
/// ply.
pub const LMR_UNIT: i32 = 1_024;
/// Combined quiet score is clamped to this before the term is applied.
pub const LMR_HISTORY_CLAMP: i32 = 8_033;
/// Divisor of the quiet history term, in 1024ths of a ply.
const LMR_QUIET_HISTORY_DIV: i32 = 3_419;
/// Divisor of the noisy history term (unused: captures are never reduced by
/// history in the live schedule).
const LMR_NOISY_HISTORY_DIV: i32 = 3_257;

/// Returns the late-move-reduction history contribution in 1024ths of a ply.
///
/// The combined history score is clamped to +/-8,033 first; the `precise` form
/// multiplies before dividing so ordinary history scores keep their sub-ply
/// fraction instead of truncating to zero.
pub fn lmr_history_term_1024(combined_history: i32, quiet: bool, precise: bool) -> i32 {
    let history = combined_history.clamp(-LMR_HISTORY_CLAMP, LMR_HISTORY_CLAMP);
    let divisor = if quiet {
        LMR_QUIET_HISTORY_DIV
    } else {
        LMR_NOISY_HISTORY_DIV
    };
    if precise {
        history * LMR_UNIT / divisor
    } else {
        history / divisor * LMR_UNIT
    }
}

/// Largest butterfly bonus.
const BUTTERFLY_BONUS_MAX: i32 = 1_202;
/// Butterfly bonus per depth ply.
const BUTTERFLY_BONUS_SCALE: i32 = 207;
/// Largest butterfly malus.
const BUTTERFLY_MALUS_MAX: i32 = 1_255;
/// Butterfly malus per depth ply.
const BUTTERFLY_MALUS_SCALE: i32 = 200;

/// Largest continuation bonus.
const CONTINUATION_BONUS_MAX: i32 = 1_229;
/// Continuation bonus per depth ply.
const CONTINUATION_BONUS_SCALE: i32 = 204;
/// Largest continuation malus.
const CONTINUATION_MALUS_MAX: i32 = 1_067;
/// Continuation malus per depth ply.
const CONTINUATION_MALUS_SCALE: i32 = 205;

/// Largest capture bonus.
const CAPTURE_BONUS_MAX: i32 = 1_250;
/// Capture bonus per depth ply.
const CAPTURE_BONUS_SCALE: i32 = 192;

/// Ply distances of the four continuation tables.
pub const CONTINUATION_DISTANCES: [usize; 4] = [1, 2, 4, 6];

const BUTTERFLY_LEN: usize = 2 * 64 * 64;
const CAPTURE_LEN: usize = 6 * 64 * 6;
const CONTINUATION_LEN: usize = 2 * 64 * 64 * 64;
const COUNTER_LEN: usize = 64 * 64;

/// The continuation-history context of one node: the destination square of the
/// move played 1, 2, 4 and 6 plies earlier, or `None` when no move exists at
/// that distance. The order matches [`CONTINUATION_DISTANCES`].
pub type Continuations = [Option<Square>; 4];

/// The move that produced a position: the piece that moved (before any
/// promotion), its origin square and its destination square.
///
/// The countermove table is keyed by the two squares; the correction history's
/// countermove term is keyed by the piece type and the destination.
pub type Previous = (Piece, Square, Square);

/// Butterfly, capture, continuation and countermove tables.
pub struct History {
    butterfly: Box<[i16]>,
    capture: Box<[i16]>,
    continuation: Box<[i16]>,
    counter: Box<[Move]>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// Allocates empty tables.
    pub fn new() -> Self {
        Self {
            butterfly: zeros(BUTTERFLY_LEN),
            capture: zeros(CAPTURE_LEN),
            continuation: zeros(CONTINUATION_LEN * CONTINUATION_DISTANCES.len()),
            counter: vec![Move::NONE; COUNTER_LEN].into_boxed_slice(),
        }
    }

    /// Returns the allocated size in bytes.
    pub fn size_bytes(&self) -> usize {
        (self.butterfly.len() + self.capture.len() + self.continuation.len())
            * core::mem::size_of::<i16>()
            + self.counter.len() * core::mem::size_of::<Move>()
    }

    /// Empties every table.
    pub fn clear(&mut self) {
        self.butterfly.fill(0);
        self.capture.fill(0);
        self.continuation.fill(0);
        self.counter.fill(Move::NONE);
    }

    /// Forgets the countermove table only; the score tables are kept.
    pub fn clear_counters(&mut self) {
        self.counter.fill(Move::NONE);
    }

    /// Ages every score by 7/8.
    pub fn age(&mut self) {
        for entry in self.butterfly.iter_mut() {
            *entry = (i32::from(*entry) * 7 / 8) as i16;
        }
        for entry in self.capture.iter_mut() {
            *entry = (i32::from(*entry) * 7 / 8) as i16;
        }
        for entry in self.continuation.iter_mut() {
            *entry = (i32::from(*entry) * 7 / 8) as i16;
        }
    }

    /// Returns the butterfly score for a quiet move.
    #[inline]
    pub fn butterfly(&self, color: Color, mv: Move) -> i32 {
        i32::from(self.butterfly[butterfly_index(color, mv)])
    }

    /// Returns the raw capture score for `attacker` (a piece *type* in
    /// `0..6`) capturing `victim` (also a type) on `mv.to()`.
    #[inline]
    pub fn capture(&self, attacker: usize, mv: Move, victim: usize) -> i32 {
        i32::from(self.capture[capture_index(attacker, mv, victim)])
    }

    /// Returns the combined quiet ordering score: the butterfly cell plus the
    /// 1-ply continuation in full and the 2/4/6-ply continuations halved.
    ///
    /// This is the score the staged picker orders quiet moves — including
    /// quiet promotions — by.
    pub fn picker_score(&self, color: Color, mv: Move, cont: &Continuations) -> i32 {
        let mut score = self.butterfly(color, mv);
        for (table, _) in CONTINUATION_DISTANCES.iter().enumerate() {
            if let Some(previous_to) = cont[table] {
                let cell = self.continuation_cell(color, table, previous_to, mv);
                score += if table == 0 { cell } else { cell / 2 };
            }
        }
        score
    }

    /// Returns the combined pruning score: the butterfly cell plus *all four*
    /// continuation cells halved, clamped to the butterfly bound.
    ///
    /// This is the score the late-move-reduction history term and the history
    /// pruner consume.
    pub fn pruning_score(&self, color: Color, mv: Move, cont: &Continuations) -> i32 {
        let mut score = self.butterfly(color, mv);
        for (table, _) in CONTINUATION_DISTANCES.iter().enumerate() {
            if let Some(previous_to) = cont[table] {
                score += self.continuation_cell(color, table, previous_to, mv) / 2;
            }
        }
        score.clamp(-BUTTERFLY_MAX, BUTTERFLY_MAX)
    }

    /// Returns one continuation cell.
    #[inline]
    fn continuation_cell(&self, color: Color, table: usize, previous_to: Square, mv: Move) -> i32 {
        i32::from(self.continuation[continuation_index(color, table, previous_to, mv)])
    }

    /// Returns the remembered countermove for `previous`, if any.
    #[inline]
    pub fn counter_move(&self, previous: Option<Previous>) -> Move {
        match previous {
            Some((_, previous_from, previous_to)) => {
                self.counter[counter_index(previous_from, previous_to)]
            }
            None => Move::NONE,
        }
    }

    /// Credits a quiet move that caused a cutoff and penalises the quiets
    /// that were searched before it and failed low.
    ///
    /// The bonus reaches the butterfly table and every present continuation
    /// table; the malus is applied to the same set for every failed quiet.
    pub fn update_quiet(
        &mut self,
        color: Color,
        mv: Move,
        depth: i32,
        bad_quiets: &[Move],
        cont: &Continuations,
    ) {
        let bonus = (BUTTERFLY_BONUS_SCALE * depth).min(BUTTERFLY_BONUS_MAX);
        let slot = butterfly_index(color, mv);
        gravity(&mut self.butterfly[slot], bonus, BUTTERFLY_MAX);

        for (table, _) in CONTINUATION_DISTANCES.iter().enumerate() {
            if let Some(previous_to) = cont[table] {
                let slot = continuation_index(color, table, previous_to, mv);
                let bonus = (CONTINUATION_BONUS_SCALE * depth).min(CONTINUATION_BONUS_MAX);
                gravity(&mut self.continuation[slot], bonus, CONTINUATION_MAX);
            }
        }

        let malus = (BUTTERFLY_MALUS_SCALE * depth).min(BUTTERFLY_MALUS_MAX);
        let cont_malus = (CONTINUATION_MALUS_SCALE * depth).min(CONTINUATION_MALUS_MAX);
        for &bad in bad_quiets {
            let slot = butterfly_index(color, bad);
            gravity(&mut self.butterfly[slot], -malus, BUTTERFLY_MAX);
            for (table, _) in CONTINUATION_DISTANCES.iter().enumerate() {
                if let Some(previous_to) = cont[table] {
                    let slot = continuation_index(color, table, previous_to, bad);
                    gravity(&mut self.continuation[slot], -cont_malus, CONTINUATION_MAX);
                }
            }
        }
    }

    /// Credits a capture that caused a cutoff.
    ///
    /// Only the capture history is written: captures never touch the
    /// butterfly or continuation tables, which are quiet-move signals.
    pub fn update_capture(&mut self, attacker: usize, mv: Move, victim: usize, depth: i32) {
        let bonus = (CAPTURE_BONUS_SCALE * depth).min(CAPTURE_BONUS_MAX);
        let slot = capture_index(attacker, mv, victim);
        gravity(&mut self.capture[slot], bonus, CAPTURE_MAX);
    }

    /// Applies a plain butterfly bonus (used by the early
    /// transposition-table-cutoff update).
    pub fn update_butterfly_bonus(&mut self, color: Color, mv: Move, bonus: i32) {
        let slot = butterfly_index(color, mv);
        gravity(&mut self.butterfly[slot], bonus, BUTTERFLY_MAX);
    }

    /// Remembers `reply` as the countermove to `previous`.
    ///
    /// The reply is stored in identity form (no double-push marker); see
    /// [`Move::without_double_push`]. [`History::counter_move`] therefore
    /// returns that form, and callers compare against it the same way.
    pub fn update_counter(&mut self, previous: Option<Previous>, reply: Move) {
        if let Some((_, previous_from, previous_to)) = previous {
            self.counter[counter_index(previous_from, previous_to)] = reply.without_double_push();
        }
    }
}

/// Returns a zeroed history slice.
fn zeros(len: usize) -> Box<[i16]> {
    vec![0_i16; len].into_boxed_slice()
}

/// Applies the gravity update `h += u - h*|u|/MAX`.
///
/// The result stays inside the bound for any in-range inputs, so no explicit
/// clamp is applied; the asymptote is the bound-keeper.
fn gravity(entry: &mut i16, update: i32, max: i32) {
    let value = i32::from(*entry);
    let next = value + update - value * update.abs() / max;
    *entry = next as i16;
}

#[inline(always)]
fn butterfly_index(color: Color, mv: Move) -> usize {
    (color as usize * 64 + mv.from() as usize) * 64 + mv.to() as usize
}

#[inline(always)]
fn capture_index(attacker: usize, mv: Move, victim: usize) -> usize {
    (attacker * 64 + mv.to() as usize) * 6 + victim
}

#[inline(always)]
fn continuation_index(color: Color, table: usize, previous_to: Square, mv: Move) -> usize {
    (((table * 2 + color as usize) * 64 + previous_to as usize) * 64 + mv.from() as usize) * 64
        + mv.to() as usize
}

#[inline(always)]
fn counter_index(previous_from: Square, previous_to: Square) -> usize {
    previous_from as usize * 64 + previous_to as usize
}

#[cfg(test)]
mod tests {
    use super::{
        BUTTERFLY_MAX, CAPTURE_MAX, CONTINUATION_DISTANCES, CONTINUATION_MAX, Continuations,
        History, capture_index, continuation_index,
    };
    use crate::types::color::{BLACK, WHITE};
    use crate::types::r#move::{Move, flags};
    use crate::types::piece::{PAWN, QUEEN, WN, WP};

    fn mv(from: u8, to: u8) -> Move {
        Move::new(from, to, flags::NORMAL)
    }

    /// A continuation context where only the 1-ply distance has a move (to square 8).
    fn cont() -> Continuations {
        [Some(8), None, None, None]
    }

    #[test]
    fn tables_start_empty_and_are_separated_by_colour() {
        let history = History::new();
        assert_eq!(history.butterfly(WHITE, mv(0, 8)), 0);
        assert_eq!(history.capture(QUEEN, mv(8, 16), PAWN), 0);
        assert_eq!(history.picker_score(WHITE, mv(0, 8), &cont()), 0);
        assert!(history.counter_move(None).is_none());
        assert!(history.size_bytes() > 1_000_000);
        // The capture table is colour-agnostic: types index it, not pieces.
        assert_ne!(
            capture_index(PAWN, mv(8, 16), QUEEN),
            capture_index(QUEEN, mv(8, 16), PAWN)
        );
    }

    #[test]
    fn quiet_update_scores_butterfly_and_continuation() {
        let mut history = History::new();
        let context = cont();
        history.update_quiet(WHITE, mv(9, 17), 5, &[], &context);
        assert!(history.butterfly(WHITE, mv(9, 17)) > 0);
        assert!(history.picker_score(WHITE, mv(9, 17), &context) > 0);
        assert_eq!(history.butterfly(BLACK, mv(9, 17)), 0);
        assert_eq!(history.picker_score(BLACK, mv(9, 17), &context), 0);
    }

    #[test]
    fn picker_score_weights_the_first_continuation_in_full() {
        let mut history = History::new();
        let context = cont();
        history.update_quiet(WHITE, mv(9, 17), 5, &[], &context);
        // The 1-ply continuation cell earned a bonus; the picker adds it in
        // full, the pruning score at half weight.
        let butterfly = history.butterfly(WHITE, mv(9, 17));
        let cont_cell = history.picker_score(WHITE, mv(9, 17), &context) - butterfly;
        assert!(cont_cell > 0);
        assert_eq!(
            history.pruning_score(WHITE, mv(9, 17), &context),
            butterfly + cont_cell / 2
        );
    }

    #[test]
    fn malus_lowers_previously_tried_quiets() {
        let mut history = History::new();
        let context = cont();
        let bad = mv(1, 9);
        history.update_quiet(WHITE, bad, 3, &[], &context);
        let before = history.butterfly(WHITE, bad);
        history.update_quiet(WHITE, mv(2, 10), 5, &[bad], &context);
        assert!(history.butterfly(WHITE, bad) < before);
        assert!(history.pruning_score(WHITE, bad, &context) < 0);
    }

    #[test]
    fn capture_update_is_keyed_by_piece_types_only() {
        let mut history = History::new();
        history.update_capture(QUEEN, mv(8, 16), PAWN, 4);
        assert!(history.capture(QUEEN, mv(8, 16), PAWN) > 0);
        assert_eq!(history.capture(QUEEN, mv(8, 16), 3), 0);
        assert_eq!(history.butterfly(WHITE, mv(8, 16)), 0);
    }

    #[test]
    fn gravity_keeps_values_inside_each_bound() {
        let mut history = History::new();
        let context = cont();
        for _ in 0..2_000 {
            history.update_quiet(WHITE, mv(8, 16), 30, &[], &context);
            history.update_capture(QUEEN, mv(0, 9), PAWN, 30);
        }
        assert!(history.butterfly(WHITE, mv(8, 16)) <= BUTTERFLY_MAX);
        assert!(history.butterfly(WHITE, mv(8, 16)) > 0);
        assert!(history.capture(QUEEN, mv(0, 9), PAWN) <= CAPTURE_MAX);
        assert!(history.pruning_score(WHITE, mv(8, 16), &context).abs() <= BUTTERFLY_MAX);
        let _ = CONTINUATION_MAX;
    }

    #[test]
    fn age_reduces_by_seven_eighths() {
        let mut history = History::new();
        let context = cont();
        history.update_quiet(WHITE, mv(8, 16), 4, &[], &context);
        let before = history.butterfly(WHITE, mv(8, 16));
        history.age();
        let after = history.butterfly(WHITE, mv(8, 16));
        assert_eq!(after, before * 7 / 8);
        assert!(after > 0);
    }

    #[test]
    fn countermove_is_stored_per_previous_move() {
        let mut history = History::new();
        let previous = Some((WP, 0, 8));
        let reply = mv(16, 24);
        history.update_counter(previous, reply);
        assert_eq!(history.counter_move(previous), reply);
        assert!(history.counter_move(Some((WP, 0, 9))).is_none());
        // The table is keyed by origin and destination, not by piece: the
        // same destination from another origin square is a different cell.
        assert!(history.counter_move(Some((WP, 1, 8))).is_none());
        assert_eq!(history.counter_move(Some((WN, 0, 8))), reply);
        assert!(history.counter_move(None).is_none());
    }

    #[test]
    fn clear_empties_every_table() {
        let mut history = History::new();
        let context = cont();
        let previous = Some((WP, 0, 8));
        history.update_quiet(WHITE, mv(9, 17), 5, &[], &context);
        history.update_capture(3, mv(0, 9), 1, 4);
        history.update_counter(previous, mv(1, 2));
        history.clear();
        assert_eq!(history.butterfly(WHITE, mv(9, 17)), 0);
        assert_eq!(history.capture(3, mv(0, 9), 1), 0);
        assert_eq!(history.picker_score(WHITE, mv(9, 17), &context), 0);
        assert!(history.counter_move(previous).is_none());
    }

    #[test]
    fn continuation_distances_are_the_reference_set() {
        assert_eq!(CONTINUATION_DISTANCES, [1, 2, 4, 6]);
        // Every distance has its own slice of the table.
        let mut history = History::new();
        for distance in 0..CONTINUATION_DISTANCES.len() {
            let mut context: Continuations = [None; 4];
            context[distance] = Some(8);
            history.update_quiet(
                WHITE,
                mv(distance as u8, 16 + distance as u8),
                4,
                &[],
                &context,
            );
            assert!(
                history.picker_score(WHITE, mv(distance as u8, 16 + distance as u8), &context) > 0
            );
        }
        let _ = continuation_index(WHITE, 1, 0, mv(0, 0));
    }
}
