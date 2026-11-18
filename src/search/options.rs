//! UCI option catalogue.
//!
//! One declarative table drives the `option` handshake, `setoption` dispatch and
//! the documentation. Three groups exist:
//!
//! * **public options** — the twelve names a GUI expects (`Hash`, `Threads`,
//!   `Move Overhead`, `Clear Hash`, book/tablebase placeholders, `EvalFile`,
//!   `Contempt`). The ones that are functional today are marked in the help
//!   text; the rest are accepted and ignored until their phase lands.
//! * **reference levers** — all 83 hidden research levers of the reference,
//!   exposed under their reference names, defaults and ranges. The ones whose
//!   feature is not ported yet are marked `wired == false` in
//!   [`crate::search::lever_table`]: they validate and are accepted, but have
//!   no effect until their feature lands (a knob that silently lies about its
//!   range is worse than an inert knob with the right range).
//! * **KnightByte-only levers** — three research knobs that switch off
//!   features the reference always runs; see
//!   [`crate::search::lever_table::KNIGHTBYTE_ONLY`].
//!
//! Adding a lever means one row in [`crate::search::lever_table`] plus a
//! getter/setter arm here; the handshake and the parser pick it up
//! automatically.

use crate::search::lever_table::{LEVERS, LeverSpec};
use crate::search::params::SearchParams;

