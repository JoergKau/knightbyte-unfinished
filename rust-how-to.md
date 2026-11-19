# Rust how-to: KnightByte

KnightByte is a headless **UCI chess engine** written in Rust. It speaks UCI on
stdin/stdout and has no graphical interface. The toolchain is pinned by
`rust-toolchain.toml` (Rust 1.100.0, with `rustfmt` and `clippy`), so any
`cargo`/`rustup` command run inside this directory automatically uses that
toolchain.

All commands below are run from the repository root (`C:\dev\knightbyte-unfinished`).

---

## 1. Build / compile

| Command | What it does |
|---|---|
| `cargo build` | Debug build → `target\debug\knightbyte.exe` (fast compile, slow engine) |
| `cargo build --release` | Optimized build → `target\release\knightbyte.exe` (**use this for actual play**) |
| `cargo check` | Type-checks everything without producing a binary — fastest feedback loop |
| `cargo check --all-targets` | Also type-checks tests and examples |
| `cargo clean` | Deletes the whole `target/` directory (full rebuild next time) |
| `cargo build --release -v` | Verbose build output, useful for debugging build issues |

The release profile (`Cargo.toml`) uses `opt-level = 3`, fat LTO,
`codegen-units = 1`, `panic = "abort"` and symbol stripping, so release builds
take noticeably longer than debug builds — that is expected.

The NNUE network `net/jokausch.nnue` is **embedded into the binary at compile
time**. There is no external network file to ship or configure at runtime.

## 2. Run the engine

```powershell
# Run the release build directly
.\target\release\knightbyte.exe

# Or let cargo build + run in one step
cargo run
cargo run --release
```

Then type UCI commands by hand, e.g.:

```text
uci
isready
ucinewgame
position startpos
go depth 6
quit
```

Supported UCI commands: `uci`, `isready`, `ucinewgame`, `setoption`,
`position` (`startpos` or a six-field FEN), `go` (`depth` / `nodes` /
`movetime` / clock limiters), `bench`, `perft`, `nnueeval`, `quit`.

## 3. Engine diagnostics (from the UCI prompt)

```text
perft 5            ; legal-move node counts for the current position
nnueeval           ; NNUE static evaluation of the current position
bench 12           ; deterministic benchmark (four positions, one thread)
go depth 10        ; fixed-depth search
go movetime 1000   ; search for 1 second
setoption name Threads value 8   ; Lazy SMP: 1 main + 7 helper threads
```

### The node-identity pins (the most important numbers in the project)

`bench` at depths 6 / 12 / 16 / 18 must print **exactly** these node counts
(one thread, default options, 256 MB hash):

| `bench 6` | `bench 12` | `bench 16` | `bench 18` |
|---:|---:|---:|---:|
| 15,528 | 456,308 | 2,815,799 | 6,892,874 |

The engine is a behaviour-preserving port of its predecessor, so a speed-up
must never change these numbers. If a pin moves, your change altered the search
(move order, pruning, evaluation), not just its speed. Revert it. The same
check at every depth from 1 to 18 is:

```powershell
python tools\capture_bench.py compare --engine .\target\release\knightbyte.exe `
    --baseline tests\golden\search_parity.json --depths 1-12,14,16,18   # every delta must be +0
```

## 4. Test

| Command | What it does |
|---|---|
| `cargo test` | Runs all unit + integration tests |
| `cargo test --release` | Same, but with optimizations (perft tests run much faster) |
| `cargo test perft` | Only tests with `perft` in their name/path |
| `cargo test --test uci_smoke` | Runs one integration test file (`uci_smoke`, `search`, `perft`, `legality`, `nnue_parity`, `uci_transcripts`, `thread_stress`, ...; see `tests/`) |
| `cargo test --lib helpers_publish` | Runs unit tests (inside `src/`) whose name contains the text |
| `cargo test -- --nocapture` | Shows `println!` output from tests |
| `cargo test -- --exact golden_perft` | Runs a single test by exact name |

Most tests require an external FEN verifier. Set it before running the suite:

```powershell
# PowerShell (this session)
$env:KNIGHTBYTE_FEN_VERIFIER = "C:\path\to\uci-verifier.exe"
cargo test
```

The verifier is a local test tool and is not distributed with KnightByte.

## 5. Lint and format (CI runs these — check before committing)

```powershell
cargo fmt --all                    # format everything
cargo fmt --all -- --check        # CI-style check, no changes written
cargo clippy --all-targets -- -D warnings   # CI-style lint, fail on warnings
```

Note: `[lints.rust] warnings = "deny"` is set in `Cargo.toml`, so ordinary
`cargo build` also fails on warnings.

## 5a. Measure speed (the performance workflow)

Speed work in this project follows one rule: **one change, one measurement,
one commit; no gain, no merge**. Measure with the release build, on a quiet
machine (AC power, high-performance power plan, nothing else compiling).

```powershell
# Deterministic bench, five runs, median NPS and the node-pin check
python tools\perf_bench.py --engine .\target\release\knightbyte.exe --depth 12 --runs 5

