# KnightByte research levers

Generated from `src/search/lever_table.rs` by `tools/levers_doc.py`; do not
edit by hand. There are exactly **sixty** levers, all functional (decision:
`docs/decisions/ADR-0014-the-sixty-levers.md`). Each one is a UCI option:
`setoption name <Name> value <value>`. A check lever takes `true` or `false`.

`tests/lever_effect.rs` sets every lever to a non-default value and requires the
search to change (run it with `cargo test --release --test lever_effect`);
`UseStableTimeManagement` only acts on a clock and is covered by a time-manager
test. Defaults reproduce the pinned search: `bench 6/12/16/18` =
15,528 / 456,308 / 2,815,799 / 6,892,874.

**Origin:** *reference* = a lever of the predecessor's catalogue; *constant,
exposed* = a value that was hard-coded in the search, now a lever whose default
is that value; *ported, ships off* = a feature added with a switch that is off
by default; *KnightByte switch* = switches off a feature the reference always
runs.

**Prior evidence** is the predecessor's v9 campaign: 400 games per arm (10+0.1,
one thread, UHO), error bar about +/-22 Elo, every arm formally inconclusive.
It is a hint about direction, not a result, and it was measured on a different
engine and net.


## Root search and time

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 1 | `AspirationBase` | spin | 50 | 10 to 100 | reference | =33: +6.1 | Initial aspiration window in centipawns. |
| 2 | `AspirationStartDepth` | spin | 5 | 1 to 10 | reference |  | First depth that uses an aspiration window. |
| 3 | `AspirationMaxCycles` | spin | 8 | 1 to 16 | reference |  | Maximum aspiration re-searches per depth. |
| 4 | `UseStableTimeManagement` | check | true | true / false | reference | off: -24.4 | Scale the soft limit by the best-move, score and node-fraction stability signals. |

## Null move

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 5 | `NmpBaseReduction` | spin | 3 | 1 to 6 | reference |  | Fixed part of the null-move reduction. |
| 6 | `NmpDepthDivisor` | spin | 4 | 1 to 8 | reference | =2: -27.0 | Depth divisor of the null-move reduction. |
| 7 | `NmpEvalDivisor` | spin | 191 | 50 to 400 | constant, exposed |  | Divisor of the evaluation surplus (eval minus beta) in the null-move reduction bonus. |
| 8 | `NmpEvalBonusMax` | spin | 4 | 0 to 8 | constant, exposed |  | Largest evaluation bonus of the null-move reduction, in plies. |
| 9 | `NmpMinDepth` | spin | 3 | 2 to 8 | constant, exposed |  | Shallowest remaining depth at which the null move is tried. |

## Reverse futility, razoring, futility, late move pruning

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 10 | `RfpFormulaMaxDepth` | spin | 8 | 4 to 12 | reference |  | Reverse-futility gate for the linear formula. |
| 11 | `RfpImprovingScale` | spin | 65 | 10 to 200 | reference |  | Reverse-futility margin per ply when improving. |
| 12 | `RfpNotImprovingScale` | spin | 95 | 10 to 300 | reference |  | Reverse-futility margin per ply when not improving. |
| 13 | `RfpDamping` | spin | 3 | 1 to 8 | constant, exposed |  | Divisor of the overshoot that a reverse-futility cutoff returns above beta. |
| 14 | `RazorMaxDepth` | spin | 3 | 2 to 8 | reference |  | Deepest node that may use razoring. |
| 15 | `RazorMarginScale` | spin | 100 | 25 to 300 | constant, exposed |  | Razoring margins in percent of their base values (86 and 353). |
| 16 | `FutilityMaxDepth` | spin | 7 | 4 to 10 | reference |  | Deepest node that may use futility pruning. |
| 17 | `FutilityScale` | spin | 100 | 25 to 300 | constant, exposed |  | Futility margins in percent of their base table. |
| 18 | `LmpMaxDepth` | spin | 4 | 2 to 8 | reference |  | Deepest node that may use late move pruning. |
| 19 | `LmpScale` | spin | 100 | 25 to 300 | constant, exposed |  | Late-move-pruning quiet-move limits in percent of their base table. |

## History pruning and SEE pruning

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 20 | `UseHistoryPruning` | check | true | true / false | constant, exposed |  | Skip quiet moves whose combined history is far below the depth-scaled threshold. |
| 21 | `HistPruneMaxDepth` | spin | 3 | 1 to 8 | constant, exposed |  | Deepest node that may use history pruning. |
| 22 | `HistPruneMargin` | spin | 2000 | 800 to 4000 | reference |  | History-pruning margin scale: the threshold is the margin times the depth. |
| 23 | `SeePruningMaxDepth` | spin | 8 | 2 to 10 | reference | SEE thresholds + depth 8 off: -8.7 | Deepest node that may prune captures by SEE. |
| 24 | `SeeQuietMargin` | spin | -43 | -120 to 0 | constant, exposed |  | Quiet-move SEE-pruning margin per ply (negative = loss tolerance). |
| 25 | `SeeNoisyMargin` | spin | -26 | -100 to 0 | constant, exposed |  | Capture SEE-pruning margin per squared ply (negative = loss tolerance). |
| 26 | `SeeHistoryDivisor` | spin | 128 | 32 to 512 | constant, exposed |  | Divisor of the combined-history refund in the SEE-pruning threshold. |

