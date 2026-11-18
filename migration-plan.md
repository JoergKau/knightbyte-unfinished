# KnightByte â€” Migration Plan: the predecessor C# engine â†’ Rust

**Status:** Phase 0 complete; Phase 1 oracle capture and Phase 2 perft are complete; Phase 3 scalar evaluation is bit-exact. SIMD kernels and the incremental accumulator are deferred to the performance phase. Phase 4 (search), including 4d time management and lever exposure, is complete: the search is node-identical to the reference (ADR-0005, which supersedes the ADR-0004 deviation). **PHASE 1 RESTART COMPLETE (2026-11-18).** Round 1: predecessor bugs 143 to 145 fixed in the predecessor (commit bffe98a, with unit tests and the full unit suite passing); bug 146 was not reproduced. Round 2: former quirks Q2, Q3 and Q4 were reviewed against the predecessor source, reclassified as predecessor bugs 147 to 149, and fixed in the predecessor (commit f274b39, regression tests `PredecessorBugs147to149Tests`, full suite 734/734; ADR-0008 records the decision). The Phase 1 restart then re-captured the oracle and adopted the fixes in KnightByte (`07b401c`): B35 (table-move markers kept), B36 (quiescence searches en-passant captures), B37 (correction quiet gate tests the opponent occupancy), B33/B34 (UCI position handling follows the predecessor's decision; the engine starts on the start position and `ucinewgame` resets to it). Node identity with the fixed reference holds at every captured depth and position: `bench 6/12/16/18` = **15,528 / 456,308 / 2,815,799 / 6,892,874** (the old pins 14,979 / 416,533 / 3,169,859 / 6,925,762 described the pre-fix build); verified with `tools/capture_bench.py compare`, all deltas zero. Q1 (singular extensions never fire) stays a reproduced quirk — ADR-0006 measured the fixed behaviour and found no gain. Phase 5 is planned as 14 ordered steps (0-13, see its section); steps 0-9 are complete (oracle transcripts, command surface and parser, option surface and `setoption` validation, `info` line completeness, the search thread with the stop flag, `go ponder`/`ponderhit`, `go mate`/`searchmoves`, the shared-tables refactor, the persistent Lazy SMP worker pool, and the multi-thread stress gate with the B39 fix). The step 0 capture immediately found **predecessor bug 150** (`setoption Hash` above the declared maximum crashed the reference via an overflowing `TTable.Resize` allocation; KnightByte not affected), the migration stopped per the project rules, and resumed the same day after the fix (predecessor commit `acaff1f`, full suite 745/745) and the transcript re-capture.
**Companion documents:** `project-rules.md`, `additional-nodes.md`, `maximizing_performance_in_a_rust_uci_chess_engine.md`  
**Source of behaviour:** the predecessor C# tree (kept in the migration folder outside this repository; during the migration everything inside the repository is published)  
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
* Copying code from the GPL reference engines. They are design references only. but their code can be a blueprint to write and optimize our code. 

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
| **Predecessor material during the migration** | The predecessor tree and the migration files live in the migration folder outside the repository. Everything inside the repository is published, and files may refer to the predecessor. The publishing policy for predecessor material is decided at the end of the migration. |
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

**Predecessor tree:** it lives in the migration folder outside the repository, not in the repository. Golden files used by the tests are committed as derived values, and each one is documented with how it was produced. The publishing policy for predecessor material is decided at the end of the migration.

**Crate policy:** keep dependencies close to zero. A chess engine needs no async runtime, no logging framework and no serialization framework in the hot path. Every dependency must be justified in an ADR, audited for license compatibility, and kept out of the search/evaluation path.

### 4.1 Cross-platform workflow

Primary development stays on the M4. The Rust code is portable; only the Windows packaging, ISA-specific optimization and PGO work need a Windows host.

* **Move code with git, not by copying the working directory.** The single remote is the public repository `https://github.com/JoergKau/knightbyte-unfinished`; clone and pull on every host (no private remote, no bundles). Never copy `target/`: it contains host-specific build artifacts and is ignored.
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
- [x] Record the initial golden values; the C# tree itself stays in the migration folder outside the repository.

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

**Progress:** 4a-4d complete, including the full time-management port
(allocation, panic mode, stability ladders, clock imbalance, minimum-depth
guard, per-move node attribution) and lever exposure (36 of the 83 reference levers exposed under their reference names; 47 are not yet exposed; the lever catalogue also holds three KnightByte-only research levers, see the Phase 5 policy). The
parity gate is **passed with exact node identity** (ADR-0005, 2026-11-17).
The first audit round (B1-B17) had closed the gate as a documented
deviation (ADR-0004); a second round, driven by per-node ledgers of both
engines, found that the residual was not learned-state drift but eleven
concrete porting differences (B18-B28) plus one predecessor quirk (Q2). The
Phase 1 restart (2026-11-18) re-captured the pins against the bug-fixed
reference: now `bench 6` = 15,528, `bench 12` = 456,308, `bench 16` =
2,815,799 and `bench 18` = 6,892,874, and every captured per-position count
matches. `tests/search.rs` asserts the `bench 6` total and the per-position
counts of depths 1-8; the audit record is `docs/audit/migration-audit.md`.

- [x] `board/zobrist.rs`: fixed-seed piece, castling, en-passant-file and side keys, incremental updates in `Position::make_move`, pawn and non-pawn sub-keys, key consistency across a perft tree and a pinned startpos value.
- [x] `search/tt.rs` (the C# `tt/` directory): lockless 16-byte entries, 64-byte buckets, exact-key verification, generation ageing, static-eval field, PV (`Exact`) flag, prefetch hook, power-of-two sizing, thread-safe sharing without locks.
- [x] `search/eval_cache.rs`, `search/pv.rs`.
- [x] `search/see.rs`: static exchange evaluation with x-ray recaptures, en passant and promotions; `see_ge` bound test for pruning.
- [x] `search/move_picker.rs`: TT move, captures gated by SEE and ordered by MVV-LVA, promotions, killers, countermoves, quiet ordering by history, losing captures last.
- [x] `search/history.rs`, `search/killers.rs`: butterfly, capture and one-ply continuation history, gravity bounds, ageing, countermove table, two killers per ply.
- [x] `search/history.rs` follow-ups: correction history and the longer 2/4/6-ply continuation chains.
- [x] `search/negamax.rs` (skeleton): iterative deepening, aspiration windows, PVS, TT probe/store, null-move pruning, RFP, razoring, LMP, futility, SEE pruning, LMR, check extensions with a path cap, fifty-move and twofold-repetition detection.
- [x] `search/negamax.rs` (parity pass): history pruning, singular extensions, negative extensions, internal iterative reduction, hindsight pair, re-search shaving, the reference LMR term set. ProbCut and quiescence checks stay unported (the reference ships both off).
- [x] `search/quiescence.rs`: check-aware, TT probe, delta pruning, SEE filtering, capture ordering.
- [x] `search/quiescence.rs` (parity pass): recapture/promotion narrowing (lever `UseQsNarrowing`, off by default, matching the reference).
- [x] `search/time_manager.rs` (full port): move-time and clock allocation with soft/hard budgets and move overhead, tournament ten-move cap, sudden-death increment, tactical floor, safety buffer, panic mode, the legacy PV-stability multipliers, the stable-TM three-signal scaler (best-move ladder, score ladder, node-fraction term), the five-iteration minimum-depth guard, one-sided time advantage, two-sided clock imbalance and per-move node attribution.
- [x] `search/options.rs`: declarative catalogue for the 12 public options and the 39 wired research levers; generates the handshake and drives `setoption`.
- [ ] Port the remaining levers (47 of 83 are not exposed yet). They gate reference features that ship off or are not ported (ProbCut, SEE-reach, LMR v2 and its terms, quiescence checks, book, tablebases); expose each one when its feature lands. A test that setting each lever changes the search on a probe position is still open. *(Update 2026-11-18, step 2: all 83 reference levers are now advertised — the 47 unported ones as validated, accepted, inert `wired: false` rows in `src/search/lever_table.rs` — so the handshake surface is complete; wiring a lever's feature remains the work item.)*
- [ ] Port the C# search tests: mate tests, PVS, LMR tables, futility, history, singular, extension cap, TT, root TT cutoff, repetition, time compliance, lever independence.
- [ ] Build `docs/SEARCH-PARITY.md`: every C# search feature, its Rust module, its lever name, its test.
- [x] Deterministic `bench [depth]` and `go depth|nodes`, with search tests (mate in one/two, playable PV, reproducible node totals, node limit).

**Exit gate G3 + G4 (search part):** **passed** with exact node identity
(ADR-0005, superseding ADR-0004), re-established at the Phase 1 restart
(2026-11-18) against the **bug-fixed predecessor build** (commits bffe98a and
f274b39): `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874 and
every captured per-position count identical (`tools/capture_bench.py compare`,
all deltas zero). The first identity pass ran against the pre-fix build
(14,979 / 416,533 / 3,169,859 / 6,925,762); predecessor bugs 147-149 (the
reclassified former quirks Q2-Q4) changed the reference tree, and KnightByte
adopted the fixes as B35-B37 together with the re-pin (ADR-0008). Q1 remains a
reproduced quirk; ADR-0006 measured the fixed behaviour and found no gain.
Predecessor behaviour that looks wrong is first recorded in the quirk register,
and reclassified as a bug only with code evidence against intentionality, as
happened for Q2-Q4.

> If node counts change after a search change, it is a search change: update
> `tests/search.rs`, `tests/golden/bench.json` and `docs/baselines.md` in the
> same commit and record the reason. Identity with the predecessor is the
> regression discipline again.

---

### Phase 5 â€” UCI, threading, time compliance (3â€“5 days)

**Objective:** a robust, tournament-safe UCI engine.

**Predecessor alignment (decided)**

- **Reference:** the predecessor's publication folder and its local files, kept in a separate migration folder outside this repository (`C:\dev\Migration-Project`). It is a plain folder with no history of its own. It is the specification for behaviour.
- **Quirks:** reproduced now and recorded in a local, unpublished register. Each one is checked again after the migration (`project-rules.md`). Updated 2026-11-18: Q2, Q3 and Q4 were reclassified as predecessor bugs 147-149 and fixed in the predecessor (commit f274b39); KnightByte inherits them as bugs B35-B37 (`docs-new/new-bugs.md`) and stops reproducing them at the restart. They are removed from the quirk register.
- **Bugs (decided 2026-11-18):** the migration stops. The predecessor bugs are fixed in the predecessor first, then the migration resumes. Reproduced: bug 143 (`go` after `ucinewgame` without a position prints nothing), bug 144 (a rejected `position` keeps the previous position), bug 145 (an illegal move in `position` leaves a partial position). Unverified: bug 146 (helper threads may outlive the search). The details and reproduction commands are in the predecessor bug log in the migration folder. Fixed in the predecessor at commit bffe98a: a rejected `position` keeps the previous position with an error line (the loud rejection chosen over Stockfish's fatal exit), and `ucinewgame` resets to the start position. Bug 146 was not reproduced. Fixed at commit f274b39: bugs 147-149, the reclassified former quirks Q2-Q4 (TT move flag loss, quiescence skipping en-passant captures, the correction-history quiet gate testing the wrong occupancy). KnightByte adopted all of these at the Phase 1 restart (`07b401c`); the KnightByte-side entries are B33-B37, all fixed.
- **No new deviations in Phase 5.** Any KnightByte-only behaviour is listed in the feature list and approved first. Ideas that are worth doing after the migration, such as the helper skip table, are recorded in `docs-new/enhancements-for-knightbyte.md` and are not carried out during the migration.
- **Levers:** all 83 reference levers are exposed with the reference names, defaults and ranges. Three KnightByte-only research levers stay (`UseCorrectionHistory`, `UseCheckAwareQuiescence`, `UseSeeApproxPrefilter`). They switch off features the reference always runs and are kept for later ablation tests. They are documented as KnightByte-only and excluded from the reference option-set check. `SingularExcludeTtCutoff` was removed in step 2 (2026-11-18).

**Revised estimate:** 10-12 focused days. The 3-5 day estimate in the heading is superseded: it did not include the search-thread refactor, the shared-table refactor, the oracle transcripts or the multi-thread stress gates.

**Ordering principle:** the steps follow dependencies and risk. First the oracle (what the reference actually does), then the protocol surface without threads, then single-thread search control (stop, ponder), then shared state and Lazy SMP, then the closing tests and match gates. Each step ends with:

- the quality gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets` with `KNIGHTBYTE_FEN_VERIFIER` set;
- the bench pins for any step that touches search or threads: `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874 (single thread, defaults; the bug-fixed reference build, ADR-0008);
- the documentation update required by the project rules (this checklist, `docs-new/KnightByte-feature-list.md`, and `docs-new/new-bugs.md` for any defect found). The next step does not start before this update is done.

**Invariants for every step**

- Protocol changes are not search changes. Node counts move only where a step says so explicitly.
- After step 4, no clock read and no atomic access happens per node. Time and stop checks run every 1024 nodes; the exact `go nodes` limit stays a per-node integer comparison.
- Test FENs are verified against Stockfish at test time, as before.

**Step 0 - Oracle transcripts (no Rust changes, about 0.5 day) - captured 2026-11-18; the capture found predecessor bug 150, fixed the same day (predecessor commit `acaff1f`), transcripts re-captured against the fixed build**

Capture the reference engine's behaviour before writing protocol code. Cover: `go` with every limiter (`depth`, `nodes`, `movetime`, clocks, `movestogo`, `infinite`, `ponder`, `mate`, `searchmoves`); `stop` during a search and after one; `ponderhit` after `go ponder`; `ucinewgame`; `setoption` with valid, out-of-range, unknown and button values; `isready` during a search.

Output: derived transcripts in `tests/golden/transcripts/` (11 files, produced by the extended `tools/oracle_capture.py` with `--transcripts-only`; the reference binary was rebuilt at `f274b39` and re-verified against the bench pins before the capture). The files are published, so the reference's identity lines are replaced by neutral placeholders and the transcripts contain no reference names, paths or source excerpts. Commands are recorded as `> ` lines; `# ` lines are derived measurements; `time`, `nps`, `hashfull`, `tbhits` and (where noted) `nodes` values are masked.

Exit: each open question below has an answer. Answers from the capture (2026-11-18):

1. **`stop` during depth 1** — yes to both: the reference prints `info depth 0 score cp <static-eval> pv <first-legal-move>` followed by `bestmove`. A `stop` during a deeper search adds NO new `info` line; `bestmove` carries the last completed iteration's move. A `stop` with no search and after a finished search prints nothing.
2. **`ponderhit` and the clock** — the clock starts at the `ponderhit` moment (measured: 0.38 s from `ponderhit` to `bestmove` on a stored 5 s clock; the 0.6 s spent pondering was not deducted). On `ponderhit` the reference starts a fresh search of the position WITH the ponder move applied: iteration output restarts at depth 1 from the opponent's view. A `stop` without `ponderhit` ends a ponder search with `bestmove 0000` (quirk Q5 confirmed). A stray `ponderhit` with no search is ignored.
3. **`mate` and `searchmoves`** — both implemented. `go mate N` stops at the first completed iteration that proves a mate in at most N (`go mate 1` on a mate-in-one position: depth 1, `score mate 1 pv a1a8`); with no provable mate it searches on without any depth cap until stopped. `go searchmoves` restricts the root moves (node counts collapse accordingly); `searchmoves` with no moves is ignored and the search runs normally.
4. **Which thread reports with `Threads > 1`** — the reporting thread's own search, not an aggregate: single-thread depth 12 is 305,691 nodes, the `Threads=4` run reported 465,765 with a different intermediate PV (multi-thread search is nondeterministic; only the main thread's `bestmove` and `info` lines appear).
5. **`isready` during a search** — answered at once (`readyok arrived after 0.001 s`), preceded by the re-stated `info string eval-source: …` line (P6).

Additional observations: the reference emits `info currmove … currmovenumber … ` lines during iteration 1 (KnightByte lacks them; see 4.2.2); every `go` limiter is accepted in any order; `go perft N` and `movestogo` alone (depth 6) behave as the plan's step 1 describes; a rejected or unusual `setoption` prints nothing (no error `info string`) — except the crash below.

**The capture found a new predecessor bug (2026-11-18), since fixed.** `setoption name Hash value 999999` — a spin value above the option's declared maximum — killed the reference with native heap corruption (0xC0000374 / 0xC0000005 in `TTable.Resize`), with a delayed, wandering manifestation. Root cause and fix are in the predecessor bug log as **bug 150**: the Hash callback now clamps into the declared range (`EngineOrchestrator.ClampHashSize`), and `TTable.Resize`/the constructor validate their argument before touching state (predecessor commit `acaff1f`, regression tests `PredecessorBug150Tests`, full suite 745/745, bench identity unchanged). KnightByte was NOT affected (spin values are clamped at the option layer and the resize is clamped to 1..1024), verified 2026-11-18. All transcripts were re-captured against the fixed build; the setoption transcript now records ten surviving validation scenarios instead of the crash. The migration resumed with step 1.

**Step 1 - Command surface and parser (about 0.5 day) - complete 2026-11-18**

Recognise every UCI command, including `stop` and `ponderhit`. Command words are matched case-insensitively (accepting more input never rejects a valid line; the old doc comment in `uci/parser.rs` that calls matching case-sensitive is updated with this step). Parse `go` into one structure holding all limiters (`depth`, `nodes`, `movetime`, clocks, `movestogo`, `mate`, `infinite`, `ponder`, `searchmoves`). Match the reference's `go` parsing: unknown tokens are ignored, a `go` with no limiter searches depth 6, `movestogo` alone also searches depth 6, and `go perft N` is supported. Add the reference's diagnostic commands (`debug`, `d`, `eval`, `nnueregions`, `searchstats`, `checkmoves`) and its argv handling (`param:Name=Value` overrides and the one-shot command line). Until their steps land, `ponder`, `mate` and `searchmoves` keep today's fallback: a legal `bestmove` and an `info string`. The `go`-without-position case is resolved: bug 143's fix (start position at startup and at `ucinewgame`) is adopted, so a bare `go` searches the start position.

Exit: parser table tests for every token; no change in replies for the existing commands. **Done:** the parser table test walks all 18 command words, the `go` parser is unit-tested against the reference's rules (default depth, unknown tokens, unparsable values, case rules, `searchmoves` consumption, `perft`), the UCI smoke suite gained end-to-end tests for the diagnostics, `go perft`, the bare-`go` depth, the argv one-shot and the fail-fast `param:` override, and the old unsupported-limiter test was updated (unknown `go` tokens are now ignored, matching the reference). Recorded deviations: `go perft` prints no `bestmove` (the reference's behaviour, verified in the step-0 capture); `nnueregions` and `searchstats` are recognised and answer honestly that the instrumentation is not ported yet — the king-region histogram and the search instrumentation land with their features; `go ponder/mate/searchmoves` keep the step-1 fallback. The bench pins are unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874), and the reply formats of the existing commands are unchanged except the unknown-command line, which now matches the reference (`info string Unknown command: <word>`).

