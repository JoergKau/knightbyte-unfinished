# KnightByte — Feature List

KnightByte is a UCI chess engine written in Rust. The neural network is fixed
and embedded in the binary. This list tracks what is implemented and what is
still planned; it is updated after every migration step.

## Current status — Phases 0–3 complete; Phase 4 complete with full node identity (2026-11-17)

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
- [ ] Incremental accumulator with dirty-piece tracking.
- [ ] NEON/DotProd, AVX2, AVX-VNNI and AVX-512-VNNI kernels.
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
- [ ] Persistent Lazy SMP workers.
- [x] Full lever-catalogue surface: all 83 reference levers are advertised
      under their reference names, defaults and ranges (step 2,
      `src/search/lever_table.rs`), plus three KnightByte-only research
      levers. 36 reference levers are wired into the search; the other 47
      carry `wired: false` and stay validated-but-inert until their feature
      lands (ProbCut, SEE-reach, LMR v2 and its terms, quiescence checks,
      book, tablebases). The catalogue is pinned against
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
      reference).
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
- [ ] `nnueregions` / `searchstats` full instrumentation (the commands are
      recognised and answer honestly; the king-region histogram and the search
      instrumentation land with their features).
- [ ] Real `stop`, `ponder`, `mate`, `searchmoves` behaviour (steps 4-6; the
      fallback answers a legal move and an `info string`).

### Endgame and book

- [ ] Syzygy WDL/DTZ probing and root ranking.
- [ ] Own opening book, off by default.

### Packaging

- [ ] macOS Apple Silicon release.
- [ ] Ubuntu x64 release.
- [ ] Windows universal executable with per-variant PGO.
- [ ] CCRL submission package.
