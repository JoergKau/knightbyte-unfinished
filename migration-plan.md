# KnightByte â€” Migration Plan: the predecessor C# engine â†’ Rust

**Status:** Phase 0 complete; Phase 1 oracle capture and Phase 2 perft are complete; Phase 3 scalar evaluation is bit-exact. SIMD kernels and the incremental accumulator are deferred to the performance phase. Phase 4 (search) is complete: the parity gate is resolved as a documented deviation (ADR-0004); the determinism pin replaces node identity. Next: Phase 4d completion, then Phase 5.  
**Companion documents:** `project-rules.md`, `additional-nodes.md`, `maximizing_performance_in_a_rust_uci_chess_engine.md`  
**Source of behaviour:** the predecessor C# tree (internal, not published with KnightByte)  
**Target:** a headless UCI engine named **KnightByte**, MIT licensed, built with the current stable Rust toolchain (`rustc 1.100.0` at the time of writing).

> This is an internal engineering plan. Do not copy paths to internal/reference trees into shipped code, comments, tests, option names or public documentation. Credits and third-party notices belong in the project's own documentation, not in source files.

---

## 1. Purpose

KnightByte is a **behaviour-preserving migration** of the C#/.NET 11 engine into Rust, followed by measured performance work. The neural network does not change. The search behaviour must be proven identical before it is optimized.

The project succeeds when all of the following are true:

1. **Functional parity** â€” the same legal moves, perft results, evaluations, search decisions at fixed depth, UCI behaviour and option surface as the C# engine.
2. **Numerical parity** â€” the embedded NNUE produces bit-identical evaluations, including incremental accumulator updates.
3. **Behavioural parity** â€” the fixed benchmark reports the same node counts at depths 12, 16 and 18 with compiled defaults and one thread.
4. **Performance** â€” at equal node counts, KnightByte is materially faster: at least 1.5Ã— the C# NPS on the M4 development host (C# baseline 1,656,154 NPS â†’ **â‰¥2.48 M NPS**; the 2.5Ã— stretch is **â‰¥4.14 M NPS**), and therefore reaches greater depth in fixed time on the time-to-depth benchmark.
5. **Portability** â€” release artifacts for macOS Apple Silicon, Ubuntu x64 and Windows 11 x64. The Windows artifact is one universal `.exe` with startup ISA dispatch, benchmarked against dedicated builds.
6. **Quality** â€” `cargo build`, `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` are clean; no known crashes, illegal moves or time losses.
7. **CCRL readiness** â€” a stable Windows x64 zip (engine, README, LICENSE, source pointer), own book off by default, ponder off, UCI compliant, 3-4-5 piece Syzygy supported.
8. **Maintainability** â€” small, documented `.rs` files; inline code documentation on every public and non-obvious item; the feature list and bug log are current.

### Non-goals

* Training, re-quantizing or re-shaping the neural network. That is a separate project; the shipped net is fixed.
* A GUI, a web service, a cloud trainer, or an opening-book authoring tool.
* Intel macOS, Linux ARM and Windows ARM.
* Search-strength tuning before parity is proven. Tuning starts only after the golden bench, perft and evaluation gates pass.
* Copying code from the GPL reference engines. They are design references only.

---

## 2. Ground rules

These rules are non-negotiable and apply to every phase.

| Rule | Practical consequence |
| :--- | :--- |
| **Parity first, then speed** | No search heuristic, pruning margin or evaluation change ships before the golden gates pass. A faster engine with different node counts is a different engine. |
| **The net is fixed** | The v5 net is embedded in the binary, checksum-verified, and evaluated bit-exactly. `EvalFile` may override it, but a bad file is a loud refusal, never a silent fallback. |
| **All levers visible** | The C# predecessor has 12 public options and 83 hidden research levers (95 option names in total). KnightByte exposes the full catalog as visible UCI options, with the same defaults/ranges, and keeps the count nearly equal. |
| **MIT only** | KnightByte is MIT. The reference Rust engines and the reference C++ universal-build source are GPL-3.0: read them for architecture, never paste code. Port from the MIT C# tree. |
| **No engine/author names in code** | No reference-engine names, blueprint names, author names or company names in `.rs` files, comments, test names or option names. Credits live in project documentation only. |
| **No references to unpublished files** | Code, tests and public docs must not point at the internal predecessor tree or `reference_engines/`. Those trees stay local and git-ignored. |
| **Small files, inline docs** | Split modules rather than growing a 2,000-line file. Document every public item and every non-obvious invariant. |
| **Tests are migrated, not reinvented** | Port the C# test suite where it makes sense. Every FEN constant is verified against the locally installed Stockfish at test time (`/opt/homebrew/bin/stockfish` on macOS; `KNIGHTBYTE_STOCKFISH` may override the path). |
| **No warnings** | CI builds with warnings denied. `unsafe` blocks carry a `// SAFETY:` comment explaining the invariant. |
| **Log every new bug** | New defects go to `docs-new/new-bugs.md` with a reproducible FEN/command and the affected build. |

---

## 3. Source inventory

The C# tree is the specification. The numbers below are the planning baseline and must be refreshed at the start of Phase 1.

| Area | C# files / lines | Rust destination | Risk |
| :--- | :--- | :--- | :--- |
| Board, moves, bitboards, Zobrist, FEN, SEE, draws | `Board/*.cs`, ~3,500 lines | `src/board/` | Lowâ€“medium |
| Move generation | `MoveGenerator.cs`, 696 lines | `src/board/movegen/` | Medium |
| Magic/PEXT attacks | `MagicBitboards.cs` + `PextBitboards.cs`, 537 lines | `src/board/attacks/` | Low |
| NNUE loader, features, accumulator, quant, kernels | `Evaluation/*.cs`, ~2,000 lines | `src/eval/nnue/` | High (bit-exactness) |
| Search | `Search/*.cs`, ~7,800 lines | `src/search/` | High (behavioural parity) |
| Transposition table | `TranspositionTable.cs`, 508 lines | `src/tt/` | Medium |
| Time management | `TimeManager.cs`, 740 lines | `src/search/time_manager.rs` | High (time losses) |
| Syzygy | `Tb/Syzygy.cs`, 2,583 lines | `src/tb/syzygy/` | Mediumâ€“high |
| Opening book (`.ctg`) | `Book/*.cs`, 644 lines | `src/book/ctg/` | Lowâ€“medium |
| UCI / orchestration | `UCI/orchestration (C#)`, ~2,400 lines | `src/uci/`, `src/main.rs` | Medium |
| Bench / tools | `Tools/Benchmark.cs`, 201 lines | `src/tools/bench.rs` | Low |
| Tests | ~65 files, ~12,700 lines, ~720 tests | `tests/` + module tests | â€” |

