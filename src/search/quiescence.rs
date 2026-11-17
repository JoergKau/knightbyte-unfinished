//! Quiescence search.
//!
//! A static evaluation is only trustworthy once the tactical dust has settled.
//! Quiescence extends the search with captures until no favourable capture
//! remains, which prevents the horizon effect where a search evaluates a
//! position one ply before a queen is lost.
//!
//! The node order follows the ported reference exactly, because node counts are
//! the parity gate:
//!
//! 1. count the node, poll for a stop, track selective depth;
//! 2. stop at ply 100, then apply draw detection;
//! 3. detect check and probe the table (only depth-0 entries may cut here);
//! 4. in check: seed a mated score and search every evasion;
//! 5. not in check: stand pat, fail-soft return on `stand >= beta`, then
//!    node-level delta pruning;
//! 6. captures in ordering order, filtered by the approximate-SEE prefilter and
//!    a `-50` bound;
//! 7. a fail-high breaks the loop and the result is averaged with beta
//!    (`use_micro_riders`);
//! 8. store the result at depth 0.
//!
//! Quiet promotions are not quiescence moves: the reference classifies a move
//! as a capture only by its target square, so a promotion to an empty square is
//! skipped.

use crate::board::movegen;
use crate::board::position::Position;
use crate::search::MAX_PLY;
use crate::search::history::History;
use crate::search::history::Previous;
use crate::search::move_picker::MAX_MOVES;
use crate::search::searcher::{
    DRAW_SCORE, MATE_LIMIT, NO_STATIC_EVAL, Searcher, mated_in, tt_entry,
};
use crate::search::see::{see_approximate, see_ge, see_value};
use crate::search::tt::Bound;
use crate::types::r#move::{Move, flags};
use crate::types::piece::{EMPTY, PAWN, piece_type};

/// Largest losing exchange that quiescence still searches.
const SEE_THRESHOLD: i32 = -50;

/// Material values used by the capture-ordering term (pawn 100 ... king
/// 20,000), matching the main picker's ordering weights.
const ORDER_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 20_000];

impl Searcher {
    /// Resolves captures at a leaf node.
    pub(crate) fn quiescence(
        &mut self,
        position: &Position,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        previous: Option<Previous>,
    ) -> i32 {
        self.nodes += 1;
        if ply > self.seldepth {
            self.seldepth = ply;
        }
        if self.should_stop() {
            return 0;
        }
        if ply >= 100 || ply >= MAX_PLY - 1 {
            return self.evaluate(position);
        }
        if ply > 0 && self.is_draw(position, ply) {
            return DRAW_SCORE;
        }
        self.keys[ply] = position.key();

        let in_check = self.params.use_check_aware_quiescence && position.in_check(position.side());

        let mut tt_eval: Option<i32> = None;
        let mut tt_probe: Option<crate::search::tt::TtEntry> = None;
        if self.params.use_qs_tt_probe
            && let Some(entry) = self.tt.probe(position.key())
        {
            tt_probe = Some(entry);
            let stored_eval = i32::from(entry.eval);
            if stored_eval != NO_STATIC_EVAL {
                tt_eval = Some(stored_eval);
            }
            if entry.depth == 0 {
                let score = Self::probe_score(&entry, ply);
                let usable = match entry.bound {
                    Bound::Exact => true,
                    Bound::Lower => score >= beta,
                    Bound::Upper => score <= alpha,
                };
                if usable {
                    return score;
                }
            }
        }

        let alpha_original = alpha;
        let mut best_score;
        let mut static_eval = NO_STATIC_EVAL;
        let mut stand;

        if in_check {
            best_score = mated_in(ply as i32);
        } else {
            stand = tt_eval.unwrap_or_else(|| self.evaluate(position));
            static_eval = stand;

            if self.params.use_tt_score_eval
                && let Some(entry) = tt_probe
                && Self::can_use_tt_score(&entry, stand, ply)
            {
                stand = Self::probe_score(&entry, ply);
            }

            if stand >= beta {
                let returned = if self.params.use_micro_riders {
                    stand
                } else {
                    beta
                };
                self.store_tt(tt_entry(
                    position,
                    ply,
                    0,
                    returned,
                    static_eval,
                    Bound::Lower,
                    Move::NONE,
                ));
                return returned;
            }
            if alpha < stand {
                alpha = stand;
            }

            // Delta pruning: even the most valuable capture cannot reach alpha.
            if stand + self.params.qs_delta_margin < alpha {
                return alpha;
            }
            best_score = stand;
        }

        let moves = movegen::legal(position);
        let mut ordered = [Move::NONE; MAX_MOVES];
        let mut scores = [0_i32; MAX_MOVES];
        let count = if in_check {
            let count = moves.len().min(MAX_MOVES);
            ordered[..count].copy_from_slice(&moves.as_slice()[..count]);
            count
        } else {
            order_captures(
                position,
                &self.history,
                moves.as_slice(),
                &mut ordered,
                &mut scores,
            )
        };

        if in_check && count == 0 {
            return mated_in(ply as i32);
        }

        let mut best_move = Move::NONE;
        let mut legal_moves = 0;

        for &mv in &ordered[..count] {
            if self.params.use_qs_narrowing && !in_check {
                if ply as i32 >= self.params.qs_promotion_max_ply && mv.is_promotion() {
                    continue;
                }
                if ply as i32 >= self.params.qs_recapture_only_ply
                    && let Some((_, target)) = previous
                    && mv.to() != target
                {
                    continue;
                }
            }

            let quiet = !is_capture(position, mv) && !mv.is_promotion();
            if !in_check {
                if self.params.use_see_approx_prefilter
                    && see_approximate(position, mv) < SEE_THRESHOLD
                {
                    continue;
                }
                if !see_ge(position, mv, SEE_THRESHOLD) {
                    continue;
                }
            }

            let next = position.make_move(mv);
            legal_moves += 1;
            let score = -self.quiescence(
                &next,
                -beta,
                -alpha,
                ply + 1,
                Some((position.piece_at(mv.from()), mv.to())),
            );
            if self.stopped {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move = mv;
            }
            if score >= beta {
                if self.params.use_micro_riders {
                    break;
                }
                return beta;
            }
            if score > alpha {
                alpha = score;
            }

            // Evasion narrowing: once an evasion that is not getting mated has
            // been found, stop searching further quiet evasions. Captures and
            // promotions are ordered first and therefore already searched.
            if in_check && quiet && legal_moves > 1 && best_score > -MATE_LIMIT {
                break;
            }
        }

        if in_check && legal_moves == 0 {
            return mated_in(ply as i32);
        }

        let mut result = if in_check || (self.params.use_micro_riders && best_score >= beta) {
            best_score
        } else {
            alpha
        };
        if self.params.use_micro_riders && result >= beta {
            result = qs_fail_high_score(result, beta);
        }

        self.store_tt(tt_entry(
            position,
            ply,
            0,
            result,
            static_eval,
            if result >= beta {
                Bound::Lower
            } else if result > alpha_original {
                Bound::Exact
            } else {
                Bound::Upper
            },
            best_move,
        ));
        result
    }
}

