# KnightByte â€” Migration Plan: the predecessor C# engine â†’ Rust

**Status:** Phase 0 complete; Phase 1 oracle capture and Phase 2 perft are complete; Phase 3 scalar evaluation is bit-exact. SIMD kernels and the incremental accumulator are deferred to the performance phase. Phase 4 (search), including 4d time management and lever exposure, is complete: the search is node-identical to the reference (ADR-0005, which supersedes the ADR-0004 deviation). **PHASE 1 RESTART COMPLETE (2026-11-18).** Round 1: predecessor bugs 143 to 145 fixed in the predecessor (commit bffe98a, with unit tests and the full unit suite passing); bug 146 was not reproduced. Round 2: former quirks Q2, Q3 and Q4 were reviewed against the predecessor source, reclassified as predecessor bugs 147 to 149, and fixed in the predecessor (commit f274b39, regression tests `PredecessorBugs147to149Tests`, full suite 734/734; ADR-0008 records the decision). The Phase 1 restart then re-captured the oracle and adopted the fixes in KnightByte (`07b401c`): B35 (table-move markers kept), B36 (quiescence searches en-passant captures), B37 (correction quiet gate tests the opponent occupancy), B33/B34 (UCI position handling follows the predecessor's decision; the engine starts on the start position and `ucinewgame` resets to it). Node identity with the fixed reference holds at every captured depth and position: `bench 6/12/16/18` = **15,528 / 456,308 / 2,815,799 / 6,892,874** (the old pins 14,979 / 416,533 / 3,169,859 / 6,925,762 described the pre-fix build); verified with `tools/capture_bench.py compare`, all deltas zero. Q1 (singular extensions never fire) stays a reproduced quirk — ADR-0006 measured the fixed behaviour and found no gain. Phase 5 is planned as 14 ordered steps (0-13, see its section); steps 0-10 and 12 are complete (step 11 was dropped; oracle transcripts, command surface and parser, option surface and `setoption` validation, `info` line completeness, the search thread with the stop flag, `go ponder`/`ponderhit`, `go mate`/`searchmoves`, the shared-tables refactor, the persistent Lazy SMP worker pool, the multi-thread stress gate with the B39 fix, and the output decision ADR-0007 with the `info currmove` line). The step 0 capture immediately found **predecessor bug 150** (`setoption Hash` above the declared maximum crashed the reference via an overflowing `TTable.Resize` allocation; KnightByte not affected), the migration stopped per the project rules, and resumed the same day after the fix (predecessor commit `acaff1f`, full suite 745/745) and the transcript re-capture. **Phase 6 is complete (2026-11-19):** Syzygy WDL/DTZ probing with 3-4-5 (and larger) material support, search and quiescence integration, root DTZ ranking, and the `.ctg` opening book are ported and tested; the four bench pins are unchanged. **The macOS Apple Silicon release is complete (2026-11-20, ADR-0015):** the AArch64 target was fixed (B52), the 128-bit NEON kernels are the single AArch64 backend, and one `aarch64-apple-darwin` binary built with `-Ctarget-cpu=apple-m1` (M1-M5, no Intel) is built, verified and packaged by `tools/build_release_macos.py`; bench 12-18 on the M4 is 1.72-1.89 MNPS plain and 1.95-2.01 MNPS with PGO (+6.0-7.5 %), pins exact.
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
5. **Portability** â€” release artifacts for macOS Apple Silicon, Ubuntu x64 and Windows 11 x64. The Windows release is three PGO executables (`v3`, `v4-vnni`, `v2`, ADR-0011, done 2026-11-19); the macOS release is one `aarch64-apple-darwin` binary with the `apple-m1` baseline that runs on M1-M5 (ADR-0015, done 2026-11-20; Intel Macs are not supported). Ubuntu x64 remains open.
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
| **The net is fixed** | The v5 net is embedded in the binary, checksum-verified, and evaluated bit-exactly. There is no `EvalFile` option (removed 2026-11-19, ADR-0014): an option that pretends to load another network is a trap. |
| **All levers visible** | **Revised 2026-11-19 (ADR-0014):** KnightByte advertises ten public options and exactly sixty research levers, every one functional and visible (`docs/LEVERS.md`). The predecessor's 83 hidden levers were not carried over wholesale: the inert and the measured-flat ones were dropped and promising constants and features were added, so the owner can test the whole set. |
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
- [ ] **Open item (partly done 2026-11-19, ADR-0011):** the acceptance gate was measured on the Zen 4 host: a `v3` baseline is within 1.5-1.8 % of dedicated `v4-vnni`, the old no-`target-cpu` build was not (2.1-4.1 %). The second Windows host is still to be measured. Original item: build the two-variant Windows prototype on the Windows host and run the 2% acceptance gate. This cannot be completed on the macOS host and is scheduled for the first Windows session (Phase 7/8).
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
- [ ] Finish the harness modernization (reconciled 2026-11-19: `tools/capture_bench.py compare` gives the per-depth node-identity diff and `tools/perf_bench.py` the five-run median with the pin check; the fake-engine fixture and the stable JSON schema are still missing). Remaining:
  - `--baseline` diff mode: exact node identity, depth/EBF delta, NPS ratio, PASS/FAIL;
  - robust timeouts and process cleanup so a silent engine cannot hang CI;
  - extract the duplicated UCI session code into one shared helper and add a fake-UCI-engine fixture so the harness itself is tested;
  - a stable JSON schema/version and a baseline directory.
- [ ] Build `tools/uci_oracle.py`: runs two engine binaries with the same script and diffs node counts, best moves, eval lines and option sets. (Not built. The two-engine differential tools that exist are `tools/tb_compare.py` and `tools/book_compare.py`; the UCI surface is covered by `tests/uci_transcripts.rs` against the re-captured transcripts.)
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
- [x] Zobrist keys (piece, castling, en passant, side, pawn/non-pawn) (done in Phase 4a: `src/board/zobrist.rs`) â€” needed for the transposition table in Phase 4.
- [x] SEE and full draw detection (repetition, fifty-move, insufficient material) (done in Phase 4: `src/search/see.rs`, `Searcher::is_draw`) â€” needed by the search.
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
- [x] Incremental accumulator with dirty-piece tracking and lazy own-king refresh (done 2026-11-19, Phase 7: per-ply accumulator pairs, a full refresh only when the own king moves) â€” needed by search; deferred to the performance phase with the search/evaluator integration.
- [x] AVX2 and AVX-512 kernels (done 2026-11-19: the `v3`, `v3-vnni` and `v4-vnni` rungs, node identity across backends pinned by `tests/kernels_backends.rs`)
- [x] NEON kernels (done 2026-11-20, ADR-0015): the 128-bit AArch64 row, refresh and SCReLU-head kernels are the single AArch64 backend (NEON is baseline, no runtime probe), bit-exact against the scalar oracle; bench 12-18 on the M4 rose 3.9-4.4x with the pins unchanged. AVX-VNNI remains unused: the VNNI dot-product cannot express the SCReLU per-element square-shift, so the `v3-vnni`/`v4-vnni` rungs keep the AVX2 kernels.
- [x] `EvalFile` override (decided 2026-11-19, ADR-0014: not offered; the option is removed instead of advertised and ignored) â€” deferred until the UCI option layer lands.
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
- [x] **Resolved 2026-11-19 (ADR-0014):** the visible set is exactly sixty functional levers; the inert reference levers are dropped, three promising features were ported, and 25 hard-coded constants were exposed. Original item: Port the remaining levers (47 of 83 are not exposed yet). They gate reference features that ship off or are not ported (ProbCut, SEE-reach, LMR v2 and its terms, quiescence checks, book, tablebases); expose each one when its feature lands. A test that setting each lever changes the search on a probe position is still open. *(Update 2026-11-18, step 2: all 83 reference levers are now advertised — the 47 unported ones as validated, accepted, inert `wired: false` rows in `src/search/lever_table.rs` — so the handshake surface is complete; wiring a lever's feature remains the work item.)* *(Update 2026-11-18, step 11: the node-count lever test is dropped by decision of the project owner — lever strength is measured after the migration, one lever at a time with SPRT on the final fast engine.)*
- [ ] Port the C# search tests: mate tests, PVS, LMR tables, futility, history, singular, extension cap, TT, root TT cutoff, repetition, time compliance, lever independence. *(Reconciled 2026-11-19: tests exist for mate, LMR, futility, history, killers, TT, correction, repetition and time; no test names singular, extension cap, PVS or root TT cutoff, so those stay open.)* *(Update 2026-11-18, step 11: the lever-independence test is dropped — lever strength is measured after the migration with SPRT, one lever at a time.)*
- [x] Build `docs/SEARCH-PARITY.md` (exists; component map in section 6): every C# search feature, its Rust module, its lever name, its test.
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
- **Quirks:** reproduced now and recorded in a local, unpublished register. Each one is checked again after the migration (`project-rules.md`). Updated 2026-11-18: Q2, Q3 and Q4 were reclassified as predecessor bugs 147-149 and fixed in the predecessor (commit f274b39); KnightByte inherits them as bugs B35-B37 (`docs-new/new-bugs.md`) and stops reproducing them at the restart. They are removed from the quirk register. **Update 2026-11-19 (ADR-0010):** the remaining quirks Q5-Q9 were resolved too, in both engines: Q5 (a stopped ponder search answers its best move instead of `bestmove 0000`), Q6 (the banner leaves stdout), Q7 (the warm-up uses a private table), Q8 (the single-legal-move shortcut serves time-managed searches only) and Q9 (a corrupt DTZ file no longer disables WDL; predecessor Bug 151 / B49). Only Q1 remains. The step 0, 4, 5 and 6 records below describe the behaviour that was reproduced at the time and are kept as history; the step-0 transcripts were re-captured from the fixed predecessor.
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
- Verification: 244 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874) and `capture_bench.py compare` all deltas +0. *(Correction 2026-11-18, B41: the predecessor's UCI layer builds a fresh per-thread state for every `go`, so its learned tables do NOT survive a search; `Searcher::search()` now clears history and correction history per search, and the TT same-key rule is depth-only across searches — see `docs-new/new-bugs.md`.)*

**Step 9 - Helper diversity and multi-thread safety (about 1 day) - complete 2026-11-18**

Helpers search the same iterations as the main thread, as in the reference; they start at depth 1 and differ only by their own history. A skip table for helpers is a deviation from the reference and is not part of Phase 5. It can be tested later as a Phase 7 experiment, with its own SPRT.

Stress tests with 1, 2, 4 and 8 threads, 500 random `go`/`stop`/`position`/`setoption` cycles: no panic, every `bestmove` legal, exactly one `bestmove` per `go`, every TT probe key verified (the existing exact-key check). Node counts are not asserted. **Done:**

- Helper diversity was already the step-8 design (own persistent search state, iterative deepening from depth 1, no skip table); nothing to change.
- New `tests/thread_stress.rs`: a seeded, replayable walk of 500 random cycles (`position start|kiwipete`, `go depth 1-4`, `stop`, `setoption Threads 1/2/4/8` + `Hash` + a lever, `isready`) driving the real binary. The walk mirrors the current position through the engine library and checks every `bestmove` for legality in it. Instrumentation built in from the start: per-search bounded waits (a dead or stuck engine fails at the offending cycle, not later), a snapshot-based bestmove wait (a cumulative wait once passed on stale answers — a test-side defect, fixed), and the engine's stderr captured to a file so a panic that kills the engine names itself.
- **The walk found and fixed a real defect (B39, `docs-new/new-bugs.md`):** `setoption Hash` after a helper search killed the engine — the persistent helpers held their `SharedTables` clones forever (blocking the resize's exclusive `Arc::get_mut`), and a *queued* job could still hold clones during the helpers' drain window. Fixed in three layers: the helper releases the tables when its job ends, `stop_search` waits on the pool's idle barrier, and the resize waits bounded instead of failing instantly. **The reference cannot have this bug** (its .NET references never constrain a resize); probed for the record — the reference survives a `Hash` resize sent mid-search with `Threads 4` attached and never even stops the search.
- Verification: 247 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874).

**Step 10 - Output decision (about 0.25 day) - complete 2026-11-18**

The bounded-channel writer thread from the checklist is needed only if output can block the search. One `info` line per iteration cannot block it, so the synchronous writer stays for now. The decision, with the measured output time per search from step 9, is recorded in `docs/decisions/ADR-0007`. `info currmove` is added only if the step-0 transcripts show the reference emits it. **Done:**

- `ADR-0007` records the decision: the synchronous writer stays (a search emits a handful of short lines; the step-9 walk measured the output side as microseconds per line against the 1024-node poll interval; a broken pipe drains via the stop flag).
- `info currmove` is emitted because the step-0 transcripts show it: exactly one line per search (`info currmove a2a3 currmovenumber 1 nodes 1` on the start position), when iteration 1's first root move begins, as the picker yields it — before the depth-1 line, and never when the search stops before iteration 1's move loop (the step-0 stop transcript shows no line there). Implemented as a `SearchEvent` channel (`Info` / `CurrMove`) threaded from `search` through `search_root` into the root move loop; a report only, so the node pins are unchanged (verified below).
- Verification: 247 tests pass with the Stockfish verifier set, fmt/clippy clean, bench pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874).

**Step 11 - Lever independence - dropped (2026-11-18, decision by the project owner)**

The planned node-count independence test is dropped. A lever that does not change a probe search's node count is not a defect: node-neutral levers exist by design, effects can be position- or depth-dependent, and the time-management levers never touch a fixed-depth search. Note: strength measurement of all 83 (+3) levers happens after the migration, as a dedicated tuning campaign with SPRT - one lever at a time, on the final fast engine. That matches "we measure at the end all levers against playing strength".

**Step 12 - UCI transcript tests (about 0.5 day) - complete 2026-11-18**

Compare the engine's transcripts for handshake, `position`, `go`, `stop`, `ponderhit`, `ucinewgame`, `setoption` and `bench` with the step-0 captures. Compare the order and structure of lines, the option sets, and the legality of every `bestmove`. Do not compare timing fields or `info` values; node counts are compared only at fixed depth with `Threads=1`.

**Done:**

- New `tests/uci_transcripts.rs` drives the real binary through all eleven published scenarios (handshake, deterministic and timed `go` limiters, `go infinite` + `stop`, the four `stop` cases, ponder/ponderhit, mate/searchmoves, `ucinewgame`, setoption validation, `isready` during a search, `Threads`) and a `bench 6` transcript. A shared session helper reads stdout/stderr on background threads, so live `isready`/`stop` interleaving is observable and a crashed engine names itself.
- Transcript lines are reduced to protocol shapes: identity, identity-bearing eval-source, timing fields, scores, node values and PV tails are masked. The public option set is compared exactly (the research levers are KnightByte's documented superset and are filtered out for the handshake check). One report block per search survives host-dependent iteration counts; the number of `bestmove` answers is compared exactly and every non-null answer is checked for legality with the engine's own move generator. All FEN constants are verified with Stockfish at test time; a position derived from a verified FEN by a legal move is checked without re-verification (the verifier's en-passant normalization differs).
- Cold and warm fixed-depth pins: 47 nodes at `go depth 1`, 43 at the mate-in-one `go mate 1`, `Nodes: 8902` for perft 3 and `Nodes: 15528` for `bench 6`; after B41 the warm sequential values match the reference too, so `go_limiter_depth` pins its repeated-search counts (5,741 / 5,731 / 34,979 / 169).
- The replay found one protocol gap: `go perft`/`perft` printed only `Nodes:`, while the reference prints `Nodes:`, `Time:` and `NPS:`. `handle_perft_depth` now measures the count and prints all three lines (zero-time guard, as for search nps). No search or node-count change.
- **B41 (found and fixed in step 12):** in a sequential session the reference reuses the shared table more aggressively than KnightByte did: after the bare `go`, `go depth 7` reported 5,731 reference nodes vs 10,500 KnightByte; `go nodes 50000` 34,979 vs 48,863; `movestogo` 169 vs 3,924. Root cause found with the per-node ledger (`docs/SEARCH-PARITY.md` §8.6): KnightByte stamped TT entries with its advancing generation, so a shallower store evicted the previous search's deeper entry, and it kept history/correction history between searches while the predecessor builds a fresh per-thread state per `go`. Fixed: the TT same-key rule is now a pure depth comparison (`src/search/tt.rs`) and `Searcher::search()` clears history and correction history (`src/search/searcher.rs`). The re-captured `go_limiter_depth.txt` transcript reproduces every warm node count exactly; cold pins and the ledger are unchanged (cold 5,741 = 5,741, warm 326 = 326, zero divergence). Details in `docs-new/new-bugs.md`.
- Verification: 264 tests pass locally with the Stockfish verifier set, fmt/clippy clean, release build clean, `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874 and `capture_bench.py compare` at depths 1-12, 14, 16, 18 shows every delta at +0.

**Step 13 - Match gates (about 1 day of work, plus background match time) complete 2026-11-18**

- Smoke: short matches with fastchess or cutechess-cli at 1, 2, 4 and 8 threads against the C# engine: no illegal moves, crashes, time losses or stray stdout; clean `quit` and `stop` handling.
- G5: 1,000 games against the C# engine on the Windows host, at the ADR-0006 control (8+0.08). Read the `Timeouts` and `Crashed` columns, not only the exit code. Runs in the background; the phase does not close until it finishes.

I played 3 x 100 games with fastchess:

single thread:
--------------------------------------------------
Results of PF vs KB (8+0.08, 1t, 64MB, UHO_Lichess_4852_v1.epd):
Elo: 138.99 +/- 43.27, nElo: 244.66 +/- 68.10
LOS: 100.00 %, DrawRatio: 44.00 %, PairsRatio: inf
Games: 100, Wins: 44, Losses: 6, Draws: 50, Points: 69.0 (69.00 %)
Ptnml(0-2): [0, 0, 22, 18, 10], WL/DD Ratio: 0.38
--------------------------------------------------

2-threads
--------------------------------------------------
Results of PF vs KB (8+0.08, 1t - 2t, 128MB, UHO_Lichess_4852_v1.epd):
Elo: 99.95 +/- 46.07, nElo: 157.15 +/- 68.10
LOS: 100.00 %, DrawRatio: 40.00 %, PairsRatio: 6.50
Games: 100, Wins: 47, Losses: 19, Draws: 34, Points: 64.0 (64.00 %)
Ptnml(0-2): [1, 3, 20, 19, 7], WL/DD Ratio: 2.33
--------------------------------------------------

4-threads
--------------------------------------------------
Results of PF vs KB (8+0.08, 1t - 4t, 128MB, UHO_Lichess_4852_v1.epd):
Elo: -38.37 +/- 41.90, nElo: -63.19 +/- 68.10
LOS: 3.45 %, DrawRatio: 46.00 %, PairsRatio: 0.59
Games: 100, Wins: 19, Losses: 30, Draws: 51, Points: 44.5 (44.50 %)
Ptnml(0-2): [4, 13, 23, 10, 0], WL/DD Ratio: 0.64
--------------------------------------------------


**Phase 4 carry-over (required for 100 % feature compliance).** Node identity was checked on the default configuration only. Still missing against the reference: 47 of its 83 levers, and the features they gate that are off by default there (ProbCut in its main and secondary layers, quiescence checks, SEE reach, LMR v2 and its terms, verified null move, TT extension, trio extension). Syzygy probing, the `.ctg` book, the incremental accumulator and the SIMD kernels are done (Phase 6 and Phase 7). `EvalFile` and `Contempt` remain inert. Each open item needs its own step and a parity check with the lever set.

**Scope checklist** (each item is delivered by the step named in the plan above):

- [x] `uci/parser.rs`: full command set, case-insensitive tokens, `position startpos|fen`, `moves`, all `go` limiters (`depth`, `movetime`, `wtime/btime/winc/binc`, `movestogo`, `nodes`, `mate`, `infinite`, `ponder`), `stop`, `ponderhit`, `quit`, `setoption`, `ucinewgame`, `isready`, `bench`, `perft`. (Steps 1, 4, 5, 6)
- [x] `uci/options.rs`: visible catalog (delivered in `src/search/options.rs` and `src/search/lever_table.rs`, dispatch in `src/uci/mod.rs`), `option` handshake, `setoption` dispatch, button options (`Clear Hash`), type validation. (Steps 2, 7)
- [x] `uci/output.rs`: dedicated writer thread (decided otherwise in ADR-0007: the synchronous writer stays, no writer thread or channel; `info currmove` is emitted) with a bounded channel; build lines once; coalesce/drop optional `info currmove` under back-pressure; flush correctly for GUIs. (Step 10, decision recorded in ADR-0007)
- [x] Persistent Lazy SMP worker pool (`src/search/pool.rs`; 500-cycle stress gate `tests/thread_stress.rs`): start/stop signaling, `Threads` changes without rebuilding state, per-thread node counters padded to the cache line, main-thread aggregation, shared TT and read-only net. (Steps 7, 8, 9)
- [x] Search control: stop flag, ponderhit, time-up checks every N nodes, no `Instant::now()` per node, no shared atomic per node. (Steps 4, 5)
- [x] `bench` command: deterministic four-position set, per-position output, total nodes and NPS; same defaults as C#. (Verified in every search step; output unchanged in step 3)
- [x] `info` line parity (2026-11-19: byte-identical to the reference; `tbhits` joins the line only while tablebases are in use, ADR-0013): depth, seldepth, multipv? (single PV only, as C#), score cp/mate, nodes, nps, hashfull, tbhits, time, pv. (Step 3)
- [x] UCI transcript tests against the captured C# transcripts (all 11 captures plus `bench 6`, `tests/uci_transcripts.rs`). (Steps 0 and 12)
- [ ] (Not run yet. Existing evidence: `fastchess --compliance` passes for both engines, 904 finished games of the singular-extension experiment with no illegal move, time loss or crash, and 3 x 100 games from the project owner.) `fastchess` + `cutechess-cli` harness: 1,000 games vs C# at a fixed control, plus 1/2/4/8-thread smoke matches. Check legality, crashes, time losses and clean `quit`/`stop` handling. (Step 13)

**Exit gate G4 + G5:** no illegal moves, no crashes, no time losses; `Threads` scaling is sane; `ucinewgame` is fast (no multi-GB TT clear); every option is visible and functional. The UCI half of G4 is automated by the step-12 transcript suite; the game-safety half closes with step 13.

---

### Phase 6 â€” Syzygy and opening book (5â€“8 days, parallelizable)

**Objective:** restore the optional endgame and book features without destabilizing the search.

- [x] Port the tablebase prober: WDL + DTZ probing, 3-4-5 piece support and the larger material names (up to seven pieces), material gating, depth-unconditional probing in search and quiescence, 50-move-rule-aware scores, root DTZ ranking, and the direct root probe. The modules are split by concern under `src/tb/syzygy/` (`pos`, `index`, `pairs`, `table`, `probe`).
- [x] Copy the small 3-man test tables into `tests/data/tb/` with their notice. Tests cover WDL, DTZ, root ranking, missing/corrupt files, illegal and castling positions, and the search integration (leaf scores and mate announcement).
- [x] Port the `.ctg` reader and signature validation (`src/tb/book/`): canonical signature, page scan, FNV index, move-byte decoding, child-statistic weights, annotations and weighted choice. `Own Book` defaults off.
- [x] `Book File` loads explicitly; an empty value requests auto-detection next to the executable or in `books/`. No book is loaded at startup and no third-party book is published.
- [x] Missing paths, unreadable files and malformed data produce `info string` diagnostics and never panic.
- [x] The full 3-4-5 set is exercised locally through `KNIGHTBYTE_SYZYGY_PATH` (the DTZ-ranking tests ported from the predecessor use the reported 4-man fixture and skip when the variable is unset). The reference's root move (Kd2-e2) and the toggle-off lazy shuffle (Kd2-c1) both reproduce.

- [x] **Post-phase audit and differential verification (2026-11-19).** The ported unit tests were not enough: the three committed 3-man tables and a synthetic book cannot reach the 4+ piece code paths. Two new tools compare KnightByte with an independent tablebase reader and the predecessor binary: `tools/tb_compare.py` (random positions; WDL, DTZ and the chosen move) and `tools/book_compare.py` (random book lines; the set of moves each engine can return, sampled). Before the fixes 273 of 300 tablebase positions and 56 of 197 book positions disagreed; they exposed **B45-B48** (capture sign in `probe_wdl`, `encode` skipping the trailing piece groups, the 3-v-3 material family missing from the name scan, and the book's en-passant capturer colour), all fixed. After the fixes: 3,500 random 3-4-5 and 6-man positions agree across all three implementations, and 315 book positions (400 samples each, a 786,430-position book) agree with the predecessor. The root-score clamp now runs inside the aspiration loop like the predecessor, tests that named other engines were reworded, and the real-book tests (skipped when no book is present) were added.

**Exit gate:** tablebase and book tests match C#; `Own Book=false` is the default; absent files are harmless; the engine builds and runs with both features compiled in. **Passed 2026-11-19:** 318 tests, fmt/clippy clean, and `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874 unchanged.

**Reference quirks found during the port:** Q9 (a failed DTZ file load poisons WDL probing for the same material through the shared hash error flag) was reproduced first, then reclassified as a bug (predecessor Bug 151 / B49) and fixed in both engines; see the quirk clean-up below.

**Quirk clean-up (2026-11-19, ADR-0010).** The project owner decided that every remaining quirk except Q1 is fixed identically in both engines, so the engines stay equal. Predecessor first (the project rule for bugs): `Syzygy.cs` (per-kind error mask), `EngineOrchestrator` (Q5 answer, Q7 private warm-up table), `UciHandler` (Q6 banner to stderr when interactive), `Search.IterativeDeepening` (Q8 only when `!IsInfinite`); full suite 755/755 (745 plus ten regression tests, each shown to fail against the reverted fix) and the published binary rebuilt (`bench 6` unchanged). KnightByte second: B49 (per-kind error mask), Q5 and Q8 in `src/uci/mod.rs`; Q6 and Q7 already conformed. The Rust regression test for B49 also exposed **B50** (a failed DTZ read was reported as a success with a made-up value, so a WDL-only table set chose a bogus root move), fixed. The step-0 transcripts were re-captured from the fixed predecessor (only the ponder `bestmove` and the removed banner changed shape) and all 12 replay scenarios pass. Gates: 318 tests, fmt/clippy clean, `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874 unchanged; differentials on the rebuilt engines show 0 disagreements (800 tablebase and 193 book positions, 120 WDL-only positions) and the corrupt-DTZ position scores `cp 29900` in both engines.

**Deferral history (superseded 2026-11-19):** Phase 6 was deferred, not cancelled, until the performance target was met. The project owner then moved it to the front of the order of work; it ran before the remaining performance items.

---

### Phase 7 â€” Performance (5â€“10 days)

**Objective:** convert the parity build into a faster engine without changing its node counts.

**Preparation done (2026-11-18):** the performance decomposition exists
(`tests/perf_microbench.rs`, release mode): the full-refresh evaluation
dominates the node budget at ~67% (1.8 µs of 2.7 µs per node); movegen and
copy-make are ~5% each. B40 (the per-node evaluation allocations; the
reusable `Evaluator` existed but was never wired into the search) is fixed
with the bench pins unchanged. The attack order is in `next-steps.md`
§2.6.

**Step 7.0 - thread-scaling entry check (2026-11-18): the shared hardware budget saturates, not the pool.** The step-13 matches came with a measured thread-scaling anomaly: KnightByte's aggregate NPS caps at ~1 M and *drops* beyond two threads (external wall clock on `go nodes 2000000`: 1 T = 386 K, 2 T = 549 K, 4 T = 701 K, 6 T = 478 K, 8 T = 560 K, with high variance at 4-8 T), while the predecessor scales 5.99x on the same host. Findings so far:

- The host is 8 physical cores / 16 logical (AMD Ryzen 7 260), so oversubscription does not explain it.
- An 8-thread search keeps **7.98 core-equivalents** fully busy (process CPU time), so nothing blocks or idles: the threads burn CPU without producing counted nodes.
- The predecessor's per-`go` fresh state is now reproduced (B41 fix), so the warm-state theory is out.
- **Decisive experiment: 8 concurrent single-threaded *processes*** (no shared table, no pool) also cap the host at **~390 K nodes/s aggregate** (387 K / 381 K / 389 K / 390 K for 1 / 2 / 4 / 8 processes). The bottleneck is a **shared hardware resource saturated by the per-node work itself** — with the evaluation at 67 % of the budget and every node paying a full 2 x 512-wide accumulator refresh, the candidates are the net-row memory traffic through the shared L3. The transposition table is ruled out: the private-table variant (no sharing) scales the same way.
- Consequence for the order of Phase 7: the **incremental accumulator + SIMD kernels are not only the single-thread lever but also the thread-scaling fix** — shrinking the per-node traffic is what lets extra threads add throughput. Profile (ETW/samply) belongs in front of every change; the bench pins stay the gate.

**Match evidence from the same session (3 x 100 games vs the predecessor at 8+0.08, 128 MB, UHO opening set):** KnightByte scored 25.0 % (1 thread), 17.0 % (2 threads) and 25.5 % (4 threads) against the single-threaded predecessor — consistent with the measured NPS deficit and no crash, illegal move or time loss in any game (`fastchess --compliance` passes for both engines; `match_errors.log` empty). The earlier 1-thread match in this plan's step-13 notes (Elo +139 for the predecessor) and these results both predate the B41 fix.

**Incremental accumulator (the first Phase 7 item) — complete 2026-11-19.** The evaluation no longer refreshes both 2 × 512-wide accumulators per node: the searcher keeps a per-ply accumulator pair and every made move pushes the child pair by adding and removing only the changed weight rows (moved piece, captured piece including the en-passant square, the promotion piece and the castling rook). A perspective whose own king moved falls back to a full refresh (its region and mirror may change); a null-move child copies the parent pair. The output head runs over the ready pair as before. Bit-exactness is gated three ways: the incremental result must equal the full refresh at every position of seeded random walks and of deterministic castling/en-passant/promotion segments (`src/search/searcher.rs` unit tests), the 15 golden evaluation vectors are unchanged, and the search is node-identical (bench 6/12/16/18 exact, every `capture_bench.py compare` delta +0). Measured on this host: bench 12 rose from ~0.37 to **0.556 MNPS (+50 %)**, and `go nodes 2000000` from 386 K (1 T) / 549 K (2 T) to **533 K / 923 K** — the per-node traffic shrink also lifted the thread scaling, though 6–8 threads still degrade and remain the next profile target.

**SIMD kernels (the second Phase 7 item) — complete 2026-11-19 (x86-64).** The accumulator row arithmetic and the SCReLU output head run through `src/eval/nnue/kernels.rs`: 128-bit SSE4.1 kernels for the `v2` ladder rung and 256-bit AVX2 kernels for `v3` and above, dispatched once per process from the platform selector with the ADR-0001 `KNIGHTBYTE_FORCE_ISA` override. The head widens each clamped lane to `i32` before squaring (a pairwise `madd` would sum two lanes' squares before the shift and break bit-exactness), multiplies by the widened output weight and folds the `i32` lanes into the `i64` total on a bound that cannot overflow. The VNNI rungs of the ladder map to the AVX2 kernels for now — the dot-product instruction cannot express the per-element square-shift semantics without changing them. Measured: bench 12 **0.556 → 0.863 MNPS (+55 %)**, `go nodes 2000000` 1 T 533 K → **856 K**. The 512-bit AVX-512 kernels (`v4-vnni` rung) landed the same day and measure **identically to AVX2 on the Zen 4 host** (845 K vs 843 K — Zen 4 double-pumps the 512-bit datapath; the real 512-bit gain belongs to Zen 5 and Intel Granite-Rapids-class hosts). A cross-backend integration test (`tests/kernels_backends.rs`) forces every backend through `KNIGHTBYTE_FORCE_ISA` and pins the identical bench total (the ADR-0001 node-identity gate). **AArch64 (2026-11-20, ADR-0015):** the same three kernels exist as 128-bit NEON (`KernelKind::Neon`), the only AArch64 backend because NEON is part of the baseline; the M4 reads 1.72-1.89 MNPS at bench 12-18 against 0.43-0.44 MNPS for the scalar path (3.9-4.4x), with the four pins exact; the PGO build adds +6.0-7.5 % (1.95-2.01 MNPS).

**Slider attacks (the third Phase 7 item) — complete 2026-11-19.** BMI2 PEXT
tables in `src/board/attacks.rs`, mirroring the predecessor's
`PextBitboards.cs`: relevant-occupancy masks, per-square offsets, and the
tables filled **from the ray-walking implementation** — bit-identical by
construction. Selected once per process (BMI2 present, not a slow-BMI2 CPU,
`KNIGHTBYTE_FORCE_PEXT`/`KNIGHTBYTE_DISABLE_PEXT` overrides), verified
against the ray walk for every relevant subset of every square. Honest
result: no measurable search-level NPS gain on this host (bench 12: 850K
with PEXT vs 869K without — within noise); the predecessor's NPS advantage
does not come from the slider attacks alone. The first version deadlocked
(the table fill re-entered the `pext_tables()` initialisation through the
public dispatchers) — fixed by filling from the ray walks directly.

**Performance plan (rewritten 2026-11-19; supersedes the earlier attack order).**
The confirmed 2x gap is the **search move path**, not the evaluation:
`movegen::legal()` makes and king-tests every pseudo-legal move and the search
then makes the searched moves again, while the predecessor validates only after a
single make. The order is: **P0** measurement rig; **P1** one-pass move path
(decisive); **P2** hardware PEXT; **P3** board hot path (incremental occupancy;
make/unmake only if profiled); **P4** evaluation pair/head; **P4b** AArch64 NEON
(blocking for the M4 gate); **P5** TT prefetch and generation ownership;
**P6** PGO and per-variant `target-cpu`; **P7** thread scaling; **P8** cross-host
validation. The detailed step plan, with evidence, targets and per-step gates, is
maintained with the migration material; `next-steps.md` section 3.2 carries the
same order. Every single-thread step must keep the four bench pins exact.

**P0 results (2026-11-19): a reproducible same-session baseline, and a smaller
gap than the cross-session figures suggested.** Both engines were measured
back-to-back on the Zen 4 host (`perf_test.py --depth 20 --threads 1 --hash
256`): KnightByte 669,773 NPS vs the predecessor 1,064,034 NPS with
**identical node counts (18,662,358, EBF 2.131)** - a 1.59x per-node cost gap,
not 1.87x. At 5 s the figures are 739,702 vs 1,096,913 (1.48x). On the shallow
`bench 12` protocol KnightByte is already ~6 % faster (811,935 vs 766,904
median). The recorded 1,569,444 predecessor number came from a different
session and is no longer used for ratios; every future comparison runs both
engines in the same session. The new `node-stats` feature shows that **89.8 %
of all `make_move` calls are the legality filter** (97.7 M filter makes vs
11.1 M search makes in `bench 20`), and the filter is ~55 % of the
same-session gap. The counters also found **B42** (the eval cache stores every
evaluation while `UseEvalCache` is off); **fixed in P1.0 (2026-11-19)**, about
+3.8 % on `bench 12`, pins exact. **P1** (make-free legality, 2026-11-19), **P2**
(hardware `pext`) and the **P5 prefetch** followed the same day: the same-session
depth-20 ratio against the predecessor went 0.629 -> 1.08-1.11 (KnightByte
1.54-1.57 M vs 1.42-1.43 M NPS, identical 18,662,358-node tree), every pin exact.
P3 then found by profile that the move picker read an environment variable per
node (B43, 18-20 % of samples; fixed): depth-20 ratio **1.69-1.73** (2.38-2.43 M
vs 1.41 M NPS); incremental occupancy measured no gain and was reverted, and
make/unmake was rejected by the profile (`make_move` 3.7 %). **P7.0** then showed that
the documented multi-thread "collapse" was a node-accounting bug (B44: helpers never
published their blocks, the main thread counted itself twice): with it fixed the
scaling is 2.07x / 4.01x / 7.08x at 2 / 4 / 8 threads (aggregate 16.4 M NPS at 8
threads; the predecessor reaches 5.74x on the same host). **P6**: the PGO workflow
(`tools/pgo_build.py`, pins verified) adds +7-12 %: same-session depth 20 is
2.51-2.61 M vs 1.36-1.41 M NPS (1.79-1.92x); `target-cpu=znver4` gave no gain.
Full numbers and the composition table are in `docs/baselines.md` section 7.

**Measurement protocol**

* Fixed benchmark set: the four-position `bench` plus the 20-position time-to-depth set.
* Metrics: nodes, NPS, time-to-depth, depth at fixed time, EBF, TT hit rate, eval time share.
* Cheap gates: `perf-test/perf_test.py --depth` for machine-independent nodes/EBF/stability and `--threads 1,4,8` for scaling; `perf-test/ebf_compare.py` for head-to-head EBF. These reject breakage before an SPRT.
* Strength: `fastchess` SPRT after the cheap gates pass; `cutechess-cli` for CCRL-like controls and time-loss checks.
* Profilers: `samply`/Instruments on macOS, `perf` on Linux, VTune or ETW on Windows, `hyperfine`, `cargo asm`, `cargo llvm-lines`, `cargo bloat`.
* Baselines: compare against the C# numbers for the **same host** â€” M4 against the M4 record, Zen 4 against the Zen 4 record; the consolidated table is `docs/baselines.md`. Use fixed depth for node identity and the same time control for time-to-depth.
* A/B runs alternate and repeat at least five times; report median and spread. Record governor, temperature and background load. Do not trust a single laptop run.
* Every optimization is a separate commit with its own measurement and a node-count check. If node counts change, it is a search change, not an optimization.

**Work items, in order** *(the original Phase 7 backlog; the ordered plan above is authoritative)*

- **Entry item (2026-11-19, the project owner's directive after the step-13
  session): a dedicated performance session that starts in the predecessor
  source, line by line.** The step-13 evidence (§2.6, `next-steps.md`) shows
  KnightByte at 0.856 MNPS single-threaded vs the predecessor's 1.617 MN
  (53 %) and a 6–8 T thread-scaling regression that the project owner has
  declared blocking. The reconnaissance in the predecessor source identified
  three leads: (1) `PextBitboards.cs` slider attacks — KnightByte still
  walks rays; (2) the accumulator push and the SCReLU head lane order in
  `NnueAccumulatorV5.cs` / `NnueQuantV5.cs` (the predecessor runs them over
  `Vector<short>` hardware intrinsics); (3) the unmanaged XOR-verified TT
  probe/store. **The ordered step plan (P0–P8) with the measurements and
  gates is in the performance plan above and in `next-steps.md` §3.2** — the
  PEXT tables, the incremental accumulator and the x86-64 SIMD kernels are
  already implemented (2026-11-19); the next open step is the measurement rig
  (P0) followed by the one-pass move path (P1).
- [x] Profile the parity build; identify the top three costs. (Done 2026-11-19: `tools/profile_report.py`, first profile in `docs/baselines.md` 7.9.) Do not guess.
- [x] Zero-allocation audit: counting allocator in tests; no `Vec` growth, `String` formatting, `Box` or `format!` in search/eval. (Done 2026-11-19: `tests/alloc_audit.rs`, `docs/baselines.md` 7.21: two allocations per completed iteration, none per node, none per search.)
- [ ] Position representation: benchmark copy-make vs make/unmake on M4 and x86; pick the faster, document the ADR.
- [ ] TT: prefetch, allocation strategy, generation aging, bucket probe order, hashfull behavior.
- [ ] NNUE: incremental update validation, kernel selection per host, alignment, `loadu` vs aligned, accumulator layout.
- [ ] Lazy SMP: persistent workers, false-sharing audit, node-counter publication cadence, TT contention.
- [x] (Windows done 2026-11-19: `v3`/`v4-vnni`/`v2` with per-variant PGO and static C runtime. macOS done 2026-11-20: one `aarch64-apple-darwin` build with `target-cpu=apple-m1` and its own PGO profile (+6.0-7.5 % at bench 12-18 on the M4), ADR-0015.) Compiler: `target-cpu=apple-m1` for macOS, `v3`/`v4-vnni` for Windows, PGO per artifact, LTO validation.
- [ ] Optional experiments: PEXT vs magic per host, i8mm (nightly/asm), AVX-512 vs AVX2 on Zen 4, huge pages, mlock, QoS class.
- [x] Windows universal build: compare against dedicated `v3` and `v4-vnni`; accept only within 2%; force-ISA test every backend. (Done 2026-11-19 on the Zen 4 host, ADR-0011 and `docs/baselines.md` 7.20: the shipped baseline is `v3`; forced-scalar identity is checked by `tools/build_release.py` and `tests/kernels_backends.rs`. The second host is open.)
- [x] Update the performance notes in `docs-new/` with commands, host, and results. (`docs/baselines.md` 7.x and the feature list, kept current after every step.)

**Exit gate G6 (performance part):** at least 1.5Ã— C# NPS on the M4 at identical bench nodes; time-to-depth improvement on the 20-position set; no node-count drift; no platform-specific correctness differences.

---

### Phase 8 â€” Release engineering (3â€“5 days)

**Objective:** reproducible, tested artifacts for all three OS families.

- [x] macOS (done 2026-11-20, ADR-0015): one `aarch64-apple-darwin` build with `target-cpu=apple-m1`, stripped; the binary runs on every Apple M processor (M1-M5) and Intel Macs are unsupported. The release is PGO by default (+6.0-7.5 % at bench 12-18 on the M4), `--no-pgo` for the plain build. `tools/build_release_macos.py` verifies `uciok`, the four bench pins, the forced scalar/NEON identity and the arm64-only slice, then packs the `tar.gz`; smoke-tested on the M4 (the M5 smoke test remains open because no M5 host is available).
- [ ] Linux: `x86_64-unknown-linux-gnu`, `target-cpu=x86-64-v2`, PGO; smoke test on the Ryzen 5 5600H.
- [ ] (2026-11-19: delivered as three executables `v3`, `v4-vnni`, `v2`, each with its own PGO profile, ADR-0011; `v3-vnni` is not built because no kernel uses it. The smoke test on the i7-10750H is open.) Windows: universal `.exe` with `v3`, `v3-vnni`, `v4-vnni` paths (and `v2` if the ADR keeps it); per-variant PGO; smoke test on i7-10750H and Ryzen 7 260.
- [ ] On the i7-10750H, record a C# baseline and a KnightByte baseline at the same protocol (`v3` path) so the relative gain is apples-to-apples. Do not compare its NPS to the Zen 4 or CCRL numbers.
- [x] (Done 2026-11-19 by the import table: the executable imports only `KERNEL32`, `ntdll`, `bcryptprimitives` and `api-ms-win-core-synch`, all on Windows 10, and the static C runtime removed the `VCRUNTIME140.dll` dependency, bug B51. Not run on a Windows 10 machine.) Confirm Windows 10 compatibility if the CCRL host is not on Windows 11; avoid Win11-only APIs in the engine. The i7-10750H on Windows 11 does not cover this case.
- [x] `xtask` (or a checked-in script) builds all artifacts, verifies hashes, and emits a manifest. (Windows done 2026-11-19: `tools/build_release.py`; macOS done 2026-11-20: `tools/build_release_macos.py`; the Linux script is open.)
- [x] (Windows zip done 2026-11-19: three executables, README, LICENSE, THIRD-PARTY-NOTICES, SHA256SUMS.txt, manifest.json; the README links the repository for the source.) Release package: engine, `README.md`, `LICENSE`, `THIRD-PARTY-NOTICES.md`, source archive pointer, checksum file. The net is embedded; no separate net file is required.
- [x] README: UCI usage, options, supported platforms, build instructions, test instructions, known limitations. (Done 2026-11-19.)
- [ ] CI release workflow: macOS + Linux on every tag; Windows universal on a manual/nightly job (build time and PGO), with the dedicated builds kept as CI artifacts. (macOS done 2026-11-20: `.github/workflows/release.yml` runs `tools/build_release_macos.py` on every `v*` tag on an arm64 runner and attaches the archive; Linux remains open.)
- [x] Anti-virus sanity: avoid self-extracting/packed launchers in the release artifact; a plain `.exe` is preferable for CCRL testers. (Plain executables, no packer or launcher, by construction; no scanner was run.)

**Exit gate:** every artifact runs `uci`/`isready`/`bench` and produces the same node counts for its ISA; the universal exe is within 2% of dedicated builds on both Windows hosts; packages are reproducible from the tagged commit.

---

### Phase 9 â€” CCRL submission and maintenance (ongoing)

**Objective:** a stable public version and a clean maintenance loop.

- [ ] Run the CCRL pre-flight with `cutechess-cli`/`fastchess`: 40/15 and 40/4 games, 128/256 MB hash, 3-4-5 piece Syzygy, own book off, ponder off, no time losses.
- [x] Verify the exact UCI requirements: `uci`, `isready`, `ucinewgame`, `position`, `go`, `quit`; correct `bestmove` even after `stop`; no stray stdout. (2026-11-19: all three release executables pass the 40-step `fastchess --compliance` check; the stop/ponder/banner behaviour is pinned by ADR-0010 and the transcript tests.)
- [ ] Package the Windows x64 zip and host it publicly with source, readme and license.
- [ ] Prepare the announcement text per `CCRL-chess-engine.md`: engine name, version, author/contact as the owner decides, protocol, supported platforms, source link, net description, known limitations.
- [ ] Tag `1.0.0`; freeze the search and net for the submission. Do not submit moving targets.
- [ ] Maintain `docs-new/KnightByte-feature-list.md` and `docs-new/new-bugs.md` after every change.
- [ ] After CCRL results: triage crashes/time losses first, strength second. Any search change requires a new SPRT/regression gate and a new version.

### Project order of work (decided 2026-11-19) and the last step

1. **Phase 6** - Syzygy (3-4-5 piece) and the opening book (Windows / Zen 4).
2. **Finish the performance plan** (`knightbyte_perf_diagnosis.md`): the remaining Zen 4 items (P4 eval/SEE; P1c was done 2026-11-19) and the match gate (legality, time losses, 1/2/4/8 threads, same time control against the predecessor).
3. **Cross-platform validation, last of the engineering work:** Linux x64, macOS, the M4 NEON kernels (P4b), P8, the universal Windows build against the dedicated variants.
4. **The final step: search tuning (`docs-new/enhancements-for-knightbyte.md` 1.9).** Lower the effective branching factor with SPRT-gated experiments (LMR constants, pruning margins, singular extensions, the features that ship off, a final SPSA pass) on the frozen, pinned baseline. Every accepted change gets new pins, an ADR and a baseline entry. Then tag and submit to CCRL.

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
| macOS | `knightbyte-macos-arm64` | `apple-m1`, PGO (ADR-0015) | M4 (M5 Pro open) |
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
- [x] Every advertised option and lever is visible and functional; the count is documented (revised 2026-11-19, ADR-0014: ten public options and sixty levers, `docs/LEVERS.md`; the effect of each lever is proven by `tests/lever_effect.rs`).
- [ ] 1,000-game safety match shows no illegal moves, crashes or time losses.
- [ ] macOS, Linux and Windows artifacts run `uci`/`isready`/`bench` and agree on node counts.
- [ ] The universal Windows `.exe` is within 2% of the dedicated builds on both Windows hosts. (Zen 4 host done, ADR-0011; second host open.)
- [ ] Time-to-depth on the fixed 20-position set is at least as good as C#, with the Phase 7 target met or the gap documented.
- [ ] Baselines are reproducible with the project harnesses (`perf_test.py`, `ebf_compare.py`, `fastchess`, `cutechess-cli`) on the documented host, with engine/net/host metadata recorded.
- [ ] The net is embedded, checksum-verified and bit-identical to the C# evaluation.
- [ ] `docs-new/KnightByte-feature-list.md` and `docs-new/new-bugs.md` are current.
- [ ] The CCRL package is built, tested and ready to announce.
- [x] (Checked 2026-11-19: `src`, `tests` and `tools` contain no engine, author or tree names. The CI workflow installs the FEN verifier package by name; the owner confirmed on 2026-11-19 that this is fine because it is the verifier.) No source file references an unpublished internal tree, and no reference-engine/author names appear in the code.