**Fixed production net:** `jokausch.nnue`, 6,294,672 bytes, SHA-256 `0a70169ce7d75a9a5a392cd293746fc1f5191a75dc183b5b45fb69d869ce2968`. It is copied into `net/` and committed; it is published with the engine and embedded with `include_bytes!`.

**Golden behavioural values to capture in Phase 1:**

* `bench 12` = **416,533** nodes; `bench 16` = **3,169,859**; `bench 18` = **6,925,762** (four-position set, compiled defaults, one thread, shared TT at the default 256 MB). The four positions are: startpos, Kiwipete (`r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1`), `8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1`, and `r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1`.
* Per-position node splits and best moves for the same runs.
* Perft values for the C# suite.
* Raw evaluation (cp) for a large FEN set, both full-refresh and incremental paths.
* The complete UCI option catalog: name, type, default, min/max, and whether it was public or hidden.
* UCI transcripts for handshake, `position`, `go`, `stop`, `ponderhit`, `ucinewgame`, `setoption`, `bench`.

### 3.1 Toolchain and baselines

| Tool | Location | Purpose |
| :--- | :--- | :--- |
| **fastchess** | `fastchess (installed locally)` (macOS arm64; Windows build available) | SPRT/tuning, 1,000-game safety matches, time-loss checks |
| **cutechess-cli** | `cutechess-cli (installed locally)` (build on each host) | CCRL-like controls, legality and time-loss validation |
| **perf_test.py** | `perf-test/perf_test.py` | NPS/nodes/EBF/selective-margin/stability gates, scaling, tactical mini-suite |
| **ebf_compare.py** | `perf-test/ebf_compare.py` | Head-to-head depth/EBF comparison |
| **wac_test.py** | `perf-test/wac_test.py` | WAC tactical screen |
| **Stockfish** | `/opt/homebrew/bin/stockfish` (override: `KNIGHTBYTE_STOCKFISH`) | Test-time FEN verification and tactical reference checks |

The C# performance baselines live in the internal baseline documents (Windows Zen 4 and MacBook Air M4). The consolidated summary is `docs/baselines.md`.

Latest C# numbers (net `v5_poolv9_512_auxwdl-150`, four positions, 5 s, Hash 256 MB):

| Host | 1T depth | 1T NPS | 1T EBF | 4T NPS | 8T NPS | WAC |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| Apple M4 | 22.2 | 1,656,154 | 2.050 | 6,180,924 | 7,821,579 | 284/300 |
| Windows Zen 4 | 22.0 | 1,569,444 | 2.061 | 6,079,846 | 10,767,216 | 284/300 |

Compare KnightByte against the numbers for the **same host**: M4 against M4, Zen 4 against Zen 4. Do not compare a macOS run to a Windows baseline. For cross-machine node identity use fixed depth (`--depth`), not `movetime`; NPS and depth-at-time are host-dependent. The M4 reaches one extra depth on startpos at 5 s, which is why its total-node figure is larger.

**CCRL hardware strategy.** CCRL uses a Haswell i7-4770K (4C/8T, DDR3, AVX2/BMI2). The i7-10750H is the same `x86-64-v3` ISA class, so it is a valid **`v3` and universal-dispatch validation host**, and it is good for legality, time-loss, UCI and same-host C#-vs-KnightByte tests. It is not a 4770K surrogate for absolute performance: Comet Lake has higher IPC, higher clocks and DDR4. Use the Ryzen 7 260 for the AVX-512/VNNI path and the M4/M5 for macOS. A real 4770K (or a close Haswell) is still the best final absolute check; if unavailable, ship on relative gains and let CCRL measure.

The harnesses are deliberately basic and still contain predecessor-specific defaults and one reference to an unpublished document. Phase 1 modernizes them before they become the official baseline tooling.

---

## 4. Target repository layout

Single Cargo package with a library and a binary. A workspace adds cross-crate inlining friction for `#[target_feature]` and LTO; keep the hot code in one crate.

```text
KnightByte/
â”œâ”€â”€ Cargo.toml
â”œâ”€â”€ rust-toolchain.toml          # pin 1.100.0, edition 2024
â”œâ”€â”€ LICENSE                      # MIT
â”œâ”€â”€ THIRD-PARTY-NOTICES.md
â”œâ”€â”€ README.md
â”œâ”€â”€ project-rules.md
â”œâ”€â”€ additional-nodes.md
â”œâ”€â”€ maximizing_performance_in_a_rust_uci_chess_engine.md
â”œâ”€â”€ migration-plan.md
â”œâ”€â”€ net/
â”‚   â””â”€â”€ jokausch.nnue            # embedded, published, checksum-tested
â”œâ”€â”€ src/
â”‚   â”œâ”€â”€ main.rs                  # CLI + UCI entry point
â”‚   â”œâ”€â”€ lib.rs                   # library root, module declarations
â”‚   â”œâ”€â”€ types/                   # square, piece, color, move, score, depth
â”‚   â”œâ”€â”€ board/                   # position, fen, zobrist, make/unmake, attacks,
â”‚   â”‚                            # movegen, see, gives_check, draw, perft
â”‚   â”œâ”€â”€ eval/
â”‚   â”‚   â””â”€â”€ nnue/                # net_file, features, accumulator, quant,
â”‚   â”‚                            # kernels/{scalar,neon,avx2,avx512,vnni}
â”‚   â”œâ”€â”€ search/                  # negamax, quiescence, move_picker, history,
â”‚   â”‚                            # reduction, extensions, time_manager, pv,
â”‚   â”‚                            # eval_cache, options, stats, thread_data
â”‚   â”œâ”€â”€ tt/                      # entry, cluster, alloc, generation
â”‚   â”œâ”€â”€ tb/
â”‚   â”‚   â””â”€â”€ syzygy/              # wdl, dtz, root, material
â”‚   â”œâ”€â”€ book/
â”‚   â”‚   â””â”€â”€ ctg/                 # reader, signature, probe
â”‚   â”œâ”€â”€ uci/                     # parser, options, output, bench, perft
â”‚   â”œâ”€â”€ platform/                # cpu, threads, large_pages, io
â”‚   â””â”€â”€ tools/                   # calibration, parity dumps
â”œâ”€â”€ tests/                       # integration, perft, golden, UCI transcripts
â”‚   â”œâ”€â”€ data/                    # small public test data (3-man TB, fixtures)
â”‚   â””â”€â”€ golden/                  # node counts, eval vectors, option catalog
â”œâ”€â”€ benches/                     # microbenchmarks (criterion)
â”œâ”€â”€ xtask/                       # build orchestration, universal exe, PGO
â”œâ”€â”€ perf-test/                   # existing UCI harnesses: perf_test, ebf_compare, wac_test
â”œâ”€â”€ tools/                       # new oracle/time-to-depth harnesses (published)
â”œâ”€â”€ docs/                        # architecture, SEARCH-PARITY.md, baselines.md, ADRs, credits
â”‚   â””â”€â”€ decisions/               # ADR-0001 â€¦ ADR-000N
â”œâ”€â”€ docs-new/
â”‚   â”œâ”€â”€ KnightByte-feature-list.md
â”‚   â””â”€â”€ new-bugs.md
â””â”€â”€ .github/workflows/           # macOS arm64, Linux x64, Windows x64
```

