# KnightByte — Feature List

KnightByte is a UCI chess engine written in Rust. The neural network is fixed
and embedded in the binary. This list tracks what is implemented and what is
still planned; it is updated after every migration step.

## Current status — Phases 0–3 complete; Phase 4 complete, parity resolved as a documented deviation (2026-11-17)

Parity status: depths 1-4 exact at all four bench positions, `bench 6` −0.1 %
(14,963 vs 14,979), `bench 18` −1.8 % (6,797,622 vs 6,925,762). The residual
divergence is accepted (ADR-0004); the determinism pin (`bench 6` = 14,963)
replaces node identity. See `docs/SEARCH-PARITY.md`,
`docs/audit/migration-audit.md` and `docs-new/new-bugs.md` (B1-B17).

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
- [x] Early TT-cutoff history bonus and the repetition gate for table
      cutoffs.
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
- [ ] Full lever catalogue parity (39 of the 83 research levers are wired and
      advertised; the remaining 44 gate reference features that ship off or
      are not ported — ProbCut, SEE-reach, LMR v2, quiescence checks, book,
      tablebases).
- [x] Search-parity gate resolved as a documented deviation (ADR-0004);
      depths 1-4 exact, `bench 6` within 0.1 % of the reference.
- [ ] Exact search-parity gate (`bench 6/12/16/18` node-identical).

### Endgame and book

- [ ] Syzygy WDL/DTZ probing and root ranking.
- [ ] Own opening book, off by default.

### Packaging

- [ ] macOS Apple Silicon release.
- [ ] Ubuntu x64 release.
- [ ] Windows universal executable with per-variant PGO.
- [ ] CCRL submission package.