**Step 2 - Option surface and `setoption` validation (about 0.5 day) - complete 2026-11-18**

Validate every `setoption` against the catalogue. Unknown names give an `info string`. Spin values outside min/max are rejected and the previous value is kept. `check` accepts only `true`/`false`. Buttons act without a value. `Threads` is validated (1-256) but still searches on one thread until step 8.

Measure `Clear Hash` / `ucinewgame` at `Hash 1024`. If the synchronous clear takes more than about 100 ms, log it in `docs-new/new-bugs.md` and fix it in step 7.

Remove `SingularExcludeTtCutoff` (Q1 is reproduced; ADR-0006 records that the lever gave no gain). Remove the three-lever exemption list from the option check; `UseCorrectionHistory`, `UseCheckAwareQuiescence` and `UseSeeApproxPrefilter` stay and are marked KnightByte-only. The handshake also prints the reference's book, tablebase and eval-source `info string` lines.

Exit: the handshake matches the step-0 transcript (after the placeholder rule) and the lever golden file, which lists 83 reference levers plus the three KnightByte-only research levers; one test per validation rule. **Done:**

- The lever catalogue moved to `src/search/lever_table.rs` and now lists **all 83 reference levers** with the golden file's names, types, defaults and ranges, plus the three KnightByte-only levers (marked in `KNIGHTBYTE_ONLY`). The 47 levers whose features are not ported yet carry `wired: false`: they validate, are accepted, and stay inert (`get` reports the recorded default, `set` stores nothing). A golden-file test (`the_catalogue_matches_the_reference_lever_file`) compares every catalogue entry against `tests/golden/levers.json`, and a second test (`the_only_extra_levers_are_the_knightbyte_only_three`) is the old three-lever exemption's replacement: the catalogue may not hold any name outside the 83 plus the marked three.
- `setoption` validation is catalogue-driven (`apply_set_option` in `src/uci/mod.rs`), with one unit test per rule: unknown name → `info string Unknown option: <name>`; out-of-range, unparsable or missing spin values and non-boolean check values are rejected silently with the previous value kept; the `Clear Hash` button acts without a value; `Threads` validates 1-256 and stays single-threaded; the inert public names (book, tablebase, `EvalFile`, `Contempt`) are accepted silently but still validated. An end-to-end test replays the step-0 setoption transcript and asserts the ten scenarios produce exactly the transcript's one line.
- The handshake prints the reference's closing `info string` lines (`no opening book loaded`, `syzygy tablebases: none (set SyzygyPath)`, `eval-source: nnue net jokausch.nnue`) before `uciok`, and every `isready` restates the eval-source line before `readyok`. A smoke test walks the step-0 handshake transcript as an ordered subsequence of the engine's reply (the research-lever option lines are KnightByte's documented superset in between). String options print their (possibly empty) `default`, buttons print none — matching the transcript byte for byte.
- `SingularExcludeTtCutoff` is removed: the catalogue row, the `SearchParams` field and the verification-skip branch in `src/search/negamax.rs` (the comment there records why the cutoff stays taken — Q1 is reproduced parity, ADR-0006). The default was off, so the node pins are unchanged. The lever-based test in `tests/search.rs` is removed with it.
- The step-2 measurement: steady-state `Clear Hash`/`ucinewgame` at `Hash 1024` cost ~45-55 ms (inside the ~100 ms threshold), but the **first** clear after a fresh 1 GB resize pays the OS page commit (~190-275 ms, measured with the new `tools/time_clear_hash.py`). Logged as **B38** in `docs-new/new-bugs.md` (a performance note, not a correctness defect); step 7's bounded `ucinewgame` covers the fix, and the re-measure belongs there.
- Verification: all 218 tests pass with the Stockfish verifier set, fmt/clippy clean, and the bench pins are unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874).

