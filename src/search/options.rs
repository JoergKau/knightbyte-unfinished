//! UCI option catalogue.
//!
//! One declarative table drives the `option` handshake, `setoption` dispatch and
//! the documentation. Two groups exist:
//!
//! * **public options** - the ten names a GUI or match runner needs (`Hash`,
//!   `Threads`, `Move Overhead`, `Clear Hash`, the book and tablebase options).
//!   Every one of them does something; `EvalFile` and `Contempt` were removed
//!   because the network is embedded and the engine has no contempt.
//! * **research levers** - exactly sixty, all functional, listed in
//!   [`crate::search::lever_table`] (ADR-0014). Nothing inert is advertised.
//!
//! Adding a lever means one row in [`crate::search::lever_table`], a
//! `SearchParams` field and a getter/setter arm here; the handshake and the
//! parser pick it up automatically.

use crate::search::lever_table::{LEVERS, LeverSpec};
use crate::search::params::SearchParams;

/// Type of a UCI option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionKind {
    /// Integer with a range.
    Spin,
    /// Boolean.
    Check,
    /// Free text.
    String,
    /// Action without a value.
    Button,
}

/// One advertised UCI option.
#[derive(Clone, Copy, Debug)]
pub struct UciOption {
    /// Option name as advertised and accepted by `setoption`.
    pub name: &'static str,
    /// Value type.
    pub kind: OptionKind,
    /// Default value rendered as text (empty for buttons).
    pub default: &'static str,
    /// Minimum for spin options.
    pub min: Option<i64>,
    /// Maximum for spin options.
    pub max: Option<i64>,
}

/// Public options advertised to a GUI.
///
/// All ten are functional: the table size, the thread count, the move overhead,
/// the hash reset, the opening book and the tablebases.
pub const PUBLIC_OPTIONS: &[UciOption] = &[
    UciOption {
        name: "Hash",
        kind: OptionKind::Spin,
        default: "256",
        min: Some(1),
        max: Some(1024),
    },
    UciOption {
        name: "Threads",
        kind: OptionKind::Spin,
        default: "1",
        min: Some(1),
        max: Some(256),
    },
    UciOption {
        name: "Move Overhead",
        kind: OptionKind::Spin,
        default: "50",
        min: Some(0),
        max: Some(5000),
    },
    UciOption {
        name: "Clear Hash",
        kind: OptionKind::Button,
        default: "",
        min: None,
        max: None,
    },
    UciOption {
        name: "Own Book",
        kind: OptionKind::Check,
        default: "false",
        min: None,
        max: None,
    },
    UciOption {
        name: "Book File",
        kind: OptionKind::String,
        default: "",
        min: None,
        max: None,
    },
    UciOption {
        name: "Book Depth",
        kind: OptionKind::Spin,
        default: "16",
        min: Some(0),
        max: Some(100),
    },
    UciOption {
        name: "SyzygyPath",
        kind: OptionKind::String,
        default: "",
        min: None,
        max: None,
    },
    UciOption {
        name: "SyzygyRootProbe",
        kind: OptionKind::Check,
        default: "false",
        min: None,
        max: None,
    },
    UciOption {
        name: "Syzygy50MoveRule",
        kind: OptionKind::Check,
        default: "true",
        min: None,
        max: None,
    },
];

/// Returns the lever spec with this name.
pub fn lever(name: &str) -> Option<&'static LeverSpec> {
    LEVERS.iter().find(|spec| spec.name == name)
}

/// Returns the public option with this name.
pub fn public(name: &str) -> Option<&'static UciOption> {
    PUBLIC_OPTIONS.iter().find(|spec| spec.name == name)
}

