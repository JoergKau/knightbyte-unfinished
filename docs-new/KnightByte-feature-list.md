# KnightByte — Feature List

KnightByte is a UCI chess engine written in Rust. The neural network is fixed
and embedded in the binary. This list tracks what is implemented and what is
still planned; it is updated after every migration step.

## Current status — Phases 0–6 complete; Phase 4 with full node identity (2026-11-17)

Phase 6 (Syzygy WDL/DTZ probing, root DTZ ranking and the `.ctg` opening book)
completed 2026-11-19; the bench pins are unchanged because the default
configuration loads neither tables nor a book.

Parity status: the search is **node-identical** to the bug-fixed reference
(2026-11-18, predecessor commits bffe98a and f274b39) at every captured depth
and position (depths 1-12, 14, 16 and 18, all four bench positions):
`bench 6` = 15,528, `bench 12` = 456,308, `bench 16` = 2,815,799,
`bench 18` = 6,892,874. The earlier accepted deviation (ADR-0004) is superseded
by ADR-0005; ADR-0008 records the adoption of the reference's bug fixes
(former quirks Q2-Q4 / predecessor bugs 147-149, ported as KnightByte fixes
B35-B37, plus the UCI position alignment B33/B34). `bench 6` is asserted in
`tests/search.rs` and the per-position counts for depths 1-8 are asserted as
well. See `docs/SEARCH-PARITY.md`, `docs/audit/migration-audit.md` and
`docs-new/new-bugs.md` (B1-B37).

### Engine skeleton

- [x] Single `knightbyte` binary with a reusable library crate.
- [x] UCI handshake: `uci` → `id name`, `id author`, `uciok`.
- [x] `isready` → `readyok`.
- [x] `ucinewgame` clears the search tables.
- [x] `quit` and clean end-of-input handling.
- [x] `bench [depth]` with a cleared table per position, node counting and a
      reproducible total.
- [x] `go depth|nodes|movetime|clock` search with `info` lines and `bestmove`.
- [x] `option` handshake, `setoption` (Hash, Clear Hash, Move Overhead, research
      levers) and a declarative option catalogue.
- [x] Diagnostic `position startpos|fen [moves ...]`, `perft <depth>` and
      `nnueeval` commands.
- [x] Unknown commands produce an `info string` diagnostic instead of a panic.

### Platform and build

- [x] Runtime x86 feature detection and the `v2 → v3 → v3-vnni → v4-vnni`
      selection ladder.
- [x] Slow-BMI2 detection for AMD families that microcode PEXT/PDEP.
- [x] AArch64 detection path.
- [x] x86 selector code-path cross-check compiled for `x86_64-apple-darwin`
      and run under Rosetta (development check only).
- [x] Universal-Windows build direction recorded in `docs/decisions/`.
- [ ] Two-variant Windows prototype and the 2% NPS acceptance gate (requires a
      Windows host; carried into Phase 8).
- [x] Release profile with LTO, one codegen unit, abort-on-panic and stripped
      symbols.
- [x] CI matrix for macOS, Ubuntu and Windows: format, clippy, tests, release
      build.

### Network and evaluation

- [x] Production network committed under `net/`.
- [x] Network embedded with `include_bytes!`.
- [x] Size and SHA-256 checksum tests.
- [x] v5 loader with magic/version/scheme/region/L1/bucket/quant validation,
      payload bounds and XOR checksum.
- [x] Scheme-2 feature mapping: 768 slots, eight king regions, per-perspective
      rank flip and file mirror.
- [x] Two-perspective `i16` full-refresh accumulator.
- [x] Scalar forward pass with the exact SCReLU/output chain and `i64` tail.
- [x] Bit-exact evaluation parity on the 15 captured region/mirror FENs.
- [x] Negative loader tests (bad magic, version, checksum, truncation).
- [x] `nnueeval` diagnostic UCI command.
- [x] Incremental accumulator: a per-ply accumulator pair in the searcher;
      every made move pushes the child pair by applying only the changed
      weight rows (moved piece, captures incl. en passant, promotion piece,
      castling rook), and a perspective whose own king moved falls back to a
      full refresh. Bit-exact against the full refresh on seeded random walks
      and deterministic castling/en-passant/promotion segments; bench pins
      unchanged; bench 12 ~0.37 → 0.556 MNPS on the Windows host.
