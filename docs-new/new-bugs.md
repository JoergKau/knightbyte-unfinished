# KnightByte — New Bugs

Bugs found during the search-parity work and the migration audit rounds of
2026-11-17. All are porting defects (KnightByte diverged from the reference
implementation at the pinned commit); all are fixed unless marked open. B1-B17
come from the first audit round, B18-B29 from the second round (the one that
reached full node identity), B30-B32 from the CCRL-readiness audit that
followed (time compliance and protocol robustness). B33-B34 record the
predecessor's UCI input-handling bugs on the KnightByte side; B35-B37 record
the defects KnightByte deliberately inherited from the reference (the former
quirks Q2-Q4, reclassified as predecessor bugs 147-149 and fixed there). The
last section lists predecessor quirks that are reproduced on purpose.

## B1 — Missing insufficient-material draw detection

- **Date:** 2026-11-17
- **Build / commit:** search-parity port, pre-audit
- **Platform / CPU:** all
- **Command or FEN:** `4k3/8/8/8/8/8/8/4KB2 w - - 0 1` (any K+B vs K / K+N vs K)
- **Expected:** the search scores the position as a draw (0), like the
  reference's `IsDrawForSearch`.
- **Actual:** KnightByte had no insufficient-material check at all; sparse
  endings were searched and scored with the neural evaluation.
- **Reproduction:** search any bare king-plus-minor-piece ending and compare
  the score with the reference.
- **Status:** fixed — `Searcher::insufficient_material` added to `is_draw`
  (`src/search/searcher.rs`).

## B2 — Insufficient-material check mis-drew sparse positions (audit round)

- **Date:** 2026-11-17
- **Build / commit:** the audit round's intermediate working tree (never
  committed to `main`)
- **Platform / CPU:** all
- **Command or FEN:** any position with a pawn plus a same-coloured-bishop
  pair on the board (e.g. K+P vs K+B)
- **Expected:** the position is sufficient material — not a draw.
- **Actual:** the first version of `insufficient_material` skipped the
  reference's early gate (any pawn, rook or queen on the board means
  sufficient), so K+P vs K+B matched the KB-vs-KB count conditions and drew
  the subtree. The depth-12 bench tree lost ~15 % of its nodes to false
  draws.
- **Reproduction:** run `bench 12` with the gateless version.
- **Status:** fixed — the early gate restored, matching the reference.

## B3 — Promotion emission order

- **Date:** 2026-11-17
- **Build / commit:** search-parity port, pre-audit
- **Platform / CPU:** all
- **Command or FEN:** any position with promotion moves (e.g. the bench's
  tactical position, `b2a1` promotions)
- **Expected:** promotions are generated queen, rook, bishop, knight (the
  reference's emission order), so the picker's generation-order tie-break
  tries the queen promotion first.
- **Actual:** KnightByte generated knight, bishop, rook, queen; every
  promotion node ordered differently and node counts diverged (visible as
  `b2b1n` vs `b2b1q` in the trace ledger).
- **Reproduction:** `go depth 1` on the bench's tactical position; compare
  the move order.
- **Status:** fixed — `add_promotions` emits Q, R, B, N (`src/board/movegen.rs`).

## B4 — Tier picker's swap displaced moves across stage boundaries

- **Date:** 2026-11-17
- **Build / commit:** search-parity port, pre-audit
- **Platform / CPU:** all
- **Command or FEN:** the bench's endgame position, `go depth 3`
- **Expected:** the quiet moves keep generation order while no history is
  learned (the reference's per-stage buffers preserve it).
- **Actual:** the tier picker scored all moves into ONE array; the partial
  selection sort's swap moved a capture into the front and displaced a quiet
  move to a later position, so generation-order tie-breaking broke in every
  node that yielded a capture. (Observed: `e2e3` demoted behind the rook
  moves.)
- **Reproduction:** dump the picker order for the endgame position with
  empty history tables.
- **Status:** fixed — the picker is now staged with per-stage buffers and
  lazy quiet scoring (`src/search/move_picker.rs`).

## B5 — Correction-history update gate excluded capture cutoffs

- **Date:** 2026-11-17
- **Build / commit:** search-parity port, pre-audit
- **Platform / CPU:** all
- **Command or FEN:** the bench's tactical position, `go depth 2` (node 562
  of the depth-2 tree)
- **Expected:** the correction-history update's quiet gate reduces to "not en
  passant, not a promotion" (the reference's own-capture occupancy test can
  never fire), so capture beta-cutoffs DO update the correction tables.
- **Actual:** KnightByte used a real capture test and excluded capture
  cutoffs; the correction tables stayed empty where the reference's had
  learned, so the reverse-futility margins diverged (eval −646 vs −622 at
  the traced node).
- **Reproduction:** trace the RFP verdicts (`T f` ledger lines) and compare
  the eval/raw deltas.
- **Status:** fixed — the gate now excludes only en passant and promotions
  (`src/search/searcher.rs`, `finish_node`).

## B6 — Quiescence searched en-passant captures

- **Date:** 2026-11-17
- **Build / commit:** search-parity port, pre-audit
- **Platform / CPU:** all
- **Command or FEN:** the bench's tactical position, `go depth 3` (a qsearch
  node after `c7c5`, white pawn on b5, en-passant target c6)
- **Expected:** the reference classifies quiet by the target square only
  (mailbox empty AND not a promotion), so an en-passant capture counts as
  quiet and is never searched in quiescence.
- **Actual:** KnightByte's quiescence classified en passant as a capture and
  searched it, adding qsearch nodes the reference never visited.
- **Reproduction:** trace the quiescence node ledger at the position after
  `c7c5`.
- **Status:** fixed — the mailbox-based quiet classification is reproduced,
  including the in-check narrowing behaviour.

## B7 — Quiescence in-check evasion ordering

- **Date:** 2026-11-17
- **Build / commit:** search-parity port, pre-audit
- **Platform / CPU:** all
- **Command or FEN:** the bench's tactical position, deep qsearch node with
  the king in check
- **Expected:** the evasions go through the staged picker: good captures
  (the recapture of the checking piece) first, then quiets in generation
  order.
- **Actual:** KnightByte ordered the evasions in raw generation order, so a
  quiet block was tried before a winning recapture and the fail-high break
  fired at different moves.
