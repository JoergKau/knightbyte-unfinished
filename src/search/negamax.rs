//! Negamax search.
//!
//! The node function follows the usual shape: probe and use the transposition
//! table, compute the static evaluation, apply conservative pruning at
//! non-principal-variation nodes, then run the ordered moves with a null-window
//! search (principal variation search) and re-search the interesting ones with
//! the full window.
//!
//! Ordering state is borrowed only while the picker is alive: the ordered move
//! list is copied into a small local array first, so recursion never has to
//! hold a borrow of `self`.

use crate::board::movegen;
use crate::board::position::Position;
use crate::search::history::Previous;
use crate::search::move_picker::{MAX_MOVES, MovePicker};
use crate::search::searcher::{
    DRAW_SCORE, INFINITE, MATE_LIMIT, MATE_SCORE, NO_STATIC_EVAL, NodeResult, Searcher, mated_in,
};
use crate::search::see::see_ge;
use crate::search::tt::Bound;
use crate::types::r#move::{Move, flags};
use crate::types::piece::{PAWN, piece_type};

impl Searcher {
    /// Searches `position` to `depth` plies and returns its score.
    ///
    /// `previous` identifies the move that produced `position`; it selects the
    /// continuation-history and countermove entries. `ply` is the distance from
    /// the root and indexes the per-ply arrays.
    pub(crate) fn negamax(
        &mut self,
        position: &Position,
        mut alpha: i32,
        mut beta: i32,
        mut depth: i32,
        ply: usize,
        previous: Option<Previous>,
    ) -> i32 {
        self.pv.clear(ply);
        self.nodes += 1;
        if ply > self.seldepth {
            self.seldepth = ply;
        }
        if self.should_stop() {
            return 0;
        }
        if ply >= super::MAX_PLY - 1 {
            return self.evaluate(position);
        }
        let is_root = ply == 0;
        if !is_root && self.is_draw(position, ply) {
            return DRAW_SCORE;
        }
        self.keys[ply] = position.key();

        // Mate-distance pruning is skipped at the root, which must complete its
        // move loop and report a real best move.
        if !is_root {
            let mate_value = MATE_SCORE - ply as i32;
            if alpha < -mate_value {
                alpha = -mate_value;
            }
            if beta > mate_value - 1 {
                beta = mate_value - 1;
            }
            if alpha >= beta {
                return alpha;
            }
        }

        // Node-level extensions: a check extends the node itself, and a forced
        // single reply extends it once the search is away from the horizon. Both
        // share the per-path extension budget.
        let in_check = position.in_check(position.side());
        let mut extensions_used = self.extensions[ply];
        let cap = self.params.max_path_extensions;
        if in_check && extensions_used < cap {
            depth += 1;
            extensions_used += 1;
        }
        if !in_check
            && ply > 0
            && depth >= 3
            && extensions_used < cap
            && movegen::pseudo_legal(position).len() == 1
        {
            depth += 1;
            extensions_used += 1;
        }
        self.extensions[ply] = extensions_used;

        if depth <= 0 {
            return self.quiescence(position, alpha, beta, ply, previous);
        }

        let pv_node = beta - alpha > 1;

        let mut tt_move = Move::NONE;
        let mut tt_probe = None;
        let mut tt_eval = None;
        if let Some(entry) = self.tt.probe(position.key()) {
            tt_move = entry.mv;
            tt_probe = Some(entry);
            let stored_eval = i32::from(entry.eval);
            if stored_eval != NO_STATIC_EVAL {
                tt_eval = Some(stored_eval);
            }
            if ply > 0 && i32::from(entry.depth) >= depth {
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

        let raw_eval = tt_eval.unwrap_or_else(|| self.evaluate(position));
        let mut eval = raw_eval;

        // Correction history: a learned offset applied before every pruning
        // decision. The reference skips it when a usable TT move exists and when
        // the node is in check, and only corrects nodes with remaining depth.
        if self.params.use_correction_history
            && self.params.use_ref_correction_history
            && depth > 0
            && !in_check
            && tt_move.is_none()
        {
            let correction = self.correction.correction(position, previous);
            eval = self.correction.corrected_eval(raw_eval, correction);
        }

        // A stored score that is consistent with the raw evaluation is more
        // informed than raw + correction, so it replaces the evaluation.
        if self.params.use_tt_score_eval
            && !in_check
            && let Some(entry) = tt_probe
            && Self::can_use_tt_score(&entry, raw_eval, ply)
        {
            eval = Self::probe_score(&entry, ply);
        }
        self.evals[ply] = eval;

        let improving = self.is_improving(ply, eval);
        let alpha_window = alpha;

        if !pv_node && !in_check {
            // Reverse futility: formula margin `depth * scale`, damped fail-soft
            // return so a fail-high does not hand the parent the full overshoot.
            let rfp_gate = if self.params.use_formula_rfp {
                self.params.rfp_formula_max_depth
            } else {
                self.params.rfp_max_depth
            };
            if depth <= rfp_gate && eval >= beta {
                let scale = if improving {
                    self.params.rfp_improving_scale
                } else {
                    self.params.rfp_not_improving_scale
                };
                let margin = if self.params.use_formula_rfp {
                    depth * scale
                } else {
                    rfp_margin(depth)
                };
                if eval - margin >= beta {
                    // Formula arm: damp the spill so the parent's alpha rises by a
                    // third of the overshoot instead of the whole of it.
                    return if self.params.use_formula_rfp {
                        beta + (eval - beta) / 3
                    } else {
                        eval
                    };
                }
            }

            // Razoring: very low evaluation at low depth drops into quiescence;
            // a quiescence score below alpha confirms the node fails low.
            if depth <= self.params.razor_max_depth {
                let margin = razor_margin(depth);
                if eval + margin <= alpha {
                    let score = self.quiescence(position, alpha, beta, ply, previous);
                    if self.stopped {
                        return 0;
                    }
                    if score < alpha {
                        return alpha;
                    }
                }
            }

            if depth >= 3
                && eval >= beta
                && Self::has_non_pawn_material(position, position.side())
                && tt_probe.is_none_or(|entry| Self::probe_score(&entry, ply) >= beta)
            {
                // Eval-scaled extra reduction, clamped so a null move can never
                // reach the current node's depth.
                let eval_bonus = ((eval - beta) / 191).min(4);
                let mut reduction = self.params.nmp_base_reduction
                    + depth / self.params.nmp_depth_divisor
                    + eval_bonus;
                if reduction >= depth {
                    reduction = depth - 1;
                }
                let next = position.make_null();
                let score = -self.negamax(
                    &next,
                    -beta,
                    -beta + 1,
                    depth - 1 - reduction,
                    ply + 1,
                    None,
                );
                if self.stopped {
                    return 0;
                }
                if score >= beta {
                    // A mate score from a null-window line is not trustworthy;
                    // report the bound without claiming a distance.
                    return if score >= MATE_LIMIT { beta } else { score };
                }
            }
        }

        let moves = movegen::legal(position);
        if moves.is_empty() {
            return if in_check {
                mated_in(ply as i32)
            } else {
                DRAW_SCORE
            };
        }

        let mut ordered = [Move::NONE; MAX_MOVES];
        let mut count = 0;
        {
            let picker = MovePicker::new(
                position,
                moves.as_slice(),
                tt_move,
                &self.killers,
                &self.history,
                ply,
                previous,
            );
            for mv in picker {
                ordered[count] = mv;
                count += 1;
            }
        }

        let late_move_limit = if !pv_node && !in_check && depth <= self.params.lmp_max_depth {
            let mut limit = lmp_limit(depth);
            if self.params.use_improving_flag && !improving {
                limit = (limit / 2).max(1);
            }
            Some(limit)
        } else {
            None
        };
        let futile = !pv_node
            && !in_check
            && depth <= self.params.futility_max_depth
            && eval + futility_margin(depth) < alpha;

        let mut best_score = -INFINITE;
        let mut best_move = Move::NONE;
        let mut quiet_tried = 0;
        let mut move_index = 0;
        let mut skip_quiets = false;
        let mut bad_quiets = [Move::NONE; 64];
        let mut bad_count = 0;

        for &mv in &ordered[..count] {
            move_index += 1;
            let capture = movegen::is_capture(position, mv);
            let quiet = !capture && !mv.is_promotion();
            let moving = position.piece_at(mv.from());

            if !pv_node && !in_check {
                if quiet {
                    quiet_tried += 1;
                    if move_index > 1 {
                        // Late move pruning and futility stop the whole quiet
                        // stage, not just the current move; captures (including
                        // losing ones) are still tried. Pawn moves are exempt
                        // from futility, matching the reference.
                        if late_move_limit.is_some_and(|limit| quiet_tried > limit) {
                            skip_quiets = true;
                        }
                        if futile && piece_type(moving) != PAWN {
                            skip_quiets = true;
                        }
                    }
                    if skip_quiets {
                        continue;
                    }
                } else if capture
                    && depth <= self.params.see_pruning_max_depth
                    && !see_ge(position, mv, -26 * depth * depth)
                {
                    continue;
                }
            }

            let next = position.make_move(mv);
            let gives_check = next.in_check(next.side());
            self.extensions[ply + 1] = extensions_used;
            let new_depth = depth - 1;

            let mut score;
            if move_index == 1 {
                score = -self.negamax(
                    &next,
                    -beta,
                    -alpha,
                    new_depth,
                    ply + 1,
                    Some((moving, mv.to())),
                );
            } else {
                let mut reduction = 0;
                if new_depth >= 3 && move_index > 3 && quiet && !in_check && !gives_check {
                    reduction = self.lmr_reduction(new_depth, move_index, pv_node, improving);
                }
                let reduced_depth = (new_depth - reduction).max(0);
                score = -self.negamax(
                    &next,
                    -alpha - 1,
                    -alpha,
                    reduced_depth,
                    ply + 1,
                    Some((moving, mv.to())),
                );
                if score > alpha && reduction > 0 {
                    score = -self.negamax(
                        &next,
                        -alpha - 1,
                        -alpha,
                        new_depth,
                        ply + 1,
                        Some((moving, mv.to())),
                    );
                }
                if score > alpha && score < beta {
                    score = -self.negamax(
                        &next,
                        -beta,
                        -alpha,
                        new_depth,
                        ply + 1,
                        Some((moving, mv.to())),
                    );
                }
            }
            if self.stopped {
                return 0;
            }

            if score <= alpha && quiet && bad_count < bad_quiets.len() {
                bad_quiets[bad_count] = mv;
                bad_count += 1;
            }

            if score > best_score {
                best_score = score;
                best_move = mv;
                if score > alpha {
                    alpha = score;
                    self.pv.update(ply, mv);
                    if score >= beta {
                        if quiet {
                            self.killers.record(ply, mv);
                            self.history.update_quiet(
                                previous,
                                moving,
                                mv,
                                depth,
                                &bad_quiets[..bad_count],
                            );
                        } else {
                            let captured = if mv.flag() == flags::EN_PASSANT {
                                PAWN
                            } else {
                                piece_type(position.piece_at(mv.to()))
                            };
                            self.history
                                .update_capture(previous, moving, mv, captured, depth);
                        }
                        self.history.update_counter(previous, mv);
                        return self.finish_node(NodeResult {
                            position,
                            ply,
                            depth,
                            score: best_score,
                            raw_eval,
                            corrected_eval: eval,
                            bound: Bound::Lower,
                            best_move,
                            in_check,
                            previous,
                        });
                    }
                }
            }
        }

        let bound = if best_score >= beta {
            Bound::Lower
        } else if best_score > alpha_window {
            Bound::Exact
        } else {
            Bound::Upper
        };
        let stored_move = if bound == Bound::Upper {
            tt_move
        } else {
            best_move
        };
        self.finish_node(NodeResult {
            position,
            ply,
            depth,
            score: best_score,
            raw_eval,
            corrected_eval: eval,
            bound,
            best_move: stored_move,
            in_check,
            previous,
        })
    }
}

/// Returns the number of quiet moves searched before late move pruning stops the
/// stage. Indexed by remaining depth, capped at depth 4.
fn lmp_limit(depth: i32) -> i32 {
    const LIMITS: [i32; 5] = [0, 9, 12, 17, 24];
    LIMITS[(depth as usize).min(4)]
}

/// Returns the futility margin for a remaining depth, capped at depth 5.
fn futility_margin(depth: i32) -> i32 {
    const MARGINS: [i32; 5] = [0, 61, 144, 268, 334];
    MARGINS[(depth as usize).min(4)]
}

/// Returns the razoring margin for a remaining depth.
fn razor_margin(depth: i32) -> i32 {
    if depth <= 1 { 86 } else { 353 }
}

/// Returns the reverse-futility margin from the per-depth table.
fn rfp_margin(depth: i32) -> i32 {
    const MARGINS: [i32; 5] = [0, 102, 210, 267, 394];
    MARGINS[(depth as usize).min(4)]
}

impl Searcher {
    /// Returns whether the static evaluation improved over the same line.
    ///
    /// The grandparent's eval is the comparison point: two plies are enough to
    /// see whether the current side's position is trending up or down.
    fn is_improving(&self, ply: usize, eval: i32) -> bool {
        if !self.params.use_improving_flag || ply < 2 {
            return true;
        }
        eval > self.evals[ply - 2]
    }

    /// Returns the late-move reduction for the `move_count`-th move.
    ///
    /// The reduction comes from an integer table so every platform produces the
    /// same value: `log2(depth * move_count) / 2`. The "not improving" weight
    /// adds a fixed offset, and principal-variation nodes keep a smaller
    /// reduction.
    fn lmr_reduction(&self, depth: i32, move_count: usize, pv_node: bool, improving: bool) -> i32 {
        if !self.params.use_lmr_table {
            return i32::from(depth >= 4 && move_count > 4);
        }
        let depth = depth.max(1) as u32;
        let moves = move_count.max(1) as u32;
        let mut reduction = ((depth * moves).max(2).ilog2() as i32 / 2).max(1);
        if pv_node {
            reduction -= 1;
        }
        if !improving {
            reduction += self.params.lmr_v2_not_improving_weight / 70;
        }
        reduction.clamp(0, depth as i32 - 2)
    }
}