- [x] BMI2 PEXT slider attacks (the predecessor's `PextBitboards` approach,
      2026-11-19): relevant-occupancy masks and per-square offsets, tables
      filled **from the ray-walking implementation** (bit-identical by
      construction), runtime-selected (BMI2 present, not a slow-BMI2 CPU,
      `KNIGHTBYTE_FORCE_PEXT`/`KNIGHTBYTE_DISABLE_PEXT` overrides), verified
      against the ray walk for every relevant subset of every square. Honest
      result: no measurable search-level NPS gain on this host (bench 12:
      850K with PEXT vs 869K without — within noise); the predecessor's NPS
      advantage lies elsewhere. The first version deadlocked (the fill
      re-entered the `pext_tables()` initialisation) — fixed by filling from
      the ray walks directly.
- [x] NEON/DotProd, AVX2, AVX-VNNI and AVX-512-VNNI kernels — **x86-64 part
      done (2026-11-19)**: SSE4.1 (128-bit) and AVX2 (256-bit) kernels for
      the accumulator row arithmetic and the SCReLU head, dispatched once per
      process with the ADR-0001 `KNIGHTBYTE_FORCE_ISA` override; the 512-bit
      AVX-512 kernels for the `v4-vnni` rung measure identically to AVX2 on
      the Zen 4 host (double-pumped datapath — the real 512-bit gain belongs
      to Zen 5 / Intel 512-bit-datapath hosts). Bench 12 0.556 → 0.863 MNPS.
      NEON lands with the macOS validation phase; VNNI remains documented as
      inapplicable to the SCReLU per-element semantics.
- [ ] `EvalFile` override.

### Board and rules

- [x] Position representation: mailbox, bitboards, castling, en passant,
      clocks and king squares.
- [x] Compact `Copy` position with copy-make (ADR-0002).
- [x] FEN parser, serializer and error type.
- [x] Legal move generation: promotions, castling, en passant, checks and
      evasion.
- [x] Bitboard attack tables for pawns, knights and kings.
- [x] Direction-walking slider attacks (magic/PEXT optimization deferred).
- [x] Perft implementation and UCI `perft` command.
- [x] Golden perft gate (startpos d4, Kiwipete d3, position3 d4, position4 d3,
      position5 d3) plus external verifier cross-check.
- [x] Zobrist keys: piece, castling, en-passant file and side to move,
      maintained incrementally and split into pawn/non-pawn sub-keys.
- [x] Static exchange evaluation with x-ray recaptures, en passant and
      promotions.
- [x] Insufficient-material draw detection (K vs K, K+N, K+B, same-coloured
      KB vs KB) inside the search.

### Quality

- [x] UCI smoke tests, including `position`/`perft`/`nnueeval`.
- [x] Parser unit tests.
- [x] CPU selector unit tests.
- [x] Move generation and perft tests.
- [x] Static-exchange and move-ordering tests with verifier-checked FENs.
- [x] Search tests: mate in one/two, playable PV, deterministic nodes, node
      limit, null-move key consistency, legal bench best moves.
- [x] Node-identity gate: `bench 6` total and the reference per-position node
      counts for depths 1-8; a used searcher behaves like a fresh one.
- [x] Move-ordering identity tests: pawn capture generation order (both
      colours, en passant interleaved), killer/countermove identity across
      double pushes, castle table moves, capture-score timing and the
      quiescence picker's missing capture history.
- [x] Option catalogue: defaults match `SearchParams`, every advertised lever
      round-trips, spin values clamp, names are unique.
- [x] Time management: fixed-time and clock allocation unit tests; the
      stability ladders, panic mode, clock imbalance and the minimum-depth
      guard are unit-tested; timed `go` covered by a UCI smoke test.
- [x] Network loader and evaluation parity tests.
- [x] Every FEN used by a test is verified by an external UCI verifier at test
      time.
- [x] No compiler warnings; clippy clean.

### Search

- [x] Lock-free transposition table with the reference replacement policy
      (5-bit generation, depth-2 ageing, same-key deeper-keep rule) and
      prefetch.
