# Search parity map

This file tracks the port of the predecessor search, feature by feature, and
records where KnightByte currently diverges. It is the working document for the
Phase 4 node-count gate.

**Status:** oracle captured and pinned. Ported: Zobrist tables, quiescence,
SEE, node-level extensions and correction history. Depth-1 positions 1–3 are
exact; the remaining divergence is formula-level pruning and reductions.
Phase 4c remains open.

## 1. Oracle

The reference implementation is a local checkout of the predecessor project with
a published Windows build. The parity pass uses:

| Item | Value |
| :--- | :--- |
| Reference commit | `79b255a` (2026-11-14) |
| Build | published `win-x64` self-contained binary |
| Command | `bench <depth>` (four fixed positions, TT cleared per position) |
| Hash | 256 MB (default) |
| Threads | 1 |
| Search mode | infinite (no clock), fixed depth |

Captured data lives in `tests/golden/search_parity.json`: per-position node
counts for depths 1–12, 14, 16 and 18, plus totals. The recorded identity totals
(`416,533` / `3,169,859` / `6,925,762`) reproduced exactly, so the capture is
trustworthy.

The capturing tool is `tools/capture_bench.py`; it accepts any UCI engine path
at run time and never writes that path into the baseline file:

```
python tools/capture_bench.py capture --engine <engine> --label <label> \
    --depths 1-12,14,16,18 --out tests/golden/search_parity.json
python tools/capture_bench.py compare --engine <engine> \
    --baseline tests/golden/search_parity.json --depths 1-12
```

Node counts are parsed by stripping locale separators, because the reference
prints numbers with the host's digit grouping.

## 2. Divergence status (2026-11-17, after the pruning-formula port)

Per-position node counts, reference → KnightByte:

| Depth | Pos 1 | Pos 2 | Pos 3 | Pos 4 | Total ref | Total ours |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 47 → 47 | 110 → 110 | 33 → 33 | 371 → 364 | 561 | 554 |
| 2 | 213 → 289 | 331 → 411 | 140 → 184 | 893 → 909 | 1,577 | 1,793 |
| 3 | 439 → 675 | 551 → 612 | 273 → 321 | 1,251 → 1,206 | 2,514 | 2,814 |
| 4 | 1,204 → 1,969 | 965 → 1,100 | 1,068 → 1,236 | 1,757 → 1,653 | 4,994 | 5,958 |
| 5 | 3,947 → 3,495 | 1,660 → 1,652 | 1,299 → 1,561 | 3,025 → 2,730 | 9,931 | 9,438 |
| 6 | 5,728 → 4,984 | 2,706 → 2,585 | 2,011 → 2,306 | 4,534 → 4,802 | 14,979 | 14,677 |
| 7 | 10,815 → 11,072 | 4,277 → 5,980 | 2,855 → 3,386 | 6,117 → 7,003 | 24,064 | 27,441 |
| 8 | 27,811 → 19,460 | 7,774 → 10,385 | 4,737 → 4,651 | 7,881 → 9,793 | 48,203 | 44,289 |
| 9 | 41,511 → 33,223 | 16,854 → 23,596 | 8,233 → 9,199 | 11,528 → 16,695 | 78,126 | 82,713 |
| 10 | 95,947 → 57,814 | 27,260 → 38,615 | 11,643 → 17,737 | 20,885 → 26,948 | 155,735 | 141,114 |
| 11 | 151,132 → 116,827 | 39,915 → 62,168 | 16,101 → 27,689 | 32,569 → 44,984 | 239,717 | 251,668 |
| 12 | 276,529 → 211,162 | 61,017 → 93,612 | 28,547 → 42,655 | 50,440 → 64,685 | 416,533 | 412,114 |
| 14 | 828,843 → 923,062 | 195,860 → 206,338 | 56,030 → 79,050 | 141,069 → 239,422 | 1,221,802 | 1,447,872 |
| 16 | — | — | — | — | 3,169,859 | 3,584,452 |
| 18 | — | — | — | — | 6,925,762 | 8,506,398 |

What the port already fixed:

* **Node accounting** is identical (depth 1, positions 1–3 match exactly).
* **Keys are identical to the reference.** The Zobrist tables now use the
  reference generator (`xorshift64`, seed `0x243F6A8885A308D3`, draw order
  pieces → en-passant → castling → side). The start position hashes to
  `0x48E86881DC3DFD16` in both engines, which is what makes the
  transposition-table and correction-table indexing comparable at all.