- **Reproduction:** trace the qsearch node order in a checked position.
- **Status:** fixed — the in-check path runs through the staged picker
  (`qsearch`/`captures_only` modes).

## B8 — Transposition-table replacement policy diverged

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** deep benches (depth 14+, the table near capacity)
- **Expected:** 5-bit generation starting at one, the victim chosen by
  `stored depth - 2 per generation of age`, the same-key refresh keeping a
  deeper current-generation entry, and no deeper-entry guard.
- **Actual:** KnightByte used a 6-bit generation, `depth - 8` ageing, a
  deeper-entry slack guard, and always overwrote same-key entries.
- **Reproduction:** unit-test the replacement policy on a filled bucket.
- **Status:** fixed — `search/tt.rs` implements the reference policy.

## B9 — Null-move fail-high shape

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any null-move fail-high node
- **Expected:** a non-mate fail-high cuts fail-hard at beta; a MATE-line
  fail-high falls through and keeps searching (the mate distance through a
  null move is not trusted).
- **Actual:** KnightByte returned the fail-soft score for non-mate and
  beta for mate — inverted.
- **Reproduction:** compare the null-move return values on a mate-line
  fail-high.
- **Status:** fixed — `src/search/negamax.rs`.

## B10 — Improving flag at the root and first ply

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any root/first-ply node
- **Expected:** improving is FALSE when there is no comparison point (ply
  < 2), so the not-improving scales apply.
- **Actual:** KnightByte returned TRUE there; the reverse-futility and
  late-move margins used the improving scales at the search's shallowest
  plies.
- **Reproduction:** compare the RFP margins at ply 1.
- **Status:** fixed — `Searcher::is_improving`.

## B11 — Aspiration window widening formula

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any aspiration re-search (depth ≥ 5)
- **Expected:** on a fail-low the window halves (`beta = (alpha + beta) / 2`)
  and alpha shifts by the delta; on a fail-high beta rises by the delta.
- **Actual:** KnightByte re-centred the window on the score instead.
- **Reproduction:** `go depth 6` and compare the aspiration re-search node
  counts.
- **Status:** fixed — `Searcher::search_root`.

## B12 — Continuation-history keying and missing distances

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any quiet-ordering node
- **Expected:** four continuation tables keyed `(colour, previous target,
  from, to)` at the 1/2/4/6-ply distances; the picker adds the 1-ply cell in
  full and the others halved.
- **Actual:** KnightByte had one table keyed
  `(previous piece, previous target, piece, target)` — different cells, and
  the 2/4/6-ply distances were missing entirely.
- **Reproduction:** unit-test the table indices.
- **Status:** fixed — `src/search/history.rs`.

## B13 — Capture cutoffs updated continuation history

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any capture beta-cutoff node
- **Expected:** a capture cutoff updates ONLY the capture-history table.
- **Actual:** KnightByte also credited the continuation tables, which the
  reference never does (captures never write the quiet-move tables).
- **Reproduction:** unit-test `History::update_capture`.
- **Status:** fixed — `src/search/history.rs`.

## B14 — Capture-ordering scores

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any capture-ordering node
- **Expected:** the material term uses the classical values
  (100/320/330/500/900/20000), the raw capture-history cell is added, and an
  en-passant victim counts as a pawn.
- **Actual:** KnightByte used the SEE-scale values (109/403/435/679/1242),
  no capture history, and an en-passant victim of 0 — every capture node
  ordered differently.
- **Reproduction:** compare the picker's capture order on the tactical bench
  position.
- **Status:** fixed — both pickers (`move_picker.rs`, the staged partition).

## B15 — Late-move-reduction term set diverged

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** any LMR-eligible node
- **Expected:** the base table `int(1.0 + ln(d)·ln(m)/1.75)` with the
  mover-material, continuous-history (1024ths, precise form), cut-node, PV
  and killer terms; the not-improving term ships OFF.
- **Actual:** KnightByte used an integer `log2(d·m)/2` approximation with a
  not-improving term ON and no other terms.
- **Reproduction:** compare the reduction values per (depth, move number).
- **Status:** fixed — the reference schedule and term set are implemented.

## B16 — Missing search features at the port checkpoint

- **Date:** 2026-11-17
- **Build / commit:** the earlier port checkpoint
- **Platform / CPU:** all
- **Command or FEN:** deep benches
- **Expected:** singular extensions (with double/triple tiers), negative
  extensions, internal iterative reduction (with the quality gate), the
  hindsight depth pair, history pruning, quiet SEE pruning with history
  refund, the re-search depth shave, the early TT-cutoff history bonus and
  the repetition gate for table cutoffs.
- **Actual:** none of these existed; the deep tree diverged by +22.8 % at
  `bench 18`.
- **Reproduction:** `bench 18` against `tests/golden/search_parity.json`.
- **Status:** fixed — all implemented in the coupled port + audit round; the
  parity gate remains open (see `docs/SEARCH-PARITY.md` §2).

## B17 — PowerShell `Set-Content` corruption recurred

- **Date:** 2026-11-17
- **Build / commit:** audit round, working tree
- **Platform / CPU:** Windows host tooling
- **Command or FEN:** a `-replace ... | Set-Content` pipeline over
  `src/search/history.rs`
- **Expected:** non-ASCII source content survives the round trip as UTF-8.
- **Actual:** `Set-Content` wrote the local code page; the `±` characters in
  the doc comments corrupted. The known hazard (`next-steps.md` §2.10) fired
  again through a one-liner pipeline, not just plain `Set-Content`.
- **Reproduction:** pipe a UTF-8 file with non-ASCII through
  `Set-Content` on a Windows host with a non-UTF-8 code page.
- **Status:** fixed (the file was repaired with
  `[System.IO.File]::WriteAllText`); the rule stands — use the editor/write
  tool or `WriteAllText` for anything non-ASCII.

## B18 — Null-move pruning was gated to non-PV, non-excluded nodes

- **Date:** 2026-11-17 (second parity round)
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** `bench 6`; any principal-variation node with
  `eval >= beta` at depth >= 3
- **Expected:** the null-move gate is `not root, depth >= 3, not in check,
  eval >= beta, (no table hit or cut node or table score >= beta)` and a
  non-pawn piece — with no node-type or exclusion condition. A PV node and a
  singular-verification node both try the null move.
