# Search parity map

This file tracks the port of the predecessor search, feature by feature, and
records where KnightByte currently diverges. It is the working document for the
Phase 4 node-count gate.

**Status:** the search is **node-identical** to the reference (second audit
round, 2026-11-17, ADR-0005, which supersedes the ADR-0004 deviation). Every
captured depth (1-12, 14, 16, 18) matches at all four bench positions:
`bench 6` = 14,979, `bench 12` = 416,533, `bench 16` = 3,169,859, `bench 18` =
6,925,762. `tests/search.rs` asserts the `bench 6` total and the per-position
counts of depths 1-8. The audit record is `docs/audit/migration-audit.md`; the
defects are `docs-new/new-bugs.md` (B18-B28, quirks Q1-Q4). Everything below
the first divergence table is the history of how the gap was closed.

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

## 2. Divergence status (2026-11-17): none

Current state, reference and KnightByte (the per-position counts are identical
too; the oracle is `tests/golden/search_parity.json`):

| Depth | Pos 1 | Pos 2 | Pos 3 | Pos 4 | Total | Delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 6 | 5,728 | 2,706 | 2,011 | 4,534 | 14,979 | 0 |
| 12 | 276,529 | 61,017 | 28,547 | 50,440 | 416,533 | 0 |
| 14 | 828,843 | 195,860 | 56,030 | 141,069 | 1,221,802 | 0 |
| 16 | 2,041,912 | 599,976 | 118,626 | 409,345 | 3,169,859 | 0 |
| 18 | 4,461,267 | 1,396,061 | 246,520 | 821,914 | 6,925,762 | 0 |

Depths 1-5 and 7-11 are exact at every position as well (see
`tests/search.rs` for depths 1-8).

### 2.1 How the last differences were found

Each row is one round of "first divergent node, then the deciding input":

| First divergence | Deciding input | Fix |
| :--- | :--- | :--- |
| `bench 6`, all positions at depth 5-7 | null-move gate, evaluation-stack timing, cutoff branch, countermove keying | B18-B22 |
| Kiwipete depth 7, endgame/tactical depth 8-9 | capture scores read before the table move's subtree; capture history inside quiescence; castle/en-passant table moves | B23, B24, Q2 |
| startpos depth 5, node 836 | early table-cutoff bonus only on a cutting entry | B25 |
| endgame depth 14, node 9391 | killer recorded as a pawn double push, rook move on the same squares | B26 |
| startpos depth 13, node 7816 | black pawn capture direction order, en passant interleave | B27 |
| startpos depth 14, node 625555 | countermove read live instead of once before the loop | B28 |

### 2.2 History: the state before the second round

The table below is the state recorded at the end of the first audit round
(when ADR-0004 accepted the deviation). It is kept as history only.


Per-position node counts, reference → KnightByte, before the second round
(the first audit fixed the correction-update gate, the promotion order, the staged picker, the
quiescence EP quirk and added the missing insufficient-material draws):

| Depth | Pos 1 | Pos 2 | Pos 3 | Pos 4 | Total ref | Total ours | Delta |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | exact | exact | exact | exact | 561 | 561 | 0 |
| 2 | exact | exact | exact | exact | 1,577 | 1,577 | 0 |
| 3 | exact | exact | exact | exact | 2,514 | 2,514 | 0 |
| 4 | exact | exact | exact | exact | 4,994 | 4,994 | 0 |
| 5 | 3,947 → 3,940 | 1,660 → 1,658 | 1,299 → 1,299 | 3,025 → 3,022 | 9,931 | 9,919 | -12 |
| 6 | 5,728 → 5,719 | 2,706 → 2,702 | 2,011 → 2,011 | 4,534 → 4,531 | 14,979 | 14,963 | -16 |
| 7 | 10,815 → 10,028 | 4,277 → 4,176 | 2,855 → 2,855 | 6,117 → 6,112 | 24,064 | 23,171 | -893 |
| 8 | 27,811 → 20,987 | 7,774 → 7,826 | 4,737 → 4,733 | 7,881 → 7,873 | 48,203 | 41,419 | -6,784 |
| 10 | 95,947 → 85,716 | 27,260 → 22,538 | 11,643 → 11,075 | 20,885 → 20,823 | 155,735 | 140,152 | -15,583 |
| 12 | 276,529 → 230,995 | 61,017 → 47,794 | 28,547 → 28,865 | 50,440 → 48,608 | 416,533 | 356,262 | -60,271 |
| 14 | — | — | — | — | 1,221,802 | 1,034,542 | -187,260 |
| 16 | — | — | — | — | 3,169,859 | 3,077,942 | -91,917 |
| 18 | — | — | — | — | 6,925,762 | 6,797,622 | -128,140 |

