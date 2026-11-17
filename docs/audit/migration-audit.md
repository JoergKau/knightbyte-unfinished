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
| 3 | Move generation + perft | `Board/MoveGenerator.cs` | `board/movegen.rs`, `board/perft.rs` | [ ] |
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

## Open items (next audit session)

- The correction/history **update values** at matched nodes (the depth-5
  seed: the killer-state divergence visible at node 1481 of the startpos
  depth-5 tree) — moot while ADR-0004 stands: the residual divergence is the
  documented, accepted deviation, and the determinism pin guards regressions.
- Time management (Phase 4d) — **closed**: the full port (allocation, panic
  mode, stability ladders, clock imbalance, minimum-depth guard, per-move
  node attribution) is in and unit-tested; the pins are unchanged.
- Lever exposure — **closed for the wired features**: 39 of 83 levers are
  advertised; the remaining 44 gate reference features that ship off or are
  not ported (ProbCut, SEE-reach, LMR v2, quiescence checks, book,
  tablebases). They move with their features to later phases.