# Primary speed gate: fixed depth 20, identical tree, NPS is the readout
python perf-test\perf_test.py --engine .\target\release\knightbyte.exe --depth 20 --threads 1 --hash 256

# Thread scaling (aggregate NPS over all threads)
python perf-test\perf_test.py --engine .\target\release\knightbyte.exe --depth 18 --threads 1,2,4,8 --hash 256
```

Rules of thumb:

* **Always compare in the same session, back to back.** Run the old binary
  and the new one alternately (copy the old `knightbyte.exe` somewhere first).
  NPS drifts by several percent between runs and by tens of percent between
  sessions; only a ratio measured in one session means anything.
* The first run after a build is a warm-up. Use the median of several runs.
* A 3 % swing is noise; a 10 % swing is a different machine state.
* Never compare NPS between different hosts (Zen 4, Apple M4, ...).
* Never use `bench 12` as the performance gate (it flatters shallow
  searches). It is the node-identity check.

### Profiling (find the real hot spots before optimizing)

```powershell
cargo build --profile release-with-debug      # release speed + symbols (profiling only)
samply record --save-only --unstable-presymbolicate -o prof.json.gz `
    .\target\release-with-debug\knightbyte.exe bench 18
python tools\profile_report.py prof.json.gz .\target\release-with-debug\knightbyte.exe --top 30
```

`samply` is installed with `cargo install samply`. `profile_report.py`
(Windows only) prints the share of samples per function. Guessing is
unreliable: the biggest wins so far (an environment-variable lookup that ate
20 % of the run, a node counter that made 8 threads look broken) were found
by profiling and by measuring, not by reading code.

For per-node counters (moves tried, SEE calls, TT hits, ...):

```powershell
cargo run --release --features node-stats -- bench 12
```

### Profile-guided optimization (PGO)

```powershell
rustup component add llvm-tools-preview            # once
powershell -File tools\pgo_build.ps1                # -> target\pgo\knightbyte.exe
```

The script builds an instrumented binary, runs the bench to record which code
is hot, merges the profile and rebuilds with it (about +7-12 % speed). It
verifies the four node pins and fails if any moves. A profile belongs to the
machine and build it was made on: never reuse it elsewhere, never commit it
(`*.profdata`, `*.profraw` and `pgo-work/` are git-ignored).

### Performance pitfalls we already paid for

* **No environment-variable or file lookups in hot code.** `std::env::var`
  costs a system call sequence; call it once (e.g. in a `OnceLock<bool>`), not
  per node. See `picker_trace()` in `src/search/move_picker.rs`.
* **No allocation per node.** Use fixed-size arrays or buffers created once.
* **Shared state between threads must be rare.** The search publishes node
  counts in blocks of 2048 and polls the stop flag every 1024 nodes; there is
  no atomic access per node.
* **Per-node data stays on the stack or in per-thread buffers**; only the
  transposition table is shared between threads.
* `unsafe` needs a `// SAFETY:` comment naming the invariant (see the BMI2
  `pext` instruction in `src/board/attacks.rs`).

## 6. Dependencies

```powershell
cargo update          # bump dependencies within Cargo.lock semver ranges
cargo tree            # show the dependency tree
cargo doc --open      # build and open rustdoc for the crate
```

There are no runtime dependencies; `sha2` is test-only (network checksum).

## 6a. Where things live (a map for newcomers)

