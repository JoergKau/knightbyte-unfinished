//! Tiered move ordering.
//!
//! Move generation produces every legal move in one pass, so ordering is done
//! by scoring instead of by generating one category at a time. The scores are
//! arranged in well-separated tiers, which gives the same behaviour as a staged
//! picker: the transposition-table move is tried first, then captures that win
//! material, then promotions, the two killers, the countermove, quiet moves by
//! history, and losing captures last.
//!
//! [`MovePicker`] owns only fixed-size buffers. It borrows the position, the
//! history tables and the killer table for the duration of one node, so the
//! search can keep one picker per ply without allocation.
//!
//! Tier gaps are wide enough that the contents of a tier can never reach the
//! next tier: the largest history contribution is two tables' worth of
//! [`super::history::MAX_HISTORY`], far below the quiet base.

use crate::board::movegen;
use crate::board::position::Position;
use crate::search::history::{History, Previous};
use crate::search::killers::Killers;
use crate::search::see::{piece_value, see_ge, see_value};
use crate::types::r#move::{Move, flags};
use crate::types::piece::{color_of, piece_type};

/// Maximum number of moves a position can have; matches the generator's list.
pub const MAX_MOVES: usize = 256;

/// The transposition-table move always sorts first.
const TT_SCORE: i32 = 1 << 30;
/// Captures that do not lose material.
const GOOD_CAPTURE_SCORE: i32 = 1 << 28;
/// Quiet promotions.
const PROMOTION_SCORE: i32 = 1 << 27;
/// Most recent killer at this ply.
const KILLER_1_SCORE: i32 = 1 << 26;
/// Second killer at this ply.
const KILLER_2_SCORE: i32 = (1 << 26) - 1_000;
/// Countermove to the opponent's previous move.
const COUNTER_SCORE: i32 = (1 << 26) - 2_000;
/// Quiet moves, ordered by history on top of this base.
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
    /// selects the continuation-history and countermove entries. `tt_move` may
    /// be [`Move::NONE`], and the killers of other plies are ignored.
    pub fn new(
        position: &'a Position,
        moves: &[Move],
        tt_move: Move,
        killers: &Killers,
        history: &'a History,
        ply: usize,
        previous: Option<Previous>,
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
            picker.scores[index] = picker.score(mv, tt_move, killers, ply, previous);
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
    ) -> i32 {
        if mv == tt_move && !tt_move.is_none() {
            return TT_SCORE;
        }

        let moving = self.position.piece_at(mv.from());
        let moving_type = piece_type(moving);

        if movegen::is_capture(self.position, mv) {
            // Reference capture score: victim value in hundredths minus the
            // attacker value, plus the exact exchange value.
            let victim = if mv.flag() == flags::EN_PASSANT {
                0
            } else {
                let captured = self.position.piece_at(mv.to());
                if captured == crate::types::piece::EMPTY {
                    0
                } else {
                    piece_value(piece_type(captured))
                }
            };
            let material = victim * 100 - piece_value(moving_type);
            return if see_ge(self.position, mv, 0) {
                GOOD_CAPTURE_SCORE + material + see_value(self.position, mv)
            } else {
                BAD_CAPTURE_SCORE + material + see_value(self.position, mv)
            };
        }

        if mv.is_promotion() {
            return PROMOTION_SCORE + piece_value(promoted_type(mv)) / 16;
        }

        if killers.is_first(ply, mv) {
            return KILLER_1_SCORE;
        }
        if killers.contains(ply, mv) {
            return KILLER_2_SCORE;
        }
        if mv == self.history.counter_move(previous) {
            return COUNTER_SCORE;
        }

        QUIET_SCORE
            + self.history.butterfly(color_of(moving), mv)
            + self.history.continuation(previous, moving, mv)
    }
}

/// Returns the promoted piece type encoded in the move flag.
fn promoted_type(mv: Move) -> usize {
    use crate::types::piece::{BISHOP, KNIGHT, QUEEN, ROOK};
    match mv.flag() {
        flags::PROMO_KNIGHT => KNIGHT,
        flags::PROMO_BISHOP => BISHOP,
        flags::PROMO_ROOK => ROOK,
        _ => QUEEN,
    }
}

impl Iterator for MovePicker<'_> {
    type Item = Move;

    /// Yields the next move in ordering order.
    fn next(&mut self) -> Option<Move> {
        self.pick_next()
    }
}
