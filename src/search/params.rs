//! Search parameters.
//!
//! This struct is the single source of truth for every tunable search constant
//! that has been ported so far. The names and defaults mirror the recorded
//! lever catalogue so the UCI option layer can be generated from the same
//! table later without renaming anything.
//!
//! Formulas are provisional: they reproduce the documented feature, not yet the
//! predecessor's exact arithmetic. Node counts are therefore a determinism
//! baseline, not a parity gate, until the reference implementation can be run
//! side by side.
//!
//! Only integer arithmetic is used. Floating-point exp/log approximations can
//! differ in the last bit between platforms, and a single differing reduction
//! would change every later node count.

/// Tunable search constants and feature switches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchParams {
    /// Initial aspiration window width in centipawns.
    pub aspiration_base: i32,
    /// Maximum aspiration re-search cycles per depth.
    pub aspiration_max_cycles: i32,
    /// First depth that uses aspiration windows.
    pub aspiration_start_depth: i32,
    /// Fixed part of the null-move reduction.
    pub nmp_base_reduction: i32,
    /// Depth divisor of the extra null-move reduction.
    pub nmp_depth_divisor: i32,
    /// Deepest node that may use reverse futility with the per-depth table.
    pub rfp_max_depth: i32,
    /// Deepest node that may use reverse futility with the linear formula.
    pub rfp_formula_max_depth: i32,
    /// Reverse futility margin scale when the static evaluation improved.
    pub rfp_improving_scale: i32,
    /// Reverse futility margin scale when it did not improve.
    pub rfp_not_improving_scale: i32,
    /// Deepest node that may use razoring.
    pub razor_max_depth: i32,
    /// Deepest node that may use late move pruning.
    pub lmp_max_depth: i32,
    /// Deepest node that may use futility pruning.
    pub futility_max_depth: i32,
    /// Deepest node that may prune captures by exchange evaluation.
    pub see_pruning_max_depth: i32,
    /// Safety margin of quiescence delta pruning.
    pub qs_delta_margin: i32,
    /// Whether quiescence narrowing (recapture-only and promotion limits) runs.
    pub use_qs_narrowing: bool,
    /// Qsearch ply from which only recaptures are searched.
    pub qs_recapture_only_ply: i32,
    /// Qsearch ply from which promotions are no longer searched.
    pub qs_promotion_max_ply: i32,
    /// Whether quiescence uses the cheap approximate-SEE prefilter.
    pub use_see_approx_prefilter: bool,
    /// Whether quiescence applies its fail-soft averaging on fail-high.
    pub use_micro_riders: bool,
    /// Whether quiescence searches all evasions when the king is attacked.
    pub use_check_aware_quiescence: bool,
    /// Maximum summed extensions along one path.
    pub max_path_extensions: i32,
    /// Whether the improving flag is computed at all.
    pub use_improving_flag: bool,
    /// Whether reverse futility uses the formula margin instead of a constant.
    pub use_formula_rfp: bool,
    /// Whether late move reductions use the depth/move-count table.
    pub use_lmr_table: bool,
    /// Whether the "+1 ply when not improving" late-move-reduction term is
    /// applied. The reference ships this term OFF; the reduction schedule's
    /// fine structure comes from the other terms.
    pub use_term1_not_improving: bool,
    /// Whether the history term uses multiply-before-divide fixed point
    /// (`h * 1024 / divisor`) instead of divide-first truncation.
    pub use_lmr_history_precision: bool,
    /// Whether the cut-node flag drives the late-move-reduction term and the
    /// null-move gate arm.
    pub use_cut_node: bool,
    /// Whether the reference transposition-table replacement policy is used:
    /// a 5-bit generation (starting at one), a victim chosen by
    /// `stored depth - 2 per generation of age`, a same-key refresh that keeps
    /// a deeper current-generation entry, and no deeper-entry slack guard.
    pub use_ref_tt_replacement: bool,
    /// Whether quiet moves with very poor combined history are pruned at low
    /// depth.
    pub use_history_pruning: bool,
    /// Deepest node that may use history pruning.
    pub hist_prune_max_depth: i32,
    /// History-pruning margin scale: the threshold is `-margin * depth`.
    pub hist_prune_margin: i32,
    /// Whether the hindsight depth pair runs: a heavily reduced parent whose
    /// combined evaluation fell below zero earns the ply back, a reduced
    /// parent whose combined evaluation rose above the threshold loses one.
    pub use_hindsight_reductions: bool,
    /// Whether a node without a transposition-table move searches one ply
    /// shallower at principal-variation or root nodes.
    pub use_internal_iterative_reduction: bool,
    /// Whether the singular-extension verification search runs.
    pub use_singular_extensions: bool,
    /// Whether a singular move can earn a second or third extension ply on a
    /// wide verification fail-low margin.
    pub use_singular_double_extensions: bool,
    /// Minimum remaining depth for the singular-extension attempt.
    pub singular_min_depth: i32,
    /// Singular-beta slope: the verification window sits this many centipawns
    /// per ply below the stored score.
    pub singular_beta_slope: i32,
    /// Verification fail-low margin that earns a second extension ply.
    pub singular_double_margin: i32,
    /// Verification fail-low margin that earns a third extension ply.
    pub singular_triple_margin: i32,
    /// Whether suspicious moves (losing captures, historically bad quiets)
    /// are searched one ply shallower.
    pub use_negative_extensions: bool,
    /// Whether the reference's depth-scaled, history-refunded SEE pruning
    /// thresholds replace the flat legacy ones.
    pub use_ref_see_thresholds: bool,
    /// Quiet-move SEE-pruning margin per ply (negative = loss tolerance).
    pub see_quiet_margin: i32,
    /// Capture SEE-pruning margin per squared ply.
    pub see_noisy_margin: i32,
    /// Divisor of the combined-history refund in the SEE-pruning threshold.
    pub see_history_divisor: i32,
    /// Whether the re-search after a reduced fail-high is shaved for large
    /// reductions.
    pub use_re_search_reduction: bool,
    /// Whether a position that already repeats on the search path is refused
    /// transposition-table cutoffs (repetition-hunting protection).
    pub disable_tt_cutoff_for_repetitions: bool,
    /// Whether the stored static evaluation is reused from the table.
    pub use_tt_score_eval: bool,
    /// Whether the static evaluation is corrected by learned history.
    pub use_correction_history: bool,
    /// Whether the four-term reference correction history is used.
    pub use_ref_correction_history: bool,
    /// Whether quiescence applies the correction to its stand pat.
    pub use_qs_correction_history: bool,
    /// Whether the static-evaluation cache is consulted.
    pub use_eval_cache: bool,
    /// Whether quiescence probes the transposition table.
    pub use_qs_tt_probe: bool,
    /// Whether the first repetition of a position scores as a draw.
    pub use_twofold_repetition_draw: bool,
    /// Whether the stable time-management model (the three-signal
    /// soft-limit scaler) replaces the legacy PV-stability multipliers on
    /// clock-controlled searches.
    pub use_stable_time_management: bool,
    /// Whether the two-sided clock-imbalance scaler runs (it supersedes the
    /// one-sided time-advantage multiplier when enabled).
    pub use_clock_imbalance: bool,
    /// Ahead-side clock-imbalance scale, in percent of the ratio surplus.
    pub clock_imbalance_ahead_scale: i32,
    /// Behind-side clock-imbalance scale, in percent of the ratio deficit.
    pub clock_imbalance_behind_scale: i32,
    /// Whether the root move of a tablebase position is ordered by DTZ.
    ///
    /// Without it every win-preserving move scores the same flat tablebase
    /// value and the root keeps whichever move the ordering happens to put
    /// first, which can shuffle a won endgame into a fifty-move draw. This is
    /// a tie-break, not a search bypass: the search still runs, still handles
    /// repetitions, and a real mate still outranks the tablebase score.
    pub use_syzygy_dtz_root_ranking: bool,
    /// Whether an exact tablebase value may cut off a principal-variation node.
    ///
    /// On (the default) the probe returns the exact value everywhere, which is
    /// what makes endgame move selection perfect. Off, a PV node is instead
    /// graded by DTZ, and a node whose distance fits in the remaining depth is
    /// searched so the real mate can be announced. The default is the measured
    /// stronger policy; the off state exists for the mechanism's tests.
    pub allow_syzygy_cutoff_on_pv_nodes: bool,
    /// Divisor of the evaluation surplus (`eval - beta`) in the null-move
    /// reduction bonus.
    pub nmp_eval_divisor: i32,
    /// Largest evaluation bonus of the null-move reduction, in plies.
    pub nmp_eval_bonus_max: i32,
    /// Shallowest remaining depth at which the null move is tried.
    pub nmp_min_depth: i32,
    /// Divisor of the overshoot a reverse-futility cutoff hands back
    /// (`beta + (eval - beta) / divisor`).
    pub rfp_damping: i32,
    /// Razoring margins in percent of their base values (86 and 353).
    pub razor_margin_scale: i32,
    /// Futility margins in percent of their base table.
    pub futility_scale: i32,
    /// Late-move-pruning quiet-move limits in percent of their base table.
    pub lmp_scale: i32,
    /// Butterfly-history score below which a quiet move is searched one ply
    /// shallower (the threshold is the negative of this value).
    pub neg_ext_history_threshold: i32,
    /// Shallowest remaining depth of internal iterative reduction.
    pub iir_min_depth: i32,
    /// Combined-evaluation threshold of the hindsight depth pair: a reduced
    /// parent whose evaluation sum exceeds it loses a ply.
    pub hindsight_margin: i32,
    /// Divisor of the late-move-reduction table, in hundredths
    /// (`1 + ln(depth) * ln(move) / (divisor / 100)`).
    pub lmr_divisor: i32,
    /// Shallowest remaining depth at which late moves are reduced.
    pub lmr_min_depth: i32,
    /// Moves searched at full depth before late-move reduction starts.
    pub lmr_min_moves: i32,
    /// Divisor of the history term of late-move reduction, in 1024ths of a
    /// ply: a larger divisor lets the history reduce fewer plies.
    pub lmr_history_divisor: i32,
    /// Late-move reduction taken off at principal-variation nodes, in 1024ths
    /// of a ply.
    pub lmr_pv_weight: i32,
    /// Late-move reduction taken off for a killer or countermove, in 1024ths
    /// of a ply.
    pub lmr_killer_weight: i32,
    /// History bonuses (scale and cap) in percent of their base values.
    pub history_bonus_scale: i32,
    /// History maluses (scale and cap) in percent of their base values.
    pub history_malus_scale: i32,
    /// Whether ProbCut runs: a shallow search that proves a likely beta
    /// cutoff before the full search.
    pub use_probcut: bool,
    /// ProbCut beta margin above `beta`, in centipawns.
    pub probcut_margin: i32,
    /// Shallowest remaining depth at which ProbCut is tried.
    pub probcut_min_depth: i32,
    /// Whether a transposition-table entry that disagrees with the window
    /// extends the node by one ply.
    pub use_tt_extension: bool,
    /// Deepest node (before extension) that the table extension may extend.
    pub tt_extension_max_depth: i32,
    /// Whether the first losing capture of a hopeless node ends the move
    /// loop.
    pub use_bad_noisy_pruning: bool,
    /// Bad-noisy pruning applies below this remaining depth.
    pub bnp_depth: i32,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            aspiration_base: 50,
            aspiration_max_cycles: 8,
            aspiration_start_depth: 5,
            nmp_base_reduction: 3,
            nmp_depth_divisor: 4,
            rfp_max_depth: 4,
            rfp_formula_max_depth: 8,
            rfp_improving_scale: 65,
            rfp_not_improving_scale: 95,
            razor_max_depth: 3,
            lmp_max_depth: 4,
            futility_max_depth: 7,
            see_pruning_max_depth: 8,
            qs_delta_margin: 1_100,
            use_qs_narrowing: false,
            qs_recapture_only_ply: 8,
            qs_promotion_max_ply: 2,
            use_see_approx_prefilter: true,
            use_micro_riders: true,
            use_check_aware_quiescence: true,
            max_path_extensions: 32,
            use_improving_flag: true,
            use_formula_rfp: true,
            use_lmr_table: true,
            use_term1_not_improving: false,
            use_lmr_history_precision: true,
            use_cut_node: true,
            use_ref_tt_replacement: true,
            use_history_pruning: true,
            hist_prune_max_depth: 3,
            hist_prune_margin: 2_000,
            use_hindsight_reductions: true,
            use_internal_iterative_reduction: true,
            use_singular_extensions: true,
            use_singular_double_extensions: true,
            singular_min_depth: 8,
            singular_beta_slope: 2,
            singular_double_margin: 30,
            singular_triple_margin: 120,
            use_negative_extensions: true,
            use_ref_see_thresholds: true,
            see_quiet_margin: -43,
            see_noisy_margin: -26,
            see_history_divisor: 128,
            use_re_search_reduction: true,
            disable_tt_cutoff_for_repetitions: true,
            use_tt_score_eval: true,
            use_correction_history: true,
            use_ref_correction_history: true,
            use_qs_correction_history: false,
            use_eval_cache: false,
            use_qs_tt_probe: true,
            use_twofold_repetition_draw: true,
            use_stable_time_management: true,
            use_clock_imbalance: false,
            clock_imbalance_ahead_scale: 50,
            clock_imbalance_behind_scale: 50,
            use_syzygy_dtz_root_ranking: true,
            allow_syzygy_cutoff_on_pv_nodes: true,
            nmp_eval_divisor: 191,
            nmp_eval_bonus_max: 4,
            nmp_min_depth: 3,
            rfp_damping: 3,
            razor_margin_scale: 100,
            futility_scale: 100,
            lmp_scale: 100,
            neg_ext_history_threshold: 3_000,
            iir_min_depth: 4,
            hindsight_margin: 62,
            lmr_divisor: 175,
            lmr_min_depth: 3,
            lmr_min_moves: 3,
            lmr_history_divisor: 3_419,
            lmr_pv_weight: 1_024,
            lmr_killer_weight: 1_024,
            history_bonus_scale: 100,
            history_malus_scale: 100,
            use_probcut: false,
            probcut_margin: 235,
            probcut_min_depth: 5,
            use_tt_extension: false,
            tt_extension_max_depth: 6,
            use_bad_noisy_pruning: false,
            bnp_depth: 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SearchParams;

    #[test]
    fn defaults_match_the_recorded_lever_catalogue() {
        let params = SearchParams::default();
        assert_eq!(params.aspiration_base, 50);
        assert_eq!(params.aspiration_max_cycles, 8);
        assert_eq!(params.aspiration_start_depth, 5);
        assert_eq!(params.nmp_base_reduction, 3);
        assert_eq!(params.nmp_depth_divisor, 4);
        assert_eq!(params.rfp_max_depth, 4);
        assert_eq!(params.rfp_formula_max_depth, 8);
        assert_eq!(params.rfp_improving_scale, 65);
        assert_eq!(params.rfp_not_improving_scale, 95);
        assert_eq!(params.razor_max_depth, 3);
        assert_eq!(params.lmp_max_depth, 4);
        assert_eq!(params.futility_max_depth, 7);
        assert_eq!(params.see_pruning_max_depth, 8);
        assert_eq!(params.qs_delta_margin, 1_100);
        assert!(!params.use_qs_narrowing);
        assert_eq!(params.qs_recapture_only_ply, 8);
        assert_eq!(params.qs_promotion_max_ply, 2);
        assert!(params.use_see_approx_prefilter);
        assert!(params.use_micro_riders);
        assert!(params.use_check_aware_quiescence);
        assert_eq!(params.max_path_extensions, 32);
        assert!(params.use_improving_flag);
        assert!(params.use_formula_rfp);
        assert!(params.use_lmr_table);
        assert!(!params.use_term1_not_improving);
        assert!(params.use_lmr_history_precision);
        assert!(params.use_cut_node);
        assert!(params.use_ref_tt_replacement);
        assert!(params.use_history_pruning);
        assert_eq!(params.hist_prune_max_depth, 3);
        assert_eq!(params.hist_prune_margin, 2_000);
        assert!(params.use_hindsight_reductions);
        assert!(params.use_internal_iterative_reduction);
        assert!(params.use_singular_extensions);
        assert!(params.use_singular_double_extensions);
        assert_eq!(params.singular_min_depth, 8);
        assert_eq!(params.singular_beta_slope, 2);
        assert_eq!(params.singular_double_margin, 30);
        assert_eq!(params.singular_triple_margin, 120);
        assert!(params.use_negative_extensions);
        assert!(params.use_ref_see_thresholds);
        assert_eq!(params.see_quiet_margin, -43);
        assert_eq!(params.see_noisy_margin, -26);
        assert_eq!(params.see_history_divisor, 128);
        assert!(params.use_re_search_reduction);
        assert!(params.disable_tt_cutoff_for_repetitions);
        assert!(params.use_tt_score_eval);
        assert!(
            !params.use_eval_cache,
            "the catalogue ships the cache disabled"
        );
        assert!(params.use_qs_tt_probe);
        assert!(params.use_twofold_repetition_draw);
        assert!(params.use_stable_time_management);
        assert!(!params.use_clock_imbalance);
        assert_eq!(params.clock_imbalance_ahead_scale, 50);
        assert_eq!(params.clock_imbalance_behind_scale, 50);
    }

    #[test]
    fn the_former_constants_default_to_the_values_they_replaced() {
        // Each of these used to be a literal in the search; a different
        // default would change every node count.
        let params = SearchParams::default();
        assert_eq!(params.nmp_eval_divisor, 191);
        assert_eq!(params.nmp_eval_bonus_max, 4);
        assert_eq!(params.nmp_min_depth, 3);
        assert_eq!(params.rfp_damping, 3);
        assert_eq!(params.razor_margin_scale, 100);
        assert_eq!(params.futility_scale, 100);
        assert_eq!(params.lmp_scale, 100);
        assert_eq!(params.neg_ext_history_threshold, 3_000);
        assert_eq!(params.iir_min_depth, 4);
        assert_eq!(params.hindsight_margin, 62);
        assert_eq!(params.lmr_divisor, 175);
        assert_eq!(params.lmr_min_depth, 3);
        assert_eq!(params.lmr_min_moves, 3);
        assert_eq!(params.lmr_history_divisor, 3_419);
        assert_eq!(params.lmr_pv_weight, 1_024);
        assert_eq!(params.lmr_killer_weight, 1_024);
        assert_eq!(params.history_bonus_scale, 100);
        assert_eq!(params.history_malus_scale, 100);
        // The ported features ship off.
        assert!(!params.use_probcut);
        assert!(!params.use_tt_extension);
        assert!(!params.use_bad_noisy_pruning);
        assert_eq!(params.probcut_margin, 235);
        assert_eq!(params.probcut_min_depth, 5);
        assert_eq!(params.tt_extension_max_depth, 6);
        assert_eq!(params.bnp_depth, 6);
    }
}