- [x] Static-evaluation cache and triangular principal-variation table.
- [x] Iterative deepening, aspiration windows with the reference widening,
      PVS, TT probe and store.
- [x] Fifty-move, twofold-repetition and insufficient-material draw detection
      inside the search.
- [x] Staged move picker: TT move, good captures (material + exchange +
      capture history), killers, countermove, quiets by the combined history
      score, losing captures last — with per-stage buffers and lazy scoring.
- [x] Butterfly, capture and 1/2/4/6-ply continuation history with gravity
      bounds and ageing (reference keying).
- [x] Four-term correction history with the reference update gates.
- [x] Null move (cut-node gate, fail-hard beta, mate fall-through), reverse
      futility, razoring, LMP, futility, SEE pruning (depth-scaled with
      history refund), LMR v1 term set, re-search depth shaving, check and
      single-response extensions.
- [x] History pruning, singular extensions (double/triple tiers), negative
      extensions, internal iterative reduction, hindsight depth pair.
- [x] Early TT-cutoff history bonus (every deep-enough lower-bound entry at a
      non-PV node) and the repetition gate for table cutoffs.
- [x] Quiescence search with TT probe/store, SEE filtering and delta pruning
      (including the reference's en-passant classification quirk).
- [x] Time allocation from `movetime` and the clocks (soft and hard budgets,
      move overhead): tournament share capped at a ten-move slice, sudden-death
      clock/30 plus the full increment, tactical floor, safety buffer, hard
      limit = min(4× allocation, clock/4), overhead-clamped soft limit.
- [x] Stability rules: panic mode (1.5× on a collapsing score), the legacy
      PV-stability multipliers, and the stable-TM three-signal scaler
      (best-move ladder, score ladder, node-fraction term, clamped
      0.12–2.45) with the five-iteration minimum-depth guard; per-move node
      attribution feeds the node-fraction term.
- [x] Clock-imbalance weighting (two-sided, off by default, supersedes the
      one-sided time-advantage multiplier, which is on).
- [x] Persistent Lazy SMP workers (step 8, above).
- [x] Step 9: the multi-thread stress gate — a seeded, replayable 500-cycle
      walk across `Threads` 1/2/4/8 (`tests/thread_stress.rs`): no panic,
      exactly one `bestmove` per `go`, every `bestmove` legal for its
      position (the walk mirrors positions through the engine library). The
      walk found and fixed B39 (`setoption Hash` after a helper search
      killed the engine via the resize's exclusive borrow; the reference
      cannot have it — its .NET references never constrain a resize).
- [x] Step 10: the output decision (`docs/decisions/ADR-0007-synchronous-
      writer-and-currmove.md`) — the synchronous writer stays (a search
      emits a handful of short lines; the stress walk measured the output
      side as microseconds per line), and the reference's single
      `info currmove <move> currmovenumber 1 nodes 1` line of iteration 1
      is emitted exactly as the step-0 transcripts show it (before the
      depth-1 line, never when the search stops before iteration 1's move
      loop). A report only — the node pins are unchanged.
- [x] Full lever-catalogue surface: all 83 reference levers are advertised
      under their reference names, defaults and ranges (step 2,
      `src/search/lever_table.rs`), plus three KnightByte-only research
      levers. 36 reference levers are wired into the search; the other 47
      carry `wired: false` and stay validated-but-inert until their feature
      lands (ProbCut, SEE-reach, LMR v2 and its terms, quiescence checks). The
      catalogue is pinned against
      `tests/golden/levers.json`, with no names outside the 83 plus the
      marked KnightByte-only three.
- [x] Full `setoption` validation (step 2): an unknown name prints
      `info string Unknown option: <name>`; out-of-range, unparsable or
      missing spin values and non-boolean check values are rejected silently
      and the previous value is kept; the `Clear Hash` button acts without a
      value; `Threads` validates 1-256 and stays single-threaded until the
      worker pool lands.
- [x] Handshake `info string` lines (step 2): the book state, the tablebase
      state and the eval-source line close the handshake before `uciok`;
      every `isready` restates the eval-source line before `readyok`.
- [x] Exact search-parity gate: `bench 6/12/16/18` node-identical, and every
      captured per-position count identical (ADR-0005, supersedes ADR-0004).
- [x] Removed KnightByte-only research lever `SingularExcludeTtCutoff`
      (step 2): it was measured with no gain (ADR-0006) and the reproduced
      reference behaviour — the singular verification takes its own table
      cutoff, so singular extensions never fire — is the only code path now.
- [x] The search reuses one `Evaluator` (B40, Phase 7 preparation): the
      accumulator buffers are allocated once per searcher, not two vectors
      per node. The corrected performance decomposition lives in
      `tests/perf_microbench.rs` (P0, 2026-11-19); the `node-stats`
      composition shows that the legal-move filter is the largest measured
      slice (89.8 % of all make calls), not the evaluation.
- [x] P0 performance measurement rig (2026-11-19): `release-with-debug`
      profile, corrected microbench, the `node-stats` counter feature,
      `tools/perf_bench.py` (five-run median and node-pin check) and the
      same-session baseline in `docs/baselines.md` section 7. The counters
      found B42 (eval-cache stores while the cache is disabled).
- [x] P1.0 (2026-11-19): B42 fixed - the static-evaluation store is gated by
      `UseEvalCache` like the probe, so the default configuration never
      touches the shared cache. Node counts unchanged.
- [x] P1 (2026-11-19): make-free legality (`board/legality.rs`): checkers and
      pinned pieces are computed once per node and every pseudo-legal move is
      tested without being made; `legal_by_making` stays as the reference and
      `tests/legality.rs` proves move-for-move equality on long random walks.
      The search makes and pushes a move only after its pruning blocks.
      Same-session depth-20 speed ratio vs the predecessor: 0.63 -> ~0.9.
- [x] P2 + P5 prefetch (2026-11-19): slider index by the BMI2 `pext`
      instruction (inline asm behind the CPU gate, software fallback kept;
      non-x86 targets build without the x86 feature macro) and a TT bucket
      prefetch after every make and null move. Node counts unchanged;
      same-session depth-20 ratio vs the predecessor: ~1.08-1.11.
- [x] P3a (2026-11-19): B43 fixed - the picker's trace switch is read once per
      process instead of per node/move (it cost 18-20 % of all samples).
      Profiling rig: `samply` + `tools/profile_report.py`. Same-session
      depth-20 ratio vs the predecessor: ~1.7 (2.4 M NPS). Incremental
      occupancy was measured (no gain) and not merged; make/unmake rejected.
- [x] P7.0 (2026-11-19): B44 fixed - multi-thread node accounting (helpers
      publish their blocks, the main thread only reads the counter). Reported
      `nodes`/`nps` and the `go nodes` limit now cover all threads. Measured
      scaling 2.07x / 4.01x / 7.08x at 2/4/8 threads (aggregate NPS 16.4 M at
      8 threads).
- [x] P6 (2026-11-19): `tools/pgo_build.ps1` builds a profile-guided binary
      (instrumented build, deterministic bench workloads, merge, rebuild) and
      verifies the four node pins; +7-12 % speed, same-session depth-20 ratio
      vs the predecessor 1.79-1.92. Profiles stay local to the host.
- [x] P1c (2026-11-19): the forced-reply extension counts pseudo-legal moves with an
      early exit (`movegen::has_exactly_one_pseudo_move`) instead of building the
      list; +1.1 % balanced (docs/baselines.md 7.12), pins exact.
- [x] P4 eval path (2026-11-19, balanced ABBA, docs/baselines.md 7.13-7.15): the
      incremental push is one fused pass (child = parent - rows + rows, +5.1 %),
      the output head is an exact 16-bit `mullo`/`madd` kernel (+4.6 %), and the
      full refresh keeps each chunk in a register across all rows (+1.3-1.8 %);
      SSE4.1, AVX2 and AVX-512 kernels, scalar oracle, pins exact. Together with
      P1c: +14.6 % over the pre-P1c build. `tools/ab_bench.py` is the balanced
      A/B tool (a back-to-back pair is biased by 2-4 %).
- [x] P4e (2026-11-19): SEE computes pins lazily per colour and uses the shared
      `between` table: +1.9-3.4 % balanced, pins exact. P4 is closed (the
      contiguous accumulator pair and the dispatch hoist are dropped, see
      docs/baselines.md 7.16). Whole session since the pre-P1c build: +17.3 %
      balanced (7.17); PGO rebuild of the new code +6.4 % more (7.18).
- [x] Match gate (2026-11-19, docs/baselines.md 7.19): 500 games, 1 thread, PGO
      build vs the predecessor at 8+0.08: 75.1 % (+191.8 Elo), no time loss,
      illegal move, disconnect or crash. The 2/4/8-thread matches were run by
      the project owner: no errors reported (figures to be added).
- [x] Game history: repetitions of positions from earlier in the game are seen
      by the search (B31). Timed searches are measured from the `go` command
      (B30). Every `go` is answered with a `bestmove` (B32).

### Phase 5 — UCI robustness and threading (in progress)

- [x] Step 0: oracle transcripts captured in `tests/golden/transcripts/` (11
      scenarios; identity lines and timing fields masked). The five exit
      questions answered: `stop` during depth 1 prints a depth-0 info line and
      a `bestmove`; `ponderhit` restarts the clock from itself and re-searches
      the ponder position from depth 1; `mate` and `searchmoves` are
      implemented in the reference; the reported result with `Threads > 1` is
      the reporting thread's own search; `isready` is answered during a search.
      The capture found predecessor bug 150 (`setoption Hash` above the
      declared maximum crashed the reference), fixed at predecessor commit
      `acaff1f`; KnightByte was not affected (spin clamping).
- [x] Step 1: full command surface — every UCI command word recognised
      case-insensitively (`stop`, `ponderhit`, `debug`, `d`, `eval`,
      `nnueregions`, `searchstats`, `checkmoves` included).
- [x] Step 1: `go` parses into one limiter structure (depth, nodes, movetime,
      clocks, movestogo, mate, infinite, ponder, perft, searchmoves) with
      reference parsing: case-insensitive tokens, unknown tokens ignored,
      values consumed even when unparsable, bare `go` and `movestogo` alone
      search depth 6, `go perft N` runs the counter (no bestmove, as the
      reference). Perft prints the reference's three lines (`Nodes:`, `Time:`,
      `NPS:`), added in step 12 after the transcript replay found them missing.