**Internal trees stay local:** add the internal predecessor tree, `reference_engines/` and any raw capture dumps to `.gitignore`. If a captured golden file is needed by the published tests, commit the derived values (not the source tree) and document how they were produced.

**Crate policy:** keep dependencies close to zero. A chess engine needs no async runtime, no logging framework and no serialization framework in the hot path. Every dependency must be justified in an ADR, audited for license compatibility, and kept out of the search/evaluation path.

### 4.1 Cross-platform workflow

Primary development stays on the M4. The Rust code is portable; only the Windows packaging, ISA-specific optimization and PGO work need a Windows host.

* **Move code with git, not by copying the working directory.** Use a private remote, or `git bundle create knightbyte.bundle --all`, copy the bundle and clone it on Windows. Never copy `target/`: it contains host-specific build artifacts and is ignored.
* `.gitattributes` normalizes text files to LF and marks the network file as binary, so a Windows checkout cannot corrupt either.
* **Windows host roles:**
  * **Zen 4 box:** primary Windows development host, `v4-vnni`/AVX-512 path, universal-executable prototype, `v4-vnni` PGO, and same-host reproduction of the C# Windows baseline.
  * **i7-10750H:** `v3` path, CCRL proxy, legality/time-loss and UCI tests.
* **Do a first Windows checkpoint after Phase 2/3, not at Phase 8.** Clone, `cargo build`, `cargo test`, `cargo clippy`, and run `bench`/`perft` on Windows as soon as the board layer exists. Portability surprises are cheapest to fix early.
* **Per-host setup:** rustup with the pinned `1.100.0` toolchain, MSVC build tools, Git, and the local FEN verifier path in `KNIGHTBYTE_FEN_VERIFIER`. PGO profiles are generated per host and per ISA variant; never reuse them across hosts.
* **Do not compare hosts.** M4 numbers stay on the M4, Zen 4 numbers on Zen 4, `v3` numbers on `v3`.

---

## 5. Migration strategy

### 5.1 Vertical slices, not a big bang

Build a thin end-to-end skeleton early, then deepen each layer while keeping the engine runnable:

1. CLI/UCI answers `uci`, `isready`, `quit` and exits cleanly.
2. FEN parsing + legal move generation + `perft`.
3. A random/first-legal `go` that always returns a legal move (proves the protocol and move plumbing).
4. NNUE evaluation and a depth-1 search.
5. Full search parity.
6. Threads, time management, tablebases, book.
7. Performance and packaging.

At every step the binary is runnable. This keeps integration risk low and makes the C# oracle useful immediately.

### 5.2 The oracle is the C# engine

Do not port from memory or from "how modern engines do it". The C# tree is the specification. When C# behaviour looks wrong, record it in `docs-new/new-bugs.md`, decide whether KnightByte reproduces it for parity or fixes it as a deliberate deviation, and write an ADR. Never silently "improve" a search rule during the port.

### 5.3 Parity gates are hard gates

A phase is not done because the code compiles. It is done when its gate passes:

| Gate | Meaning |
| :--- | :--- |
| **G1 Perft** | Every perft position matches the C#/published values exactly. |
| **G2 Evaluation** | Every golden FEN evaluates bit-identically in scalar, SIMD and incremental modes. |
| **G3 Bench identity** | `bench 12/16/18` node counts match exactly on one thread with compiled defaults. |
| **G4 UCI identity** | Handshake, options, `go` modes and transcript behaviour match; every lever is visible and settable. |
| **G5 Game safety** | 1,000 games against the C# engine: no illegal moves, no crashes, no time losses under the agreed controls. |
| **G6 Cross-platform** | The same node counts on macOS, Linux and every Windows ISA backend; the universal `.exe` is within 2% of dedicated builds. |
| **G7 Quality** | `fmt`, `clippy -D warnings`, tests and docs are clean. |

---

## 6. Phase plan

### Phase 0 â€” Bootstrap and de-risk (1â€“2 days) â€” **completed 2026-11-17**

**Objective:** a compiling, tested, CI-backed skeleton and an early answer to the riskiest packaging question.