**Step 3 - `info` line completeness (about 0.5 day) - complete 2026-11-18**

Every `info` line from an iteration carries `depth`, `seldepth`, `score`, `nodes`, `nps`, `time`, `hashfull`, `tbhits 0`, and `pv`. `nps` is guarded against a zero time. `hashfull` is read once per iteration, not per node. The `bench` output format does not change.

Exit: format tests for cp and mate scores, `nps` arithmetic and the zero-time guard. **Done:**

- `SearchInfo` grew the fields `time_ms` (measured from the start of `search()`, so every search reports from its `go` regardless of whether a time manager was initialised), `nps` and `hashfull` (`src/search/searcher.rs`). The occupancy is read once per completed iteration at the observer call, not per node.
- `nps_of(nodes, time_ms)` treats a zero elapsed time as one millisecond, so a completed iteration is never reported as zero nps and nothing divides by zero.
- `format_info` prints the captured reference order exactly: `depth seldepth score nodes nps hashfull time pv`.
- **Plan deviation resolved against the oracle:** the plan text listed `tbhits 0` in the line, but the step-0 capture masks `tbhits <n>` when present and no captured line carries one — the reference omits the token while no tablebases are configured. The transcripts win; KnightByte omits `tbhits` the same way and adds it (when nonzero) with the Syzygy phase.
- **Observation recorded for step 12:** with identical score and node count, the reference's depth-2 PV tail on the start position reads `e2e4 c7c5` while KnightByte reports `e2e4 c7c6` — the first PV move and the search are identical (node identity holds at every depth); the tail is derived reporting that the reference appears to refresh from the table between iterations. Full line-by-line transcript comparison (including `pv` fields) is step 12's task; the difference is noted here so it is not rediscovered there.
- Tests: cp/mate score formats (both signs, odd/even ply rounding, the mate-limit boundary), `nps` arithmetic, the zero-time guard, and an exact full-line format test for the captured field order. The bench output format is untouched (`bench` prints its own summary lines and does not use `format_info`). All gates pass; bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874).