- **Actual:** the port put the null move inside the reverse-futility/razoring
  block (`!pv_node && !in_check && excluded_move.is_none()`).
- **Reproduction:** compare `bench 6` per position with the oracle.
- **Status:** fixed — `src/search/negamax.rs`.

## B19 — Per-ply evaluation stack published at node entry

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** `bench 6` position 2 (depth 7 and deeper)
- **Expected:** the static evaluation of a node is written to the per-ply stack
  once per *searched move*, just before the move's search call. A null-move
  child (reached before the first move) therefore reads the value an earlier
  visit of that ply left behind, in the hindsight pair and in the improving
  flag two plies further down.
- **Actual:** the port wrote `evals[ply]` at node entry, so those reads saw the
  current node's value.
- **Reproduction:** compare the hindsight-pair inputs of a null-move child
  between the two engines; the effect on node counts was not measured in
  isolation. It was fixed in the first batch together with B18, B20 and B21;
  the Kiwipete bench position only became exact at depth 7 after the second
  batch (B23, B24 and the table-move flag handling of Q2).
- **Status:** fixed. Also fixed in the same change: the per-ply stacks and the
  countermove table are now cleared at the start of every search (the
  reference clears them; the learned history tables persist). The clearing is
  hygiene — bench identity does not depend on it.

## B20 — Cutoff bookkeeping chose the quiet branch by "quiet and not a promotion"

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** any beta cutoff by a non-capturing promotion
- **Expected:** the history branch is chosen by "not a capture": a quiet
  promotion updates the killers, the butterfly and continuation histories and
  the countermove (it stays out of the failed-quiet malus list).
- **Actual:** the port sent a quiet promotion to the capture-history branch.
- **Status:** fixed — `src/search/negamax.rs`.

## B21 — Excluded table move as the only legal move returned minus infinity

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** a singular verification (depth >= 8) where the excluded
  table move is the only legal move
- **Expected:** a node that searched no legal move returns the mate score when
  in check and the draw score otherwise (nothing is stored). The extension
  tiers then compare that value, not minus infinity.
- **Actual:** the port fell through to the bound computation with
  `-INFINITE`, which would earn the triple extension tier.
- **Status:** fixed — a zero legal-move count after the move loop returns the
  mate/draw score. The path is only reached when the verification search is
  not cut off by the table first, which is rare (see Q1); the fix is for
  fidelity and no node-count effect was attributed to it.

## B22 — Countermove table keyed by piece and target

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** any quiet beta cutoff with a previous move
- **Expected:** the countermove table is indexed by the previous move's origin
  and destination squares. (The correction history's countermove term is a
  different table, indexed by piece type and destination.)
- **Actual:** the port indexed the countermove table by piece and destination,
  so different previous moves of the same piece type to the same square shared
  one cell.
- **Status:** fixed — `Previous` now carries `(piece, from, to)`.

## B23 — Capture scores were computed before the table move's subtree

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** any node that has a table move and at least two captures
  with close scores
- **Expected:** the capture list is generated and scored in the stage *after*
  the table move; the capture-history cell read for the ordering therefore
  includes whatever the table move's subtree learned.
- **Actual:** the port partitioned and scored at the first picker call, before
  the table move was searched.
- **Status:** fixed — a separate `GenerateCaptures` stage
  (`src/search/move_picker.rs`).

## B24 — Quiescence ordered captures with the capture history

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** any quiescence node with two or more captures
- **Expected:** the quiescence picker is built without any history table, so the
  capture-history term is zero.
- **Actual:** the port used the capture history there too.
- **Status:** fixed.

## B25 — Early table-cutoff history bonus only on a cutoff

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** startpos, `go depth 5` (node 836, ply 2, depth 2); first
  divergence of the startpos at depth 5
- **Expected:** at a non-PV node, every sufficiently deep *lower-bound* table
  entry with a quiet, non-promoting table move awards the butterfly bonus
  (`min(207 * depth, 1202)`) when the parent found its own move quickly —
  before the node decides whether the stored score reaches beta.
- **Actual:** the port awarded the bonus only when the entry actually cut off.
- **Status:** fixed — `src/search/negamax.rs`.

## B26 — Killer and countermove identity depended on the double-push marker

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** `position fen 8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1`,
  `go depth 14` (node 9391)
- **Expected:** a move's identity is its two squares plus the castle,
  en-passant and promotion markers. A killer recorded as a pawn's `g2g4`
  double push is therefore the same move as a rook's `g2g4` in a later
  position.
- **Actual:** the port's `DOUBLE_PUSH` marker made the two unequal, so the
  killer was not found and the move order differed.
- **Status:** fixed — killers and countermoves are stored and compared in
  identity form (`Move::without_double_push`), and the picker returns the
  *generated* move so the marker stays correct for the position.

## B27 — Black pawn captures were generated in the wrong direction order

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** `position fen rnbqkbnr/pp2pppp/3p4/2P1P3/8/8/PPP2PPP/RNBQKBNR b KQkq - 0 4`
  (`d6e5` before `d6c5`); first divergence of the startpos at depth 13
- **Expected:** each pawn visits its two capture directions in a fixed order:
  white `+7` then `+9`, black `-7` then `-9`; the en-passant capture is
  interleaved per direction. The picker's tie-break between equally scored
  captures inherits this order.
- **Actual:** the port emitted ascending target squares, which is right for
  white and wrong for black, and put the en-passant capture after the ordinary
  captures.
- **Status:** fixed — `src/board/movegen.rs`. Perft is unaffected (counts only).

## B28 — Late-move-reduction countermove test read the live table

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** all
- **Command or FEN:** startpos, `go depth 14` (node 625555, ply 3, depth 11,
  move `b7b5`)
- **Expected:** the countermove is read once before the move loop; the
  reduction test for every later move sees that value.
- **Actual:** the port re-read the live table for each move, and a deeper node
  with the same previous move had overwritten the cell during the long loop.
- **Status:** fixed — the picker's snapshot is used
  (`PickerState::counter_move`).

## B29 — Two test assertions were silently disabled

- **Date:** 2026-11-17
- **Build / commit:** `989c6c5`
- **Platform / CPU:** Windows host tooling
- **Command or FEN:** `tests/search.rs` (`node_limit_stops_the_search`) and
  `tests/move_ordering.rs` (`see_handles_promotions`)