## Extensions and depth adjustments

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 27 | `UseNegativeExtensions` | check | true | true / false | reference | off: 0.0 | Search suspicious moves (losing captures, historically bad quiets) one ply shallower. |
| 28 | `NegExtHistoryThreshold` | spin | 3000 | 500 to 8000 | constant, exposed |  | A quiet move whose butterfly history is below the negative of this value is searched one ply shallower. |
| 29 | `UseInternalIterativeReduction` | check | true | true / false | constant, exposed |  | Search one ply shallower at a node that has no transposition-table move. |
| 30 | `IirMinDepth` | spin | 4 | 2 to 10 | constant, exposed |  | Shallowest remaining depth of internal iterative reduction. |
| 31 | `UseTrioHindsight` | check | true | true / false | reference |  | Run the hindsight depth pair: reduced nodes earn or lose a ply by their evaluation. |
| 32 | `HindsightMargin` | spin | 62 | 0 to 200 | constant, exposed |  | Evaluation-sum threshold above which a reduced parent loses a ply (hindsight pair). |

## Late move reductions

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 33 | `LmrDivisor` | spin | 175 | 100 to 300 | constant, exposed |  | Divisor of the late-move-reduction table in hundredths: 1 + ln(depth) * ln(move) / (value / 100). |
| 34 | `LmrMinDepth` | spin | 3 | 2 to 8 | constant, exposed |  | Shallowest remaining depth at which late moves are reduced. |
| 35 | `LmrMinMoves` | spin | 3 | 1 to 8 | constant, exposed |  | Moves searched at full depth before late-move reduction starts. |
| 36 | `LmrHistoryDivisor` | spin | 3419 | 1000 to 8000 | constant, exposed |  | Divisor of the history term of late-move reduction, in 1024ths of a ply: a larger divisor lets the history reduce fewer plies. |
| 37 | `UseTerm1NotImproving` | check | false | true / false | reference |  | Add a whole ply of late-move reduction when not improving. |
| 38 | `UseCutNode` | check | true | true / false | reference | off: +2.6 | Reduce one ply more at predicted cut nodes and admit them to the null-move gate. |
| 39 | `UseImprovingFlag` | check | true | true / false | reference | off: -10.4 | Compare the evaluation with the same line two plies ago. |
| 40 | `UseReSearchReduction` | check | true | true / false | constant, exposed |  | Shave the re-search after a reduced fail-high when the reduction was large. |

## Quiescence

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 41 | `QsDeltaMargin` | spin | 1100 | 700 to 1500 | reference |  | Delta-pruning margin in quiescence. |
| 42 | `UseQsNarrowing` | check | false | true / false | reference | on: +5.2 | Apply the recapture-only and promotion limits in quiescence. |
| 43 | `QsRecaptureOnlyPly` | spin | 8 | 0 to 16 | reference |  | Quiescence ply from which only recaptures are searched (needs UseQsNarrowing). |
| 44 | `QsPromotionMaxPly` | spin | 2 | 0 to 8 | reference |  | Quiescence ply from which promotions are no longer searched (needs UseQsNarrowing). |
| 45 | `UseMicroRiders` | check | true | true / false | reference | off: -9.6 | Fail-soft quiescence with fail-high averaging. |
| 46 | `UseCheckAwareQuiescence` | check | true | true / false | KnightByte switch |  | KnightByte research lever: switch off quiescence check evasions (the reference always searches them). |
| 47 | `UseSeeApproxPrefilter` | check | true | true / false | KnightByte switch |  | KnightByte research lever: switch off the approximate-SEE prefilter of quiescence captures. |
| 48 | `UseQsTtProbe` | check | true | true / false | reference | off: -0.9 | Probe the table in quiescence. |

## Evaluation correction and history

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 49 | `UseCorrectionHistory` | check | true | true / false | KnightByte switch |  | KnightByte research lever: switch off the learned static-evaluation correction. |
| 50 | `UseQsCorrectionHistory` | check | false | true / false | ported, ships off | on: +8.7 | Apply the learned evaluation correction to the quiescence stand-pat as well. |
| 51 | `UseTtScoreEval` | check | true | true / false | reference | off: -8.7 | Reuse a consistent transposition-table score as the evaluation. |
| 52 | `HistoryBonusScale` | spin | 100 | 25 to 300 | constant, exposed |  | History bonuses (per-ply scale and cap) in percent of their base values. |
| 53 | `HistoryMalusScale` | spin | 100 | 25 to 300 | constant, exposed |  | History maluses (per-ply scale and cap) in percent of their base values. |

## Features that ship off

| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |
| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
| 54 | `UseProbCut` | check | false | true / false | ported, ships off | on, with its secondary layer: -4.3 | ProbCut: a capture that survives quiescence and a reduced search above beta plus the margin proves the cutoff early. |
| 55 | `ProbCutMargin` | spin | 235 | 100 to 400 | ported, ships off |  | ProbCut margin above beta, in centipawns (63 less when the position is improving). |
| 56 | `ProbCutMinDepth` | spin | 5 | 4 to 10 | ported, ships off |  | Shallowest remaining depth at which ProbCut is tried. |
| 57 | `UseTtExtension` | check | false | true / false | ported, ships off | on: +9.6 | Extend a node by one ply when a deep table entry lies outside the window. |
| 58 | `TtExtensionMaxDepth` | spin | 6 | 0 to 12 | ported, ships off |  | Deepest node (before the extension) that the table extension may extend. |
| 59 | `UseBadNoisyPruning` | check | false | true / false | ported, ships off | on: +4.8 (800 games) | End the move loop at the first losing capture of a node that is hopeless even after a winning one. |
| 60 | `BnpDepth` | spin | 6 | 2 to 10 | ported, ships off |  | Bad-noisy pruning applies below this remaining depth. |
