//! The research-lever catalogue: exactly the sixty levers that are visible.
//!
//! Every lever drives a behaviour of the search that exists and is tested; there
//! are no reserved or inert rows. The set was chosen for the post-migration
//! tuning campaign (ADR-0014): the reference's levers that have a real tuning
//! range or a measured effect, the constants of the running search that decide
//! the effective branching factor (exposed with defaults equal to the values
//! they replaced, so the node pins are unchanged), and three features ported
//! with a default-off switch (ProbCut, the table extension, bad-noisy pruning).
//!
//! Levers the reference has but KnightByte does not offer (the inert, the
//! duplicated and the clearly negative ones) are not advertised and `setoption`
//! rejects them like any unknown name. The table is the single source of truth
//! for the `option` handshake, `setoption` validation and `docs/LEVERS.md`.
//!
//! The table lives in its own file to keep the option logic in
//! [`crate::search::options`] readable.

use super::options::OptionKind;

/// One research lever bound to a [`crate::search::params::SearchParams`] field.
#[derive(Clone, Copy, Debug)]
pub struct LeverSpec {
    /// Lever name as advertised and accepted by `setoption`.
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

/// The sixty visible levers, grouped by what they control.
pub const LEVERS: &[LeverSpec] = &[
    // ------------------------------------------------------------------
    // Root search and time
    // ------------------------------------------------------------------
    LeverSpec {
        name: "AspirationBase",
        kind: OptionKind::Spin,
        default: 50,
        min: 10,
        max: 100,
        help: "Initial aspiration window in centipawns.",
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
        name: "AspirationMaxCycles",
        kind: OptionKind::Spin,
        default: 8,
        min: 1,
        max: 16,
        help: "Maximum aspiration re-searches per depth.",
    },
    LeverSpec {
        name: "UseStableTimeManagement",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Scale the soft limit by the best-move, score and node-fraction stability signals.",
    },
    // ------------------------------------------------------------------
    // Null move
    // ------------------------------------------------------------------
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
        name: "NmpEvalDivisor",
        kind: OptionKind::Spin,
        default: 191,
        min: 50,
        max: 400,
        help: "Divisor of the evaluation surplus (eval minus beta) in the null-move reduction bonus.",
    },
    LeverSpec {
        name: "NmpEvalBonusMax",
        kind: OptionKind::Spin,
        default: 4,
        min: 0,
        max: 8,
        help: "Largest evaluation bonus of the null-move reduction, in plies.",
    },
    LeverSpec {
        name: "NmpMinDepth",
        kind: OptionKind::Spin,
        default: 3,
        min: 2,
        max: 8,
        help: "Shallowest remaining depth at which the null move is tried.",
    },
    // ------------------------------------------------------------------
    // Reverse futility, razoring, futility, late move pruning
    // ------------------------------------------------------------------
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
        name: "RfpDamping",
        kind: OptionKind::Spin,
        default: 3,
        min: 1,
        max: 8,
        help: "Divisor of the overshoot that a reverse-futility cutoff returns above beta.",
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
        name: "RazorMarginScale",
        kind: OptionKind::Spin,
        default: 100,
        min: 25,
        max: 300,
        help: "Razoring margins in percent of their base values (86 and 353).",
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
        name: "FutilityScale",
        kind: OptionKind::Spin,
        default: 100,
        min: 25,
        max: 300,
        help: "Futility margins in percent of their base table.",
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
        name: "LmpScale",
        kind: OptionKind::Spin,
        default: 100,
        min: 25,
        max: 300,
        help: "Late-move-pruning quiet-move limits in percent of their base table.",
    },
    // ------------------------------------------------------------------
    // History pruning and SEE pruning
    // ------------------------------------------------------------------
    LeverSpec {
        name: "UseHistoryPruning",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Skip quiet moves whose combined history is far below the depth-scaled threshold.",
    },
    LeverSpec {
        name: "HistPruneMaxDepth",
        kind: OptionKind::Spin,
        default: 3,
        min: 1,
        max: 8,
        help: "Deepest node that may use history pruning.",
    },
    LeverSpec {
        name: "HistPruneMargin",
        kind: OptionKind::Spin,
        default: 2_000,
        min: 800,
        max: 4_000,
        help: "History-pruning margin scale: the threshold is the margin times the depth.",
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
        name: "SeeQuietMargin",
        kind: OptionKind::Spin,
        default: -43,
        min: -120,
        max: 0,
        help: "Quiet-move SEE-pruning margin per ply (negative = loss tolerance).",
    },
    LeverSpec {
        name: "SeeNoisyMargin",
        kind: OptionKind::Spin,
        default: -26,
        min: -100,
        max: 0,
        help: "Capture SEE-pruning margin per squared ply (negative = loss tolerance).",
    },
    LeverSpec {
        name: "SeeHistoryDivisor",
        kind: OptionKind::Spin,
        default: 128,
        min: 32,
        max: 512,
        help: "Divisor of the combined-history refund in the SEE-pruning threshold.",
    },
    // ------------------------------------------------------------------
    // Extensions and depth adjustments
    // ------------------------------------------------------------------
    LeverSpec {
        name: "UseNegativeExtensions",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Search suspicious moves (losing captures, historically bad quiets) one ply shallower.",
    },
    LeverSpec {
        name: "NegExtHistoryThreshold",
        kind: OptionKind::Spin,
        default: 3_000,
        min: 500,
        max: 8_000,
        help: "A quiet move whose butterfly history is below the negative of this value is searched one ply shallower.",
    },
    LeverSpec {
        name: "UseInternalIterativeReduction",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Search one ply shallower at a node that has no transposition-table move.",
    },
    LeverSpec {
        name: "IirMinDepth",
        kind: OptionKind::Spin,
        default: 4,
        min: 2,
        max: 10,
        help: "Shallowest remaining depth of internal iterative reduction.",
    },
    LeverSpec {
        name: "UseTrioHindsight",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Run the hindsight depth pair: reduced nodes earn or lose a ply by their evaluation.",
    },
    LeverSpec {
        name: "HindsightMargin",
        kind: OptionKind::Spin,
        default: 62,
        min: 0,
        max: 200,
        help: "Evaluation-sum threshold above which a reduced parent loses a ply (hindsight pair).",
    },
    // ------------------------------------------------------------------
    // Late move reductions
    // ------------------------------------------------------------------
    LeverSpec {
        name: "LmrDivisor",
        kind: OptionKind::Spin,
        default: 175,
        min: 100,
        max: 300,
        help: "Divisor of the late-move-reduction table in hundredths: 1 + ln(depth) * ln(move) / (value / 100).",
    },
    LeverSpec {
        name: "LmrMinDepth",
        kind: OptionKind::Spin,
        default: 3,
        min: 2,
        max: 8,
        help: "Shallowest remaining depth at which late moves are reduced.",
    },
    LeverSpec {
        name: "LmrMinMoves",
        kind: OptionKind::Spin,
        default: 3,
        min: 1,
        max: 8,
        help: "Moves searched at full depth before late-move reduction starts.",
    },
    LeverSpec {
        name: "LmrHistoryDivisor",
        kind: OptionKind::Spin,
        default: 3_419,
        min: 1_000,
        max: 8_000,
        help: "Divisor of the history term of late-move reduction, in 1024ths of a ply: a larger divisor lets the history reduce fewer plies.",
    },
    LeverSpec {
        name: "UseTerm1NotImproving",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "Add a whole ply of late-move reduction when not improving.",
    },
    LeverSpec {
        name: "UseCutNode",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Reduce one ply more at predicted cut nodes and admit them to the null-move gate.",
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
        name: "UseReSearchReduction",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Shave the re-search after a reduced fail-high when the reduction was large.",
    },
    // ------------------------------------------------------------------
    // Quiescence
    // ------------------------------------------------------------------
    LeverSpec {
        name: "QsDeltaMargin",
        kind: OptionKind::Spin,
        default: 1_100,
        min: 700,
        max: 1_500,
        help: "Delta-pruning margin in quiescence.",
    },
    LeverSpec {
        name: "UseQsNarrowing",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "Apply the recapture-only and promotion limits in quiescence.",
    },
    LeverSpec {
        name: "QsRecaptureOnlyPly",
        kind: OptionKind::Spin,
        default: 8,
        min: 0,
        max: 16,
        help: "Quiescence ply from which only recaptures are searched (needs UseQsNarrowing).",
    },
    LeverSpec {
        name: "QsPromotionMaxPly",
        kind: OptionKind::Spin,
        default: 2,
        min: 0,
        max: 8,
        help: "Quiescence ply from which promotions are no longer searched (needs UseQsNarrowing).",
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
        name: "UseCheckAwareQuiescence",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "KnightByte research lever: switch off quiescence check evasions (the reference always searches them).",
    },
    LeverSpec {
        name: "UseSeeApproxPrefilter",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "KnightByte research lever: switch off the approximate-SEE prefilter of quiescence captures.",
    },
    LeverSpec {
        name: "UseQsTtProbe",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "Probe the table in quiescence.",
    },
    // ------------------------------------------------------------------
    // Evaluation correction and history
    // ------------------------------------------------------------------
    LeverSpec {
        name: "UseCorrectionHistory",
        kind: OptionKind::Check,
        default: 1,
        min: 0,
        max: 1,
        help: "KnightByte research lever: switch off the learned static-evaluation correction.",
    },
    LeverSpec {
        name: "UseQsCorrectionHistory",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "Apply the learned evaluation correction to the quiescence stand-pat as well.",
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
        name: "HistoryBonusScale",
        kind: OptionKind::Spin,
        default: 100,
        min: 25,
        max: 300,
        help: "History bonuses (per-ply scale and cap) in percent of their base values.",
    },
    LeverSpec {
        name: "HistoryMalusScale",
        kind: OptionKind::Spin,
        default: 100,
        min: 25,
        max: 300,
        help: "History maluses (per-ply scale and cap) in percent of their base values.",
    },
    // ------------------------------------------------------------------
    // Features that ship off
    // ------------------------------------------------------------------
    LeverSpec {
        name: "UseProbCut",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "ProbCut: a capture that survives quiescence and a reduced search above beta plus the margin proves the cutoff early.",
    },
    LeverSpec {
        name: "ProbCutMargin",
        kind: OptionKind::Spin,
        default: 235,
        min: 100,
        max: 400,
        help: "ProbCut margin above beta, in centipawns (63 less when the position is improving).",
    },
    LeverSpec {
        name: "ProbCutMinDepth",
        kind: OptionKind::Spin,
        default: 5,
        min: 4,
        max: 10,
        help: "Shallowest remaining depth at which ProbCut is tried.",
    },
    LeverSpec {
        name: "UseTtExtension",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "Extend a node by one ply when a deep table entry lies outside the window.",
    },
    LeverSpec {
        name: "TtExtensionMaxDepth",
        kind: OptionKind::Spin,
        default: 6,
        min: 0,
        max: 12,
        help: "Deepest node (before the extension) that the table extension may extend.",
    },
    LeverSpec {
        name: "UseBadNoisyPruning",
        kind: OptionKind::Check,
        default: 0,
        min: 0,
        max: 1,
        help: "End the move loop at the first losing capture of a node that is hopeless even after a winning one.",
    },
    LeverSpec {
        name: "BnpDepth",
        kind: OptionKind::Spin,
        default: 6,
        min: 2,
        max: 10,
        help: "Bad-noisy pruning applies below this remaining depth.",
    },
];