- [x] Step 1: reference diagnostic formats — `d` (FEN and zobrist line), `eval`
      (evaluation with the net source name), `checkmoves` (per-move
      gives-check verdicts), `debug on/off`.
- [x] Step 1: argv surface — `param:Name=Value` overrides validated against the
      lever catalogue and announced (fail-fast on unknown names), other
      arguments joined into a one-shot command line.
- [x] Step 2: option surface and `setoption` validation — the full lever
      catalogue advertised and golden-file-pinned, one validation test per
      rule, the handshake's book/tablebase/eval-source `info string` lines,
      the `isready` eval-source line, and the removal of
      `SingularExcludeTtCutoff`. The step-2 `Clear Hash`/`ucinewgame`
      measurement at `Hash 1024` found B38 (first-clear page commit,
      ~190–275 ms); the bounded clear belongs to step 7.
- [x] Step 3: complete `info` lines — every iteration reports
      `depth seldepth score nodes nps hashfull time pv` in the captured
      order; `nps` guards the zero elapsed time (counts as one millisecond);
      `hashfull` is read once per iteration. `tbhits` is omitted like the
      reference (the capture masks `tbhits <n>` when present and no captured
      line carries one) until the Syzygy phase. Format tests cover cp/mate
      scores, nps arithmetic, the zero-time guard and the exact field order.
