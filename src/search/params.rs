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
    /// Weight of the "not improving" term in late move reductions.
    pub lmr_v2_not_improving_weight: i32,
    /// Whether the improving flag is computed at all.
    pub use_improving_flag: bool,
    /// Whether reverse futility uses the formula margin instead of a constant.
    pub use_formula_rfp: bool,
    /// Whether late move reductions use the depth/move-count table.
    pub use_lmr_table: bool,
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
            lmr_v2_not_improving_weight: 70,
            use_improving_flag: true,
            use_formula_rfp: true,
            use_lmr_table: true,
            use_tt_score_eval: true,
            use_correction_history: true,
            use_ref_correction_history: true,
            use_qs_correction_history: false,
            use_eval_cache: false,
            use_qs_tt_probe: true,
            use_twofold_repetition_draw: true,
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
        assert_eq!(params.lmr_v2_not_improving_weight, 70);
        assert!(params.use_improving_flag);
        assert!(params.use_formula_rfp);
        assert!(params.use_lmr_table);
        assert!(params.use_tt_score_eval);
        assert!(
            !params.use_eval_cache,
            "the catalogue ships the cache disabled"
        );
        assert!(params.use_qs_tt_probe);
        assert!(params.use_twofold_repetition_draw);
    }
}
