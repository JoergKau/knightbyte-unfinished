# KnightByte — Feature List

KnightByte is a UCI chess engine written in Rust. The neural network is fixed
and embedded in the binary. This list tracks what is implemented and what is
still planned; it is updated after every migration step.

## Current status — Phases 0–3 complete; Phase 4 complete except the exact parity gate (2026-11-17)

Parity status: `bench 6` −2.0 %, `bench 8` −8.1 %, `bench 12` −1.1 %,
`bench 18` +22.8 %. One coupled port of the remaining reference search features
is required; see `next-steps.md` §3.1 and `docs/SEARCH-PARITY.md`.

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
- [ ] Insufficient-material draw detection.

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
- [x] Time management: fixed-time and clock allocation unit tests; timed `go`
      covered by a UCI smoke test.
- [x] Network loader and evaluation parity tests.
- [x] Every FEN used by a test is verified by an external UCI verifier at test
      time.
- [x] No compiler warnings; clippy clean.

### Search

- [x] Lock-free transposition table with generation aging and prefetch.
- [x] Static-evaluation cache and triangular principal-variation table.
- [x] Iterative deepening, aspiration windows, PVS, TT probe and store.
- [x] Fifty-move and twofold-repetition draw detection inside the search.
- [x] Move picker: TT move, SEE-gated captures with MVV-LVA, promotions,
      killers, countermoves and losing captures last.
- [x] Butterfly, capture and continuation history with gravity bounds and
      ageing.
- [ ] Correction history and longer continuation chains.
- [x] Null move, reverse futility, razoring, LMP, futility, SEE capture
      pruning, LMR and check extensions.
- [ ] History pruning, ProbCut, singular extensions and negative extensions.
- [x] Quiescence search with TT probe, SEE filtering and delta pruning.
- [x] Time allocation from `movetime` and the clocks (soft and hard budgets,
      move overhead).
- [ ] Clock-imbalance weighting, stability rules and persistent Lazy SMP
      workers.
- [ ] Full lever catalogue parity (29 of the 83 research levers are wired and
      advertised; the rest need their features first).

### Endgame and book

- [ ] Syzygy WDL/DTZ probing and root ranking.
- [ ] Own opening book, off by default.

### Packaging

- [ ] macOS Apple Silicon release.
- [ ] Ubuntu x64 release.
- [ ] Windows universal executable with per-variant PGO.
- [ ] CCRL submission package.
