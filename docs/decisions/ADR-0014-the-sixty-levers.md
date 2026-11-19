# ADR-0014 - Sixty visible levers, all functional; `EvalFile` and `Contempt` removed

**Date:** 2026-11-19
**Status:** accepted (the project owner set the limit of 60 and delegated the choice)

## Context

The predecessor had about 80 levers and testing them was "the absolute limit"
for the owner; the post-migration tuning campaign for KnightByte has to fit in
the same effort. KnightByte advertised 85 levers (36 + 3 functional, 47 reserved
and inert) plus two inert public options (`EvalFile`, `Contempt`). Testing an
inert lever measures nothing, so the surface had to shrink to levers that act,
and the owner asked for the 60 most promising ones to be the only ones visible.

"Promising" was judged on three kinds of evidence:

1. **The predecessor's v9 campaign** (`audit-tests.md`): 39 arms, 400 games each,
   all formally inconclusive (error bar about +/-22 Elo). The point estimates
   still say which levers matter: stable time management (-24 off), the null-move
   depth divisor (-27 at 2), the improving flag (-10 off), micro riders (-10 off),
   and which features are flat or negative: LMR v2 (-17), the extension trio
   (-18), the eval-swing bonus (-15), the LMR floor (-12), clock imbalance (-11),
   SEE reach, ProbCut's secondary layer, verified null move, quiescence checks
   (all within noise or negative).
2. **KnightByte's own goal.** The last project step is lowering the effective
   branching factor (EBF 2.13 against 1.79 for a strong engine of its class). The
   levers that decide EBF are late-move reductions, null-move, the pruning
   margins and move ordering through the history tables, not the research
   toggles of features that already measured flat.
3. **Testability.** A lever whose feature never acts cannot be measured. The
   singular-extension levers stay hidden because quirk Q1 (singular verification
   never fires, ADR-0006) makes them inert by construction.

## Decision

1. **`EvalFile` is removed.** The network is embedded by project rule; an option
   that pretends to load another one is a trap.
2. **`Contempt` is removed.** It did nothing, and the predecessor's own test of
   it (10 and 15) was flat (+2.6 and -7.8, inside the noise).
3. **The visible surface is ten public options plus exactly sixty levers**
   (`docs/LEVERS.md`, generated from `src/search/lever_table.rs`). Every lever
   drives a search parameter. The set:
   - **26 reference levers** that have a real range or a measured effect (the
     aspiration trio, stable time management, the null-move pair, the
     reverse-futility gates and scales, razor, futility and LMP depth gates, the
     history-pruning margin, SEE pruning depth, negative extensions, hindsight,
     the improving, cut-node and "not improving" terms, the quiescence delta
     margin, narrowing, micro riders, the quiescence table probe, table-score
     evaluation, and the quiescence plies and correction switch that were
     reserved before).
   - **23 constants exposed** with defaults equal to the values they replaced
     (`NmpEvalDivisor`, `NmpEvalBonusMax`, `NmpMinDepth`, `RfpDamping`,
     `RazorMarginScale`, `FutilityScale`, `LmpScale`, `UseHistoryPruning`,
     `HistPruneMaxDepth`, the three SEE margins, `NegExtHistoryThreshold`,
     `UseInternalIterativeReduction`, `IirMinDepth`, `HindsightMargin`,
     `LmrDivisor`, `LmrMinDepth`, `LmrMinMoves`, `LmrHistoryDivisor`,
     `UseReSearchReduction`, `HistoryBonusScale`, `HistoryMalusScale`). The
     tables of margins are exposed as one percentage per table (100 = unchanged)
     rather than as one lever per entry, so a single lever moves a whole
     schedule; this is how the 60 stay testable.
   - **3 KnightByte switches** that ablate always-on features
     (`UseCorrectionHistory`, `UseCheckAwareQuiescence`,
     `UseSeeApproxPrefilter`).
   - **8 ported features that ship off**, each behind its own switch with its
     depth or margin knobs: ProbCut (`UseProbCut`, `ProbCutMargin`,
     `ProbCutMinDepth`; the one structural change aimed at EBF), the table
     extension (`UseTtExtension`, `TtExtensionMaxDepth`; +9.6 in the v9 run),
     bad-noisy pruning (`UseBadNoisyPruning`, `BnpDepth`; +4.8 over 800 games)
     and the quiescence correction (`UseQsCorrectionHistory`; +8.7). They were
     ported from the predecessor's source with defaults that leave the search
     untouched.
