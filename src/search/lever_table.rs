//! The research-lever catalogue.
//!
//! The reference ships 83 hidden research levers; the ground rules expose all
//! of them under their reference names, defaults and ranges. A lever whose
//! feature is not ported yet (or ships off in the reference and the knob is
//! only kept for ablations) is marked `wired == false`: it validates and is
//! accepted like any other option, but setting it has no effect until its
//! feature lands.
//!
//! Three levers are KnightByte's own research knobs (see
//! [`KNIGHTBYTE_ONLY`]); they are not part of the reference's 95-option
//! catalogue and are reported separately by the golden-file test.
//!
//! The table itself lives in its own file to keep the option logic in
//! [`crate::search::options`] readable.

use super::options::OptionKind;

/// One research lever bound to (or reserved for) a [`crate::search::params::SearchParams`] field.
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
    /// Whether the lever is wired into the current search. An unwired lever is
    /// validated and accepted, but silently inert.
    pub wired: bool,
    /// Short description shown in the documentation.
    pub help: &'static str,
}

/// The lever names that are KnightByte's own research knobs, not part of the
/// reference catalogue.
///
/// They switch off features the reference always runs (correction history,
/// check-aware quiescence, the approximate-SEE quiescence prefilter) and are
/// kept for later ablation tests. Every catalogue check treats them as the
/// only allowed extras beyond the 83 reference levers.
pub const KNIGHTBYTE_ONLY: &[&str] = &[
    "UseCorrectionHistory",
    "UseCheckAwareQuiescence",
    "UseSeeApproxPrefilter",
];

/// Help text shared by the reserved rows: the lever is accepted and inert
/// until its feature is ported.
const RESERVED: &str = "reserved reference lever: its feature is not ported yet, setting it is accepted and has no effect";