/// Reads the current value of a lever.
pub fn get(params: &SearchParams, spec: &LeverSpec) -> i64 {
    match spec.name {
        "AspirationBase" => i64::from(params.aspiration_base),
        "AspirationStartDepth" => i64::from(params.aspiration_start_depth),
        "AspirationMaxCycles" => i64::from(params.aspiration_max_cycles),
        "UseStableTimeManagement" => i64::from(params.use_stable_time_management),
        "NmpBaseReduction" => i64::from(params.nmp_base_reduction),
        "NmpDepthDivisor" => i64::from(params.nmp_depth_divisor),
        "NmpEvalDivisor" => i64::from(params.nmp_eval_divisor),
        "NmpEvalBonusMax" => i64::from(params.nmp_eval_bonus_max),
        "NmpMinDepth" => i64::from(params.nmp_min_depth),
        "RfpFormulaMaxDepth" => i64::from(params.rfp_formula_max_depth),
        "RfpImprovingScale" => i64::from(params.rfp_improving_scale),
        "RfpNotImprovingScale" => i64::from(params.rfp_not_improving_scale),
        "RfpDamping" => i64::from(params.rfp_damping),
        "RazorMaxDepth" => i64::from(params.razor_max_depth),
        "RazorMarginScale" => i64::from(params.razor_margin_scale),
        "FutilityMaxDepth" => i64::from(params.futility_max_depth),
        "FutilityScale" => i64::from(params.futility_scale),
        "LmpMaxDepth" => i64::from(params.lmp_max_depth),
        "LmpScale" => i64::from(params.lmp_scale),
        "UseHistoryPruning" => i64::from(params.use_history_pruning),
        "HistPruneMaxDepth" => i64::from(params.hist_prune_max_depth),
        "HistPruneMargin" => i64::from(params.hist_prune_margin),
        "SeePruningMaxDepth" => i64::from(params.see_pruning_max_depth),
        "SeeQuietMargin" => i64::from(params.see_quiet_margin),
        "SeeNoisyMargin" => i64::from(params.see_noisy_margin),
        "SeeHistoryDivisor" => i64::from(params.see_history_divisor),
        "UseNegativeExtensions" => i64::from(params.use_negative_extensions),
        "NegExtHistoryThreshold" => i64::from(params.neg_ext_history_threshold),
        "UseInternalIterativeReduction" => i64::from(params.use_internal_iterative_reduction),
        "IirMinDepth" => i64::from(params.iir_min_depth),
        "UseTrioHindsight" => i64::from(params.use_hindsight_reductions),
        "HindsightMargin" => i64::from(params.hindsight_margin),
        "LmrDivisor" => i64::from(params.lmr_divisor),
        "LmrMinDepth" => i64::from(params.lmr_min_depth),
        "LmrMinMoves" => i64::from(params.lmr_min_moves),
        "LmrHistoryDivisor" => i64::from(params.lmr_history_divisor),
        "UseTerm1NotImproving" => i64::from(params.use_term1_not_improving),
        "UseCutNode" => i64::from(params.use_cut_node),
        "UseImprovingFlag" => i64::from(params.use_improving_flag),
        "UseReSearchReduction" => i64::from(params.use_re_search_reduction),
        "QsDeltaMargin" => i64::from(params.qs_delta_margin),
        "UseQsNarrowing" => i64::from(params.use_qs_narrowing),
        "QsRecaptureOnlyPly" => i64::from(params.qs_recapture_only_ply),
        "QsPromotionMaxPly" => i64::from(params.qs_promotion_max_ply),
        "UseMicroRiders" => i64::from(params.use_micro_riders),
        "UseCheckAwareQuiescence" => i64::from(params.use_check_aware_quiescence),
        "UseSeeApproxPrefilter" => i64::from(params.use_see_approx_prefilter),
        "UseQsTtProbe" => i64::from(params.use_qs_tt_probe),
        "UseCorrectionHistory" => i64::from(params.use_correction_history),
        "UseQsCorrectionHistory" => i64::from(params.use_qs_correction_history),
        "UseTtScoreEval" => i64::from(params.use_tt_score_eval),
        "HistoryBonusScale" => i64::from(params.history_bonus_scale),
        "HistoryMalusScale" => i64::from(params.history_malus_scale),
        "UseProbCut" => i64::from(params.use_probcut),
        "ProbCutMargin" => i64::from(params.probcut_margin),
        "ProbCutMinDepth" => i64::from(params.probcut_min_depth),
        "UseTtExtension" => i64::from(params.use_tt_extension),
        "TtExtensionMaxDepth" => i64::from(params.tt_extension_max_depth),
        "UseBadNoisyPruning" => i64::from(params.use_bad_noisy_pruning),
        "BnpDepth" => i64::from(params.bnp_depth),
        _ => spec.default,
    }
}

