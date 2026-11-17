# KnightByte — New Bugs

Bugs found during the search-parity work and the migration audit round of
2026-11-17. All are porting defects (KnightByte diverged from the reference
implementation at the pinned commit); all are fixed unless marked open.

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

## Open

- The search-parity residual (a learned-history-state seed from depth 5 on)
  is accepted per ADR-0004 — not an open defect. No open defects.

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
