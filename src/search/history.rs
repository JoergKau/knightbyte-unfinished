//! Move-ordering histories.
//!
//! Three tables cooperate:
//!
//! * **Butterfly history** — one score per `(colour, from, to)`. It remembers
//!   how often a quiet move caused a cutoff anywhere in the tree.
//! * **Capture history** — one score per `(moving piece, target, captured
//!   piece type)`. It refines capture ordering beyond material values.
//! * **Continuation history** — one score per `(previous piece, previous
//!   target, moving piece, target)`. It captures the idea that a good reply
//!   depends on the move that produced the position.
//!
//! Every update uses the reference's gravity formula
//! `h += u - h*|u|/MAX`, which keeps a cell inside its table-specific bound
//! without a hard clamp: the asymptote is the bound. Bonuses and maluses are
//! linear in depth (not quadratic), and the cutoff move earns a bonus while the
//! quiet moves that already failed low earn a malus.
//!
//! Tables are aged by 7/8 after each completed iteration from depth 4 on. That
//! keeps the steady-state magnitude near ±4,100, which is the scale the
//! late-move-reduction history term assumes.
//!
//! The tables are flat boxed slices with index helpers. The continuation table
//! alone is a few megabytes; building that as a nested array literal would put
//! a multi-megabyte temporary on the stack, which is not acceptable on
//! platforms with a 1 MiB main-thread stack.

use crate::types::color::Color;
use crate::types::r#move::Move;
use crate::types::piece::{Piece, color_of};
use crate::types::square::Square;

/// Bound of the butterfly table.
pub const BUTTERFLY_MAX: i32 = 8_033;
/// Bound of the continuation table.
pub const CONTINUATION_MAX: i32 = 8_003;
/// Bound of the capture table.
pub const CAPTURE_MAX: i32 = 8_488;

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

const BUTTERFLY_LEN: usize = 2 * 64 * 64;
const CAPTURE_LEN: usize = 12 * 64 * 6;
const CONTINUATION_LEN: usize = 12 * 64 * 12 * 64;
const COUNTER_LEN: usize = 12 * 64;