/// Sets a lever, clamping spin values into range.
///
/// Returns `false` for a name that is not one of the sixty levers. Values that
/// arrive through the UCI layer are already validated against the spec's
/// range; the clamp also protects the `param:` startup overrides, which are
/// only type-checked.
pub fn set(params: &mut SearchParams, name: &str, value: i64) -> bool {
    let Some(spec) = lever(name) else {
        return false;
    };
    let value = value.clamp(spec.min, spec.max) as i32;
    let flag = value != 0;
    match name {
        "AspirationBase" => params.aspiration_base = value,
        "AspirationStartDepth" => params.aspiration_start_depth = value,
        "AspirationMaxCycles" => params.aspiration_max_cycles = value,
        "UseStableTimeManagement" => params.use_stable_time_management = flag,
        "NmpBaseReduction" => params.nmp_base_reduction = value,
        "NmpDepthDivisor" => params.nmp_depth_divisor = value,
        "NmpEvalDivisor" => params.nmp_eval_divisor = value,
        "NmpEvalBonusMax" => params.nmp_eval_bonus_max = value,
        "NmpMinDepth" => params.nmp_min_depth = value,
        "RfpFormulaMaxDepth" => params.rfp_formula_max_depth = value,
        "RfpImprovingScale" => params.rfp_improving_scale = value,
        "RfpNotImprovingScale" => params.rfp_not_improving_scale = value,
        "RfpDamping" => params.rfp_damping = value,
        "RazorMaxDepth" => params.razor_max_depth = value,
        "RazorMarginScale" => params.razor_margin_scale = value,
        "FutilityMaxDepth" => params.futility_max_depth = value,
        "FutilityScale" => params.futility_scale = value,
        "LmpMaxDepth" => params.lmp_max_depth = value,
        "LmpScale" => params.lmp_scale = value,
        "UseHistoryPruning" => params.use_history_pruning = flag,
        "HistPruneMaxDepth" => params.hist_prune_max_depth = value,
        "HistPruneMargin" => params.hist_prune_margin = value,
        "SeePruningMaxDepth" => params.see_pruning_max_depth = value,
        "SeeQuietMargin" => params.see_quiet_margin = value,
        "SeeNoisyMargin" => params.see_noisy_margin = value,
        "SeeHistoryDivisor" => params.see_history_divisor = value,
        "UseNegativeExtensions" => params.use_negative_extensions = flag,
        "NegExtHistoryThreshold" => params.neg_ext_history_threshold = value,
        "UseInternalIterativeReduction" => params.use_internal_iterative_reduction = flag,
        "IirMinDepth" => params.iir_min_depth = value,
        "UseTrioHindsight" => params.use_hindsight_reductions = flag,
        "HindsightMargin" => params.hindsight_margin = value,
        "LmrDivisor" => params.lmr_divisor = value,
        "LmrMinDepth" => params.lmr_min_depth = value,
        "LmrMinMoves" => params.lmr_min_moves = value,
        "LmrHistoryDivisor" => params.lmr_history_divisor = value,
        "UseTerm1NotImproving" => params.use_term1_not_improving = flag,
        "UseCutNode" => params.use_cut_node = flag,
        "UseImprovingFlag" => params.use_improving_flag = flag,
        "UseReSearchReduction" => params.use_re_search_reduction = flag,
        "QsDeltaMargin" => params.qs_delta_margin = value,
        "UseQsNarrowing" => params.use_qs_narrowing = flag,
        "QsRecaptureOnlyPly" => params.qs_recapture_only_ply = value,
        "QsPromotionMaxPly" => params.qs_promotion_max_ply = value,
        "UseMicroRiders" => params.use_micro_riders = flag,
        "UseCheckAwareQuiescence" => params.use_check_aware_quiescence = flag,
        "UseSeeApproxPrefilter" => params.use_see_approx_prefilter = flag,
        "UseQsTtProbe" => params.use_qs_tt_probe = flag,
        "UseCorrectionHistory" => params.use_correction_history = flag,
        "UseQsCorrectionHistory" => params.use_qs_correction_history = flag,
        "UseTtScoreEval" => params.use_tt_score_eval = flag,
        "HistoryBonusScale" => params.history_bonus_scale = value,
        "HistoryMalusScale" => params.history_malus_scale = value,
        "UseProbCut" => params.use_probcut = flag,
        "ProbCutMargin" => params.probcut_margin = value,
        "ProbCutMinDepth" => params.probcut_min_depth = value,
        "UseTtExtension" => params.use_tt_extension = flag,
        "TtExtensionMaxDepth" => params.tt_extension_max_depth = value,
        "UseBadNoisyPruning" => params.use_bad_noisy_pruning = flag,
        "BnpDepth" => params.bnp_depth = value,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{LEVERS, OptionKind, PUBLIC_OPTIONS, get, lever, public, set};
    use crate::search::params::SearchParams;

    #[test]
    fn there_are_exactly_sixty_levers_and_ten_public_options() {
        assert_eq!(LEVERS.len(), 60);
        assert_eq!(PUBLIC_OPTIONS.len(), 10);
        // The two removed public options stay gone.
        assert!(public("EvalFile").is_none());
        assert!(public("Contempt").is_none());
    }

    #[test]
    fn lever_defaults_match_the_parameters() {
        let params = SearchParams::default();
        for spec in LEVERS {
            assert_eq!(
                get(&params, spec),
                spec.default,
                "lever {} default drifted from SearchParams",
                spec.name
            );
        }
    }

    #[test]
    fn every_lever_round_trips_at_both_ends_of_its_range() {
        let mut params = SearchParams::default();
        for spec in LEVERS {
            let targets = if spec.kind == OptionKind::Check {
                vec![1 - spec.default, spec.default]
            } else {
                vec![spec.min, spec.max, spec.default]
            };
            for target in targets {
                assert!(set(&mut params, spec.name, target), "{}", spec.name);
                assert_eq!(get(&params, spec), target, "{} = {target}", spec.name);
            }
        }
        assert_eq!(params, SearchParams::default());
    }

    #[test]
    fn every_lever_range_contains_its_default() {
        for spec in LEVERS {
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "{}",
                spec.name
            );
            if spec.kind == OptionKind::Check {
                assert_eq!((spec.min, spec.max), (0, 1), "{}", spec.name);
            }
        }
    }

    #[test]
    fn spin_values_are_clamped() {
        let mut params = SearchParams::default();
        assert!(set(&mut params, "AspirationBase", 10_000));
        assert_eq!(params.aspiration_base, 100);
        assert!(set(&mut params, "AspirationBase", -50));
        assert_eq!(params.aspiration_base, 10);
    }

    #[test]
    fn unknown_and_removed_names_are_rejected() {
        let mut params = SearchParams::default();
        assert!(!set(&mut params, "NoSuchLever", 1));
        // Levers of the reference that KnightByte does not offer (ADR-0014).
        for removed in [
            "UseLmrV2",
            "UseSeeReach",
            "UseQuiescenceChecks",
            "UseVerifiedNmp",
            "UseFormulaRfp",
            "UseEvalCache",
            "EvalFile",
            "Contempt",
        ] {
            assert!(lever(removed).is_none(), "{removed} must not be a lever");
            assert!(!set(&mut params, removed, 1), "{removed} must be rejected");
        }
    }

    #[test]
    fn names_are_unique_and_do_not_collide_with_public_options() {
        let mut names: Vec<&str> = PUBLIC_OPTIONS.iter().map(|option| option.name).collect();
        names.extend(LEVERS.iter().map(|spec| spec.name));
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "a name is spelled twice");
    }
}
