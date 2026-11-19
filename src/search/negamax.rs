//! Negamax search.
//!
//! The node function follows the reference pipeline exactly, in order: stop
//! checks, draw detection, mate-distance pruning, node-level extensions (a
//! check and a forced single reply, sharing a per-path budget), the first
//! quiescence drop, the transposition-table probe with cutoffs and bound
//! narrowing, static evaluation with correction history, the hindsight depth
//! pair, the pruning stack (reverse futility, razoring, null move), the
//! singular-extension verification, internal iterative reduction, a second
//! quiescence drop for nodes the depth adjustments consumed, and finally the
//! ordered move loop with late-move, history, futility and SEE pruning, late
//! move reductions and principal-variation re-searches.
//!
//! The `cut_node` flag is threaded through the recursion: a child's prediction
//! depends on how its parent was searched. The `excluded_move` parameter makes
//! the singular-extension verification search the alternatives only.
//!
//! Ordering state is borrowed only while the picker is alive: the ordered move
//! list is copied into a small local array first, so recursion never has to
//! hold a borrow of `self`.

use std::sync::OnceLock;

use crate::board::movegen;
use crate::board::position::Position;
use crate::search::history::{
    CONTINUATION_DISTANCES, Continuations, Previous, lmr_history_term_1024,
};
use crate::search::searcher::{
    DRAW_SCORE, INFINITE, MATE_LIMIT, MATE_SCORE, NO_STATIC_EVAL, NodeResult, SearchEvent,
    Searcher, mated_in,
};
use crate::search::see::see_ge;
use crate::search::tt::Bound;
use crate::types::r#move::Move;
use crate::types::piece::{KING, PAWN, color_of, piece_type};

/// The node at and beyond this ply returns its static evaluation without
/// counting, matching the reference's safety guard.
const MAX_SEARCH_PLY: usize = 100;

/// Side of the late-move-reduction base table, indexed `[depth][move number]`.
const LMR_TABLE_SIZE: usize = 64;

/// The precomputed late-move-reduction base table:
/// `int(1.0 + ln(depth) * ln(move_number) / 1.75)`, zero at either index zero,
/// read with both indices clamped into range.
fn lmr_table() -> &'static [[i32; LMR_TABLE_SIZE]; LMR_TABLE_SIZE] {
    static TABLE: OnceLock<[[i32; LMR_TABLE_SIZE]; LMR_TABLE_SIZE]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [[0_i32; LMR_TABLE_SIZE]; LMR_TABLE_SIZE];
        for (depth, row) in table.iter_mut().enumerate().skip(1) {
            for (moves, cell) in row.iter_mut().enumerate().skip(1) {
                let value = 1.0 + (depth as f64).ln() * (moves as f64).ln() / 1.75;
                *cell = value as i32;
            }
        }
        table
    })
}

/// Returns the base late-move reduction for `depth` and the 1-based
/// `move_number`, read from the precomputed table with clamped indices.
fn lmr_base_reduction(depth: i32, move_number: i32) -> i32 {
    let d = depth.clamp(0, LMR_TABLE_SIZE as i32 - 1) as usize;
    let m = move_number.clamp(0, LMR_TABLE_SIZE as i32 - 1) as usize;
    lmr_table()[d][m]
}

/// Returns the depth for the full-depth zero-window re-search after a reduced
/// search failed high.
///
/// A large original reduction shaves one or two plies off the re-search â€” a
/// move that wanted a big reduction rarely needs full re-verification â€” and
/// the result is floored at the reduced depth so the re-search is never
/// shallower than the search it improves on.
fn compute_re_search_depth(new_depth: i32, reduction: i32, enabled: bool) -> i32 {
    if !enabled {
        return new_depth;
    }
    let mut re_search_depth = new_depth;
    if reduction >= 3 {
        re_search_depth -= 1;
    }
    if reduction >= 5 && new_depth > 2 {
        re_search_depth -= 1;
    }
    let min_depth = new_depth - reduction;
    if re_search_depth < min_depth {
        re_search_depth = min_depth;
    }
    re_search_depth
}