/// A position's ancestry as seen by the continuation history: the piece that
/// moved and the square it landed on.
pub type Previous = (Piece, Square);

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
            continuation: zeros(CONTINUATION_LEN),
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

    /// Returns the capture score for `piece` capturing `captured` on `mv.to()`.
    #[inline]
    pub fn capture(&self, piece: Piece, mv: Move, captured: usize) -> i32 {
        i32::from(self.capture[capture_index(piece, mv, captured)])
    }

    /// Returns the continuation score following `previous`.
    #[inline]
    pub fn continuation(&self, previous: Option<Previous>, piece: Piece, mv: Move) -> i32 {
        match previous {
            Some((previous_piece, previous_to)) => i32::from(
                self.continuation[continuation_index(previous_piece, previous_to, piece, mv)],
            ),
            None => 0,
        }
    }

    /// Returns the remembered countermove for `previous`, if any.
    #[inline]
    pub fn counter_move(&self, previous: Option<Previous>) -> Move {
        match previous {
            Some((previous_piece, previous_to)) => {
                self.counter[counter_index(previous_piece, previous_to)]
            }
            None => Move::NONE,
        }
    }

    /// Credits a quiet move that caused a cutoff and penalises the quiets that
    /// were searched before it and failed low.
    pub fn update_quiet(
        &mut self,
        previous: Option<Previous>,
        piece: Piece,
        mv: Move,
        depth: i32,
        bad_quiets: &[Move],
    ) {
        let bonus = (BUTTERFLY_BONUS_SCALE * depth).min(BUTTERFLY_BONUS_MAX);
        let slot = butterfly_index(color_of(piece), mv);
        gravity(&mut self.butterfly[slot], bonus, BUTTERFLY_MAX);

        let malus = (BUTTERFLY_MALUS_SCALE * depth).min(BUTTERFLY_MALUS_MAX);
        let cont_malus = (CONTINUATION_MALUS_SCALE * depth).min(CONTINUATION_MALUS_MAX);
        for &bad in bad_quiets {
            let slot = butterfly_index(color_of(piece), bad);
            gravity(&mut self.butterfly[slot], -malus, BUTTERFLY_MAX);
            if let Some((previous_piece, previous_to)) = previous {
                let slot = continuation_index(previous_piece, previous_to, piece, bad);
                gravity(&mut self.continuation[slot], -cont_malus, CONTINUATION_MAX);
            }
        }

        if let Some((previous_piece, previous_to)) = previous {
            let slot = continuation_index(previous_piece, previous_to, piece, mv);
            let bonus = (CONTINUATION_BONUS_SCALE * depth).min(CONTINUATION_BONUS_MAX);
            gravity(&mut self.continuation[slot], bonus, CONTINUATION_MAX);
        }
    }

    /// Credits a capture that caused a cutoff.
    pub fn update_capture(
        &mut self,
        previous: Option<Previous>,
        piece: Piece,
        mv: Move,
        captured: usize,
        depth: i32,
    ) {
        let bonus = (CAPTURE_BONUS_SCALE * depth).min(CAPTURE_BONUS_MAX);
        let slot = capture_index(piece, mv, captured);
        gravity(&mut self.capture[slot], bonus, CAPTURE_MAX);

        if let Some((previous_piece, previous_to)) = previous {
            let slot = continuation_index(previous_piece, previous_to, piece, mv);
            let bonus = (CONTINUATION_BONUS_SCALE * depth).min(CONTINUATION_BONUS_MAX);
            gravity(&mut self.continuation[slot], bonus, CONTINUATION_MAX);
        }
    }

    /// Remembers `reply` as the countermove to `previous`.
    pub fn update_counter(&mut self, previous: Option<Previous>, reply: Move) {
        if let Some((previous_piece, previous_to)) = previous {
            self.counter[counter_index(previous_piece, previous_to)] = reply;
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
fn capture_index(piece: Piece, mv: Move, captured: usize) -> usize {
    ((piece as usize - 1) * 64 + mv.to() as usize) * 6 + captured
}

#[inline(always)]
fn continuation_index(previous_piece: Piece, previous_to: Square, piece: Piece, mv: Move) -> usize {
    (((previous_piece as usize - 1) * 64 + previous_to as usize) * 12 + piece as usize - 1) * 64
        + mv.to() as usize
}

#[inline(always)]
fn counter_index(previous_piece: Piece, previous_to: Square) -> usize {
    (previous_piece as usize - 1) * 64 + previous_to as usize
}

#[cfg(test)]
mod tests {
    use super::{
        BUTTERFLY_MAX, CAPTURE_MAX, CONTINUATION_MAX, History, capture_index, continuation_index,
    };
    use crate::types::color::{BLACK, WHITE};
    use crate::types::r#move::{Move, flags};
    use crate::types::piece::{BP, PAWN, WP, WQ};

    fn mv(from: u8, to: u8) -> Move {
        Move::new(from, to, flags::NORMAL)
    }

    #[test]
    fn tables_start_empty_and_are_separated_by_colour() {
        let history = History::new();
        assert_eq!(history.butterfly(WHITE, mv(0, 8)), 0);
        assert_eq!(history.capture(WP, mv(8, 16), PAWN), 0);
        assert_eq!(history.continuation(None, WP, mv(0, 8)), 0);
        assert!(history.counter_move(None).is_none());
        assert!(history.size_bytes() > 1_000_000);
        assert_ne!(
            capture_index(WP, mv(8, 16), PAWN),
            capture_index(BP, mv(8, 16), PAWN)
        );
        assert_ne!(
            continuation_index(WP, 8, WP, mv(9, 17)),
            continuation_index(WP, 9, WP, mv(9, 17))
        );
    }

    #[test]
    fn quiet_update_scores_butterfly_and_continuation() {
        let mut history = History::new();
        let previous = Some((WP, 8));
        history.update_quiet(previous, WP, mv(9, 17), 5, &[]);
        assert!(history.butterfly(WHITE, mv(9, 17)) > 0);
        assert!(history.continuation(previous, WP, mv(9, 17)) > 0);
        assert_eq!(history.continuation(Some((BP, 8)), WP, mv(9, 17)), 0);
        assert_eq!(history.butterfly(BLACK, mv(9, 17)), 0);
    }

    #[test]
    fn malus_lowers_previously_tried_quiets() {
        let mut history = History::new();
        let previous = Some((WP, 8));
        let bad = mv(1, 9);
        history.update_quiet(previous, WP, bad, 3, &[]);
        let before = history.butterfly(WHITE, bad);
        history.update_quiet(previous, WP, mv(2, 10), 5, &[bad]);
        assert!(history.butterfly(WHITE, bad) < before);
        assert!(history.continuation(previous, WP, bad) < 0);
    }

    #[test]
    fn capture_update_is_keyed_by_the_captured_piece_type() {
        let mut history = History::new();
        history.update_capture(None, WQ, mv(8, 16), PAWN, 4);
        assert!(history.capture(WQ, mv(8, 16), PAWN) > 0);
        assert_eq!(history.capture(WQ, mv(8, 16), 3), 0);
        assert_eq!(history.butterfly(WHITE, mv(8, 16)), 0);
    }

    #[test]
    fn gravity_keeps_values_inside_each_bound() {
        let mut history = History::new();
        for _ in 0..2_000 {
            history.update_quiet(None, WP, mv(8, 16), 30, &[]);
            history.update_capture(None, WQ, mv(0, 9), PAWN, 30);
        }
        assert!(history.butterfly(WHITE, mv(8, 16)) <= BUTTERFLY_MAX);
        assert!(history.butterfly(WHITE, mv(8, 16)) > 0);
        assert!(history.capture(WQ, mv(0, 9), PAWN) <= CAPTURE_MAX);
        assert!(history.continuation(None, WP, mv(8, 16)).abs() <= CONTINUATION_MAX);
    }

    #[test]
    fn age_reduces_by_seven_eighths() {
        let mut history = History::new();
        history.update_quiet(None, WP, mv(8, 16), 4, &[]);
        let before = history.butterfly(WHITE, mv(8, 16));
        history.age();
        let after = history.butterfly(WHITE, mv(8, 16));
        assert_eq!(after, before * 7 / 8);
        assert!(after > 0);
    }

    #[test]
    fn countermove_is_stored_per_previous_move() {
        let mut history = History::new();
        let previous = Some((WP, 8));
        let reply = mv(16, 24);
        history.update_counter(previous, reply);
        assert_eq!(history.counter_move(previous), reply);
        assert!(history.counter_move(Some((WP, 9))).is_none());
        assert!(history.counter_move(None).is_none());
    }

    #[test]
    fn clear_empties_every_table() {
        let mut history = History::new();
        let previous = Some((WP, 8));
        history.update_quiet(previous, WP, mv(9, 17), 5, &[]);
        history.update_capture(previous, WQ, mv(0, 9), PAWN, 4);
        history.update_counter(previous, mv(1, 2));
        history.clear();
        assert_eq!(history.butterfly(WHITE, mv(9, 17)), 0);
        assert_eq!(history.capture(WQ, mv(0, 9), PAWN), 0);
        assert_eq!(history.continuation(previous, WP, mv(9, 17)), 0);
        assert!(history.counter_move(previous).is_none());
    }
}