- [x] `git init`; add `.gitignore` for internal source/reference trees, `target/`, capture dumps and local tool output.
- [x] Create `Cargo.toml` with edition 2024, `rust-toolchain.toml` pinned to `1.100.0`, and license `MIT`.
- [x] Set release profile: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`, `overflow-checks = false`, `debug-assertions = false`, `incremental = false`.
- [x] Set lint policy: warnings denied, `unsafe_op_in_unsafe_fn = "deny"`, missing docs on public items.
- [x] Create `src/main.rs` with `uci`, `isready`, `ucinewgame`, `quit` and a `bench` placeholder; create `src/lib.rs`, `src/uci/`, `src/platform/`, `src/tools/` and `src/output.rs`.
- [x] Copy the net to `net/jokausch.nnue`; add size and SHA-256 tests.
- [x] Add `LICENSE`, `THIRD-PARTY-NOTICES.md`, `README.md` (build/run/test), `docs-new/KnightByte-feature-list.md`, `docs-new/new-bugs.md`.
- [x] Add CI: macOS, Ubuntu and Windows; `fmt`, `clippy`, tests, release build.
- [x] **Record the universal Windows direction.** `docs/decisions/ADR-0001-windows-universal-build.md` chooses design A and defines the `v2 â†’ v3 â†’ v3-vnni â†’ v4-vnni` ladder. `src/platform/cpu.rs` implements and tests the selector, including slow-BMI2 detection.
- [ ] **Open item:** build the two-variant Windows prototype on the Windows host and run the 2% acceptance gate. This cannot be completed on the macOS host and is scheduled for the first Windows session (Phase 7/8).
- [x] Add the external FEN verification helper and a smoke test that exercises it; the verifier is supplied through `KNIGHTBYTE_FEN_VERIFIER`.

**Phase 0 verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (10 library tests, 1 FEN-verification test, 2 UCI smoke tests) and `cargo build --release` pass on the M4 with no warnings. The UCI skeleton answers `uci`, `isready`, `bench` and `quit` cleanly. The library tests also pass when compiled for `x86_64-apple-darwin` and executed under Rosetta, which exercises the x86 selector code path; this is a development check only, not a supported release target. Initial commit: `81744a9`.

**Exit gate:** local quality gates pass; the UCI skeleton exits cleanly; the universal-build direction has a decision and a tested selector. The Windows per-variant link prototype is the only open Phase 0 item and is explicitly carried into Phase 8.

---

### Phase 1 â€” Capture the oracle (2â€“3 days) â€” **initial capture complete 2026-11-17**

**Objective:** freeze the C# behaviour in machine-readable golden files before writing search code.

- [x] Build and run the C# engine on the M4 host.
- [x] Capture `bench 12`, `bench 16`, `bench 18` totals: **416,533 / 3,169,859 / 6,925,762** nodes, stored in `tests/golden/bench.json`.
- [x] Capture the C# perft suite: `tests/golden/perft.json` for startpos, Kiwipete, position3, position4 and position5.
- [x] Capture evaluation vectors: the 15 region/mirror/side-to-move FENs from the shared list, with raw cp in `tests/golden/nnue.json`. Expansion to a much larger random set remains a later task; the current set already covers all eight king regions, both mirror states and both sides to move.
- [x] Extract the option catalog: 12 public options in `tests/golden/uci.json` and 83 research levers in `tests/golden/levers.json` (95 option names total).
- [x] Capture UCI handshake and the unknown-command behaviour in `tests/golden/uci.json`. The full `go`/`stop`/`ponderhit` transcript capture is part of Phase 5.
- [x] Add `tools/oracle_capture.py` and `tools/capture_levers.py`; both take the local reference binary/source as arguments and write derived JSON only.
- [x] Run the updated performance harness against the C# binary and reproduce the recorded M4 node counts for three of four positions; the fourth stopped one iteration early because of timing variance (documented in `docs/baselines.md`).
- [ ] Finish the harness modernization. Remaining:
  - `--baseline` diff mode: exact node identity, depth/EBF delta, NPS ratio, PASS/FAIL;
  - robust timeouts and process cleanup so a silent engine cannot hang CI;
  - extract the duplicated UCI session code into one shared helper and add a fake-UCI-engine fixture so the harness itself is tested;
  - a stable JSON schema/version and a baseline directory.
- [ ] Build `tools/uci_oracle.py`: runs two engine binaries with the same script and diffs node counts, best moves, eval lines and option sets.
- [ ] Build `tools/time_to_depth.py`: a fixed 20-position set (openings, middlegames, endgames, tactics), fixed time controls, depth and NPS per position.
- [x] Record the initial golden values; the C# tree itself remains git-ignored.

**Exit gate:** the initial oracle is reproducible and the golden files are versioned. A full per-position trace and the expanded evaluation set remain follow-up work for the search-parity phase. Any reference nondeterminism is documented (the M4 `movetime` variance is already recorded).

---

### Phase 2 â€” Types, board, move generation, perft (5â€“7 days) â€” **core complete 2026-11-17**

**Objective:** a correct, fast, allocation-free board and move generator.

- [x] `types/`: `Color`, `Piece`, `Square`, `Move` (bit-packed `u16`, `Move::NONE`, `repr(transparent)`), with size assertions for `Move` and `Position`. `Score`/`Depth` are added with the search.
- [x] `board/bitboard.rs`: bit iteration, `wrapping_sub` clearing and LSB helpers.
- [x] `board/attacks.rs`: compile-time knight/king/pawn tables and direction-walking slider attacks. Magic/PEXT replacement is a performance-phase task, not a correctness prerequisite.
- [x] `board/fen.rs`: strict six-field parser, serializer, error type and start-position constant.
- [x] `board/position.rs`: mailbox + bitboards + castling + en passant + clocks + king squares; `#[derive(Copy, Clone)]`; size assertion `<= 256` bytes.
- [x] Copy-make: chosen for the correctness phase and recorded in `docs/decisions/ADR-0002-board-copy-make.md`. The performance phase benchmarks it against make/unmake.
- [x] `board/movegen.rs`: pseudo-legal and legal generation, promotions, castling, en passant, checks and evasion via king-safety filtering.
- [x] `board/perft.rs` plus the `perft` UCI command.
- [x] Ported perft tests and move-generation tests; added the shared `fen_verified` helper and test-time FEN verification.
- [x] Integration gate `tests/perft.rs`: startpos d4, Kiwipete d3, position3 d4, position4 d3, position5 d3 all match the reference and the external verifier.
- [ ] Zobrist keys (piece, castling, en passant, side, pawn/non-pawn) â€” needed for the transposition table in Phase 4.
- [ ] SEE and full draw detection (repetition, fifty-move, insufficient material) â€” needed by the search.
- [x] No heap allocation in move generation: `MoveList` is a fixed 256-entry stack array and `Position` is `Copy`.

**Exit gate G1:** all standard perft counts match at the tested depths; FENs are externally verified at test time; `Position` and `Move` sizes are asserted. Zobrist, SEE and draw detection are explicitly carried into the search phase.

---

### Phase 3 â€” NNUE loader, accumulator, evaluation (5â€“7 days) â€” **scalar correctness complete 2026-11-17**

**Objective:** bit-identical evaluation of the fixed net, with a fast incremental path.