* **Quiescence semantics** match: captures-only generation with no quiet
  promotions, stand-pat fail-soft return, node-level delta pruning, the `-50`
  SEE bound, the approximate-SEE prefilter, in-check evasions with evasion
  narrowing, depth-0-only TT cutoffs and fail-high averaging.
* **SEE** matches: values `{109, 403, 435, 679, 1242, 20000}`, exact swap-list
  for ordering, boolean `-1`-tie-break form for pruning, pin exclusion and the
  king rule.
* **Extensions are node-level** (check extension and single-response extension),
  which fixed position 4's depth-1 score (`-508`, PV `g1h1 a3b4` — both exact).
* **Correction history** is ported: four terms (pawn 99 %, white non-pawn 100 %,
  black non-pawn 100 %, countermove 96 %), divided by 64, ±2048 cell clamp,
  depth-weighted update `newWeight = min(depth+1, 16)`, and the reference's
  update gates. Only the corrected value drives pruning; the TT keeps raw evals.
* **Pruning formulas** are the reference's:
  * reverse futility: `margin = depth * (improving ? 65 : 95)`, gate 8, damped
    spill `beta + (eval - beta)/3`;
  * razoring: per-depth margins `{_, 86, 353, 353}`, `eval + margin <= alpha`
    confirmed by quiescence and returning `alpha`;
  * null move: `R = 3 + depth/4 + min((eval - beta)/191, 4)` clamped to
    `depth - 1`, with the stored-score gate and the pawn-and-king-only guard;
  * late move pruning: per-depth table `{_, 9, 12, 17, 24}` halved when not
    improving; it stops the whole quiet stage;
  * futility: per-depth margins `{_, 61, 144, 268, 334}`, strict `< alpha`, pawn
    moves exempt, quiet stage stopped;
  * SEE pruning of captures: threshold `-26 · depth^2`.
* **History tables** use the reference scheme: gravity `h += u - h*|u|/MAX` with
  butterfly ±8,033 (bonus 207/ply up to 1,202; malus 200/ply up to 1,255),
  continuation ±8,003 (204/1,229 and 205/1,067) and capture ±8,488
  (192/1,250). Quiets that failed low earn the malus, the cutoff move earns the
  bonus, killers keep four slots per ply, and history ages by 7/8 after each
  completed iteration from depth 4.
* Depth 12 is **within 1.1 % of the reference** (412,114 vs 416,533), depth 6
  within 2.0 % and depth 8 within 8.1 %.

Known remaining differences, in bisection order:

1. **The remaining search slice.** Depths 14–18 drift high (18: +23 %), i.e.
   KnightByte reduces less than the reference. LMR, IIR and TT replacement were
   each ported in isolation and reverted; they must land together with the
   cut-node gates and singular extensions (section 3).
2. **Quiet SEE pruning** (`-43 * depth - history/128`, king moves exempt) is not
   active yet.
3. **IIR and TT replacement.** No internal iterative reduction yet; the
   replacement rule still uses a 6-bit age and a different depth penalty than the
   reference's 5-bit age and `depth - 2` per generation.
4. **Singular, ProbCut, history pruning, aspiration trio.**

Scores and best moves are close: depth 8 gives positions 1–3 the same best move
as the reference with scores within a few centipawns (position 3 is identical).

## 3. Isolated port attempts (2026-11-17) - measured and reverted

The reference's late-move reduction schedule, internal iterative reduction and
transposition-table replacement were ported and measured three times, both in
isolation and after the history scale had been matched. Every attempt widened
the tree at depth, so all were reverted.

| Configuration | d6 delta | d8 delta | d12 delta | d18 delta |
| :--- | ---: | ---: | ---: | ---: |
| Committed state at the time | +1,889 | −1,728 | **−8,664** | +4,863,432 |
| IIR only | +1,889 | −1,422 | +112,206 | — |
| Reference base LMR + history + IIR | −512 | −8,508 | +17,953 | +63,538,000 |
| Reference base LMR + all terms + IIR | +337 | −3,161 | +679,404 | — |

Second attempt, after the history scale was ported (committed state: d6 −302,
d8 −3,914, d12 **−4,419**, d18 +1,580,636):

| Configuration | d6 delta | d8 delta | d12 delta | d18 delta |
| :--- | ---: | ---: | ---: | ---: |
| Reference base LMR + all terms + IIR | +3,455 | +7,564 | +125,744 | — |
| Reference base LMR only + IIR | +2,591 | — | −78,522 | −2,531,052 |

Third attempt, TT replacement ported in isolation (5-bit generations that skip
zero, `depth − 2` per generation of age, same-key refresh rule, no
too-deep-to-replace guard):