**Step 4 - Search thread, stop flag, and removal of the per-node clock read (about 1.5 days) - complete 2026-11-18**

This is the core change of the phase.

- `go` runs the search on a worker thread with an explicit 16 MB stack (the Windows main thread has 1 MB, and the recursion depth must not depend on the platform; the performance audit recommends 16 MB). The `Searcher` moves into the worker and comes back on join, so history and other persistent tables survive between searches.
- The UCI loop keeps reading stdin during a search. `stop` sets a shared `AtomicBool`. `quit` stops, joins and exits. `isready` answers at once (as the Step 0 transcripts show).
- Commands that touch the searcher or the position (`position`, `go`, `setoption`, `ucinewgame`, `bench`, `perft`) first stop and join a running search. This is a fallback for GUIs that break the protocol; it must never print a second `bestmove`.
- The stop check runs every 1024 nodes. `Searcher::should_stop` currently calls `now_ms()` on every node while not stopped (`src/search/searcher.rs`, around line 484); this step removes that clock read.
- Match the reference: the stop flag is checked before each iteration and inside the search. If `stop` arrives before depth 1 finishes, the reference returns the first legal move with no `info` line; KnightByte does the same. Note that this replaces an earlier draft rule that depth 1 always completes; that rule was not the reference's behaviour.
- A single legal move is returned at once with `info depth 0 score cp X pv move` (not while pondering), as the reference does (quirk Q8).
- Exactly one `bestmove` per `go`, printed by the worker after its last `info` line.