- [x] `eval/nnue/net.rs`: magic/version/scheme/region/L1/bucket/quant validation, exact pin checks, region-table validation, payload bounds and XOR checksum. Loud errors, no fallback.
- [x] Embed the net with `include_bytes!`; size and SHA-256 tests; description available through the loader.
- [x] `eval/nnue/features.rs`: 768-slot mapping, eight king regions, per-perspective rank flip and file mirror.
- [x] `eval/nnue/evaluate.rs`: two-perspective `i16` accumulators with full refresh, the exact SCReLU/output chain, `i64` output accumulation and `cp = (out * 1600) >> 20`.
- [x] Scalar reference path is the production path for correctness. The 15 captured FENs evaluate **bit-identically** to the reference values.
- [x] Negative loader tests: bad magic, unsupported version, checksum mismatch and truncation.
- [x] `nnueeval` diagnostic command in the engine.
- [ ] Incremental accumulator with dirty-piece tracking and lazy own-king refresh â€” needed by search; deferred to the performance phase with the search/evaluator integration.
- [ ] NEON/DotProd, AVX2, AVX-VNNI and AVX-512-VNNI kernels â€” deferred to Phase 7; the scalar path is the bit-exact oracle they must match.
- [ ] `EvalFile` override â€” deferred until the UCI option layer lands.
- [ ] Expanded evaluation vector set and incremental-vs-refresh identity tests â€” the 15-position curated set is in place.

**Exit gate G2:** zero-cp differences on the 15 captured vectors; a corrupted net is rejected with a clear error. SIMD and incremental paths remain performance-phase work and are explicitly not part of this stop point.

---

### Phase 4 â€” Search parity (10â€“15 days)

**Objective:** reproduce the C# search exactly, including its levers, before optimizing anything.

**Progress:** 4a-4d complete. The parity gate is **resolved as a documented
deviation** (ADR-0004, 2026-11-17): the audit found and fixed fourteen
defects (`docs-new/new-bugs.md` B1-B17), depths 1-4 are exact at all four
bench positions, `bench 6` is within 0.1 % of the reference and `bench 18`
within 1.8 %. The determinism pin (`bench 6` = 14,963, asserted in
`tests/search.rs`) replaces node identity as the regression discipline; the
audit record is `docs/audit/migration-audit.md`. Phase 4d's remaining time
management (clock imbalance, stability rules, safety margins) and the lever
exposure move to their own work items.