- **Expected:** both tests assert their numbers.
- **Actual:** a literal backslash-n had been written into a `//` comment, so the
  assertion that followed on the same line was part of the comment (the same
  tooling hazard as B17).
- **Status:** fixed; both assertions pass.

## B30 — Timed searches overran by the time since the previous search began

- **Date:** 2026-11-17 (found by the first match run)
- **Build / commit:** `989c6c5` plus the second-round parity fixes
- **Platform / CPU:** all
- **Command or FEN:** `ucinewgame`, `position startpos`, then two `go wtime 300
  btime 300 winc 40 binc 40` (or `go movetime 200`) a second apart
- **Expected:** a search is limited by the clock that starts when `go` is
  handled: 300 ms on the clock uses a few tens of milliseconds, `movetime 200`
  takes about 190 ms.
- **Actual:** `init_clock`/`init_movetime` stored their absolute limits on the
  searcher's *old* clock, and `search()` then restarted that clock. Every limit
  was shifted by the time since the previous search began. In `fastchess`
  4+0.04 games both sides lost on time (overruns of 129 ms to 1,052 ms); a
  300 ms clock took 463 ms; `go movetime 200` once returned after 0 ms.
- **Reproduction:** a script that sends two timed `go` commands with a pause in
  between; `tests/search.rs`
  (`a_timed_search_is_measured_from_the_go_command_not_from_an_earlier_search`).
- **Status:** fixed — the clock starts in the `init_*` calls and `search()` no
  longer restarts it (`src/search/searcher.rs`). Bench identity is unaffected
  (bench is untimed). The time-compliance gate of Phase 5 is still open; this
  was one concrete cause of time losses, not a validation.

## B31 — Repetitions of earlier game positions were invisible to the search

- **Date:** 2026-11-17 (CCRL-readiness audit)
- **Build / commit:** `989c6c5` plus the second-round parity fixes
- **Platform / CPU:** all
- **Command or FEN:** `position startpos moves g1f3 g8f6 f3g1 f6g8` then `go`;
  any game in which a position recurs
- **Expected:** the search counts a position that repeats an earlier *game*
  position like one that repeats along its own path: twofold repetition scores
  as a draw, and a repeated position is refused table cutoffs. An engine that
  is winning avoids repeating; one that is losing can steer into it.
- **Actual:** `parse_position` kept only the final position. The repetition
  checks looked back only along the search path (`keys[ply - back]`), so a
  position repeated from before the root was never recognised. In real games
  the engine could throw away a won position by repeating, or miss a draw.
  Bench and every parity gate start from a bare FEN, so none of them saw it.
- **Reproduction:** `src/search/searcher.rs`
  (`a_repetition_of_an_earlier_game_position_is_seen_by_the_search`).
- **Status:** fixed — the UCI layer passes the keys of all earlier game
  positions to `Searcher::set_game_history`; `repetition_count` looks back
  through the search path and then the history, bounded by the fifty-move
  clock. With no history the behaviour (and bench identity) is unchanged.

## B32 — An unparsable `go` produced no `bestmove`

- **Date:** 2026-11-17 (CCRL-readiness audit)
- **Build / commit:** `989c6c5` plus the second-round parity fixes
- **Platform / CPU:** all
- **Command or FEN:** `position startpos`, `go ponder wtime 1000 btime 1000`
  (also `go mate 3`, `go searchmoves e2e4`, and `go` before any `position`)
- **Expected:** every `go` is answered with a `bestmove`; a GUI waits for it
  and loses on time otherwise.
- **Actual:** the handler printed an `info string` and returned silently.
- **Status:** fixed — the engine now answers such a `go` with the first legal
  move (or `0000` with no position or no legal move). The limiters themselves
  (`ponder`, `mate`, `searchmoves`) are still unsupported; CCRL plays with
  ponder off, so that is a Phase 5 item, not a blocker.

## B33 — A rejected `position` command keeps the previous position

- **Date:** 2026-11-18
- **Build / commit:** release build of 2026-11-18 00:14 (no source change since)
- **Platform / CPU:** Windows 11 x64
- **Command or FEN:** `position startpos moves e2e4`, then `position fen this-is-not-a-fen`, then `nnueeval`
- **Expected:** after a rejected `position` command the engine must not search a position the GUI did not set; either the new position is set, or no position is set and `go` says so.
- **Actual:** `nnueeval` still reports the position after `e2e4` (-45 cp; the start position gives 27). The same happens when `position startpos moves e2e4 e2e4 d7d5` is rejected for an illegal move.
- **Status:** fixed 2026-11-18 (`07b401c`, Phase 1 restart). KnightByte adopts the predecessor's decided behaviour: the error lines are the reference's exact texts (`Error: invalid position, previous position kept: …` / `Error: illegal move …, position command rejected, previous position kept`), the previous position and its history stay, and the moves of a rejected command are applied to a copy first. Regression test: `tests/uci_smoke.rs::a_rejected_position_command_is_reported_and_keeps_the_previous_one`.

## B34 — `ucinewgame` does not clear the current position

- **Date:** 2026-11-18
- **Build / commit:** release build of 2026-11-18 00:14 (no source change since)
- **Platform / CPU:** Windows 11 x64
- **Command or FEN:** `position startpos moves e2e4`, `ucinewgame`, `go depth 1`
- **Expected:** a new game starts without a position, so `go` must not search the previous game's position (the reference answers nothing here; see B32 for KnightByte's answer when no position is set).
- **Actual:** the search runs on the old position and answers `bestmove e7e5`.
- **Status:** fixed 2026-11-18 (`07b401c`, Phase 1 restart). KnightByte adopts the predecessor's bug-143 behaviour: the engine starts on the start position and `ucinewgame` resets to it, so a `go` after `ucinewgame` searches the start position. Regression test: `tests/uci_smoke.rs::ucinewgame_resets_to_the_start_position`; the bare-`go` test now asserts the start position is searched.

## B35 — Inherited reference bug 147: castle and en-passant table moves are stripped of their marker