For history: the pre-audit state of this table (after the first coupled port)
had depth 1 at -7 and depth 18 at +53,743 (+0.8 %); the original port
checkpoint had depth 18 at +1,580,636 (+22.8 %).

What the coupled port fixed (compare the depth-18 column: +22.8 % before,
+0.8 % after):

* **Cut-node threading** through the recursion with the reference's child
  rules: a principal-variation child of a PV node is never a cut node, scouts
  always are, the re-search after a shaved fail-high flips the flag, and the
  full-window PV re-search is never a cut node.
* **Reference transposition-table replacement**: 5-bit generation starting at
  one, the victim chosen by `stored depth - 2 per generation of age`, the
  same-key refresh that keeps a deeper current-generation entry, and no
  deeper-entry guard.
* **The early transposition-table-cutoff history bonus** (a quiet table move
  that cuts off quickly at the parent earns a butterfly bonus) and the
  repetition gate (a position that already repeats on the path is refused
  table cutoffs and table-score-as-eval).
* **History pruning** (legacy thresholds: depth <= 3, combined score <
  -2,000·depth), **quiet SEE pruning** with the depth-scaled, history-refunded
  thresholds (-43·depth quiets, -26·depth² captures, king exempt, ceiling 8),
  and the first-legal-move protection for SEE pruning.
* **The reference late-move-reduction schedule**: the `int(1.0 +
  ln(d)·ln(m)/1.75)` base table, the mover-at-most-four-pieces term, the
  continuous history term (1024ths of a ply, precise form), the cut-node, PV
  and killer/counter terms. The not-improving term ships OFF, matching the
  reference. The re-search depth is shaved for large reductions
  (`ComputeReSearchDepth`).
* **Internal iterative reduction** with the micro-rider quality gate, and the
  **hindsight depth pair** (extend at prior reduction >= 2 and negative
  combined eval, reduce at >= 1 and combined eval > 62).
* **Singular extensions** with the double/triple tiers (margins 30/120),
  verified at half depth with the table move excluded, applied to the table
  move only; plus **negative extensions** (losing captures and historically
  bad quiets searched one ply shallower).
* **The combined quiet score**: butterfly + the 1-ply continuation in full +
  the 2/4/6-ply continuations halved; the continuation tables re-keyed to
  `(colour, previous target, from, to)` at the 1/2/4/6-ply distances, and
  capture cutoffs updating the capture history only. Capture ordering now uses
  the classical material values (100/320/330/500/900/20000), the raw
  capture-history cell, and an en-passant victim counted as a pawn — in the
  main picker and in quiescence.
* **Aspiration windows** with the reference widening (fail-low halves the
  window and shifts alpha by the delta; fail-high raises beta by the delta),
  and the `improving` flag returning *not improving* at the root and first
  ply.
* **Null move**: the gate carries the cut-node arm; a non-mate fail-high cuts
  fail-hard at beta while a mate line falls through and keeps searching.

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
  * SEE pruning of captures: threshold `-26 · depth^2` under the flat legacy
    lever; the depth-scaled history-refunded thresholds are the live default.
* **History tables** use the reference scheme: gravity `h += u - h*|u|/MAX` with
  butterfly ±8,033 (bonus 207/ply up to 1,202; malus 200/ply up to 1,255),
  continuation ±8,003 (204/1,229 and 205/1,067) and capture ±8,488
  (192/1,250). Quiets that failed low earn the malus, the cutoff move earns the
  bonus, killers keep four slots per ply, and history ages by 7/8 after each
  completed iteration from depth 4.
* Depth 18 is within **0.8 %** of the reference (6,979,505 vs 6,925,762),
  depth 12 within 3.1 %, depth 16 within 6.8 %.

(The first round's list of "known remaining differences" is obsolete: all
of them were traced and fixed in the second round, see §2.1.)

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

## 4a. Full coupled port (2026-11-17) — the recommendation above, executed