| Configuration | d6 delta | d8 delta | d12 delta |
| :--- | ---: | ---: | ---: |
| Reference TT replacement | −311 | −2,944 | +284,258 |

Matching the history scale was necessary but not sufficient, and the TT
replacement likewise widens the tree on its own. Every piece that was ported
in isolation so far interacts with the pieces that are still missing:

* **Cut-node plumbing.** The reference's `cutNode` flag drives its LMR term, its
  NMP gate (`!hadProbe || cutNode || score >= beta`) and its history pruning.
  This engine has only a partial NMP gate and no history pruning.
* **TT replacement interacts with reductions.** Replacing entries without the
  reference's reduction ladder destroys deep entries that its search never
  needed; the two must land together.
* **Singular extensions and the extension trio** are absent, so the reference's
  reduction ladder operates on a differently shaped tree.

Conclusion about methodology: the remaining search features are a coupled set.
The next attempt should port **TT replacement, the cut-node flag with its
pruning gates, LMR terms, IIR and singular extensions as one slice**, then A/B
that slice as a whole against the oracle. Porting them one at a time has been
measured three times and each individual piece made the node counts worse.

Recommended order inside the slice:

1. Cut-node flag threaded through the recursion (it is inert on its own).
2. TT replacement (5-bit age, `depth − 2`).
3. History pruning and the cut-node NMP gate.
4. LMR terms and IIR.
5. Singular extensions and the extension trio.
6. Then bisect inside the slice with the d12 and d18 gates.

## 4. Coupled-slice attempt (2026-11-17) — built, measured, reverted

Following section 3's conclusion, the remaining features were built together
behind one switch: reference TT replacement, cut-node threading, history
pruning, quiet SEE pruning, IIR, and the full LMR term set.

| Slice variant | d6 | d8 | d12 |
| :--- | ---: | ---: | ---: |
| Committed state | −302 | −3,914 | **−4,419** |
| TT replacement + cut node (cut inert) | −311 | — | +284,258 |
| + LMR terms | +2,603 | — | +184,812 |
| + IIR + history pruning + quiet SEE | +2,132 | −207 | +68,729 |
| Same, but with the previous TT policy | +2,137 | **+64** | +71,645 |
| Same, plus legacy LMR instead of the term set | −932 | −1,150 | −34,715 |

No variant beats the committed configuration at depth 12, although the
`d8 +64` row shows the slice can be extremely accurate at intermediate depth.
What is still missing from the slice is exactly what the reference layers on top
of these features:

* singular extensions (on by default) and the double/triple tiers,
* the extension trio (`UseExtensionTrio` off by default, but the negative
  extensions it interacts with are on),
* the reference's staged move picker with the combined 1/2/4/6-ply quiet score,
  which changes which move is "late" in the first place.

Recommendation recorded: do not keep stacking isolated slices. The next parity
work should be a dedicated full-negamax port reviewed against the reference
line by line (singular search included), or the project accepts this checkpoint
and moves to Phase 4d, returning to exact parity with that port later.

Status of the gate: `bench 6` -2.0 %, `bench 8` -8.1 %, `bench 12` -1.1 %,
`bench 18` +22.8 %. Exact parity is **not** achieved; it is a reproducible,
documented checkpoint.

## 5. Bisection order for the port

Work strictly in this order; each step is checked against the depth-1 row first,
then the full table.

1. **~~Quiescence gating~~ — done.** `search/quiescence.rs` now mirrors the
   reference: captures-only generation, no quiet promotions, fail-soft stand pat,
   node-level delta pruning, the `-50` SEE bound, the approximate-SEE prefilter,
   in-check evasions with evasion narrowing, depth-0-only TT cutoffs and the
   fail-high averaging. Quiescence narrowing (`UseQsNarrowing`) stays off by
   default and is implemented behind the switch.
2. **~~Zobrist tables~~ — done (prerequisite).** The key generator is now the
   reference xorshift64 with seed `0x243F6A8885A308D3` and its draw order. The
   start position hashes to `0x48E86881DC3DFD16` in both engines, so every
   key-indexed table (TT, correction, eval cache) is comparable.
3. **~~SEE values and tie-break~~ — done.** `search/see.rs` uses
   `{109, 403, 435, 679, 1242, 20000}`, the exact swap-list for ordering, the
   boolean `-1`-tie-break form for pruning, pin exclusion and the king rule.
   Node-level extensions (check extension and single-response extension) are
   also ported, since they were required for the depth-1 score to match.