| Path | What is in it |
|---|---|
| `src/board/` | Position, FEN, attack tables (`attacks.rs`, BMI2 `pext`), move generation (`movegen.rs`), the make-free legality test (`legality.rs`), Zobrist keys, perft |
| `src/eval/nnue/` | Network loader, accumulator, SIMD kernels (scalar / AVX2 / AVX-512) |
| `src/search/` | Iterative deepening and negamax (`searcher.rs`, `negamax.rs`), quiescence, move picker, history tables, transposition table (`tt.rs`), thread pool (`pool.rs`), time manager, options |
| `src/uci/` | The UCI protocol loop and parser |
| `tests/` | Integration tests (perft, legality, parity, transcripts, thread stress) and golden data |
| `tools/` | Python/PowerShell helpers: `perf_bench.py`, `capture_bench.py`, `profile_report.py`, `pgo_build.ps1` |
| `docs/`, `docs-new/` | Baseline numbers, decisions (ADRs), bug log (`new-bugs.md`), feature list |

Two ideas explain most of the code you will touch:

* **Copy-make.** `Position` is a small `Copy` struct, so making a move is
  `let next = position.make_move(mv);` and "unmaking" is just dropping `next`.
  The search never mutates the position it was given.
* **Legal moves without making them.** `Legality::new(&position)` computes the
  checkers and pinned pieces once; `is_legal(&position, mv)` then answers for
  each pseudo-legal move without copying the board. `legal_by_making` is the
  slow reference the fast version is tested against.

## 7. Toolchain management

`rust-toolchain.toml` pins the version — `rustup` installs/uses it
automatically. Useful commands:

```powershell
rustup show                # which toolchain this project resolves to
rustup toolchain list      # installed toolchains
rustup update              # update toolchains
rustfmt --version          # confirm the pinned formatter is active
```

## 8. Useful git workflow reminders (from README)

* Never copy or commit `target/` — it contains host-specific build artifacts.
* Pull before a work session; commit after each completed step.
* One step = one commit. Put the measurement table (node pins, before/after
  speed, which gates passed) into the commit message, and update the
  documentation (`docs/baselines.md`, `docs-new/KnightByte-feature-list.md`,
  `migration-plan.md`, `next-steps.md`) in the same commit.

### Windows PowerShell gotchas

* **Do not edit source files with `Get-Content`/`Set-Content`** in Windows
  PowerShell 5: it re-encodes UTF-8 text (em dashes, arrows) into garbage
  such as `â€”`. Use your editor, or write with an explicit UTF-8 encoding.
  (This bug has bitten the project more than once; see B17 in
  `docs-new/new-bugs.md`.)
* When a script drives the engine through pipes, **read its stdout while you
  write to stdin** (a thread or async reader). The `uci` handshake prints
  hundreds of option lines; if nobody reads them the pipe fills up and the
  engine blocks, which looks like a hang.
* `cargo` writes its progress to stderr, which PowerShell may print as a red
  "error" even when the command succeeded. Check the exit code or the last
  `Finished` line.

---

## 9. Playing against KnightByte with a chess GUI

The engine has no GUI of its own — a UCI-compliant chess GUI drives it over
stdin/stdout. You do **not** point the GUI at a source file or the network:
you point it at the **engine executable**.

### The path to give your GUI

| Build | Path |
|---|---|
| Release (recommended for play) | `C:\dev\knightbyte-unfinished\target\release\knightbyte.exe` |
| Debug (slow, for development) | `C:\dev\knightbyte-unfinished\target\debug\knightbyte.exe` |

### Steps

1. Build the release binary first: `cargo build --release`.
2. In your GUI's engine-management dialog ("Add engine" / "Install engine"),
   select the `.exe` path above and declare it as a **UCI engine** (WinBoard/
   XBoard protocol is not supported — KnightByte only speaks UCI).
3. Nothing else needs configuring: the NNUE network is embedded in the binary,
   so there is no `EvalFile`/network file to select in the GUI. (The `EvalFile`
   option is accepted but inert.)

Common GUIs that work this way: Arena, Banksia GUI, Cute Chess (cutechess-cli),
En Croissant, Fritz/ChessBase, SCID vs. PC.

## One practical note

`KNIGHTBYTE_FEN_VERIFIER` is a per-session PowerShell variable, not persisted. Every new terminal needs:

```powershell
$env:KNIGHTBYTE_FEN_VERIFIER = (Get-ChildItem "C:\Program Files\ChessEngines" -Recurse -Filter "stockfish*.exe" | Select-Object -First 1).FullName
```

(You could set it permanently with `[Environment]::SetEnvironmentVariable(...,'User')` if retyping it gets annoying.)