Tests: `go infinite` then `stop` gives a `bestmove` within 100 ms; 200 go/stop cycles at depths 1-6 give exactly one `bestmove` each; `quit` during a search exits cleanly; bench pins unchanged. **Done:**

- The worker runs in `handle_go` (`src/uci/mod.rs`): a fresh stop flag per `go` (`Searcher::arm_stop_flag`), the searcher moves into a 16 MB `Builder::spawn` closure and returns through the join handle. The worker owns its own `Output` writer (both writers lock standard output per line, so replies never interleave mid-line); the first failed write sets the stop flag, so a GUI that walks away cannot leave the search spinning. A spawn failure is fatal and propagates as an I/O error — the moved searcher cannot be recovered.
- `should_stop` now reads the clock and the flag only every 1024 nodes; the exact `go nodes` limit stays a per-node integer comparison. The flag is also checked before each iteration, so a `stop` between iterations never starts a new one.
- `quit`, `stop`, end of input, and every state-touching command (`position`, `go`, `setoption`, `ucinewgame`, `bench`, `perft`) go through `stop_search`: set the flag, join, take the searcher back. Exactly one `bestmove` per `go` (the worker's), never a second one from the join.
- **Draft rule corrected against the oracle:** the step text said a `stop` before depth 1 finishes returns the first legal move with *no* `info` line, but the step-0 capture (stop.txt, scenario b) shows the reference answering `info depth 0 score cp 27 pv a2a3` + `bestmove a2a3`. The capture wins: KnightByte prints the depth-0 line with the static evaluation and the first legal move whenever no iteration completed. **Verified against the reference binary itself** (2026-11-18, publish build 15,528-bench): a burst `go depth 6` + `quit` and the one-shot argv run `go depth 6` both produce exactly this answer there too — the reference's `quit` also stops a running search, and so does KnightByte's.
- Q8 implemented as specified: exactly one legal move → `info depth 0 score cp X pv move` + `bestmove`, no search, never while pondering (the ponder fallback still answers as in step 1 until step 5).
- Tests (all in `tests/uci_smoke.rs`): `go infinite` + `stop` answers inside 100 ms (a timestamped line reader measures stop→bestmove); 200 go/stop cycles over depths 1-6 answer exactly 200 bestmoves; `quit` during a deep search exits cleanly with one bestmove; `isready` during a search gets `readyok` at once; the Q8 fast path on the Stockfish-verified FEN `7k/8/8/8/8/1p6/r7/K7 w - - 0 1`; the stop-during-depth-1 depth-0 answer; the join-first fallback (`position` during a search applies after the join, one bestmove total). The former burst-style tests were reworked to the interactive pattern (wait for `bestmove`, then `quit`) — under the reference's own semantics a burst script's `quit` stops the search, which the reference binary confirms.
- Verification: 228 tests pass with the Stockfish verifier set, fmt/clippy clean, all four bench pins exact (15,528 / 456,308 / 2,815,799 / 6,892,874) and `tools/capture_bench.py compare` at depths 1-12, 14, 16, 18 shows every delta at +0.

**Step 5 - `ponder` and `ponderhit` (about 0.5 day, after step 4) - complete 2026-11-18**

`go ponder` searches with no time limit until `ponderhit` or `stop`. On `ponderhit` the time manager starts from the clock values stored with the `go` (behaviour as recorded in step 0). `stop` ends a ponder search with `bestmove 0000` when `ponderhit` has not arrived (the reference's behaviour, quirk Q5). A `ponderhit` with no active ponder search is ignored. The ponder flag is an `AtomicBool` read with the step-4 stop check.

The `Ponder` option is not advertised (CCRL requires ponder off).

Exit: transcript structure matches step 0; a test starts the clock at `ponderhit` with a fixed clock. **Done:**

- `go ponder` (real, replacing the step-1 fallback): the search runs unbounded (the clock limiters are stored with the job, `init_infinite` while pondering; depth and node limiters still apply) and prints its iteration lines like any search. The worker answers no `bestmove` itself — the ending belongs to the loop. When a ponder search ends without a completed iteration it still prints the interrupted-search depth-0 line, exactly like the reference (verified against the reference binary: a burst `go ponder` + `quit` prints `info depth 0 score cp 27 pv a2a3` + `bestmove 0000` there too).
- `stop` without `ponderhit` → `bestmove 0000` (Q5); the same answer covers the join-first fallback hitting a ponder search. Verified byte-identically against the reference binary for the burst `go ponder` + `quit` case.
- `ponderhit`: the ponder worker is joined silently, its best move (the last completed iteration's PV move) is applied to the position, the game history grows by one position, and a fresh normal search starts — restart at depth 1 from the opponent's view — with the clock beginning at the ponderhit moment (the stored limiters run from there, the pondered time is not deducted). **Verified against the reference binary:** with zero completed ponder iterations the reference re-searches the unchanged position (a burst `go ponder` + `ponderhit` + `quit` prints two depth-0 lines and answers the first legal move) — KnightByte reproduces exactly that.
- A stray `ponderhit` (no search) is ignored, and so is one arriving during a normal search (decision; only scenario c is captured). The `Ponder` option stays unadvertised.
- Tests: the Q5 null-move answer after a 400 ms ponder phase; the fixed-clock test (`go ponder movetime 300` — nothing answers during a 700 ms ponder phase, the fresh search answers inside the movetime that started at `ponderhit`, two depth-1 lines prove the restart, exactly one `bestmove`); the stray `ponderhit` silence; the `ponderhit` during a normal search. The stop-latency budget scales with the build (`cfg!(debug_assertions)`): the plan's 100 ms is the release requirement — the release binary answers in 0.1-3 ms, while the debug build needs its ~100× slower 1024-node poll window.
- Verification: 233 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874).

**Step 6 - `go mate N` and `go searchmoves` (about 0.5 day) - complete 2026-11-18**

`mate N` stops at the first completed iteration that proves a mate in at most N moves; other limits still apply. `searchmoves` restricts the root moves in `search_root` and has no effect when absent. Both follow the step-0 captures. If the reference does not implement one of them, KnightByte answers it the same way and records that in the feature list.

Exit: tests for both; bench pins unchanged. **Done:**

- `Limits` grew `mate: Option<u32>` and `root_moves: Vec<Move>` (the struct dropped `Copy` for the move list). `go mate N` stops at the first completed iteration proving a mate within N moves; a slower proven mate keeps searching (the step-0 capture, scenario b: with no provable mate in N the search runs on without a depth cap until stopped), and being mated always ends the search. Other limiters still apply (`go mate 1` on the capture's mate-in-one FEN stops at depth 1 with `score mate 1 nodes 43 pv a1a8` — node-identical to the reference; the clock or `movetime` case reuses the step-4/5 machinery). With no mate limiter the old any-proven-mate break is unchanged.
- `go searchmoves` restricts the root to the listed moves that are legal in the root position (`search()` builds the filter once, negamax applies it at every root node; the search itself is untouched). **Decoded against the reference binary:** the capture's collapsed node counts (3/5/7/… for one listed move) are a warm-TT artifact of the sequential capture — the reference's cold restricted run (nodes 3/53/199/584/1448, scores 45/33/21/43/32, seldepth 1/2/4/5/8) is **exactly** KnightByte's, so the restricted search is a normal search over a restricted root. An empty `searchmoves` and a list whose every entry is illegal both run the unrestricted search (probed on the reference for the illegal-only case: it ignores the list — nodes 410,713 ≈ the unrestricted 410,719). The Q8 single-legal-move fast path stays on the position's raw legal count, before any filter.
- Every `go` limiter is now implemented — the step-1 fallback (`answer_without_search`) only remains for the impossible no-position state, and the old unsupported-limiter smoke test was removed with it.
- Tests: the mate-in-one pin (depth 1, 43 nodes, `pv a1a8`), the unbounded mate search (running, no bestmove until the stop, which answers a real bestmove), the restricted root (pinned to the reference's cold run), the two-move list (the answer stays inside the list, the tree stays below 4,001 nodes), and empty/illegal-only lists (the unrestricted search, pinned at 4,001 nodes); the unbounded-mate and filter-resolution rules are pinned deterministically at the library level (`a_mate_limiter_has_no_depth_cap`, `searchmoves_resolves_legal_moves_into_the_root_filter`).
- Verification: 239 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874) and `capture_bench.py compare` all deltas +0.

**Step 7 - Shared tables refactor (about 0.5 day) - complete 2026-11-18**

Move the transposition table and the evaluation cache behind a shared handle that every search thread can reference. `resize` and `clear` run only when no search is active, which step 4's join rule guarantees. Make `ucinewgame` cost bounded (generation bump or parallel zeroing), as step 2's measurement requires.

Exit: pure refactor for `Threads=1`; bench pins and the existing TT tests (resize, ageing) unchanged. **Done:**

- New `src/search/tables.rs`: `SharedTables` wraps both tables in `Arc`s. Both tables were already lock-free inside (probe/store/clear/new_generation all take `&self` and use atomics — `tt.rs` was designed for this), so the handle is exactly what step 8's workers will clone for the duration of a search and drop at join; a resize always then finds the table unshared and takes the exclusive borrow (`Arc::get_mut`, the panicking invariant documented). The `Searcher` owns one `SharedTables`; all `self.tt`/`self.eval_cache` uses go through `self.tables`.
- B38 fixed: `TranspositionTable::clear` zeroes in parallel (`std::thread::scope` over bucket chunks) above 64 MB. The first clear after a fresh 1 GB resize — which pays the operating system's page commit on first touch — dropped from ~190-275 ms to **59-64 ms** (`tools/time_clear_hash.py`, three runs: 64.1/62.6/63.6 ms first-clear, ~59 ms steady state), inside the step-2 threshold of about 100 ms. `docs-new/new-bugs.md` B38 marked fixed.
- Verification: pure refactor — 239 tests pass (the existing TT resize/ageing tests untouched), fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874) and `capture_bench.py compare` all deltas +0.

**Step 8 - Persistent workers and `Threads` (about 2 days, the Lazy SMP core) - complete 2026-11-18**

- A pool of N-1 helper threads, started when `Threads > 1` and kept between searches. Each helper owns its own search state (history, correction, killers, PV, stacks, picker). Helpers must persist: history tables survive between searches, so rebuilding them per search would change the search, not only its speed.
- Shared: the transposition table and evaluation cache from step 7, the read-only net, and the stop flag. Helpers stop when the main thread stops.
- Only the main thread runs the time manager, prints `info`, and prints `bestmove`. Its result is the answer, as recorded in step 0.
- Node counts: each thread publishes its count to a shared counter in blocks of 2048 nodes (no per-node atomic). `go nodes N` and the reported `nodes` use the aggregate.
- `setoption Threads` resizes the pool without rebuilding the main thread's state; surplus helpers are joined.
- `Threads=1` runs the step 4-7 path unchanged, with no helpers.

Exit: `Threads=1` bench pins unchanged. Multi-thread node counts are not compared (non-deterministic by design, plan 7.3). **Done:**

- New `src/search/pool.rs`: the `WorkerPool` holds `Threads - 1` persistent helpers, each a 16 MB thread with its own `Searcher` (own history, correction, killers, PV, stacks, picker — 1 MB private table, unused; the searches run on the shared handle) blocking on a command channel. `set_threads` grows the pool by spawning helpers and shrinks it by sending `Quit` and joining the surplus; the surviving helpers keep their learned tables between searches.
- A search hands every helper a `SearchJob`: the same root position, the same limits minus the time budgets (only the main thread runs the time manager), the main searcher's current `SearchParams` (so `setoption` levers reach the helpers every search), a `SharedTables` clone (step 7), the main search's stop flag, and a shared node counter. The helper adopts flag and tables, runs its own iterative deepening from depth 1 (differing only by its own history — the step-9 model), publishes its node count in 2048-node blocks from the 1024-node poll (never per node) with a forced final flush, and waits again.
- Only the main thread reports: its info lines and result report `reported_nodes()` — its own exact count plus the helpers' published blocks. With helpers attached the `go nodes` limit is enforced on the aggregate at the 1024-node poll; the per-node integer check keeps the `Threads=1` behaviour bit-exact. The main worker sets the stop flag and detaches the counter when its search returns, so the helpers end their iteration and return to waiting; the helpers are shut down on `quit` and at end of input.
- Measured on the Windows host (release, depth 12 startpos): `Threads=1` reports the exact pin (305,691 local nodes, ~0.37 MNPS); `Threads=4` runs ~0.47 s at ~790K aggregate NPS (2.1× single-threaded), with nondeterministic aggregates across runs (340,925 / 361,806 nodes) — multi-thread counts are not compared, per the plan.
- Tests: pool unit tests (grow/shrink cycles keep the requested size and join; a job hands off and the pool survives a `stop`), the `Threads` setoption arm reaches the pool, and a smoke test runs searches at `Threads=4 → 2 → 1` with a resize between every search — three searches, three real bestmoves, no panic. `Threads=1` pins and the parity compare are unchanged.
- Verification: 244 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874) and `capture_bench.py compare` all deltas +0.

**Step 9 - Helper diversity and multi-thread safety (about 1 day) - complete 2026-11-18**

Helpers search the same iterations as the main thread, as in the reference; they start at depth 1 and differ only by their own history. A skip table for helpers is a deviation from the reference and is not part of Phase 5. It can be tested later as a Phase 7 experiment, with its own SPRT.

Stress tests with 1, 2, 4 and 8 threads, 500 random `go`/`stop`/`position`/`setoption` cycles: no panic, every `bestmove` legal, exactly one `bestmove` per `go`, every TT probe key verified (the existing exact-key check). Node counts are not asserted. **Done:**

- Helper diversity was already the step-8 design (own persistent search state, iterative deepening from depth 1, no skip table); nothing to change.
- New `tests/thread_stress.rs`: a seeded, replayable walk of 500 random cycles (`position start|kiwipete`, `go depth 1-4`, `stop`, `setoption Threads 1/2/4/8` + `Hash` + a lever, `isready`) driving the real binary. The walk mirrors the current position through the engine library and checks every `bestmove` for legality in it. Instrumentation built in from the start: per-search bounded waits (a dead or stuck engine fails at the offending cycle, not later), a snapshot-based bestmove wait (a cumulative wait once passed on stale answers — a test-side defect, fixed), and the engine's stderr captured to a file so a panic that kills the engine names itself.
- **The walk found and fixed a real defect (B39, `docs-new/new-bugs.md`):** `setoption Hash` after a helper search killed the engine — the persistent helpers held their `SharedTables` clones forever (blocking the resize's exclusive `Arc::get_mut`), and a *queued* job could still hold clones during the helpers' drain window. Fixed in three layers: the helper releases the tables when its job ends, `stop_search` waits on the pool's idle barrier, and the resize waits bounded instead of failing instantly. **The reference cannot have this bug** (its .NET references never constrain a resize); probed for the record — the reference survives a `Hash` resize sent mid-search with `Threads 4` attached and never even stops the search.
- Verification: 247 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874).

**Step 10 - Output decision (about 0.25 day)**

The bounded-channel writer thread from the checklist is needed only if output can block the search. One `info` line per iteration cannot block it, so the synchronous writer stays for now. The decision, with the measured output time per search from step 9, is recorded in `docs/decisions/ADR-0007`. `info currmove` is added only if the step-0 transcripts show the reference emits it.

**Step 11 - Lever independence (about 0.5 day)**

Closes the Phase 4d open item: a probe-position test that each of the 39 wired levers changes the search (the C# suite uses the same pattern). A lever that changes nothing is logged in `docs-new/new-bugs.md`.

**Step 12 - UCI transcript tests (about 0.5 day)**

Compare the engine's transcripts for handshake, `position`, `go`, `stop`, `ponderhit`, `ucinewgame`, `setoption` and `bench` with the step-0 captures. Compare the order and structure of lines, the option sets, and the legality of every `bestmove`. Do not compare timing fields or `info` values; node counts are compared only at fixed depth with `Threads=1`.

**Step 13 - Match gates (about 1 day of work, plus background match time)**

- Smoke: short matches with fastchess or cutechess-cli at 1, 2, 4 and 8 threads against the C# engine: no illegal moves, crashes, time losses or stray stdout; clean `quit` and `stop` handling.
- G5: 1,000 games against the C# engine on the Windows host, at the ADR-0006 control (8+0.08). Read the `Timeouts` and `Crashed` columns, not only the exit code. Runs in the background; the phase does not close until it finishes.

**Dependency summary:** 0 -> 1 -> 2 -> 3 (protocol, no threads); 4 -> 5 -> 6 (single-thread search control); 7 -> 8 -> 9 (threads); 10-12 (closing); 13 (gates). Steps 1-3 are protocol-only and low risk, so they go first; they give step 4 a tested parser and option layer to build on. Step 4 is the biggest risk and the first change to the search loop.

**Phase 4 carry-over (required for 100 % feature compliance, not yet scheduled).** Node identity was checked on the default configuration only. Still missing against the reference: 47 of its 83 levers, and the features they gate that are off by default there (ProbCut in its main and secondary layers, quiescence checks, SEE reach, LMR v2 and its terms, verified null move, TT extension, trio extension). Features outside the lever list are missing too: Syzygy probing, the `.ctg` book, the `EvalFile` net override, Contempt (currently inert), and the incremental accumulator and SIMD kernels (Phase 7). Each needs its own step, a parity check with the lever set, and a lever-independence test. The order is open: a proposal is to do them after Phase 5 step 13 and before Phase 6. Your decision is needed.

**Scope checklist** (each item is delivered by the step named in the plan above):

- [ ] `uci/parser.rs`: full command set, case-insensitive tokens, `position startpos|fen`, `moves`, all `go` limiters (`depth`, `movetime`, `wtime/btime/winc/binc`, `movestogo`, `nodes`, `mate`, `infinite`, `ponder`), `stop`, `ponderhit`, `quit`, `setoption`, `ucinewgame`, `isready`, `bench`, `perft`. (Steps 1, 4, 5, 6)
- [ ] `uci/options.rs`: visible catalog, `option` handshake, `setoption` dispatch, button options (`Clear Hash`), type validation. (Steps 2, 7)
- [ ] `uci/output.rs`: dedicated writer thread with a bounded channel; build lines once; coalesce/drop optional `info currmove` under back-pressure; flush correctly for GUIs. (Step 10, decision recorded in ADR-0007)
- [ ] Persistent Lazy SMP worker pool: start/stop signaling, `Threads` changes without rebuilding state, per-thread node counters padded to the cache line, main-thread aggregation, shared TT and read-only net. (Steps 7, 8, 9)
- [ ] Search control: stop flag, ponderhit, time-up checks every N nodes, no `Instant::now()` per node, no shared atomic per node. (Steps 4, 5)
- [ ] `bench` command: deterministic four-position set, per-position output, total nodes and NPS; same defaults as C#. (Verified in every search step; output unchanged in step 3)
- [ ] `info` line parity: depth, seldepth, multipv? (single PV only, as C#), score cp/mate, nodes, nps, hashfull, tbhits, time, pv. (Step 3)
- [ ] UCI transcript tests against the captured C# transcripts. (Steps 0 and 12)
- [ ] `fastchess` + `cutechess-cli` harness: 1,000 games vs C# at a fixed control, plus 1/2/4/8-thread smoke matches. Check legality, crashes, time losses and clean `quit`/`stop` handling. (Step 13)

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
- [ ] The node-identity gates pass on every host: `bench 6/12/16/18` reproduce
      the reference totals exactly (ADR-0005). Verified on Windows x64 (Zen 4);
      pinned in `tests/search.rs` (`bench 6` and the per-position counts of
      depths 1-8) and recorded in `docs/baselines.md`. Still to confirm on
      macOS and Linux (G6).
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