The full-negamax port was done: every formula above plus singular extensions
with double/triple tiers, negative extensions, the hindsight pair, IIR with the
quality gate, the reference aspiration widening, the combined quiet score with
the re-keyed 1/2/4/6-ply continuation tables, the classical capture-ordering
values with the raw capture-history cell, the early TT-cutoff history bonus,
the repetition gate, the re-search depth shave, and the reference TT
replacement (5-bit generation, `depth - 2` ageing, deeper-keep same-key rule,
no deeper-entry guard).

Result (divergence table in §2): depth 18 improved from **+22.8 % to +0.8 %**,
depth 14 from +18.5 % to −10.5 %, depth 16 from +13.1 % to +6.8 %, depth 8
from −8.1 % to +2.2 %. Depth 5–6 regressed (−302/−2.0 % to −1,861/−12.4 %) —
the mid-depth wobble seeded by the single-node differences listed in §2.

Bisection findings recorded for the next session:

* Toggling each new feature OFF individually (singular, IIR, hindsight,
  history pruning, quiet SEE, negative extensions, re-search shave, TT
  policy, cut-node term) changes the depth-5 total by less than 1.5 % each —
  the features are individually near-inert at shallow depth and only matter
  as a set.
* The old LMR schedule inside the new pipeline measures d5 −1,990 / d6 −948;
  the reference schedule measures d5 −1,571 / d6 −1,861. Neither dominates;
  the seed difference is elsewhere.
* The reference binary's per-iteration `info` lines are a usable oracle for
  per-depth cumulative node counts (`go depth N` with a kept-open stdin);
  they localised the first divergence to iteration 3.

Status of the gate: `bench 6` −12.4 %, `bench 8` +2.2 %, `bench 12` +3.1 %,
`bench 18` +0.8 %. Exact parity is **not** achieved; it is a reproducible,
documented checkpoint, materially closer than the §4 checkpoint above.

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
| `Search/Search.cs` | `search/searcher.rs` | Ported: state, driver, draw detection, time-manager integration, per-move node attribution; threads missing (Phase 5) |
| `Search/Search.IterativeDeepening.cs` | `search/searcher.rs` | Ported: aspiration widening, aging, single-legal fallback |
| `Search/Search.Negamax.cs` | `search/negamax.rs` | Ported for the default configuration: PVS, cut-node threading, hindsight, IIR, singular, negative extensions, pruning stack, LMR v1, re-search shave |
| `Search/Search.Reduction.cs` | `search/negamax.rs` (LMR helpers) | Ported: base table, re-search depth, singular tiers |
| `Search/QuiescenceSearch.cs` | `search/quiescence.rs` | Ported for the default configuration; capture ordering now uses the classical values and the capture-history cell |
| `Search/MovePicker.cs` | `search/move_picker.rs`, `search/quiescence.rs` | Staged picker, node-identical: separate capture-generation stage after the table move, killers and countermove by identity, no capture history in quiescence |
| `Search/SearchHeuristics.cs` | `search/history.rs`, `search/correction.rs` | Ported: butterfly/capture/1-2-4-6-ply continuation (reference keying) and correction history |
| `Search/Search.Options.cs` | `search/params.rs`, `search/options.rs` | Defaults mirrored for the shipped configuration; 36 of the 83 reference levers wired and advertised under their reference names (the rest gate off-by-default or unported features); three KnightByte-only research levers are also advertised |
| `Search/EvalCache.cs` | `search/eval_cache.rs` | Structural parity; disabled by default in both |
| `Search/PvTable.cs` | `search/pv.rs` | Ported |
| `Search/TimeManager.cs` | `search/time_manager.rs` | Ported (Phase 4d complete): allocation, panic mode, stability ladders, clock imbalance, minimum-depth guard, node attribution |
| `TranspositionTable/TranspositionTable.cs` | `search/tt.rs` | Ported: 5-bit generation, depth-2 ageing, deeper-keep rule, no guard |
| `Board/Position.See.cs` | `search/see.rs` | Ported (values, swap list, `-1` tie-break, pins) |
| `Board/Position.Draw.cs` | `searcher::is_draw` | Fifty-move and twofold match; contempt not implemented |
| `Tools/Benchmark.cs` | `tools/bench.rs` | Protocol matched (four positions, TT cleared per position, infinite mode) |

## 7. Lever inventory