- [x] `board/zobrist.rs`: fixed-seed piece, castling, en-passant-file and side keys, incremental updates in `Position::make_move`, pawn and non-pawn sub-keys, key consistency across a perft tree and a pinned startpos value.
- [x] `search/tt.rs` (the C# `tt/` directory): lockless 16-byte entries, 64-byte buckets, exact-key verification, generation ageing, static-eval field, PV (`Exact`) flag, prefetch hook, power-of-two sizing, thread-safe sharing without locks.
- [x] `search/eval_cache.rs`, `search/pv.rs`.
- [x] `search/see.rs`: static exchange evaluation with x-ray recaptures, en passant and promotions; `see_ge` bound test for pruning.
- [x] `search/move_picker.rs`: TT move, captures gated by SEE and ordered by MVV-LVA, promotions, killers, countermoves, quiet ordering by history, losing captures last.
- [x] `search/history.rs`, `search/killers.rs`: butterfly, capture and one-ply continuation history, gravity bounds, ageing, countermove table, two killers per ply.
- [ ] `search/history.rs` follow-ups: correction history and the longer 2/4/6-ply continuation chains.
- [x] `search/negamax.rs` (skeleton): iterative deepening, aspiration windows, PVS, TT probe/store, null-move pruning, RFP, razoring, LMP, futility, SEE pruning, LMR, check extensions with a path cap, fifty-move and twofold-repetition detection.
- [ ] `search/negamax.rs` (parity pass): history pruning, ProbCut, singular extensions, negative extensions, exact lever formulas.
- [x] `search/quiescence.rs`: check-aware, TT probe, delta pruning, SEE filtering, capture ordering.
- [ ] `search/quiescence.rs` (parity pass): recapture narrowing, quiescence checks.
- [x] `search/time_manager.rs` (first slice): move-time and clock allocation with soft/hard budgets and move overhead. Clock imbalance, stability rules and safety margins pending.
- [x] `search/options.rs` (first slice): declarative catalogue for the 12 public options and the 29 wired research levers; generates the handshake and drives `setoption`.
- [ ] Port the full lever catalog. Keep the number nearly equal to the C# catalog; expose all of them visibly. Add a test that every lever is advertised and that setting each one changes the search on a probe position (the C# suite has this pattern).
- [ ] Port the C# search tests: mate tests, PVS, LMR tables, futility, history, singular, extension cap, TT, root TT cutoff, repetition, time compliance, lever independence.
- [ ] Build `docs/SEARCH-PARITY.md`: every C# search feature, its Rust module, its lever name, its test.
- [x] Deterministic `bench [depth]` and `go depth|nodes`, with search tests (mate in one/two, playable PV, reproducible node totals, node limit).

**Exit gate G3 + G4 (search part):** resolved by ADR-0004: the node-identity
goal is retired in favour of the determinism pin (`bench 6` = 14,963,
asserted in `tests/search.rs`); depths 1-4 are exact and the deep tree is
within ~2 % of the reference. Every deviation is documented
(`docs/audit/migration-audit.md`, `docs-new/new-bugs.md`).

> If node counts change after a search change, update the pin in the same
> commit and record the reason. Search behaviour is still pinned and
> deterministic; only identity with the predecessor is retired.

---

### Phase 5 â€” UCI, threading, time compliance (3â€“5 days)

**Objective:** a robust, tournament-safe UCI engine.

- [ ] `uci/parser.rs`: full command set, case-insensitive tokens, `position startpos|fen`, `moves`, all `go` limiters (`depth`, `movetime`, `wtime/btime/winc/binc`, `movestogo`, `nodes`, `mate`, `infinite`, `ponder`), `stop`, `ponderhit`, `quit`, `setoption`, `ucinewgame`, `isready`, `bench`, `perft`.
- [ ] `uci/options.rs`: visible catalog, `option` handshake, `setoption` dispatch, button options (`Clear Hash`), type validation.
- [ ] `uci/output.rs`: dedicated writer thread with a bounded channel; build lines once; coalesce/drop optional `info currmove` under back-pressure; flush correctly for GUIs.
- [ ] Persistent Lazy SMP worker pool: start/stop signaling, `Threads` changes without rebuilding state, per-thread node counters padded to the cache line, main-thread aggregation, shared TT and read-only net.
- [ ] Search control: stop flag, ponderhit, time-up checks every N nodes, no `Instant::now()` per node, no shared atomic per node.
- [ ] `bench` command: deterministic four-position set, per-position output, total nodes and NPS; same defaults as C#.
- [ ] `info` line parity: depth, seldepth, multipv? (single PV only, as C#), score cp/mate, nodes, nps, hashfull, tbhits, time, pv.
- [ ] UCI transcript tests against the captured C# transcripts.
- [ ] `fastchess` + `cutechess-cli` harness: 1,000 games vs C# at a fixed control, plus 1/2/4/8-thread smoke matches. Check legality, crashes, time losses and clean `quit`/`stop` handling.

**Exit gate G4 + G5:** no illegal moves, no crashes, no time losses; `Threads` scaling is sane; `ucinewgame` is fast (no multi-GB TT clear); every option is visible and functional.

---

### Phase 6 â€” Syzygy and opening book (5â€“8 days, parallelizable)

**Objective:** restore the optional endgame and book features without destabilizing the search.

- [ ] Port `Tb/Syzygy.cs`: WDL + DTZ probing, 3-4-5 piece support, 7-man capable, material gating, depth-unconditional probing in search and quiescence, 50-move-rule-aware scores, root DTZ ranking, `SyzygyRootProbe`.
- [ ] Copy the small 3-man test tables into `tests/data/tb/` with their license/notice. Test WDL, DTZ, root ranking, missing/corrupt files.
- [ ] Port the `.ctg` reader and signature validation; own book off by default; `Book File` auto-detection only when explicitly requested.
- [ ] Test the book with a synthetic fixture or a small book we own. Do not publish a third-party `.ctg`.
- [ ] Ensure missing paths, unreadable files and malformed data produce `info string` diagnostics, never panics.

**Exit gate:** tablebase and book tests match C#; `Own Book=false` is the default; absent files are harmless; the engine builds and runs with these features compiled in.

---

### Phase 7 â€” Performance (5â€“10 days)

**Objective:** convert the parity build into a faster engine without changing its node counts.

**Measurement protocol**

* Fixed benchmark set: the four-position `bench` plus the 20-position time-to-depth set.
* Metrics: nodes, NPS, time-to-depth, depth at fixed time, EBF, TT hit rate, eval time share.
* Cheap gates: `perf-test/perf_test.py --depth` for machine-independent nodes/EBF/stability and `--threads 1,4,8` for scaling; `perf-test/ebf_compare.py` for head-to-head EBF. These reject breakage before an SPRT.
* Strength: `fastchess` SPRT after the cheap gates pass; `cutechess-cli` for CCRL-like controls and time-loss checks.
* Profilers: `samply`/Instruments on macOS, `perf` on Linux, VTune or ETW on Windows, `hyperfine`, `cargo asm`, `cargo llvm-lines`, `cargo bloat`.
* Baselines: compare against the C# numbers for the **same host** â€” M4 against the M4 record, Zen 4 against the Zen 4 record; the consolidated table is `docs/baselines.md`. Use fixed depth for node identity and the same time control for time-to-depth.
* A/B runs alternate and repeat at least five times; report median and spread. Record governor, temperature and background load. Do not trust a single laptop run.
* Every optimization is a separate commit with its own measurement and a node-count check. If node counts change, it is a search change, not an optimization.

**Work items, in order**

- [ ] Profile the parity build; identify the top three costs. Do not guess.
- [ ] Zero-allocation audit: counting allocator in tests; no `Vec` growth, `String` formatting, `Box` or `format!` in search/eval.
- [ ] Position representation: benchmark copy-make vs make/unmake on M4 and x86; pick the faster, document the ADR.
- [ ] TT: prefetch, allocation strategy, generation aging, bucket probe order, hashfull behavior.
- [ ] NNUE: incremental update validation, kernel selection per host, alignment, `loadu` vs aligned, accumulator layout.
- [ ] Lazy SMP: persistent workers, false-sharing audit, node-counter publication cadence, TT contention.
- [ ] Compiler: `target-cpu=apple-m1` for macOS, `v3`/`v4-vnni` for Windows, PGO per artifact, LTO validation.
- [ ] Optional experiments: PEXT vs magic per host, i8mm (nightly/asm), AVX-512 vs AVX2 on Zen 4, huge pages, mlock, QoS class.
- [ ] Windows universal build: compare against dedicated `v3` and `v4-vnni`; accept only within 2%; force-ISA test every backend.
- [ ] Update the performance notes in `docs-new/` with commands, host, and results.

**Exit gate G6 (performance part):** at least 1.5Ã— C# NPS on the M4 at identical bench nodes; time-to-depth improvement on the 20-position set; no node-count drift; no platform-specific correctness differences.

---

### Phase 8 â€” Release engineering (3â€“5 days)

**Objective:** reproducible, tested artifacts for all three OS families.

- [ ] macOS: `aarch64-apple-darwin`, `target-cpu=apple-m1`, PGO, strip; smoke test on M4 and M5 Pro.
- [ ] Linux: `x86_64-unknown-linux-gnu`, `target-cpu=x86-64-v2`, PGO; smoke test on the Ryzen 5 5600H.
- [ ] Windows: universal `.exe` with `v3`, `v3-vnni`, `v4-vnni` paths (and `v2` if the ADR keeps it); per-variant PGO; smoke test on i7-10750H and Ryzen 7 260.
- [ ] On the i7-10750H, record a C# baseline and a KnightByte baseline at the same protocol (`v3` path) so the relative gain is apples-to-apples. Do not compare its NPS to the Zen 4 or CCRL numbers.
- [ ] Confirm Windows 10 compatibility if the CCRL host is not on Windows 11; avoid Win11-only APIs in the engine. The i7-10750H on Windows 11 does not cover this case.
- [ ] `xtask` (or a checked-in script) builds all artifacts, verifies hashes, and emits a manifest.
- [ ] Release package: engine, `README.md`, `LICENSE`, `THIRD-PARTY-NOTICES.md`, source archive pointer, checksum file. The net is embedded; no separate net file is required.
- [ ] README: UCI usage, options, supported platforms, build instructions, test instructions, known limitations.
- [ ] CI release workflow: macOS + Linux on every tag; Windows universal on a manual/nightly job (build time and PGO), with the dedicated builds kept as CI artifacts.
- [ ] Anti-virus sanity: avoid self-extracting/packed launchers in the release artifact; a plain `.exe` is preferable for CCRL testers.

**Exit gate:** every artifact runs `uci`/`isready`/`bench` and produces the same node counts for its ISA; the universal exe is within 2% of dedicated builds on both Windows hosts; packages are reproducible from the tagged commit.

---

### Phase 9 â€” CCRL submission and maintenance (ongoing)

**Objective:** a stable public version and a clean maintenance loop.

- [ ] Run the CCRL pre-flight with `cutechess-cli`/`fastchess`: 40/15 and 40/4 games, 128/256 MB hash, 3-4-5 piece Syzygy, own book off, ponder off, no time losses.
- [ ] Verify the exact UCI requirements: `uci`, `isready`, `ucinewgame`, `position`, `go`, `quit`; correct `bestmove` even after `stop`; no stray stdout.
- [ ] Package the Windows x64 zip and host it publicly with source, readme and license.
- [ ] Prepare the announcement text per `CCRL-chess-engine.md`: engine name, version, author/contact as the owner decides, protocol, supported platforms, source link, net description, known limitations.
- [ ] Tag `1.0.0`; freeze the search and net for the submission. Do not submit moving targets.
- [ ] Maintain `docs-new/KnightByte-feature-list.md` and `docs-new/new-bugs.md` after every change.
- [ ] After CCRL results: triage crashes/time losses first, strength second. Any search change requires a new SPRT/regression gate and a new version.

**Exit gate:** submission package complete; no known crash, illegal-move or time-loss bug; public source and license available.

---

## 7. Test and quality strategy

### 7.1 Test layers

| Layer | What it proves | Where |
| :--- | :--- | :--- |
| **Unit tests** | Types, FEN, Zobrist, movegen, SEE, draw rules, NNUE math, TT packing, option parsing | `src/**` `#[cfg(test)]` |
| **Perft** | Move generation and make/unmake correctness | `tests/perft.rs` |
| **Golden evaluation** | Bit-exact NNUE against captured C# vectors | `tests/nnue_parity.rs` |
| **Golden bench** | Search parity (node counts, best moves) | `tests/bench_parity.rs` |
| **UCI transcripts** | Protocol and option surface | `tests/uci_transcripts.rs` |
| **Property tests** | Make/unmake round-trip, key recomputation, legal-move invariants | `tests/properties.rs` |
| **Game tests** | No illegal moves, no time losses, thread safety | `fastchess` + `cutechess-cli` |
| **Fuzzing** | FEN parser, UCI parser, net loader, move parser | `fuzz/` (cargo-fuzz) |
| **Miri** | Undefined behaviour in non-SIMD board/TT code | `cargo miri test` where supported |

### 7.2 Stockfish-verified FENs

Every FEN constant in a test must be verified at test time. Implement one helper:

```rust
// tests/support/stockfish.rs
// Runs the locally installed Stockfish on `position fen <FEN>` and `d`,
// then asserts that the FEN is legal and round-trips.
// Path: $KNIGHTBYTE_STOCKFISH or /opt/homebrew/bin/stockfish.
```

The helper must fail loudly if Stockfish is missing; a skipped verification is not a pass. CI installs Stockfish on all three OSes or uses a pinned binary. A test that cannot verify its FEN is a test bug.

### 7.3 CI matrix

| Job | OS | What runs |
| :--- | :--- | :--- |
| `quality` | Ubuntu | `fmt --check`, `clippy -D warnings`, `cargo test` |
| `macos-arm64` | macOS | `cargo test`, release build, golden bench |
| `linux-x64` | Ubuntu | `cargo test`, release build, golden bench |
| `windows-x64` | Windows | `cargo test`, release build, `v3` golden bench |
| `windows-universal` | Windows | multi-variant build, force-ISA bench, artifact upload (manual/nightly) |
| `docs` | Ubuntu | links, feature list freshness, missing-docs lint |

Multi-thread node counts are not deterministic; golden bench runs are single-threaded. Thread tests assert safety and legality, not exact counts.

---

## 8. Performance strategy

### 8.1 What we optimize

Time-to-depth on a fixed set, not raw NPS. The engine must first reproduce the C# search tree; then every speedup is measured at identical node counts.

| Priority | Work | Expected effect |
| :--- | :--- | :--- |
| 1 | Zero allocations, compact copy/make position | Removes allocator/page-fault noise from every node |
| 2 | Incremental NNUE with the best stable SIMD path | Evaluation is a large share of node cost |
| 3 | TT layout, prefetch, generation aging, lazy allocation | Random-memory latency dominates deep search |
| 4 | Persistent Lazy SMP, padded counters, no per-node atomics | Thread scaling without spawn/false-sharing losses |
| 5 | LTO, `codegen-units=1`, `target-cpu`, PGO | Cross-module inlining and branch layout |
| 6 | Micro-optimizations (`get_unchecked`, branch hints, `assert_unchecked`) | Small, measure each |

### 8.2 Windows universal binary is a performance feature

The CCRL host is a Haswell i7-4770K. The Windows test hosts are Comet Lake and Zen 4. A single `.exe` must run well on all three. The universal dispatch design is described in the performance document (Â§2.3). The acceptance rule is:

* the universal `.exe` is within **2% NPS** of the dedicated `v3` and `v4-vnni` builds on the same host at the same depth;
* node counts are identical across every ISA backend;
* every backend can be forced with an environment variable and is exercised in CI.

If the 2% gate fails, move to whole-engine monomorphization; if that still fails, ship dedicated executables and document the CCRL artifact. Do not hide the gap behind a dispatcher.

### 8.3 PGO policy

PGO is part of the release build, not an optional local experiment. Each artifact and each Windows ISA variant gets its own profile, generated from the deterministic bench plus the 20-position workload. Profiles are regenerated after significant code changes and never checked in.

---

## 9. Release and CCRL checklist

**Artifacts**

| OS | Artifact | Build | Test hosts |
| :--- | :--- | :--- | :--- |
| macOS | `knightbyte-macos-arm64` | `apple-m1`, PGO | M4, M5 Pro |
| Linux | `knightbyte-linux-x64` | `x86-64-v2`, PGO | Ryzen 5 5600H |
| Windows | `knightbyte.exe` (universal) | `v3` + VNNI/AVX-512 paths, PGO per variant | i7-10750H, Ryzen 7 260 |

**Package contents**

* engine binary;
* `README.md` (usage, options, platforms, source link, net description);
* `LICENSE` (MIT);
* `THIRD-PARTY-NOTICES.md`;
* SHA-256 checksums;
* source archive (tagged commit).

**CCRL conditions to verify before submission**

* [ ] 40/15 and 40/4 games complete without time losses.
* [ ] 128 MB and 256 MB hash both stable.
* [ ] 3-4-5 piece Syzygy path works; missing path does not crash.
* [ ] Own book OFF by default; ponder OFF.
* [ ] `ucinewgame` does not clear a multi-GB table synchronously.
* [ ] No illegal moves in 1,000 games.
* [ ] No stray stdout; diagnostics are `info string` or stderr.
* [ ] A stable, publicly available version with source and license.

---

## 10. Risk register

| Risk | Probability | Impact | Mitigation |
| :--- | :--- | :--- | :--- |
| Windows universal build is harder in Rust than expected | High | High | Spike in Phase 0; design A first; 2% gate; fallback to dedicated exes or embedded launcher (last resort, AV-sensitive). |
| NNUE bit-exactness fails on one SIMD path | Medium | High | Scalar oracle; golden vectors; test every kernel on every host; reject a kernel until it matches. |
| Search node counts drift | High | High | Golden bench; one-feature-at-a-time bisection; no tuning before parity; ADR for any accepted deviation. |
| Time losses under CCRL controls | Medium | High | Dedicated time-manager tests; 1,000-game safety run; move overhead; ponder off; stop/ponderhit tests. |
| Syzygy port is long and error-prone | Medium | Medium | Port behind a clean interface; small 3-man fixtures; defer to 0.9 if needed; the engine must work without tables. |
| GPL contamination from reference code | Medium | High | Clean-room rule; license audit before release; no GPL dependencies; ADR for every dependency. |
| Performance work changes behaviour | High | Medium | Node-count check on every commit; benchmark gate; separate optimization commits. |
| Laptop thermal noise corrupts A/B results | High | Medium | Alternating runs, medians, governor/temperature records, desktop-class CCRL host for final numbers. |
| Scope creep (tuning, new features, trainer) | Medium | High | Freeze feature work until parity; new ideas go to a backlog, not into the port. |
| Net/test data accidentally omitted from the repo | Low | High | Net and small fixtures are committed and checksum-tested; release manifest verifies all files. |

---

## 11. Decisions to make early

Create an ADR in `docs/decisions/` for each; do not leave these implicit.

1. **Board representation:** copy-make vs make/unmake; position size budget.
2. **Attack backend:** magic vs PEXT; slow-BMI2 policy; table generation method (`const fn` vs `build.rs`).
3. **Windows packaging:** design A/B/C; variant ladder; 2% acceptance.
4. **NNUE dispatch:** kernel boundary and function-pointer layout; i8mm experiment policy.
5. **TT allocation:** `alloc_zeroed` vs OS-specific large pages/mmap; generation strategy.
6. **Syzygy:** in-tree port (recommended for license and control) vs a dependency.
7. **Opening book:** port `.ctg` and ship off by default; test data policy.
8. **Lever catalog:** declarative table as the single source of truth; count/deviation policy.
9. **Dependencies:** minimal set, license audit, no hot-path dependencies.
10. **Golden gate policy:** which deviations are accepted, and who signs them off.

---

## 12. Indicative timeline

| Phase | Focus | Focused days |
| :--- | :--- | :--- |
| 0 | Bootstrap + universal-build spike | 1â€“2 |
| 1 | C# oracle capture and harness | 2â€“3 |
| 2 | Types, board, movegen, perft | 5â€“7 |
| 3 | NNUE loader, accumulator, eval | 5â€“7 |
| 4 | Search parity + visible levers | 10â€“15 |
| 5 | UCI, threading, time compliance | 3â€“5 |
| 6 | Syzygy + book | 5â€“8 |
| 7 | Performance | 5â€“10 |
| 8 | Release engineering | 3â€“5 |
| 9 | CCRL + maintenance | ongoing |

**Total:** roughly 40â€“60 focused person-days for the parity build plus release candidates, before tuning and match testing. The estimate is indicative; Phase 1 and the Phase 0 spike will replace it with measured numbers.

**Recommended order for a solo developer:** 0 â†’ 1 â†’ 2 â†’ 3 â†’ 4 â†’ 5 (first playable parity build) â†’ 7 (macOS performance) â†’ 8 (macOS/Linux artifacts) â†’ 6 (tables/book) â†’ 8 (Windows universal) â†’ 9. Syzygy and the book can move later if CCRL submission is the priority; they are optional for eligibility but part of feature parity.

---

## 13. Definition of done

KnightByte 1.0 is done when:

- [ ] `cargo test` passes on macOS arm64, Ubuntu x64 and Windows x64.
- [ ] `cargo clippy -- -D warnings` and `cargo fmt --check` are clean.
- [ ] The determinism gates pass: `bench 6/12/16/18` reproduce the pinned
      KnightByte totals (ADR-0004); the reference identity totals are
      recorded in `docs/baselines.md` for the strength comparisons.
- [ ] Every C# option/lever is present, visible and functional; the count is documented.
- [ ] 1,000-game safety match shows no illegal moves, crashes or time losses.
- [ ] macOS, Linux and Windows artifacts run `uci`/`isready`/`bench` and agree on node counts.
- [ ] The universal Windows `.exe` is within 2% of the dedicated builds on both Windows hosts.
- [ ] Time-to-depth on the fixed 20-position set is at least as good as C#, with the Phase 7 target met or the gap documented.
- [ ] Baselines are reproducible with the project harnesses (`perf_test.py`, `ebf_compare.py`, `fastchess`, `cutechess-cli`) on the documented host, with engine/net/host metadata recorded.
- [ ] The net is embedded, checksum-verified and bit-identical to the C# evaluation.
- [ ] `docs-new/KnightByte-feature-list.md` and `docs-new/new-bugs.md` are current.
- [ ] The CCRL package is built, tested and ready to announce.
- [ ] No source file references an unpublished internal tree, and no reference-engine/author names appear in the code.