- [x] Step 4: the search runs on a dedicated worker thread with an explicit
      16 MB stack (the searcher moves into the worker and back on join, so
      the learned tables survive between searches). The UCI loop keeps
      reading commands while the search runs; `stop` sets a shared flag that
      is polled every 1024 nodes and before every iteration (no per-node
      clock read; the exact `go nodes` limit stays a per-node integer
      comparison); `quit` and end of input stop, join and exit; every
      state-touching command (`position`, `go`, `setoption`, `ucinewgame`,
      `bench`, `perft`) stops and joins a running search first — the
      fallback for protocol-breaking GUIs, printing never a second
      `bestmove`. Exactly one `bestmove` per `go`, from the worker. A stop
      before depth 1 finishes answers `info depth 0 score cp <eval> pv
      <first legal move>` (the step-0 capture, scenario b — verified against
      the reference binary, whose `quit` during a search behaves the same).
      A single legal move is answered at once with the depth-0 line for
      time-managed searches only (clock or `go movetime`; since 2026-11-19,
      ADR-0010 - formerly quirk Q8 it applied to every `go`, so `go infinite`
      answered instantly; infinite, depth, nodes, mate and ponder searches now
      run normally).
- [x] Step 5: `go ponder` runs unbounded (the stored clock limiters start
      only at `ponderhit`, whose fresh search restarts at depth 1 with the
      ponder move applied; the pondered time is not deducted). A `stop`
      without `ponderhit` answers the best move of the last completed
      iteration (the first legal move when none completed; since 2026-11-19,
      ADR-0010 - formerly quirk Q5 it answered `bestmove 0000`), after the
      interrupted-search depth-0 line; a stray `ponderhit` — and one during
      a normal search — is ignored. The `Ponder` option stays unadvertised
      (CCRL requires ponder off). The burst and zero-iteration ponderhit
      cases are byte-identical to the reference binary.