- **Date:** 2026-11-18
- **Build / commit:** `fdff189` (search node identity) — the strip is deliberate parity, not a port slip
- **Platform / CPU:** all
- **Command or FEN:** any search node whose table move is a castling or en-passant move (e.g. Kiwipete, `bench 6` position 2)
- **Expected (post-fix reference):** a stored castling or en-passant best move comes back from the table with its marker and compares equal to the generated move — it is tried first, excluded in the singular-verification search and can earn a singular extension.
- **Actual:** `without_special_flags` (`src/search/negamax.rs`) strips the `CASTLE`/`EN_PASSANT` flag from every table move at the probe site, reproducing the reference's lossy 16-bit packing. The stripped move never matches its generated move: not tried first, not excluded in verification, no singular extension — while still counting as "a table move exists".
- **Reproduction:** `cargo test castle_and_en_passant_markers_are_dropped_from_a_table_move` pins the strip; the pre-fix reference behaviour is documented in the predecessor bug log, Bug 147.
- **Status:** fixed 2026-11-18 (`07b401c`, Phase 1 restart). The probe-site strip is deleted: the table move is used as stored, markers included (KnightByte's own packing always preserved them). The strip's unit test is replaced by `a_table_move_keeps_its_castle_and_en_passant_markers`. Node identity with the fixed reference holds at every captured depth (`tools/capture_bench.py compare`, all deltas zero).

## B36 — Inherited reference bug 148: quiescence never searches an en-passant capture

- **Date:** 2026-11-18
- **Build / commit:** `fdff189` (search node identity) — the omission is deliberate parity, not a port slip
- **Platform / CPU:** all
- **Command or FEN:** `r3k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1` (Stockfish-verified; the en-passant capture is white's only capture and wins a clean pawn)
- **Expected (post-fix reference):** an en-passant capture is a capture in quiescence: it takes the capture path (SEE prefilter, `see_ge`, MVV-LVA ordering) and is searched.
- **Actual:** the quiescence quiet test is `position.piece_at(mv.to()) == EMPTY && !mv.is_promotion()` (`src/search/quiescence.rs`). An en-passant capture lands on an empty square, so it is classified quiet and skipped when quiet checks are off — quiescence never searches it.
- **Reproduction:** quiescence on the fixture above returns the stand-pat score although the en-passant capture improves on it.
- **Status:** fixed 2026-11-18 (`07b401c`, Phase 1 restart). The quiet test is `position.piece_at(mv.to()) == EMPTY && !mv.is_promotion() && mv.flag() != EN_PASSANT`. Node identity with the fixed reference holds at every captured depth.

## B37 — Inherited reference bug 149: the correction-history quiet gate omits the capture disjunct

- **Date:** 2026-11-18
- **Build / commit:** `fdff189` (search node identity) — the omission is deliberate parity, not a port slip
- **Platform / CPU:** all
- **Command or FEN:** any Exact node whose best move is an ordinary capture
- **Expected (post-fix reference):** the quiet gate treats "destination occupied by the opponent" as a capture, so ordinary captures do NOT update the correction tables (only en passant and promotions were excluded before the fix — and captures, wrongly, passed as quiet).
- **Actual:** KnightByte's gate omits the reference's own-occupancy disjunct entirely (`src/search/searcher.rs`): `best_move.flag() != EN_PASSANT && !best_move.is_promotion()`. Because the reference's own-occupancy test could never fire (a mover never lands on its own piece), the omission reproduces the reference's effective behaviour exactly — capture cutoffs DO update the tables.
- **Reproduction:** search any position to an Exact node with a capture best move under `UseCorrectionHistory=true` and `UseRefCorrectionHistory=true`; the correction tables receive the capture's eval-error diff.
- **Status:** fixed 2026-11-18 (`07b401c`, Phase 1 restart). The gate classifies through the new `is_quiet_store_move` helper (`src/search/searcher.rs`): quiet means the destination is not occupied by the opponent and the move is neither en passant nor a promotion — the fixed reference's semantics. Unit-tested in `searcher.rs::the_correction_quiet_gate_accepts_quiet_moves_and_rejects_captures`.

## B38 — The first `Clear Hash`/`ucinewgame` after a large `Hash` resize costs unbounded page-commit time

- **Date:** 2026-11-18
- **Build / commit:** Phase 5 step 2 working tree
- **Platform / CPU:** Windows 11, AMD Ryzen 7 260 (Zen 4); release build
- **Command or FEN:** `setoption name Hash value 1024` → `setoption name Clear Hash` / `ucinewgame` (measured with `tools/time_clear_hash.py`)
- **Expected:** a synchronous table clear stays inside the step-2 threshold of about 100 ms at the maximum hash size.
- **Actual:** the steady-state clear is fine (~45–55 ms for 1 GB), but the **first** clear after a fresh resize costs ~190–275 ms: the resize reserves 1 GB of zeroed memory lazily, and the clear's first touch pays the OS page commit. One `ucinewgame` sample under memory pressure reached ~126 ms.
- **Reproduction:** `python tools/time_clear_hash.py target/release/knightbyte.exe` — the "Clear Hash (2nd)" line is the commit-paying first clear.
- **Status:** fixed 2026-11-18 (Phase 5 step 7) — `TranspositionTable::clear` zeroes in parallel (`std::thread::scope` over bucket chunks above 64 MB, `src/search/tt.rs`), so the parallel first touch divides the page-commit cost: the first clear after a fresh 1 GB resize now measures 59-64 ms (was ~190-275 ms), inside the step-2 threshold. Re-measured with `tools/time_clear_hash.py` (three runs, all bounded); node identity is untouched.

## B39 — `setoption Hash` after a helper search killed the engine (the resize's exclusive borrow)

- **Date:** 2026-11-18
- **Build / commit:** Phase 5 step 8 working tree; found by the step-9 stress walk (seeded 500-cycle test)
- **Platform / CPU:** Windows 11, AMD Ryzen 7 260; debug build under test
- **Command or FEN:** `setoption name Threads value 4` → `position startpos` → `go depth 3` → `setoption name Hash value 16` → the process died before the next `go` was answered
- **Expected:** a hash resize after any search keeps the engine alive.
- **Actual:** the persistent helpers kept their `SharedTables` clones between searches, so the transposition table's `Arc` count never returned to one; `SharedTables::resize`'s `Arc::get_mut` failed its `expect` and the panic killed the engine. The stress walk then compared a stale `bestmove` against the wrong position (a second, test-side defect: the walk's wait used a satisfiable cumulative count instead of a snapshot).
- **Root cause and fix, in three layers:**
  1. A helper must not hold the handle between searches: `Searcher::release_tables` runs at the end of every helper job (`src/search/pool.rs`).
  2. The helpers abort their iteration at their next 1024-node poll after the main search's stop flag, which can lag the main worker's join — `stop_search` now waits on the pool's idle barrier (`WorkerPool::wait_until_idle`, a busy counter per helper job).
  3. A *queued* job in a helper's channel can still hold clones while the busy counter momentarily reads zero; `SharedTables::resize` therefore waits bounded (5 s) for `Arc::get_mut` instead of failing instantly, and panics loudly only if a holder never releases.
- **Is this bug in the reference? No — it cannot be.** The reference's helpers hold .NET references to the shared `TTable` object, and `TTable.Resize` (the bug-150-fixed one) replaces the bucket array on the shared object with no exclusivity requirement; a garbage-collected reference cannot block a resize, so the crash mode does not exist there. Probed for the record (2026-11-18, publish build): `Threads 4`, `go depth 14`, the `Hash 16` resize sent mid-search — the reference never even stops the search (it completes to depth 14 and answers normally, exit code 0). The defect is a consequence of Rust's ownership rules in KnightByte's handle design, so it is logged here, not in the predecessor's log.
- **Status:** fixed 2026-11-18 (step 9). The 500-cycle stress walk passes (8 s, seeded and therefore replayable), with per-search bounded waits, the engine's stderr captured to a file, and bestmove legality checked against the walk's mirrored position.

## B40 — The search's evaluation allocated two accumulator vectors per node

- **Date:** 2026-11-18
- **Build / commit:** Phase 7 preparation (found by the performance decomposition, not by a behaviour test)
- **Platform / CPU:** Windows 11, AMD Ryzen 7 260; release build
- **Command or FEN:** any search (one evaluation per node)
- **Expected:** `evaluate.rs`'s own contract — "The search will reuse an `Evaluator` instead of allocating one per position."
- **Actual:** `Searcher::evaluate` called the `evaluate_position` convenience function, which allocates two accumulator `Vec`s per call and memsets both — two heap allocations plus two full-buffer clears per node, on top of the full-refresh refresh itself.
- **Measured context** (`tests/perf_microbench.rs`, release, Ryzen 7 260): full-refresh evaluation ~548K/s (1.8 µs), legal move generation ~1.5M/s, copy-make ~1.5M/s; the search at 370K nodes/s (2.7 µs/node) — so the evaluation is ~67 % of the entire node budget, and the per-node allocations are part of that.
- **Status:** fixed 2026-11-18 — the `Searcher` owns an `Evaluator` over the embedded net (allocated once); `evaluate` uses it. The values are bit-identical (the same code path), verified by the unchanged bench pins. The bigger wins (incremental accumulator, SIMD kernels, `target-cpu`/PGO builds) are the Phase 7 items proper.

## B45-B48 - Phase 6 port defects found by the post-phase audit's differential checks

All four were introduced by the Phase 6 port (commit `fd7658e`) and fixed the
same day. None of them could be seen by the ported unit tests, which use only
the three committed 3-man tables or a synthetic book. They were found by
differential runs against an independent tablebase reader and the
predecessor binary (`tools/tb_compare.py`, `tools/book_compare.py`): 273 of 300
random 4/5-piece positions disagreed before the fixes, none of 3,500 do after
(3-4-5 and 6-man sets, WDL, DTZ and chosen move), and 56 of 197 book positions
disagreed before versus 0 of 315 (400 samples each) after.

- **B45 - `probe_wdl` used the capture value without negating it.**
  - Command or FEN: any 4/5/6-piece position where a capture is available,
    for example `1B6/p7/8/8/8/4N2K/3k4/8 w - - 0 1` (winning capture, DTZ 1).
  - Expected: the child is probed from the opponent's side, so its value is
    negated (`v = -ProbeAb(...)`); the winning capture returns DTZ 1.
  - Actual: the unnegated value was compared with the running best, so
    captures were scored from the wrong side (DTZ 3 here).
  - Status: fixed 2026-11-19 (`src/tb/syzygy/probe.rs`).
- **B46 - `encode` skipped the trailing piece groups of pawnless tables.**
  - Command or FEN: any 4+ piece pawnless table, for example
    `4k3/8/7N/8/6K1/B7/8/8 w - - 0 1` (KBNvK, DTZ 54).
  - Expected: after the leading three pieces (or two kings) the remaining
    groups each add a combination index; the reference runs that loop for both
    encodings.
  - Actual: the pawnless branch returned right after the leading group, so
    only the first three pieces fixed the index (DTZ 14 here). The 3-man
    tables, which have no extra groups, were unaffected, which is why the
    committed tests passed.
  - Status: fixed 2026-11-19 (`src/tb/syzygy/index.rs`; one shared loop).
- **B47 - the material-name scan omitted the 3-v-3 six-piece family.**
  - Command or FEN: `6K1/8/6k1/5p1n/2N5/8/4Q3/8 w - - 0 1` (KQN v KPN).
  - Expected: `K.. v K..` names are registered (the reference's loop with
    `l = (i == k) ? j : k`).
  - Actual: those tables were never registered, so every 3-v-3 position
    reported "not covered" (97 of 292 six-piece samples).
  - Status: fixed 2026-11-19 (`src/tb/syzygy/table.rs`); the unit test
    `the_name_scan_registers_every_material_family` registers one dummy file
    per family so CI covers it without the large tables.
- **B48 - the book's en-passant availability used the wrong capturer colour.**
  - Command or FEN: `position startpos moves e2e4 e7e6 g1f3 b8c6 d2d4`
    (an own pawn stands next to the pushed pawn).
  - Expected: the capturing pawn belongs to the side to move.
  - Actual: the colour was inverted, so an own adjacent pawn recorded a
    capture that cannot happen; the child signature then missed the book and
    double-push moves such as `d2d4`, `c2c4` and `f2f4` were dropped from the
    candidates in many positions.
  - Status: fixed 2026-11-19 (`src/tb/book/signature.rs`). The earlier
    en-passant test compared two different positions and proved nothing; it now
    compares the same board with and without the square and covers the own-pawn
    case.

## Predecessor behaviour

Predecessor quirks are tracked in the local quirk register, which is not part of
the published documentation. Predecessor bugs stop the migration until decided
(`project-rules.md`). Both are described in the migration plan, Phase 5.

**Reclassification (2026-11-18).** Quirks Q2, Q3 and Q4 were reviewed against the
predecessor source and reclassified as predecessor bugs 147, 148 and 149 — the
code evidence (a known-and-worked-around packing loss, self-contradicting
capture classification, a gate comment that misdescribes its own line) rules out
intentional behaviour. All three are fixed in the predecessor (commit `f274b39`,
with the regression tests `PredecessorBugs147to149Tests`, full suite 734/734).
KnightByte carries the same three defects as **B35, B36 and B37** above.
Consequences for KnightByte:

- KnightByte must **stop reproducing** them (B35/B36/B37) when the migration
  restarts; the fixed predecessor is the new specification.
- The Phase 4 node-identity pins (`bench 6/12/16/18` = 14,979 / 416,533 /
  3,169,859 / 6,925,762) were captured against the old build and no longer
  describe the reference. New reference totals: **15,528 / 456,308 / 2,815,799 /
  6,892,874** (deterministic, two passes). They are re-captured and re-pinned in
  Phase 1 when the migration restarts; until then the KnightByte pins describe
  the old build.
- Q1 (singular extensions never fire) was reviewed in the same pass and left as
  a quirk; ADR-0006 measured the fixed behaviour and found no gain (negative
  point estimate).

## Open

- **B42 - eval cache stores every evaluation while the cache is disabled
  (found by the P0 node counters, 2026-11-19).**
  - Build / commit: 2026-11-19 performance session, working tree before P1
    (release, `node-stats` feature).
  - Platform / CPU: Windows 11, AMD Ryzen 7 260.
  - Command: `cargo run --release --features node-stats -- bench 20`.
  - Expected: with `UseEvalCache=false` (the compiled default) the evaluation
    path touches the cache zero times; the predecessor returns from its
    evaluation before any cache instruction when the lever is off.
  - Actual: 0 eval-cache probes, but **8,326,971 stores** in `bench 20` (0.547
    per node): a random write to the shared 1 MB cache for every static
    evaluation, with no reader. The same pattern appears in `bench 12`
    (234,673 stores, 0 probes). Node counts are unaffected; this is a
    performance defect (wasted memory traffic and cache pollution), not a
    search defect.
  - Reproduction: the `eval_cache_probes`, `eval_cache_stores` and
    `eval_head_calls` lines of the command above.
  - Status: **fixed 2026-11-19 (P1.0)**. `evaluate_stacked` now gates the
    store with `self.params.use_eval_cache`, like the probe; behaviour-neutral
    for both lever settings (bench pins unchanged). Regression test:
    `the_eval_cache_is_not_written_while_the_lever_is_off`. Measured effect:
    `bench 12` median 799-833 K -> 847-863 K NPS (about +3-4 %, three
    interleaved five-run pairs).

- **B44 - multi-thread node accounting: the helpers never published and the main thread published itself (found 2026-11-19 while chasing the "thread collapse"; fixed same day).**
  - Build / commit: `6fe6d19`, Windows 11, AMD Ryzen 7 260.
  - Command: `setoption name Threads value 4`, `go movetime 4000` on the start position.
  - Expected: the reported `nodes`/`nps` are the sum over all threads.
  - Actual: the helper searchers were never given the shared counter (`attach_helpers` was only called on the main searcher), so `publish_node_blocks` was a no-op for them; the main searcher, which holds the counter to read from it, published its **own** blocks into it from the 1024-node poll and then reported `own + counter`, i.e. about **twice its own count and no helper nodes**. A direct probe (per-helper node print) showed each helper searching at 2.6 M nodes/s while the reported total looked like a collapse. The documented "thread scaling collapse" (1 T 810 K, 4 T 1.42 M, 8 T 895 K; 6-8 T below 1 T) was largely this reporting defect (plus B43's contended environment lookups, which cost ~45 % of the samples in an 8-thread run). `go nodes N` with Threads > 1 was also enforced on the wrong aggregate.
  - Status: **fixed 2026-11-19 (P7.0)**. Helpers attach as publishers (`Searcher::attach_publisher`, `publishes_nodes`), the main searcher only reads the counter. Regression test: `helpers_publish_their_nodes_and_the_main_searcher_does_not` (`src/search/pool.rs`). `Threads = 1` is untouched (pins exact). Measured with `perf_test.py --depth 18`: 1 T 2.32 M, 2 T 4.79 M (2.07x), 4 T 9.29 M (4.01x), 8 T 16.4 M (7.08x); predecessor 1.34 M / 2.58 M / 5.04 M / 7.68 M.

- **B43 - the move picker read the environment on every node and every scored quiet move (found by the first sampling profile, 2026-11-19; fixed same day).**
  - Build / commit: `a867fbe` (release-with-debug), Windows 11, AMD Ryzen 7 260.
  - Command: `samply record --save-only --unstable-presymbolicate ... knightbyte.exe bench 18`, aggregated per function with `tools/profile_report.py`.
  - Expected: a debug trace switch is read once per process.
  - Actual: `PickerState::trace_reset` (per node) and the quiet-scoring stage (per move) called `std::env::var_os("KNIGHTBYTE_PICKER_TRACE")`. On Windows that is a UTF-16 conversion plus a kernel environment query: `to_u16s`, `getenv` and the ntdll environment routines together took roughly **18-20 % of all samples**. Node counts were unaffected; this is a pure performance defect.
  - Status: **fixed 2026-11-19 (P3a)**. The flag is read once into a `OnceLock<bool>` (`picker_trace()` in `src/search/move_picker.rs`); the trace output itself is unchanged. Measured: `bench 12` 1.20-1.23 M -> 1.68-1.74 M NPS; same-session depth 20 1.54 M -> 2.38-2.43 M (identical nodes, pins exact).

- **B41 — warm-transposition-table node divergence (found by Phase 5 step 12, fixed 2026-11-18).**
  - Build / commit: step-12 release build; bench pins exact (`capture_bench.py compare`, all deltas +0).
  - Platform / CPU: Windows 11, AMD Ryzen 7 260.
  - Command / FEN: replay `tests/golden/transcripts/go_limiter_depth.txt` in one session: `position startpos`, `go`, `go depth 7`, `go nodes 50000`, `go movestogo 40`.
  - Expected: the golden's unmasked warm values, e.g. depth 7 = 5,731 nodes after the bare `go`; `go nodes 50000` reports 34,979; `movestogo` 169.
  - Actual: 10,500 / 48,863 / 3,924 nodes. Cold fixed-depth searches remain identical (bench 6/12/16/18 = 15,528 / 456,308 / 2,815,799 / 6,892,874); step 12 pins only cold values (47 at depth 1, 43 at the mate in one). The goldens were captured with a warm shared table in a sequential session; PV tails also differ in derived reporting (step 3). No crash, illegal move or time loss is involved.
  - Controlled 2026-11-18 probe against the local reference binary (one process, `position startpos` then repeated searches): cold `go depth 6` is 5,741 nodes in **both** engines; the second `go depth 6` gives reference **326** vs KnightByte **3,351**, and `go depth 7` after that warm-up gives **4,311** vs **9,999**. The reference also reports `hashfull 10`, KnightByte `0` at that point. The divergence is deterministic and confined to states with entries from an earlier search.
  - Table-size probe (same sequence at `Hash` 1, 16 and 64 MB): each engine's warm counts are identical across all three sizes (reference 326 / 4,311; KnightByte 3,351 / 9,999), so capacity and entry count are **not** the cause.
  - Rejected hypothesis (2026-11-18): removing `TranspositionTable::new_generation()` from `Searcher::search()` moved the numbers (3,351 → 164 and 9,999 → 7,367) but did not match the reference. The predecessor source then showed that its main thread calls `_tt.NewSearch()` and increments `_currentAge` at the start of **every** `IterativeDeepening` (`Search.IterativeDeepening.cs:43-48`, `TranspositionTable.cs:274`), i.e. the generation bump is correct. The change was reverted; fmt/clippy, all 264 tests, release build and every bench delta (+0) are green again.
  - Status: **fixed (2026-11-18)**, via the per-node ledger method of `docs/SEARCH-PARITY.md` §8.6. The ledger diff of the repeated `go depth 6` localized the first divergent node, and the probe dump at that node showed the cause. Two state-carryover differences, both invisible to cold searches:
    1. **TT same-key replacement evicted deeper entries across searches.** KnightByte stamped entries with its advancing generation, so each new search's shallow root entry overwrote the previous search's deep Exact entry (root probe in the warm search: reference `depth 6 Exact` vs KnightByte `depth 1`). The predecessor stamps every entry with the same per-search age — its UCI layer builds a fresh `SearchEngine` for each `go` whose `_currentAge` resets to 1 → 2 (`EngineOrchestrator.cs:1261`, `Search.Options.cs:122`) — so its same-key rule (`TranspositionTable.cs:456`) reduces to a pure depth comparison across searches. Fix: `choose_slot` (`src/search/tt.rs`) refreshes a same-key slot only when `depth >= stored depth`; the generation clause is gone.
    2. **History and correction history survived the search.** The predecessor builds a fresh `ThreadData` for every `go` (`EngineOrchestrator.cs:1231`), so its learned tables start cold each search; KnightByte persisted them (the step-8 persistent-pool design). Fix: `Searcher::search()` clears history and correction history next to the killers (`src/search/searcher.rs`); the persistent worker pool keeps its per-helper searchers, only their learned tables reset per search.
  - Verification: the per-node ledger now shows cold 5,741 = 5,741 and warm 326 = 326 with **zero ledger divergence**; the re-captured `go_limiter_depth.txt` transcript reproduces every warm node count exactly (213 / 439 / 1,210 / 4,001 / 11,999 / 34,979 / 169) — the remaining transcript differences are the documented PV-tail reporting (step 3), the timing values and the start-up banner. All 264 tests pass, fmt/clippy clean, release build clean and every bench delta at +0. Step 13 / CCRL game safety is unblocked.

- **Predecessor bug 150 (found by the Phase 5 step 0 capture, 2026-11-18; fixed same day):** `setoption name Hash value 999999` — any spin value above the declared maximum — killed the reference with native heap corruption (`TTable.Resize`, delayed manifestation). Fixed in the predecessor at commit `acaff1f` (option-layer clamp `ClampHashSize` + validation in `TTable.Resize`/constructor; regression tests `PredecessorBug150Tests`, full suite 745/745, bench identity unchanged). KnightByte is **not** affected: spin values are clamped into range at the option layer (`src/search/options.rs`) and the Hash resize is clamped to 1..1024 (`src/search/searcher.rs`), verified 2026-11-18. All oracle transcripts were re-captured against the fixed build.
- No open KnightByte defects. B33-B37 were fixed at the Phase 1 restart (`07b401c`,
  2026-11-18): the UCI position handling follows the predecessor's decided
  behaviour, and KnightByte adopts the reference's fixes for the former quirks
  Q2-Q4 (predecessor bugs 147-149). Node identity with the bug-fixed reference
  holds at every captured depth and position.
- **No open KnightByte defects.** B33-B37 were fixed at the Phase 1 restart (`07b401c`,
  2026-11-18): the UCI position handling follows the predecessor's decided
  behaviour, and KnightByte adopts the reference's fixes for the former quirks
  Q2-Q4 (predecessor bugs 147-149). Node identity with the bug-fixed reference
  holds at every captured depth and position. B38 (the first-clear page
  commit after a large `Hash` resize) was fixed in Phase 5 step 7 with the
  parallel table zeroing. B39 (the resize's exclusive borrow vs the
  persistent helpers) was found by the step-9 stress walk and fixed in three
  layers (release + idle barrier + bounded resize wait); it has no
  counterpart in the reference (see its entry).

## Verified platforms

- **2026-11-17 — Windows 11 (build 26300), AMD Ryzen 7 260 (Zen 4, AVX-512/VNNI):**
  rustc 1.100.0 / cargo 1.100.0 (pinned by `rust-toolchain.toml`).
  `cargo build`, `cargo build --release`, `cargo test --all-targets` (with the
  FEN verifier set), `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` all pass. UCI smoke test returns `uciok`,
  `readyok`, `Nodes: 8902` for `perft 3` and `nnueeval 27`.
  No portability issues found.

## Template

When a new defect is found, append an entry with:

- **Date:**
- **Build / commit:**
- **Platform / CPU:**
- **Command or FEN:**
- **Expected:**
- **Actual:**
- **Reproduction steps:**
- **Status:** open / fixed / cannot reproduce