/// The full lever catalogue: the 83 reference levers plus the three
/// KnightByte-only research knobs.
pub const LEVERS: &[LeverSpec] = &[
    // ------------------------------------------------------------------
    // Wired reference levers (36): each one drives a search behaviour that
    // is ported and node-identical with the reference.
    // ------------------------------------------------------------------
    LeverSpec {
        name: "AspirationBase",
        kind: OptionKind::Spin,
        default: 50,
        min: 10,
        max: 100,
        wired: true,
        help: "Initial aspiration window in centipawns.",
    },
    LeverSpec {
        name: "AspirationMaxCycles",
        kind: OptionKind::Spin,
        default: 8,
        min: 1,
        max: 16,
        wired: true,
        help: "Maximum aspiration re-searches per depth.",
    },
    LeverSpec {
        name: "AspirationStartDepth",
        kind: OptionKind::Spin,
        default: 5,
        min: 1,
        max: 10,
        wired: true,
        help: "First depth that uses an aspiration window.",
    },
    LeverSpec {
        name: "NmpBaseReduction",
        kind: OptionKind::Spin,
        default: 3,
        min: 1,
        max: 6,
        wired: true,
        help: "Fixed part of the null-move reduction.",
    },
    LeverSpec {
        name: "NmpDepthDivisor",
        kind: OptionKind::Spin,
        default: 4,
        min: 1,
        max: 8,
        wired: true,
        help: "Depth divisor of the null-move reduction.",
    },
    LeverSpec {
        name: "RfpMaxDepth",
        kind: OptionKind::Spin,
        default: 4,
        min: 4,
        max: 12,
        wired: true,
        help: "Reverse-futility gate for the per-depth margin table.",
    },
    LeverSpec {
        name: "RfpFormulaMaxDepth",
        kind: OptionKind::Spin,
        default: 8,
        min: 4,
        max: 12,
        wired: true,
        help: "Reverse-futility gate for the linear formula.",
    },
    LeverSpec {
        name: "RfpImprovingScale",
        kind: OptionKind::Spin,
        default: 65,
        min: 10,
        max: 200,
        wired: true,
        help: "Reverse-futility margin per ply when improving.",
    },
    LeverSpec {
        name: "RfpNotImprovingScale",
        kind: OptionKind::Spin,
        default: 95,
        min: 10,
        max: 300,
        wired: true,
        help: "Reverse-futility margin per ply when not improving.",
    },
    LeverSpec {
        name: "RazorMaxDepth",
        kind: OptionKind::Spin,
        default: 3,
        min: 2,
        max: 8,
        wired: true,
        help: "Deepest node that may use razoring.",
    },
    LeverSpec {
        name: "HistPruneMargin",
        kind: OptionKind::Spin,
        default: 2_000,
        min: 800,
        max: 4_000,
        wired: true,
        help: "History-pruning margin scale: the threshold is the margin times the depth.",
    },
    LeverSpec {
        name: "LmpMaxDepth",
        kind: OptionKind::Spin,
        default: 4,
        min: 2,
        max: 8,
        wired: true,
        help: "Deepest node that may use late move pruning.",
    },
    LeverSpec {
        name: "ClockImbalanceAheadScale",
        kind: OptionKind::Spin,
        default: 50,
        min: 0,
        max: 100,
        wired: true,
        help: "Ahead-side clock-imbalance scale, in percent of the ratio surplus.",
    },
    LeverSpec {
        name: "ClockImbalanceBehindScale",
        kind: OptionKind::Spin,
        default: 50,
        min: 0,
        max: 80,
        wired: true,
        help: "Behind-side clock-imbalance scale, in percent of the ratio deficit.",
    },
    LeverSpec {
        name: "FutilityMaxDepth",
        kind: OptionKind::Spin,
        default: 7,
        min: 4,
        max: 10,
        wired: true,
        help: "Deepest node that may use futility pruning.",
    },
    LeverSpec {
        name: "SeePruningMaxDepth",
        kind: OptionKind::Spin,
        default: 8,
        min: 2,
        max: 10,
        wired: true,
        help: "Deepest node that may prune captures by SEE.",
    },
    LeverSpec {
        name: "QsDeltaMargin",
        kind: OptionKind::Spin,
        default: 1100,
        min: 700,
        max: 1500,
        wired: true,
        help: "Delta-pruning margin in quiescence.",
    },
    LeverSpec {
        name: "MaxPathExtensions",
        kind: OptionKind::Spin,
        default: 32,
        min: 8,
        max: 64,
        wired: true,
        help: "Extension budget along one path.",
    },
    LeverSpec {
        name: "UseTerm1NotImproving",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: true,
        help: "Add a whole ply of late-move reduction when not improving.",
    },
    LeverSpec {
        name: "UseCutNode",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Reduce one ply more at predicted cut nodes and admit them to the null-move gate.",
    },
    LeverSpec {
        name: "UseImprovingFlag",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Compare the evaluation with the same line two plies ago.",
    },
    LeverSpec {
        name: "UseFormulaRfp",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Use the linear reverse-futility margin.",
    },
    LeverSpec {
        name: "UseLmrTable",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Use the log table for late move reductions.",
    },
    LeverSpec {
        name: "UseLmrHistoryPrecision",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Use multiply-before-divide fixed point for the late-move-reduction history term.",
    },
    LeverSpec {
        name: "UseNegativeExtensions",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Search suspicious moves (losing captures, historically bad quiets) one ply shallower.",
    },
    LeverSpec {
        name: "UseTtScoreEval",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Reuse a consistent transposition-table score as the evaluation.",
    },
    LeverSpec {
        name: "UseEvalCache",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: true,
        help: "Consult the static-evaluation cache.",
    },
    LeverSpec {
        name: "UseQsTtProbe",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Probe the table in quiescence.",
    },
    LeverSpec {
        name: "UseTwofoldRepetitionDraw",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Score the second occurrence of a position as a draw.",
    },
    LeverSpec {
        name: "UseRefCorrectionHistory",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Use the four-term correction tables.",
    },
    LeverSpec {
        name: "UseRefSeeThresholds",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Use the depth-scaled, history-refunded SEE-pruning thresholds.",
    },
    LeverSpec {
        name: "UseStableTimeManagement",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Scale the soft limit by the best-move, score and node-fraction stability signals.",
    },
    LeverSpec {
        name: "UseTrioHindsight",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Run the hindsight depth pair: reduced nodes earn or lose a ply by their evaluation.",
    },
    LeverSpec {
        name: "UseMicroRiders",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "Fail-soft quiescence with fail-high averaging.",
    },
    LeverSpec {
        name: "UseClockImbalance",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: true,
        help: "Scale the time allocation by the two-sided clock ratio (supersedes the time advantage).",
    },
    LeverSpec {
        name: "UseQsNarrowing",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: true,
        help: "Apply the recapture-only and promotion limits in quiescence.",
    },
    // ------------------------------------------------------------------
    // KnightByte-only research levers (3): wired, but not part of the
    // reference catalogue. Each one switches off a feature the reference
    // always runs, for later ablation tests.
    // ------------------------------------------------------------------
    LeverSpec {
        name: "UseCorrectionHistory",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "KnightByte research lever: switch off the learned static-evaluation correction.",
    },
    LeverSpec {
        name: "UseCheckAwareQuiescence",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "KnightByte research lever: switch off quiescence check evasions (the reference always searches them).",
    },
    LeverSpec {
        name: "UseSeeApproxPrefilter",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: true,
        help: "KnightByte research lever: switch off the approximate-SEE prefilter of quiescence captures.",
    },
    // ------------------------------------------------------------------
    // Reserved reference levers (47): valid catalogue entries whose feature
    // ships off in the reference or is not ported yet (ProbCut, SEE-reach,
    // LMR v2 and its terms, quiescence checks, and friends). They validate
    // and are accepted, but have no effect yet.
    // ------------------------------------------------------------------
    LeverSpec {
        name: "LmrV2NotImprovingWeight",
        kind: OptionKind::Spin,
        default: 70,
        min: 0,
        max: 600,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "QsPromotionMaxPly",
        kind: OptionKind::Spin,
        default: 2,
        min: 0,
        max: 8,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "QsRecaptureOnlyPly",
        kind: OptionKind::Spin,
        default: 8,
        min: 0,
        max: 16,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "RazorNotImprovingDelta",
        kind: OptionKind::Spin,
        default: 0,
        min: -200,
        max: 200,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "RfpImprovingDelta",
        kind: OptionKind::Spin,
        default: 0,
        min: -200,
        max: 200,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "RfpNotImprovingDelta",
        kind: OptionKind::Spin,
        default: 0,
        min: -200,
        max: 200,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "RfpOpponentWorseningDelta",
        kind: OptionKind::Spin,
        default: 0,
        min: -200,
        max: 200,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "SeeReachHistoryDivisor",
        kind: OptionKind::Spin,
        default: 133,
        min: 32,
        max: 512,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "SeeReachMaxDepth",
        kind: OptionKind::Spin,
        default: 10,
        min: 2,
        max: 12,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "SeeReachNoisyMargin",
        kind: OptionKind::Spin,
        default: -17,
        min: -200,
        max: 0,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "SeeReachQuietMargin",
        kind: OptionKind::Spin,
        default: -44,
        min: -200,
        max: 0,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "TtExtensionMaxDepth",
        kind: OptionKind::Spin,
        default: 6,
        min: 0,
        max: 12,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseAdaptiveReSearch",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseAlphaRaiseReduction",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseBadNoisyPruning",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseCapturesOnlyMovegen",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseEvalSwingBonus",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseExtensionTrio",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseFormulaLmp",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLazySee",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLinearFutilityMargin",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrNoisyMoves",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrReductionFloor",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermComplexity",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermCutNode",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermFailHighCount",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermFutile",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermHistory",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermNotImproving",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermPvDistance",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrTermTtPv",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseLmrV2",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseProbCut",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseQsCorrectionHistory",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseQuiescenceChecks",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseRefFutilityFormula",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseRefHistoryPruning",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseRefLmp",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseRefNmpEvalScale",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseSecondaryProbCut",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseSeeReach",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseSelectiveSingular",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseSkipFans",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseTrioAlphaRaise",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseTrioTtShrink",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseTtExtension",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
    LeverSpec {
        name: "UseVerifiedNmp",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        wired: false,
        help: RESERVED,
    },
];

#[cfg(test)]
mod tests {
    use super::{KNIGHTBYTE_ONLY, LEVERS, LeverSpec};
    use crate::search::options::OptionKind;

    /// The golden lever file, captured from the reference in Phase 1
    /// (83 entries, sorted by name). It lists derived values only.
    const GOLDEN_LEVERS: &str = include_str!("../../tests/golden/levers.json");

    /// Extracts the value of one flat JSON string/number/null field.
    ///
    /// The golden file is machine-generated with one flat object per lever,
    /// so a targeted scan is enough; no JSON dependency is pulled in.
    fn json_field(block: &str, key: &str) -> String {
        let needle = format!("\"{key}\":");
        let start = block
            .find(&needle)
            .unwrap_or_else(|| panic!("golden file field '{key}' missing"));
        let rest = block[start + needle.len()..].trim_start();
        if let Some(rest) = rest.strip_prefix('"') {
            let end = rest.find('"').expect("unterminated JSON string");
            rest[..end].to_owned()
        } else {
            // Numbers and `null` end at a separator — or at the block end,
            // when the field is the last one of the scanned block.
            let end = rest.find([',', '}']).unwrap_or(rest.len());
            rest[..end].trim().to_owned()
        }
    }

    /// Splits the golden array into its flat object blocks.
    fn golden_objects() -> Vec<String> {
        GOLDEN_LEVERS
            .split('{')
            .skip(1)
            .map(|block| block[..block.find('}').expect("unclosed JSON object")].to_owned())
            .collect()
    }

    /// Renders a [`LeverSpec`] the way the golden file records it.
    fn spec_default_text(spec: &LeverSpec) -> String {
        if spec.kind == OptionKind::Check {
            if spec.default != 0 {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        } else {
            spec.default.to_string()
        }
    }

    #[test]
    fn the_catalogue_matches_the_reference_lever_file() {
        // Every reference lever must be advertised with the reference's name,
        // type, default and range, so a GUI or a research script that sets a
        // recorded lever gets exactly the recorded knob.
        let objects = golden_objects();
        assert_eq!(objects.len(), 83, "the golden file should list 83 levers");
        for block in &objects {
            let name = json_field(block, "name");
            let spec = LEVERS
                .iter()
                .find(|spec| spec.name == name)
                .unwrap_or_else(|| panic!("reference lever {name} is not advertised"));
            let kind = json_field(block, "type");
            assert_eq!(
                kind,
                match spec.kind {
                    OptionKind::Spin => "spin",
                    OptionKind::Check => "check",
                    _ => panic!("lever {name} must be a spin or check option"),
                },
                "lever {name}: type drift"
            );
            assert_eq!(
                json_field(block, "default"),
                spec_default_text(spec),
                "lever {name}: default drift"
            );
            if spec.kind == OptionKind::Spin {
                assert_eq!(
                    json_field(block, "min"),
                    spec.min.to_string(),
                    "lever {name}: min drift"
                );
                assert_eq!(
                    json_field(block, "max"),
                    spec.max.to_string(),
                    "lever {name}: max drift"
                );
            }
        }
    }

    #[test]
    fn the_only_extra_levers_are_the_knightbyte_only_three() {
        // The catalogue may not grow silent extras: every advertised lever is
        // either one of the 83 reference levers or one of the three marked
        // KnightByte-only research knobs. This is the check that caught the
        // now-removed singular-verification lever.
        let objects = golden_objects();
        let reference_names: Vec<String> = objects
            .iter()
            .map(|block| json_field(block, "name"))
            .collect();
        for spec in LEVERS {
            if KNIGHTBYTE_ONLY.contains(&spec.name) {
                continue;
            }
            assert!(
                reference_names.iter().any(|name| name == spec.name),
                "lever {} is neither a reference lever nor KnightByte-only",
                spec.name
            );
        }
        assert_eq!(
            LEVERS.len(),
            83 + KNIGHTBYTE_ONLY.len(),
            "the catalogue holds 83 reference levers plus the KnightByte-only set"
        );
    }

    #[test]
    fn knightbyte_only_names_all_exist_in_the_table() {
        for name in KNIGHTBYTE_ONLY {
            assert!(
                LEVERS.iter().any(|spec| spec.name == *name),
                "KnightByte-only lever {name} is not in the table"
            );
        }
    }
}