4. **~~Static evaluation correction history~~ — done.** `search/correction.rs`
   carries the four terms with the reference's weights, clamp, blend weight and
   update gates; only the corrected value drives pruning.
5. **Internal iterative reduction** (`UseInternalIterativeReduction`, true) and
   TT depth/replacement rules. Reference replacement is
   "depth − 2 per generation of age" with a 5-bit age; KnightByte uses
   "depth − 8 per age" with a 6-bit age and a separate replace slack.
6. **~~Pruning formulas~~ — done for the default configuration**: RFP formula
   and damped spill, razoring table, NMP eval bonus and clamp, LMP table,
   futility table and the SEE capture threshold. Still open: quiet SEE pruning
   (needs the combined 1/2/4/6-ply history scale) and history pruning.
7. **LMR**: reference has a table plus 20+ tuned terms (complexity cap, cut
   node, fail-high count, futile, history, not-improving, PV distance, TT-PV).
   KnightByte currently has an integer approximation of the table plus one
   "not improving" weight.
8. **Extensions**: singular extensions (on by default, `SingularMinDepth` 8,
   `SingularBetaSlope` 2), single-response extension, recapture extension (off),
   per-path extension cap, TT extension (off), negative extensions (on).
9. **Aspiration and root**: `UseAspirationReSearchReduction` (true), the "trio"
   levers (`UseTrioAlphaRaise`, `UseTrioHindsight`, `UseTrioTtShrink`), and the
   root best-move safety rules.
10. **Constants**: the mate base is now `±31000` with the `30000` mate bound,
   matching the reference; infinity stays at `±32000` (the reference uses the
   `int` extremes, which is equivalent for non-mate scores). The quiescence ply
   guard is 100 in both.

## 6. Component map

| Reference | KnightByte | Status |
| :--- | :--- | :--- |
| `Search/Search.cs` | `search/searcher.rs` | Partial: state, driver, draw detection; threads/time missing |
| `Search/Search.IterativeDeepening.cs` | `search/searcher.rs` | Partial: aspiration without re-search reduction |
| `Search/Search.Negamax.cs` | `search/negamax.rs` | Partial: PVS, node-level extensions, main pruning; singular, ProbCut, history pruning, correction history, trio, IIR pending |
| `Search/Search.Reduction.cs` | `search/negamax.rs` (LMR helper) | Provisional integer approximation |
| `Search/QuiescenceSearch.cs` | `search/quiescence.rs` | Ported for the default configuration; narrowing switch implemented |
| `Search/MovePicker.cs` | `search/move_picker.rs` | Partial: tiers and SEE/MVV; ordering scores and dynamic bonuses differ |
| `Search/SearchHeuristics.cs` | `search/history.rs`, `search/correction.rs` | Partial: butterfly/capture/1-ply continuation and correction history; longer chains pending |
| `Search/Search.Options.cs` | `search/params.rs` | 22 of 83 catalogued levers mapped |
| `Search/EvalCache.cs` | `search/eval_cache.rs` | Structural parity; disabled by default in both |
| `Search/PvTable.cs` | `search/pv.rs` | Ported |
| `Search/TimeManager.cs` | — | Not started (Phase 4d) |
| `TranspositionTable/TranspositionTable.cs` | `search/tt.rs` | Structural parity; replacement rule and age width differ |
| `Board/Position.See.cs` | `search/see.rs` | Ported (values, swap list, `-1` tie-break, pins) |
| `Board/Position.Draw.cs` | `searcher::is_draw` | Fifty-move and twofold match; contempt not implemented |
| `Tools/Benchmark.cs` | `tools/bench.rs` | Protocol matched (four positions, TT cleared per position, infinite mode) |

## 7. Lever inventory

* All 83 catalogued levers exist in the reference source; none of the catalogue
  names is stale.
* The reference exposes 82 further internal search properties that are not in
  the catalogue. They still shape node counts, so the parity pass reads the
  source, not the catalogue.
* KnightByte's `SearchParams` currently carries 22 of the 83, and only the
  simple ones (thresholds, switches) are formula-exact.

## 8. Rules for the parity pass

1. No guessing. If a count differs, find the reference code that decides it.
2. One feature per change, with the depth-1 row checked before the full table.
3. Never put the reference checkout path into the repository. Describe it
   generically.
4. Update `tests/golden/search_parity.json` only by re-running the capture
   against a pinned reference commit.
5. When a change alters KnightByte's totals on purpose, update
   `docs/baselines.md`, `tests/search.rs` (`BENCH_DEPTH_6_NODES`) and the
   divergence table in this file in the same commit.
