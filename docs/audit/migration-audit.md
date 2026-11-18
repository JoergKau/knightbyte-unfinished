# KnightByte migration audit

**Scope:** every ported component, audited line-by-line against the reference
source at the pinned commit. The audit replaces node-count chasing: each
component is compared field-by-field, constant-by-constant, gate-by-gate, and
every difference is either fixed or documented as a deliberate deviation.

**Method:** for each component, read the reference source and the KnightByte
port side by side. Record every difference in the findings table. Fix it,
verify (unit tests + the parity sweep), and tick the row.

**Status legend:** [ ] to audit · [~] differences found, fixing · [x] audited
and identical · [F] fixed during audit · [D] deliberate deviation (ADR)

| # | Component | Reference | KnightByte | Status |
|---|-----------|-----------|------------|--------|
| 1 | Zobrist keys | `Board/Zobrist.cs` | `board/zobrist.rs` | [x] |
| 2 | FEN parse/serialize | `Board/Fen.cs` | `board/fen.rs` | [x] |
| 3 | Move generation + perft | `Board/MoveGenerator.cs` | `board/movegen.rs`, `board/perft.rs` | [F] pawn capture order (second round) |
| 4 | SEE | `Board/Position.See.cs` | `search/see.rs` | [ ] |
| 5 | Transposition table | `TranspositionTable/TranspositionTable.cs` | `search/tt.rs` | [F] |
| 6 | Evaluation cache | `Search/EvalCache.cs` | `search/eval_cache.rs` | [x] |
| 7 | PV table | `Search/PvTable.cs` | `search/pv.rs` | [x] |
| 8 | Killers | `Search/Search.History.cs` | `search/killers.rs` | [x] |
| 9 | History tables | `Search/SearchHeuristics.cs` | `search/history.rs` | [F] |
| 10 | Correction history | `Search/CorrectionHistory.cs` | `search/correction.rs` | [F] |
| 11 | Move picker | `Search/MovePicker.cs` | `search/move_picker.rs` | [F] |
| 12 | Quiescence search | `Search/QuiescenceSearch.cs` | `search/quiescence.rs` | [F] |
| 13 | Negamax pipeline | `Search/Search.Negamax.cs` | `search/negamax.rs` | [F] |
| 14 | LMR + reductions | `Search/Search.Reduction.cs` | `search/negamax.rs` | [F] |
| 15 | Iterative deepening + aspiration | `Search/Search.IterativeDeepening.cs` | `search/searcher.rs` | [F] |
| 16 | Time management | `Search/TimeManager.cs` | `search/time_manager.rs` | [ ] |
| 17 | UCI options + levers | `Search/Search.Options.cs` | `search/options.rs`, `search/params.rs` | [ ] |
| 18 | Lever constants + defaults | `Search/Search.Options.cs` | `search/params.rs` | [x] |
| 19 | Bench protocol | `Search/Bench.cs` | `tools/bench.rs` | [x] |
| 20 | Draw detection | `Board/Position.Draw.cs` | `search/searcher.rs` | [F] |

## Findings

(numbered; each finding states the component, the difference, and the fix)

1. **Draw detection** — the reference's `IsDrawForSearch` includes
   insufficient-material draws (K vs K, K+N, K+B, KB vs KB same-coloured);
   KnightByte had none. **Fixed:** `Searcher::insufficient_material` added to
   `is_draw`, with the reference's early gate intact — any pawn, rook or
   queen on the board means sufficient material. (Skipping the gate would
   misread K+P vs K+B as KB vs KB.)
2. **Move generation order — promotions** — the reference emits promotion
   pieces Q, R, B, N; KnightByte emitted N, B, R, Q. The picker's
   generation-order tie-break inherits this, so every promotion node ordered
   differently. **Fixed** (`board/movegen.rs`).
3. **Move generation order — piece classes** — both generators walk pawns,
   knights, bishops, rooks, queens, king, castling, each LSB-first. Audited
   identical. No change.
4. **Lever constants and defaults** — every reference default was compared
   against KnightByte's `SearchParams`; all shipped defaults match (the
   not-improving LMR term OFF, ProbCut OFF, the flat SEE thresholds OFF,
   verified-NMP OFF, trio OFF, selective-singular OFF, eval-swing OFF,
   skip-fans OFF, PV table ON, micro-riders ON...). No change.