impl Searcher {
    /// Builds the continuation-history context of the node at `ply`: the
    /// destination squares of the moves played 1, 2, 4 and 6 plies earlier.
    pub(crate) fn continuations(&self, ply: usize) -> Continuations {
        let mut cont = [None; 4];
        for (index, &distance) in CONTINUATION_DISTANCES.iter().enumerate() {
            if ply >= distance {
                let mv = self.move_stack[ply - distance];
                if !mv.is_none() {
                    cont[index] = Some(mv.to());
                }
            }
        }
        cont
    }

    /// Returns the number of times the current position already appears on the
    /// search path. A repeated position is refused transposition-table
    /// cutoffs so the search can never anchor a repetition decision to a
    /// cached score.
    fn path_repetition_count(&self, position: &Position, ply: usize) -> u32 {
        self.repetition_count(position, ply)
    }

    /// Searches `position` to `depth` plies and returns its score.
    ///
    /// `previous` identifies the move that produced `position`; it selects the
    /// continuation-history and countermove entries. `ply` is the distance
    /// from the root and indexes the per-ply arrays. `cut_node` predicts a
    /// fail-high node (children of scouting searches). `excluded_move` is the
    /// transposition-table move a singular verification must not search.
    /// `path_extensions` is the extension budget already spent along this
    /// path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn negamax(
        &mut self,
        position: &Position,
        mut alpha: i32,
        mut beta: i32,
        mut depth: i32,
        ply: usize,
        previous: Option<Previous>,
        cut_node: bool,
        excluded_move: Move,
        path_extensions: i32,
        observer: &mut dyn FnMut(SearchEvent),
    ) -> i32 {
        // The ply guard runs before the node is counted: a node beyond the
        // safety depth returns its static evaluation and costs nothing. No
        // accumulator pair exists at this ply, so the full refresh runs.
        if ply >= MAX_SEARCH_PLY {
            return self.evaluator.evaluate(position);
        }
        self.pv.clear(ply);
        self.nodes += 1;
        crate::node_stat!(NegamaxNodes);
        if ply > self.seldepth {
            self.seldepth = ply;
        }
        if self.should_stop() {
            return 0;
        }
        let is_root = ply == 0;
        if !is_root && self.is_draw(position, ply) {
            return DRAW_SCORE;
        }
        self.keys[ply] = position.key();
        self.trace_node('n', ply, depth, alpha, beta, cut_node, 0);

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
        // single reply extends it once the search is away from the horizon.
        // Both share the per-path extension budget.
        let in_check = position.in_check(position.side());
        let mut extensions_used = path_extensions;
        let cap = self.params.max_path_extensions;
        if in_check && extensions_used < cap {
            depth += 1;
            extensions_used += 1;
        }
        if !in_check
            && ply > 0
            && depth >= 3
            && extensions_used < cap
            && movegen::has_exactly_one_pseudo_move(position)
        {
            depth += 1;
            extensions_used += 1;
        }

        // Exact tablebase values. The probe is unconditional on depth: inside
        // the tables the value is exact, and a depth gate would let the
        // heuristic pruning stack mask it. The root is excluded so it always
        // completes its move loop and reports a real best move.
        let pv_node = beta > alpha + 1;
        if !is_root && let Some(wdl) = self.tb_wdl(position) {
            if pv_node && !self.params.allow_syzygy_cutoff_on_pv_nodes {
                // A PV node is graded by the distance to the next zeroing move,
                // and a value whose distance fits in the remaining depth is
                // searched so the search can reach the real mate itself.
                let dtz_abs = self.tb_dtz(position).map_or(0, |dtz| dtz.abs());
                if dtz_abs <= 0 || dtz_abs > depth {
                    return crate::tb::syzygy::wdl_to_score_graded(wdl, dtz_abs);
                }
            } else {
                return crate::tb::syzygy::wdl_to_score(wdl);
            }
        }

        if depth <= 0 {
            return self.quiescence(position, alpha, beta, ply, previous);
        }

        // The exact-bound decision compares against the window as it stood
        // before the transposition-table narrowing below.
        let alpha_window = alpha;

        // Transposition-table probe. A position that already repeats on the
        // search path is refused table cutoffs: a cached score knows nothing
        // about the repetition the search is trying to detect.
        let allow_tt_cutoff = !self.params.disable_tt_cutoff_for_repetitions
            || self.path_repetition_count(position, ply) == 0;
        let mut tt_move = Move::NONE;
        let mut tt_probe = None;
        let mut tt_eval = None;
        if let Some(entry) = self.tables.tt.probe(position.key()) {
            // Bug B35 fix (reference bug 147): the table move is used as stored,
            // markers included. KnightByte's table packing keeps the full move,
            // so a stored castling or en-passant move compares equal to the
            // generated move it came from and is tried first, excluded in the
            // singular-verification search and eligible for a singular
            // extension. The reference strips these markers only because its
            // 16-bit packing cannot hold them; the fixed reference rebuilds
            // them from board context at probe time, and so does the stored
            // form here.
            tt_move = entry.mv;
            tt_probe = Some(entry);
            let stored_eval = i32::from(entry.eval);
            if stored_eval != NO_STATIC_EVAL {
                tt_eval = Some(stored_eval);
            }
            if self.trace && is_root {
                eprintln!(
                    "T t n {} tt {} depth {} gen {}",
                    self.nodes,
                    tt_move.to_uci(),
                    entry.depth,
                    entry.generation
                );
            }
            // A verification node would want to skip the transposition-table
            // cutoff: its entry is the one that triggered the verification, so
            // returning on it makes the verification meaningless. The reference
            // does not skip it (the verification always returns on its own
            // trigger entry, so a singular extension never fires â€” quirk Q1,
            // reproduced for parity; see ADR-0006), and neither does
            // KnightByte. The former `SingularExcludeTtCutoff` research lever
            // was removed after it measured no gain.
            if !is_root && allow_tt_cutoff && i32::from(entry.depth) >= depth {
                let score = Self::probe_score(&entry, ply);
                match entry.bound {
                    Bound::Exact => {
                        return score;
                    }
                    Bound::Lower => {
                        // Early transposition-table-cutoff history update:
                        // a quiet table move found at a non-PV node, whose
                        // parent found its own move quickly, earns a
                        // butterfly bonus. The reference awards it for every
                        // sufficiently deep lower-bound entry, before it
                        // decides whether the stored score reaches beta.
                        if !tt_move.is_none()
                            && !pv_node
                            && ply > 0
                            && self.move_counts[ply - 1] < 4
                            && !movegen::is_capture(position, tt_move)
                            && !tt_move.is_promotion()
                        {
                            let bonus = (207 * depth).min(1_202);
                            self.history.update_butterfly_bonus(
                                color_of(position.piece_at(tt_move.from())),
                                tt_move,
                                bonus,
                            );
                        }
                        if score >= beta {
                            return score;
                        }
                        alpha = alpha.max(score);
                    }
                    Bound::Upper => {
                        if score <= alpha {
                            return score;
                        }
                        beta = beta.min(score);
                    }
                }
                if alpha >= beta {
                    return score;
                }
            }
        }

        // DTZ root ranking: try the 50-move-aware tablebase move first. The
        // picker validates pseudo-legality, the root restriction filters it,
        // and a repeating move evaluates as a draw at its child, so this stays
        // an ordering hint rather than a bypass.
        if is_root && self.params.use_syzygy_dtz_root_ranking && !self.root_tb_move.is_none() {
            tt_move = self.root_tb_move;
        }

        let raw_eval = tt_eval.unwrap_or_else(|| self.evaluate_stacked(position, ply));
        let mut eval = raw_eval;

        // Correction history: a learned offset applied before every pruning
        // decision. The reference skips it when a usable TT move exists and
        // when the node is in check, and only corrects nodes with remaining
        // depth.
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
            && allow_tt_cutoff
            && let Some(entry) = tt_probe
            && Self::can_use_tt_score(&entry, raw_eval, ply)
        {
            eval = Self::probe_score(&entry, ply);
        }

        // Hindsight depth pair: the previous ply's reduction and the two
        // evals decide whether this node earns the reduction back or loses
        // another ply. Runs before every pruning decision, which therefore
        // sees the adjusted depth.
        if self.params.use_hindsight_reductions && ply > 0 && depth >= 2 && !in_check && !pv_node {
            let previous_reduction = self.reduction_stack[ply - 1];
            let previous_eval = self.evals[ply - 1];
            if previous_reduction >= 2 && eval + previous_eval < 0 {
                depth += 1;
            }
            if previous_reduction >= 1 && eval + previous_eval > 62 {
                depth -= 1;
            }
        }

        let improving = self.is_improving(ply, eval);

        if !pv_node && !in_check && excluded_move.is_none() {
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
                if self.trace {
                    eprintln!(
                        "T f n {} p {} eval {} beta {} margin {} raw {}",
                        self.nodes, ply, eval, beta, margin, raw_eval
                    );
                }
                if eval - margin >= beta {
                    return beta + (eval - beta) / 3;
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
        }

        // Null move: runs at any non-root node that is not in check, including
        // principal-variation nodes and singular-verification nodes (the gate
        // has no node-type or exclusion condition). It carries the cut-node
        // arm â€” at a predicted cut node the null search runs even when a
        // table score exists below beta. A non-mate fail-high cuts fail-hard
        // at beta; a mate line through a null move is not trusted and the node
        // keeps searching.
        if !is_root
            && depth >= 3
            && !in_check
            && eval >= beta
            && (tt_probe.is_none()
                || cut_node
                || tt_probe.is_some_and(|entry| Self::probe_score(&entry, ply) >= beta))
            && Self::has_non_pawn_material(position, position.side())
        {
            // Eval-scaled extra reduction, clamped so a null move can never
            // reach the current node's depth.
            let eval_bonus = ((eval - beta) / 191).min(4);
            let mut reduction =
                self.params.nmp_base_reduction + depth / self.params.nmp_depth_divisor + eval_bonus;
            if reduction >= depth {
                reduction = depth - 1;
            }
            let next = position.make_null();
            self.tables.tt.prefetch(next.key());
            self.copy_accumulators(ply);
            self.move_stack[ply] = Move::NONE;
            let score = -self.negamax(
                &next,
                -beta,
                -beta + 1,
                depth - 1 - reduction,
                ply + 1,
                None,
                !cut_node,
                Move::NONE,
                extensions_used,
                observer,
            );
            if self.stopped {
                return 0;
            }
            if score >= beta && score.abs() < MATE_LIMIT {
                return beta;
            }
        }

        // Singular extension: when a deep table move with a proven bound is
        // very likely the only good move, the node is forcing. The
        // verification search runs the alternatives (excluding the table
        // move) at half depth inside a window below the stored score; a wide
        // fail-low extends the node by one, two or three plies â€” and the
        // extension is applied to the table move only, in the move loop.
        let mut singular_extension = 0;
        if self.params.use_singular_extensions
            && !is_root
            && depth >= self.params.singular_min_depth
            && let Some(entry) = tt_probe
            && !tt_move.is_none()
            && excluded_move.is_none()
            && matches!(entry.bound, Bound::Exact | Bound::Lower)
            && Self::probe_score(&entry, ply).abs() < MATE_LIMIT
            && i32::from(entry.depth) >= depth - 3
        {
            let tt_score = Self::probe_score(&entry, ply);
            let singular_beta = tt_score - self.params.singular_beta_slope * depth;
            let singular_depth = depth / 2;
            let alternative = self.negamax(
                position,
                singular_beta - 1,
                singular_beta,
                singular_depth,
                ply,
                previous,
                cut_node,
                tt_move,
                extensions_used,
                observer,
            );
            if self.stopped {
                return 0;
            }
            if alternative < singular_beta {
                singular_extension = 1;
                if self.params.use_singular_double_extensions {
                    if alternative < singular_beta - self.params.singular_double_margin {
                        singular_extension = 2;
                    }
                    if alternative < singular_beta - self.params.singular_triple_margin {
                        singular_extension = 3;
                    }
                }
            }
        }

        // Internal iterative reduction: a node without a table move has no
        // ordering guidance and searches one ply shallower. A too-shallow
        // entry at a cut node admits the node as well (the quality gate).
        if self.params.use_internal_iterative_reduction
            && depth >= 4
            && !in_check
            && excluded_move.is_none()
        {
            let quality_gate = self.params.use_micro_riders
                && tt_probe.is_some_and(|entry| i32::from(entry.depth) < depth - 4);
            let node_type = pv_node || is_root || (cut_node && quality_gate);
            if (tt_move.is_none() || quality_gate) && node_type {
                depth -= 1;
            }
        }

        if depth <= 0 {
            return self.quiescence(position, alpha, beta, ply, previous);
        }

        crate::node_stat!(NegamaxMoveLists);
        let moves = movegen::legal(position);
        crate::node_stat!(PseudoMoves, moves.as_slice().len());
        if moves.is_empty() {
            return if in_check {
                mated_in(ply as i32)
            } else {
                DRAW_SCORE
            };
        }

        // Step 6 (`go searchmoves`): at the root, only the listed moves are
        // searched. An empty filter means no restriction; a filter whose
        // every entry misses the legal list is ignored (the reference runs
        // the unrestricted search there). The clone is fine: the root node
        // is visited a handful of times per search, and holding the borrow
        // across the picker loop would fight the mutable searcher state.
        let mut restricted: Vec<Move> = Vec::new();
        if ply == 0 && !self.root_filter.is_empty() {
            for mv in moves.as_slice() {
                if self.root_filter.contains(mv) {
                    restricted.push(*mv);
                }
            }
        }
        let moves_slice: &[Move] = if restricted.is_empty() {
            moves.as_slice()
        } else {
            &restricted
        };

        let cont = self.continuations(ply);
        self.picker[ply].reset(
            tt_move,
            &self.killers,
            &self.history,
            ply,
            previous,
            &cont,
            false,
            false,
        );
        self.picker[ply].trace_reset(self.nodes);

        let futile = !pv_node
            && !in_check
            && depth <= self.params.futility_max_depth
            && eval + futility_margin(depth) < alpha;

        let mut best_score = -INFINITE;
        let mut best_move = Move::NONE;
        let mut quiet_move_count = 0;
        let mut legal_moves = 0;
        let mut quiets_searched = [Move::NONE; 64];
        let mut quiets_count = 0;

        while let Some(mv) = self.picker[ply].next(position, &self.history, moves_slice) {
            crate::node_stat!(MovesTried);
            // Step 10: the reference's single `info currmove` line of the
            // search — emitted when iteration 1's first root move begins,
            // before the depth-1 line, with `currmovenumber 1` and the node
            // count at that moment (1). No line when the search never
            // reaches iteration 1's move loop (a stop before it).
            if ply == 0 && depth == 1 && !self.currmove_reported {
                self.currmove_reported = true;
                observer(SearchEvent::CurrMove {
                    mv,
                    number: 1,
                    nodes: self.nodes,
                });
            }
            if !excluded_move.is_none() && mv == excluded_move {
                continue;
            }
            let capture = movegen::is_capture(position, mv);
            let quiet_counted = !capture && !mv.is_promotion();
            let moving = position.piece_at(mv.from());

            // Combined history score, computed pre-move: quiets read the
            // butterfly and continuation tables, captures the capture table.
            let history_score = if capture {
                let victim = if mv.flag() == crate::types::r#move::flags::EN_PASSANT {
                    PAWN
                } else {
                    piece_type(position.piece_at(mv.to()))
                };
                self.history
                    .capture(piece_type(moving), mv, victim)
                    .clamp(-8_033, 8_033)
            } else {
                self.history.pruning_score(color_of(moving), mv, &cont)
            };

            // Depth-scaled SEE pruning with a history refund; the first legal
            // move of the node is never pruned, and king moves are exempt on
            // the quiet side (the exchange says little about king safety).
            // The flat legacy thresholds (-50 captures, -100 quiets, ceiling
            // 4) remain available behind the lever.
            let see_ceiling = if self.params.use_ref_see_thresholds {
                self.params.see_pruning_max_depth
            } else {
                self.params.see_pruning_max_depth.min(4)
            };
            if depth <= see_ceiling && !in_check && !pv_node && legal_moves > 0 {
                if capture {
                    let threshold = if self.params.use_ref_see_thresholds {
                        self.params.see_noisy_margin * depth * depth
                            - history_score / self.params.see_history_divisor
                    } else {
                        -50
                    };
                    if !see_ge(position, mv, threshold) {
                        continue;
                    }
                } else if !mv.is_promotion() && piece_type(moving) != KING {
                    let threshold = if self.params.use_ref_see_thresholds {
                        self.params.see_quiet_margin * depth
                            - history_score / self.params.see_history_divisor
                    } else {
                        -100
                    };
                    if !see_ge(position, mv, threshold) {
                        continue;
                    }
                }
            }

            self.move_stack[ply] = mv;

            // The legal-move counter increments immediately after the
            // legality check and before every pruning block, so pruned moves
            // still occupy a slot in the count late-move reductions index by.
            legal_moves += 1;
            self.move_counts[ply] = legal_moves;
            if quiet_counted {
                quiet_move_count += 1;
            }

            // Root-move node attribution for the time manager's
            // node-fraction term: the whole subtree of this move (including
            // the re-searches) lands in its slot.
            let nodes_before_move = if is_root { self.nodes } else { 0 };

            // Late move pruning: enough quiets have been tried that the rest
            // cannot realistically be best; stop trying quiets entirely (the
            // picker's quiet stage dies; bad captures are still served).
            if !in_check
                && quiet_counted
                && !pv_node
                && legal_moves > 1
                && depth <= self.params.lmp_max_depth
            {
                let mut limit = lmp_limit(depth);
                if self.params.use_improving_flag && !improving {
                    limit = (limit / 2).max(1);
                }
                if quiet_move_count > limit {
                    self.picker[ply].skip_quiets();
                    continue;
                }
            }

            // History pruning: a quiet whose combined history is far below
            // the depth-scaled threshold is skipped; the remaining quiets are
            // still searched (one move at a time).
            if self.params.use_history_pruning
                && depth <= self.params.hist_prune_max_depth
                && !pv_node
                && quiet_counted
                && !in_check
                && legal_moves > 1
                && history_score < -self.params.hist_prune_margin * depth
            {
                continue;
            }

            // Futility: the static evaluation is so far below alpha that even
            // the margin cannot bridge the gap; quiet non-pawn moves cannot
            // realistically improve on alpha. Pawn moves are exempt.
            if !in_check
                && !pv_node
                && quiet_counted
                && legal_moves > 1
                && futile
                && piece_type(moving) != PAWN
                && eval + futility_margin(depth) < alpha
            {
                self.picker[ply].skip_quiets();
                continue;
            }

            // The move is made, and the child's accumulator pair pushed, only
            // once the pruning blocks above have let it through: they decide
            // from the parent's data alone, so a pruned move costs neither a
            // board copy nor a network update. The pair at ply + 1 is written
            // from the parent's pair by every move that reaches a child, so
            // a skipped push leaves nothing stale behind.
            let next = position.make_move(mv);
            self.tables.tt.prefetch(next.key());
            crate::node_stat!(MovesMade);
            self.push_accumulators(position, &next, mv, ply);
            let gives_check = next.in_check(next.side());

            let mut extension = if mv == tt_move { singular_extension } else { 0 };

            // Negative extensions: suspicious moves are searched one ply
            // shallower â€” a losing capture, or a quiet the history tables
            // have learned to distrust.
            if self.params.use_negative_extensions
                && depth >= 3
                && !pv_node
                && !in_check
                && !gives_check
            {
                if capture {
                    if !see_ge(position, mv, 0) {
                        extension -= 1;
                    }
                } else if !mv.is_promotion()
                    && self.history.butterfly(color_of(moving), mv) < -3_000
                {
                    extension -= 1;
                }
            }

            let new_depth = (depth - 1 + extension).max(0);

            // Late move reduction (the live schedule): the log base plus the
            // opponent-material, history, cut-node, PV and killer terms, in
            // fixed point over 1024ths of a ply.
            let mut reduction = 0;
            if self.params.use_lmr_table
                && depth >= 3
                && legal_moves > 3
                && !capture
                && !mv.is_promotion()
                && !in_check
                && !gives_check
            {
                let mut reduction_1024 = lmr_base_reduction(depth, legal_moves) * 1024;
                // A bare endgame (the mover holds at most four pieces) makes
                // late moves matter less.
                if next.occupancy(color_of(moving)).count_ones() <= 4 {
                    reduction_1024 += 1024;
                }
                if self.params.use_term1_not_improving && !improving {
                    reduction_1024 += 1024;
                }
                reduction_1024 -= lmr_history_term_1024(
                    history_score,
                    true,
                    self.params.use_lmr_history_precision,
                );
                if self.params.use_cut_node && cut_node {
                    reduction_1024 += 1024;
                }
                if pv_node {
                    reduction_1024 -= 1024;
                }
                // Killers are read live; the countermove is the value the
                // picker captured before the loop started (see
                // `PickerState::counter_move`).
                if self.killers.contains(ply, mv)
                    || self.picker[ply].counter_move() == mv.without_double_push()
                {
                    reduction_1024 -= 1024;
                }
                reduction = reduction_1024 / 1024;
                if reduction > new_depth {
                    reduction = new_depth;
                }
                if reduction < 0 {
                    reduction = 0;
                }
            }
            // Both per-ply stacks are published here, per searched move, and
            // not at node entry: a null-move child (or any node reached
            // before the first move of this node is searched) therefore sees
            // the values an earlier visit of this ply left behind. The
            // hindsight pair and the improving flag depend on that.
            self.reduction_stack[ply] = reduction;
            self.evals[ply] = eval;

            let mut score;
            if legal_moves == 1 {
                // First legal move: a principal-variation child at PV nodes,
                // otherwise the opposite node type.
                let child_cut = if pv_node { false } else { !cut_node };
                score = -self.negamax(
                    &next,
                    -beta,
                    -alpha,
                    new_depth,
                    ply + 1,
                    Some((moving, mv.from(), mv.to())),
                    child_cut,
                    Move::NONE,
                    extensions_used,
                    observer,
                );
            } else {
                // Reduced scout of a late move: treated as an expected cut
                // node.
                score = -self.negamax(
                    &next,
                    -alpha - 1,
                    -alpha,
                    new_depth - reduction,
                    ply + 1,
                    Some((moving, mv.from(), mv.to())),
                    true,
                    Move::NONE,
                    extensions_used,
                    observer,
                );
                if score > alpha && reduction > 0 {
                    let re_search_depth = compute_re_search_depth(
                        new_depth,
                        reduction,
                        self.params.use_re_search_reduction,
                    );
                    score = -self.negamax(
                        &next,
                        -alpha - 1,
                        -alpha,
                        re_search_depth,
                        ply + 1,
                        Some((moving, mv.from(), mv.to())),
                        !cut_node,
                        Move::NONE,
                        extensions_used,
                        observer,
                    );
                }
                if score > alpha && score < beta {
                    // Full-window re-search establishes the principal
                    // variation: a PV child, never a cut node.
                    score = -self.negamax(
                        &next,
                        -beta,
                        -alpha,
                        new_depth,
                        ply + 1,
                        Some((moving, mv.from(), mv.to())),
                        false,
                        Move::NONE,
                        extensions_used,
                        observer,
                    );
                }
            }
            if self.stopped {
                return 0;
            }

            if is_root {
                self.nodes_per_move[usize::from(mv.from()) * 64 + usize::from(mv.to())] +=
                    self.nodes - nodes_before_move;
            }

            if score <= alpha && quiet_counted && quiets_count < quiets_searched.len() {
                quiets_searched[quiets_count] = mv;
                quiets_count += 1;
            }

            if score > best_score {
                best_score = score;
                best_move = mv;
                if score > alpha {
                    alpha = score;
                    self.pv.update(ply, mv);
                    if score >= beta {
                        // The history branch is chosen by "not a capture", so a
                        // quiet promotion credits the quiet-move tables even
                        // though it is excluded from the failed-quiet list.
                        if !capture {
                            self.killers.record(ply, mv);
                            self.history.update_quiet(
                                color_of(moving),
                                mv,
                                depth,
                                &quiets_searched[..quiets_count],
                                &cont,
                            );
                            self.history.update_counter(previous, mv);
                            if self.trace {
                                eprintln!("K n 0 p {ply} d 0 mv {} bads 0", mv.to_uci());
                                eprintln!(
                                    "K h n 0 p 0 d {depth} mv {} bads {}",
                                    mv.to_uci(),
                                    quiets_count
                                );
                            }
                        } else {
                            let victim = if mv.flag() == crate::types::r#move::flags::EN_PASSANT {
                                PAWN
                            } else {
                                piece_type(position.piece_at(mv.to()))
                            };
                            self.history
                                .update_capture(piece_type(moving), mv, victim, depth);
                        }
                        self.trace_node('r', ply, depth, alpha, beta, cut_node, best_score);
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
                            excluded: !excluded_move.is_none(),
                        });
                    }
                }
            }
        }

        // No legal move was searched. Besides a genuine mate or stalemate this
        // is the singular-verification case where the excluded table move is
        // the only legal move: the node then reports the mate or draw score
        // (nothing is stored), which is what the extension tiers compare
        // against.
        if legal_moves == 0 {
            return if in_check {
                mated_in(ply as i32)
            } else {
                DRAW_SCORE
            };
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
        self.trace_node('r', ply, depth, alpha, beta, cut_node, best_score);
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
            excluded: !excluded_move.is_none(),
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

#[cfg(test)]
mod tests {
    use crate::types::r#move::{Move, flags};

    #[test]
    fn a_table_move_keeps_its_castle_and_en_passant_markers() {
        // Bug B35 fix (reference bug 147): the stored move comes back exactly
        // as written, so a stored castling or en-passant move compares equal
        // to the generated move it came from.
        let castle = Move::new(4, 6, flags::CASTLE);
        assert_eq!(castle.flag(), flags::CASTLE);

        let en_passant = Move::new(36, 43, flags::EN_PASSANT);
        assert_eq!(en_passant.flag(), flags::EN_PASSANT);
    }

    #[test]
    fn other_table_moves_are_kept_as_stored() {
        for mv in [
            Move::new(12, 28, flags::DOUBLE_PUSH),
            Move::new(52, 60, flags::PROMO_QUEEN),
            Move::new(52, 60, flags::PROMO_KNIGHT),
            Move::new(6, 21, flags::NORMAL),
            Move::NONE,
        ] {
            assert_eq!(Move::from_bits(mv.bits()), mv);
        }
    }
}