* All 83 catalogued levers exist in the reference source; none of the catalogue
  names is stale.
* The reference exposes 82 further internal search properties that are not in
  the catalogue. They still shape node counts, so the parity pass reads the
  source, not the catalogue.
* KnightByte's `SearchParams` carries the shipped-configuration defaults for
  all ported features, and `search/options.rs` advertises 36 of the 83
  catalogue levers; the remaining 44 gate features that ship off in the
  reference or are not ported yet.

## 8. Rules for the parity pass

1. No guessing. If a count differs, find the reference code that decides it.
2. One feature per change, with the depth-1 row checked before the full table.
3. Never put the reference checkout path into the repository. Describe it
   generically.
4. Update `tests/golden/search_parity.json` only by re-running the capture
   against a pinned reference commit.
5. When a change alters KnightByte's totals on purpose, update
   `docs/baselines.md`, `tests/search.rs` (`BENCH_DEPTH_6_NODES` and the
   per-position table), `tests/golden/bench.json` and the divergence table in
   this file in the same commit, and record why.
6. To hunt a divergence, do not read code first. Compare per-position counts
   per depth, take the shallowest difference, dump the node ledger of both
   engines (`KNIGHTBYTE_TRACE`, `tools/trace_diff.py`) and take the first
   divergent node, then print the decision inputs at that node (ordering
   scores, reduction terms, history cells, table hits) on both sides until one
   input differs. The ledger needs an instrumented local build of the reference
   that writes the same line format; keep that build out of the repository.

## 9. Phase 6 - tablebase and book parity (2026-11-19)

The optional knowledge sources do not change the default search, so the four
bench pins are unchanged. Their parity surface is checked separately:

| Reference behaviour | KnightByte module | Test |
| :--- | :--- | :--- |
| WDL probe with capture/en-passant resolution | `src/tb/syzygy/probe.rs` | `tests/syzygy.rs` known values |
| DTZ probe and pawn-move resolution | `src/tb/syzygy/probe.rs` | `tests/syzygy.rs` root values |
| 3-4-5 (and larger) material scan, lazy mapping | `src/tb/syzygy/table.rs` | full-set run through `KNIGHTBYTE_SYZYGY_PATH` |
| Depth-unconditional non-root WDL probe | `src/search/negamax.rs` | `search_scores_tablebase_wins_exactly` |
| Quiescence WDL probe and mate correction | `src/search/quiescence.rs` | `search_announces_mate_when_pv_cutoff_is_disabled` |
| Root DTZ ranking and the 50-move score clamp | `src/search/searcher.rs` | `dtz_root_ranking_drives_the_search_result` (4-man fixture) |
| Root probe direct play (`SyzygyRootProbe`) | `src/uci/mod.rs` | `tests/uci_book_syzygy.rs` |
| `.ctg` signature and reader | `src/tb/book/` | `tests/book.rs` signature vectors and synthetic book |
| Book move before search, depth gate, off by default | `src/uci/mod.rs` | `tests/uci_book_syzygy.rs` |

The reference's coarser error flag (a failed DTZ load also disables WDL for the
same material) is reproduced as quirk Q9; ADR-0009 records the file-access and
dependency decisions.

### 9.1 Differential evidence (2026-11-19)

The unit tests alone were not a sufficient parity gate for this phase (they
passed while B45-B48 were present). The gate is therefore differential:

| Check | Tool | Result |
| :--- | :--- | :--- |
| WDL, DTZ and chosen move, random positions of every 3-4-5 material | `tools/tb_compare.py` | 2,300 positions, 0 disagreements (independent reader and predecessor) |
| Same with the 6-man set (captures resolve into the 5-man tables) | `tools/tb_compare.py` | 1,600 positions (1,155 six-piece), 0 disagreements |
| Book move sets per position along random lines | `tools/book_compare.py` | 315 positions x 400 samples, 0 disagreements |

Before the fixes the same runs disagreed on 273 of 300 tablebase positions and
56 of 197 book positions (`docs-new/new-bugs.md`, B45-B48). Both tools take the
other engine as an argument and are not run in CI (they need the large table
set / a local book); the CI-visible regressions are
`the_name_scan_registers_every_material_family`, the signature tests in
`tests/book.rs`, and the full-set tests that skip when
`KNIGHTBYTE_SYZYGY_PATH` is unset.