//! Tiered move ordering.
//!
//! Move generation produces every legal move in one pass, so ordering is done
//! by scoring instead of by generating one category at a time. The scores are
//! arranged in well-separated tiers, which gives the same behaviour as the
//! reference's staged picker: the transposition-table move is tried first,
//! then captures that win material (ordered by material swing, the exact
//! exchange value and the capture history), then the four killers in slot
//! order, the countermove, quiet moves — including quiet promotions — by the
//! combined history score, and losing captures last.
//!
//! [`MovePicker`] owns only fixed-size buffers. It borrows the position, the
//! history tables and the killer table for the duration of one node, so the
//! search can keep one picker per ply without allocation.
//!
//! Tier gaps are wide enough that the contents of a tier can never reach the
//! next tier: the largest history contribution is a few tables' worth of
//! [`super::history::BUTTERFLY_MAX`], far below the quiet base.

use crate::board::movegen;
use crate::board::position::Position;
use crate::search::history::{Continuations, History, Previous};
use crate::search::killers::{KILLER_SLOTS, Killers};
use crate::search::see::{see_ge, see_value};
use crate::types::r#move::{Move, flags};
use crate::types::piece::{PAWN, color_of, piece_type};

/// Material values used by the capture-ordering term (pawn 100 ... king
/// 20,000). These are the ordering weights, deliberately distinct from the
/// exchange values the search's SEE returns.
const ORDER_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 20_000];

/// Maximum number of moves a position can have; matches the generator's list.
pub const MAX_MOVES: usize = 256;

/// The transposition-table move always sorts first.
const TT_SCORE: i32 = 1 << 30;
/// Captures that do not lose material.
const GOOD_CAPTURE_SCORE: i32 = 1 << 28;
/// Most recent killer at this ply; the remaining slots follow one centipawn
/// apart, then the countermove, preserving the staged picker's emission order.
const KILLER_BASE: i32 = 1 << 26;
/// Quiet moves, ordered by the combined history score on top of this base.
const QUIET_SCORE: i32 = 1 << 20;
/// Captures that lose material; sorted from least bad to worst.
const BAD_CAPTURE_SCORE: i32 = 0;

/// Scores a fixed list of moves and hands them out in descending order.
pub struct MovePicker<'a> {
    position: &'a Position,
    history: &'a History,
    moves: [Move; MAX_MOVES],
    scores: [i32; MAX_MOVES],
    len: usize,
    index: usize,
}

impl<'a> MovePicker<'a> {
    /// Scores `moves` for the node at `ply`.
    ///
    /// `previous` identifies the move that produced the current position; it
    /// selects the continuation-history and countermove entries. `cont` carries
    /// the destination squares of the moves played 1/2/4/6 plies earlier.
    /// `tt_move` may be [`Move::NONE`], and the killers of other plies are
    /// ignored.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        position: &'a Position,
        moves: &[Move],
        tt_move: Move,
        killers: &Killers,
        history: &'a History,
        ply: usize,
        previous: Option<Previous>,
        cont: &Continuations,
    ) -> Self {
        debug_assert!(moves.len() <= MAX_MOVES);
        let len = moves.len().min(MAX_MOVES);
        let mut picker = Self {
            position,
            history,
            moves: [Move::NONE; MAX_MOVES],
            scores: [0; MAX_MOVES],
            len,
            index: 0,
        };
        for (index, &mv) in moves.iter().take(len).enumerate() {
            picker.moves[index] = mv;
            picker.scores[index] = picker.score(mv, tt_move, killers, ply, previous, cont);
        }
        picker
    }

    /// Returns the number of moves the picker was given.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` when there is nothing to pick, which only happens in a
    /// checkmate or stalemate position.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the number of moves that have not been handed out yet.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.len - self.index
    }

    /// Returns the best remaining move.
    ///
    /// Moves are selected with a partial selection sort: each call swaps the
    /// highest remaining score into the current position, so the caller sees
    /// strictly non-increasing scores and every move exactly once.
    fn pick_next(&mut self) -> Option<Move> {
        if self.index >= self.len {
            return None;
        }
        let mut best = self.index;
        for candidate in self.index + 1..self.len {
            if self.scores[candidate] > self.scores[best] {
                best = candidate;
            }
        }
        self.moves.swap(self.index, best);
        self.scores.swap(self.index, best);
        let mv = self.moves[self.index];
        self.index += 1;
        Some(mv)
    }

    /// Computes the ordering score of one move.
    fn score(
        &self,
        mv: Move,
        tt_move: Move,
        killers: &Killers,
        ply: usize,
        previous: Option<Previous>,
        cont: &Continuations,
    ) -> i32 {
        if mv == tt_move && !tt_move.is_none() {
            return TT_SCORE;
        }

        let moving = self.position.piece_at(mv.from());
        let moving_type = piece_type(moving);

        if movegen::is_capture(self.position, mv) {
            // Reference capture score: the victim material value in
            // hundredths minus the attacker value, plus the exact exchange
            // value and the capture-history cell. An en-passant capture
            // removes a pawn.
            let victim = if mv.flag() == flags::EN_PASSANT {
                PAWN
            } else {
                piece_type(self.position.piece_at(mv.to()))
            };
            let material = ORDER_VALUES[victim] * 100 - ORDER_VALUES[moving_type];
            let capture_history = self.history.capture(moving_type, mv, victim);
            return if see_ge(self.position, mv, 0) {
                GOOD_CAPTURE_SCORE + material + see_value(self.position, mv) + capture_history
            } else {
                BAD_CAPTURE_SCORE + material + see_value(self.position, mv) + capture_history
            };
        }

        // Killer slots in slot order, then the countermove — the emission
        // order of the staged picker's killer and countermove stages.
        if ply < usize::from(u8::MAX) {
            for slot in 0..KILLER_SLOTS {
                if killers.get(ply)[slot] == mv && !mv.is_none() {
                    return KILLER_BASE - slot as i32;
                }
            }
        }
        if mv == self.history.counter_move(previous) && !mv.is_none() {
            return KILLER_BASE - KILLER_SLOTS as i32;
        }

        // Every remaining non-capture — including quiet promotions — is
        // ordered by the combined history score.
        QUIET_SCORE + self.history.picker_score(color_of(moving), mv, cont)
    }
}

impl Iterator for MovePicker<'_> {
    type Item = Move;

    /// Yields the next move in ordering order.
    fn next(&mut self) -> Option<Move> {
        self.pick_next()
    }
}