5. **Correction-history update gate** — the reference's quiet test reads the
   mover's own occupancy at the target square, which can never detect a
   real capture, so its gate reduces to "not en passant, not a promotion":
   capture cutoffs DO update the correction tables. KnightByte had a real
   capture test and excluded them. **Fixed** (`search/searcher.rs`).
6. **Staged move picker** — the tier picker's single-array swap displaced
   moves across stage boundaries and broke generation-order tie-breaking;
   the reference partitions into per-stage buffers. **Fixed:** the picker is
   now staged (TT move → good captures → killers → countermove → quiets →
   bad captures) with lazy quiet scoring, stored per ply in the searcher.
7. **Quiescence en-passant quirk** — the reference classifies quiet by the
   target square only, so an en-passant capture counts as quiet and is never
   searched; the in-check narrowing may break after one. KnightByte searched
   EP captures. **Fixed** (both behaviours reproduced).
8. **Quiescence in-check ordering** — the reference orders evasions through
   the staged picker (good captures first); KnightByte used raw generation
   order. **Fixed** (the picker's `captures_only`/`qsearch` modes).
9. **Transposition-table replacement** — ported earlier in this round:
   5-bit generation, `depth - 2` ageing, same-key deeper-keep, no
   deeper-entry guard. Verified by this audit.
10. **Insufficient-material gate breadth** — the first fix (without the
    early gate) mis-drew sparse-material positions at deep iterations and
    cost ~15 % of the depth-12 tree; re-fixed with the gate in place.

### Second round (2026-11-17, node identity reached)

The first round's closing note blamed the residual on learned-state drift.
That was wrong: reading the node ledger of both engines side by side, every
remaining difference was a concrete decision. Details and reproductions are
in `docs-new/new-bugs.md` (B18-B28 and the quirks Q1-Q2).

11. **Null move gate** — the reference has no node-type or exclusion condition;
    the port had put it under `!pv_node && excluded_move.is_none()` (B18).
    **Fixed.**
12. **Per-ply evaluation stack timing** — written per searched move in the
    reference, at node entry in the port (B19). **Fixed**, together with the
    per-search reset of the stacks and the countermove table.
13. **Cutoff branch** — "not a capture", not "quiet and not a promotion"
    (B20). **Fixed.**
14. **Excluded-only-move node** — returns the mate/draw score, not minus
    infinity (B21). **Fixed.**
15. **Countermove keying** — origin and destination squares, not piece and
    destination (B22). **Fixed.**
16. **Capture scoring time** — after the table move's subtree, not before
    (B23). **Fixed** (a separate `GenerateCaptures` picker stage).
17. **Quiescence picker** — no capture history there (B24). **Fixed.**
18. **Early table-cutoff bonus** — every deep-enough lower-bound entry at a
    non-PV node, not only a cutting one (B25). **Fixed.**
19. **Move identity** — killers and countermoves compare without the
    double-push marker; the picker returns the generated move (B26).
    **Fixed.**
20. **Pawn capture generation order** — black `-7` before `-9`, en passant
    interleaved per direction (B27). **Fixed** (this closes the row-3 gap
    that the first round left open: the first round audited piece-class order
    only).
21. **Countermove snapshot** — the LMR test uses the value read before the
    move loop (B28). **Fixed.**
22. **Table moves lose castle and en-passant markers** in the reference
    (Q2). **Reproduced on purpose**; singular extensions never fire in either
    engine (Q1), also reproduced.

Result: depths 1-12, 14, 16 and 18 are node-identical at all four bench
positions (see `docs/SEARCH-PARITY.md` §2). The rows of the table above that
are still `[ ]` (SEE, time management, options/levers) were not part of this
round; node identity at depth 18 is strong evidence for the search-side
behaviour of SEE but not a line-by-line audit of it.

## Open items (next audit session)

- The correction/history **update values** — **closed**: the "depth-5 seed"
  was the early table-cutoff bonus (B25) plus the picker and generation
  differences listed above, not learned-state drift. ADR-0005 supersedes
  ADR-0004.
- Time management (Phase 4d) — **closed**: the full port (allocation, panic
  mode, stability ladders, clock imbalance, minimum-depth guard, per-move
  node attribution) is in and unit-tested; the pins are unchanged.
- Lever exposure — **closed for the wired features**: 36 of 83 reference levers are
  advertised; the remaining 44 gate reference features that ship off or are
  not ported (ProbCut, SEE-reach, LMR v2, quiescence checks, book,
  tablebases). They move with their features to later phases.