/// Returns whether `mv` captures a piece (en passant included).
#[inline]
fn is_capture(position: &Position, mv: Move) -> bool {
    mv.flag() == flags::EN_PASSANT || position.piece_at(mv.to()) != EMPTY
}

/// Fills `ordered` with the capture moves: good captures sorted by the
/// reference score, then losing captures in generation order.
fn order_captures(
    position: &Position,
    history: &History,
    moves: &[Move],
    ordered: &mut [Move; MAX_MOVES],
    scores: &mut [i32; MAX_MOVES],
) -> usize {
    let mut count = 0;
    let mut bad = [Move::NONE; MAX_MOVES];
    let mut bad_count = 0;

    for &mv in moves {
        if !is_capture(position, mv) {
            continue;
        }
        if see_ge(position, mv, 0) {
            let victim = if mv.flag() == flags::EN_PASSANT {
                PAWN
            } else {
                piece_type(position.piece_at(mv.to()))
            };
            let moving_type = piece_type(position.piece_at(mv.from()));
            ordered[count] = mv;
            scores[count] = capture_score(position, history, moving_type, mv, victim);
            count += 1;
        } else {
            bad[bad_count] = mv;
            bad_count += 1;
        }
    }

    for &mv in &bad[..bad_count] {
        ordered[count] = mv;
        scores[count] = i32::MIN;
        count += 1;
    }

    // Selection sort keeps the highest score first and preserves generation
    // order among equal scores, which is what the reference's picker does.
    for slot in 0..count {
        let mut best = slot;
        for candidate in slot + 1..count {
            if scores[candidate] > scores[best] {
                best = candidate;
            }
        }
        ordered.swap(slot, best);
        scores.swap(slot, best);
    }
    count
}

/// Returns the reference capture-ordering score for a good capture: the
/// classical material swing, the exact exchange value and the raw
/// capture-history cell.
fn capture_score(
    position: &Position,
    history: &History,
    moving_type: usize,
    mv: Move,
    victim: usize,
) -> i32 {
    let material = ORDER_VALUES[victim] * 100 - ORDER_VALUES[moving_type];
    1_000_000 + material + see_value(position, mv) + history.capture(moving_type, mv, victim)
}

/// Fail-soft averaging on a quiescence fail-high.
fn qs_fail_high_score(score: i32, beta: i32) -> i32 {
    if score >= beta && score.abs() < MATE_LIMIT && beta.abs() < MATE_LIMIT {
        (score + beta) / 2
    } else {
        beta
    }
}