pub use crate::search::lever_table::KNIGHTBYTE_ONLY;

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
/// `Hash`, `Clear Hash`, `Move Overhead` and `Threads` are functional; the book,
/// tablebase and evaluation-file names are accepted so existing GUIs and
/// configuration scripts keep working, and their help text says they are inert.
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
    UciOption {
        name: "EvalFile",
        kind: OptionKind::String,
        default: "",
        min: None,
        max: None,
    },
    UciOption {
        name: "Contempt",
        kind: OptionKind::Spin,
        default: "0",
        min: Some(-50),
        max: Some(50),
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
///
/// An unwired lever has no storage yet, so its recorded default is reported.
pub fn get(params: &SearchParams, spec: &LeverSpec) -> i64 {
    if !spec.wired {
        return spec.default;
    }
    match spec.name {
        "AspirationBase" => i64::from(params.aspiration_base),
        "AspirationMaxCycles" => i64::from(params.aspiration_max_cycles),
        "AspirationStartDepth" => i64::from(params.aspiration_start_depth),
        "NmpBaseReduction" => i64::from(params.nmp_base_reduction),
        "NmpDepthDivisor" => i64::from(params.nmp_depth_divisor),
        "RfpMaxDepth" => i64::from(params.rfp_max_depth),
        "RfpFormulaMaxDepth" => i64::from(params.rfp_formula_max_depth),
        "RfpImprovingScale" => i64::from(params.rfp_improving_scale),
        "RfpNotImprovingScale" => i64::from(params.rfp_not_improving_scale),
        "RazorMaxDepth" => i64::from(params.razor_max_depth),
        "LmpMaxDepth" => i64::from(params.lmp_max_depth),
        "FutilityMaxDepth" => i64::from(params.futility_max_depth),
        "SeePruningMaxDepth" => i64::from(params.see_pruning_max_depth),
        "QsDeltaMargin" => i64::from(params.qs_delta_margin),
        "MaxPathExtensions" => i64::from(params.max_path_extensions),
        "UseTerm1NotImproving" => i64::from(params.use_term1_not_improving),
        "UseCutNode" => i64::from(params.use_cut_node),
        "UseImprovingFlag" => i64::from(params.use_improving_flag),
        "UseFormulaRfp" => i64::from(params.use_formula_rfp),
        "UseLmrTable" => i64::from(params.use_lmr_table),
        "UseTtScoreEval" => i64::from(params.use_tt_score_eval),
        "UseEvalCache" => i64::from(params.use_eval_cache),
        "UseQsTtProbe" => i64::from(params.use_qs_tt_probe),
        "UseTwofoldRepetitionDraw" => i64::from(params.use_twofold_repetition_draw),
        "UseCorrectionHistory" => i64::from(params.use_correction_history),
        "UseRefCorrectionHistory" => i64::from(params.use_ref_correction_history),
        "UseMicroRiders" => i64::from(params.use_micro_riders),
        "UseSeeApproxPrefilter" => i64::from(params.use_see_approx_prefilter),
        "UseCheckAwareQuiescence" => i64::from(params.use_check_aware_quiescence),
        "UseQsNarrowing" => i64::from(params.use_qs_narrowing),
        "ClockImbalanceAheadScale" => i64::from(params.clock_imbalance_ahead_scale),
        "ClockImbalanceBehindScale" => i64::from(params.clock_imbalance_behind_scale),
        "HistPruneMargin" => i64::from(params.hist_prune_margin),
        "UseLmrHistoryPrecision" => i64::from(params.use_lmr_history_precision),
        "UseNegativeExtensions" => i64::from(params.use_negative_extensions),
        "UseRefSeeThresholds" => i64::from(params.use_ref_see_thresholds),
        "UseStableTimeManagement" => i64::from(params.use_stable_time_management),
        "UseTrioHindsight" => i64::from(params.use_hindsight_reductions),
        "UseClockImbalance" => i64::from(params.use_clock_imbalance),
        _ => spec.default,
    }
}

/// Sets a lever, clamping spin values into range.
///
/// Returns `false` for an unknown lever name. An unwired lever is accepted
/// (returning `true`) but stores nothing: its feature is not ported yet.
/// Values that arrive through the UCI layer are already validated against the
/// spec's range; the clamp also protects the `param:` startup overrides,
/// which are only type-checked.
pub fn set(params: &mut SearchParams, name: &str, value: i64) -> bool {
    let Some(spec) = lever(name) else {
        return false;
    };
    if !spec.wired {
        return true;
    }
    let value = value.clamp(spec.min, spec.max) as i32;
    let flag = value != 0;
    match name {
        "AspirationBase" => params.aspiration_base = value,
        "AspirationMaxCycles" => params.aspiration_max_cycles = value,
        "AspirationStartDepth" => params.aspiration_start_depth = value,
        "NmpBaseReduction" => params.nmp_base_reduction = value,
        "NmpDepthDivisor" => params.nmp_depth_divisor = value,
        "RfpMaxDepth" => params.rfp_max_depth = value,
        "RfpFormulaMaxDepth" => params.rfp_formula_max_depth = value,
        "RfpImprovingScale" => params.rfp_improving_scale = value,
        "RfpNotImprovingScale" => params.rfp_not_improving_scale = value,
        "RazorMaxDepth" => params.razor_max_depth = value,
        "LmpMaxDepth" => params.lmp_max_depth = value,
        "FutilityMaxDepth" => params.futility_max_depth = value,
        "SeePruningMaxDepth" => params.see_pruning_max_depth = value,
        "QsDeltaMargin" => params.qs_delta_margin = value,
        "MaxPathExtensions" => params.max_path_extensions = value,
        "UseTerm1NotImproving" => params.use_term1_not_improving = flag,
        "UseCutNode" => params.use_cut_node = flag,
        "UseImprovingFlag" => params.use_improving_flag = flag,
        "UseFormulaRfp" => params.use_formula_rfp = flag,
        "UseLmrTable" => params.use_lmr_table = flag,
        "UseTtScoreEval" => params.use_tt_score_eval = flag,
        "UseEvalCache" => params.use_eval_cache = flag,
        "UseQsTtProbe" => params.use_qs_tt_probe = flag,
        "UseTwofoldRepetitionDraw" => params.use_twofold_repetition_draw = flag,
        "UseCorrectionHistory" => params.use_correction_history = flag,
        "UseRefCorrectionHistory" => params.use_ref_correction_history = flag,
        "UseMicroRiders" => params.use_micro_riders = flag,
        "UseSeeApproxPrefilter" => params.use_see_approx_prefilter = flag,
        "UseCheckAwareQuiescence" => params.use_check_aware_quiescence = flag,
        "UseQsNarrowing" => params.use_qs_narrowing = flag,
        "ClockImbalanceAheadScale" => params.clock_imbalance_ahead_scale = value,
        "ClockImbalanceBehindScale" => params.clock_imbalance_behind_scale = value,
        "HistPruneMargin" => params.hist_prune_margin = value,
        "UseLmrHistoryPrecision" => params.use_lmr_history_precision = flag,
        "UseNegativeExtensions" => params.use_negative_extensions = flag,
        "UseRefSeeThresholds" => params.use_ref_see_thresholds = flag,
        "UseStableTimeManagement" => params.use_stable_time_management = flag,
        "UseTrioHindsight" => params.use_hindsight_reductions = flag,
        "UseClockImbalance" => params.use_clock_imbalance = flag,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{LEVERS, OptionKind, PUBLIC_OPTIONS, get, lever, public, set};
    use crate::search::params::SearchParams;

    #[test]
    fn wired_lever_defaults_match_the_parameters() {
        let params = SearchParams::default();
        for spec in LEVERS.iter().filter(|spec| spec.wired) {
            assert_eq!(
                get(&params, spec),
                spec.default,
                "lever {} default drifted from SearchParams",
                spec.name
            );
        }
    }

    #[test]
    fn every_wired_lever_round_trips() {
        let mut params = SearchParams::default();
        for spec in LEVERS.iter().filter(|spec| spec.wired) {
            let target = if spec.kind == OptionKind::Check {
                1 - spec.default
            } else {
                spec.min
            };
            assert!(set(&mut params, spec.name, target), "{}", spec.name);
            assert_eq!(get(&params, spec), target, "{}", spec.name);
        }
    }

    #[test]
    fn unwired_levers_are_accepted_but_inert() {
        // A reserved lever validates like any other option and is accepted,
        // but nothing is stored: its feature is not ported yet.
        let mut params = SearchParams::default();
        for spec in LEVERS.iter().filter(|spec| !spec.wired) {
            assert!(set(&mut params, spec.name, spec.max), "{}", spec.name);
            assert_eq!(
                get(&params, spec),
                spec.default,
                "unwired lever {} changed state",
                spec.name
            );
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
    fn unknown_names_are_rejected() {
        let mut params = SearchParams::default();
        assert!(!set(&mut params, "NoSuchLever", 1));
        assert!(lever("NoSuchLever").is_none());
        assert!(public("NoSuchOption").is_none());
        assert!(lever("AspirationBase").is_some());
        assert!(public("Hash").is_some());
    }

    #[test]
    fn names_are_unique_and_spelled_once() {
        let mut names: Vec<&str> = PUBLIC_OPTIONS.iter().map(|spec| spec.name).collect();
        names.extend(LEVERS.iter().map(|spec| spec.name));
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate option name");
    }
}