- [ ] `nnueregions` / `searchstats` full instrumentation (the commands are
      recognised and answer honestly; the king-region histogram and the search
      instrumentation land with their features).
- [x] Real `stop` behaviour and the worker-thread search (step 4, above).
- [x] Real `go ponder`/`ponderhit` behaviour (step 5, above).
- [x] Step 6: `go mate N` stops at the first completed iteration proving a
      mate within N moves (a slower proven mate keeps searching; being
      mated always ends it; other limiters still apply), and
      `go searchmoves` restricts the root to the listed legal moves (an
      empty list and an illegal-only list are ignored — the unrestricted
      search runs). The restricted cold run is node- and score-identical to
      the reference binary. Every `go` limiter is now implemented; the
      step-1 fallback (answering a legal move for unsupported limiters) is
      gone.
- [x] Step 7: the transposition table and the evaluation cache live behind
      the `SharedTables` Arc handle (`src/search/tables.rs`) — both tables
      were already lock-free inside (`&self` probe/store/clear with
      atomics); resize takes the exclusive borrow, which the join-first
      rule guarantees to find. B38 fixed: the table clear zeroes in
      parallel above 64 MB, bounding even the first clear after a fresh
      1 GB resize at 59–64 ms.
- [x] Step 8: the persistent Lazy SMP pool (`src/search/pool.rs`) —
      `setoption Threads` resizes it without touching the main searcher's
      state; every helper owns its search state (history, correction,
      killers, PV, stacks, picker), persists between searches, and runs the
      same job as the main thread (same root, same limits minus the time
      budgets), publishing its node count in 2048-node blocks from the
      1024-node poll. Only the main thread runs the time manager, prints
      `info` and answers `bestmove`; the reported `nodes` and the
      `go nodes` limit use the aggregate while helpers are attached.
      `Threads=1` runs the single-threaded path unchanged (exact node pins).
      Measured on the Windows host: `Threads=4` ≈ 2.1× the single-threaded
      NPS at depth 12 on the start position.
- [x] Step 12: all 11 published step-0 transcripts are replayed against the
      engine (`tests/uci_transcripts.rs`): line order and shape, public option
      set, `bestmove` count and legality, the cold fixed-depth node pins and
      the `bench 6` transcript. Timing fields, scores, node values and PV
      tails are masked; host-dependent iteration counts collapse to one
      report block per search. The warm-TT node divergence was found by the
      replay, root-caused with the per-node ledger and fixed as B41 (the TT
      same-key rule is now a pure depth comparison, and history/correction
      history reset every search — the predecessor builds a fresh per-thread
      state for every `go`).

### Phase 6 — Syzygy and opening book (complete 2026-11-19)

- [x] Syzygy WDL and DTZ probing (`src/tb/syzygy/`): on-disk format, Pairs
      decompression, piece/pawn placement encoding, the two-king triangle
      encoding, capture and en-passant resolution, winning-pawn-move
      resolution, and 50-move-aware DTZ values.
- [x] 3-4-5 piece support and the larger material names (up to seven pieces):
      the material list is enumerated by name and each file is mapped and
      parsed on first use; a missing, truncated or corrupt file fails the
      probe for that material without panicking.
