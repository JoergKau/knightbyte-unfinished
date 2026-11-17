//! UCI option catalogue.
//!
//! One declarative table drives the `option` handshake, `setoption` dispatch and
//! the documentation. Two groups exist:
//!
//! * **public options** — the twelve names a GUI expects (`Hash`, `Threads`,
//!   `Move Overhead`, `Clear Hash`, book/tablebase placeholders, `EvalFile`,
//!   `Contempt`). The ones that are functional today are marked in the help
//!   text; the rest are accepted and ignored until their phase lands.
//! * **research levers** — the tunable search parameters that are actually
//!   wired into the search. Every lever listed here changes behaviour when set;
//!   levers whose feature is not ported yet are not advertised (a knob that does
//!   nothing is worse than a missing knob).
//!
//! Adding a lever means one row here plus a getter/setter arm; the handshake and
//! the parser pick it up automatically.

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

/// A research lever bound to a [`SearchParams`] field.
#[derive(Clone, Copy, Debug)]
pub struct LeverSpec {
    /// Lever name, identical to the recorded catalogue.
    pub name: &'static str,
    /// Value type (`Spin` or `Check`).
    pub kind: OptionKind,
    /// Default value.
    pub default: i64,
    /// Minimum for spin levers.
    pub min: i64,
    /// Maximum for spin levers.
    pub max: i64,
    /// Short description shown in the documentation.
    pub help: &'static str,
}

/// Research levers that are wired into the current search.
pub const LEVERS: &[LeverSpec] = &[
    LeverSpec {
        name: "AspirationBase",
        kind: OptionKind::Spin,
        default: 50,
        min: 10,
        max: 100,
        help: "Initial aspiration window in centipawns.",
    },
    LeverSpec {
        name: "AspirationMaxCycles",
        kind: OptionKind::Spin,
        default: 8,
        min: 1,
        max: 16,
        help: "Maximum aspiration re-searches per depth.",
    },
    LeverSpec {
        name: "AspirationStartDepth",
        kind: OptionKind::Spin,
        default: 5,
        min: 1,
        max: 10,
        help: "First depth that uses an aspiration window.",
    },
    LeverSpec {
        name: "NmpBaseReduction",
        kind: OptionKind::Spin,
        default: 3,
        min: 1,
        max: 6,
        help: "Fixed part of the null-move reduction.",
    },
    LeverSpec {
        name: "NmpDepthDivisor",
        kind: OptionKind::Spin,
        default: 4,
        min: 1,
        max: 8,
        help: "Depth divisor of the null-move reduction.",
    },
    LeverSpec {
        name: "RfpMaxDepth",
        kind: OptionKind::Spin,
        default: 4,
        min: 4,
        max: 12,
        help: "Reverse-futility gate for the per-depth margin table.",
    },
    LeverSpec {
        name: "RfpFormulaMaxDepth",
        kind: OptionKind::Spin,
        default: 8,
        min: 4,
        max: 12,
        help: "Reverse-futility gate for the linear formula.",
    },
    LeverSpec {
        name: "RfpImprovingScale",
        kind: OptionKind::Spin,
        default: 65,
        min: 10,
        max: 200,
        help: "Reverse-futility margin per ply when improving.",
    },
    LeverSpec {
        name: "RfpNotImprovingScale",
        kind: OptionKind::Spin,
        default: 95,
        min: 10,
        max: 300,
        help: "Reverse-futility margin per ply when not improving.",
    },
    LeverSpec {
        name: "RazorMaxDepth",
        kind: OptionKind::Spin,
        default: 3,
        min: 2,
        max: 8,
        help: "Deepest node that may use razoring.",
    },
    LeverSpec {
        name: "LmpMaxDepth",
        kind: OptionKind::Spin,
        default: 4,
        min: 2,
        max: 8,
        help: "Deepest node that may use late move pruning.",
    },
    LeverSpec {
        name: "FutilityMaxDepth",
        kind: OptionKind::Spin,
        default: 7,
        min: 4,
        max: 10,
        help: "Deepest node that may use futility pruning.",
    },
    LeverSpec {
        name: "SeePruningMaxDepth",
        kind: OptionKind::Spin,
        default: 8,
        min: 2,
        max: 10,
        help: "Deepest node that may prune captures by SEE.",
    },
    LeverSpec {
        name: "QsDeltaMargin",
        kind: OptionKind::Spin,
        default: 1100,
        min: 700,
        max: 1500,
        help: "Delta-pruning margin in quiescence.",
    },
    LeverSpec {
        name: "MaxPathExtensions",
        kind: OptionKind::Spin,
        default: 32,
        min: 8,
        max: 64,
        help: "Extension budget along one path.",
    },
    LeverSpec {
        name: "LmrV2NotImprovingWeight",
        kind: OptionKind::Spin,
        default: 70,
        min: 0,
        max: 600,
        help: "Extra late-move reduction when not improving.",
    },
    LeverSpec {
        name: "UseImprovingFlag",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Compare the evaluation with the same line two plies ago.",
    },
    LeverSpec {
        name: "UseFormulaRfp",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Use the linear reverse-futility margin.",
    },
    LeverSpec {
        name: "UseLmrTable",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Use the log table for late move reductions.",
    },
    LeverSpec {
        name: "UseTtScoreEval",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Reuse a consistent transposition-table score as the evaluation.",
    },
    LeverSpec {
        name: "UseEvalCache",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "Consult the static-evaluation cache.",
    },
    LeverSpec {
        name: "UseQsTtProbe",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Probe the table in quiescence.",
    },
    LeverSpec {
        name: "UseTwofoldRepetitionDraw",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Score the second occurrence of a position as a draw.",
    },
    LeverSpec {
        name: "UseCorrectionHistory",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Apply the learned static-evaluation correction.",
    },
    LeverSpec {
        name: "UseRefCorrectionHistory",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Use the four-term correction tables.",
    },
    LeverSpec {
        name: "UseMicroRiders",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Fail-soft quiescence with fail-high averaging.",
    },
    LeverSpec {
        name: "UseSeeApproxPrefilter",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Prefilter quiescence captures by the approximate exchange.",
    },
    LeverSpec {
        name: "UseCheckAwareQuiescence",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Search all evasions when the king is attacked.",
    },
    LeverSpec {
        name: "UseQsNarrowing",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "Apply the recapture-only and promotion limits in quiescence.",
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
        "LmrV2NotImprovingWeight" => i64::from(params.lmr_v2_not_improving_weight),
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
        _ => 0,
    }
}

/// Sets a lever, clamping spin values into range.
///
/// Returns `false` for an unknown lever name.
pub fn set(params: &mut SearchParams, name: &str, value: i64) -> bool {
    let Some(spec) = lever(name) else {
        return false;
    };
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
        "LmrV2NotImprovingWeight" => params.lmr_v2_not_improving_weight = value,
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
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{LEVERS, OptionKind, PUBLIC_OPTIONS, get, lever, public, set};
    use crate::search::params::SearchParams;

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
    fn every_lever_round_trips() {
        let mut params = SearchParams::default();
        for spec in LEVERS {
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