4. **Dropped, not hidden:** the other 52 rows of the former 86-row table.
   Among the reserved (inert) ones: LMR v2 and its eight term switches, SEE reach,
   quiescence checks, verified null move, the extension trio, the eval-swing
   bonus, skip fans, secondary ProbCut, selective singular, adaptive re-search,
   alpha-raise reduction, the lazy-SEE and captures-only generators and the
   reference formulas that duplicate the shipped ones. Among the functional ones,
   the legacy duplicates and dead ends: `UseFormulaRfp`, `RfpMaxDepth`,
   `UseRefSeeThresholds`, `UseRefCorrectionHistory`, `UseLmrTable`,
   `UseLmrHistoryPrecision`, `UseEvalCache`, `UseTwofoldRepetitionDraw`, the
   clock-imbalance trio, and `MaxPathExtensions` (decision 5). `setoption`
   rejects all of them as unknown options. Their defaults still hold in
   `SearchParams`, so the search is unchanged.
5. **`MaxPathExtensions` was swapped out by measurement.** Its first slot was
   replaced after the effect test showed that even at its minimum (8 instead of
   32) the budget changes the tree by only 0.2 % at depth 15: it is almost never
   binding. `LmrHistoryDivisor` (the history term inside late-move reduction)
   took the slot.

## Verification

- **Defaults are the pinned search.** `bench 6/12/16/18` = 15,528 / 456,308 /
  2,815,799 / 6,892,874 and every per-depth delta +0 after replacing 16 hard-coded
  constants with parameters and after adding the three ports.
- **Every lever acts.** `tests/lever_effect.rs` (release-mode gate) sets each of
  the 59 time-independent levers to a non-default value, with the companion
  setting it needs (narrowing for the quiescence plies, ProbCut for its margin),
  and requires the node count or a best move to change at depth 9, 12 or 15. The
  first run found two caps whose probe value was too lenient; they are probed at
  their minimum. `UseStableTimeManagement` only acts on a clock and is covered by
  the time-manager test.
- **The ported features are safe.** `tests/ported_features.rs`: legal best moves
  on the bench positions with all three on, mate in one found, extreme knob values
  at both ends of their ranges. A 60-game self-play match with all three on
  against the defaults (8+0.08, one thread) ended without a time forfeit, illegal
  move, disconnect or crash; the score (52.5 %, +17 +/- 64 Elo) says nothing yet.
- **Catalogue invariants** (`options.rs` tests): exactly 60 levers and 10 public
  options, every default inside its range, every lever round-trips at both ends,
  names unique, the removed names rejected. The UCI handshake test compares the
  ten public options with the predecessor's capture and requires exactly 70
  option lines.

## Consequences

- The handshake no longer matches the predecessor's capture beyond the ten public
  options; the replay tests say so explicitly instead of skipping the lines.
  `tests/golden/levers.json` and `uci.json` stay as the recorded catalogue of the
  predecessor.
- A lever is added or removed in one place (`lever_table.rs`) plus its getter and
  setter arm; `python tools/levers_doc.py` regenerates `docs/LEVERS.md`. A new
  lever needs a row in the effect test's companions if it only acts with another
  setting.
- Features that were dropped can come back by adding a lever and a port; the
  predecessor's measurements say none of them is likely to pay.
- The percentage-scale levers move a table at once. If a campaign finds that one
  entry of a table matters more than the rest, that entry becomes its own lever
  and something else goes.