- [x] Material gating, castling refusal, position validation and the exact
      WDL score band (`±29 900`, below the mate bound, DTZ-graded above
      `±28 900` for ordering).
- [x] Search integration: a depth-unconditional WDL probe at non-root nodes,
      the checkmated-inside-the-tables mate correction in quiescence, and the
      measured PV policy (`allow_syzygy_cutoff_on_pv_nodes`, default on).
- [x] Root DTZ ranking (`UseSyzygyDtzRootRanking`, internal default on): the
      50-move-aware root probe supplies the DTZ-optimal move as an ordering
      hint, and a cursed root result clamps a flat tablebase win to a draw.
      The toggle-off behaviour reproduces the reference's lazy shuffle
      (verified on the 4-man fixture).
- [x] `SyzygyPath`, `SyzygyRootProbe` (default off) and `Syzygy50MoveRule`
      (default on) are functional; `setoption name SyzygyPath` prints the
      resulting table state.
- [x] `.ctg` opening book (`src/tb/book/`): signature encoding with the
      canonical rank flip / file mirror and the en-passant availability rule,
      one-pass page scanning into an FNV-indexed position map, move-byte
      decoding, child-statistic weighting, annotations (including
      "recommended" and "avoid") and weighted random selection.
- [x] `Own Book` (default false), `Book File` (explicit load; empty value
      auto-detects next to the executable or in `books/`) and `Book Depth`
      (0-100, default 16) are functional. A book move is answered before any
      search with `info string book move:` + `bestmove`, and is skipped for
      `go searchmoves` and pondering.
- [x] Diagnostics: `uci`/`setoption` report the loaded book and tablebase
      state; a missing book file or empty table path is reported with an
      `info string` and never panics.
- [x] Tests (`tests/syzygy.rs`, `tests/book.rs`,
      `tests/uci_book_syzygy.rs`): known WDL values, root win/draw/mate/
      stalemate, illegal/castling/out-of-range rejection, exact tablebase
      search scores and mate announcement, missing/corrupt tables, signature
      vectors, a synthetic book (weights, recommendation, annotation,
      unknown position), the UCI book/root-probe flows and the option
      diagnostics. The 3-man tables live in `tests/data/tb/`; the DTZ-ranking
      tests use the full set through `KNIGHTBYTE_SYZYGY_PATH` and skip when it
      is unset.
- [x] Bench pins unchanged: `bench 6/12/16/18` =
      15,528 / 456,308 / 2,815,799 / 6,892,874 (tables and book are absent in
      the default configuration).
- [x] Quirk clean-up (2026-11-19, ADR-0010): the predecessor's quirks Q5-Q9
      are fixed in both engines (only Q1 remains). B49: one tablebase error
      flag per table kind, so a corrupt DTZ file no longer disables WDL; B50:
      a failed DTZ read is a failed probe, not a made-up value (a WDL-only
      table set no longer produces a bogus root move). Q5/Q8 as above; Q6 (no
      stdout banner) and Q7 (private warm-up table) already held for
      KnightByte. The step-0 transcripts were re-captured from the fixed
      predecessor.
- [x] Differential verification tools: `tools/tb_compare.py` (random positions
      of every material: WDL, DTZ and chosen move against an independent
      reader and a second engine) and `tools/book_compare.py` (book move sets
      along random lines against a second engine). They found and verified the
      fixes of B45-B48; the final runs show no disagreement on 3,900 tablebase
      positions (3-4-5 and 6-man) and 315 book positions.
- [x] Real-book tests (`tests/book.rs`) run against a local `.ctg` when one is
      present (`KNIGHTBYTE_BOOK_PATH` or the git-ignored `books/` directory)
      and skip silently otherwise; a name-scan unit test registers one dummy
      file per material family so CI covers the scan without the large tables.
- [ ] `EvalFile` net override and `Contempt` remain inert (outside Phase 6's
      scope; carried in the feature-compliance list).

### Packaging

- [ ] macOS Apple Silicon release.
- [ ] Ubuntu x64 release.
- [ ] Windows universal executable with per-variant PGO.
- [ ] CCRL submission package.

