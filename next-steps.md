# KnightByte — next steps and open problems

**Repository:** https://github.com/JoergKau/knightbyte-unfinished (public, `main`)  
**History:** starts with the public snapshot of 2026-11-17; the earlier
development history is kept in the private archive `JoergKau/knightbyte-archive`.  
**Last updated:** 2026-11-17 (Phase 4 complete, including 4d time management)  
**Rule:** update this document at the end of every working session.

## 0. Current state in one paragraph

Phases 0–3 are complete and verified. Phase 4 (search) is **complete,
including 4d**: the migration audit found and fixed fourteen porting defects
(`docs-new/new-bugs.md` B1–B17, `docs/audit/migration-audit.md`), depths 1–4
are exact at all four bench positions, `bench 6` is within 0.1 % of the
reference (14,963 vs 14,979) and `bench 18` within 1.8 %. The parity gate is
**resolved as a documented deviation** (ADR-0004): the determinism pin
(`bench 6` = 14,963, asserted in `tests/search.rs`) replaces node identity as
the regression discipline. The full time-management port is in (allocation
with the tournament cap, sudden-death increment, tactical floor and safety
buffer; panic mode; the legacy PV-stability multipliers; the stable-TM
three-signal scaler with the five-iteration minimum-depth guard; one-sided
time advantage; two-sided clock imbalance; per-move node attribution), and
39 of the 83 research levers are wired and advertised. Next: Phase 5
(threading, `stop`/`ponderhit`, Lazy SMP).

Verification status on the last run: all 159 tests pass locally on Windows
with the Stockfish verifier from `C:\dev\c#-chess-engines\stockfish`;
fmt/clippy clean; `bench 6` pin = 14,963.

---

## 1. What is finished

### Phases 0–3

- **Phase 0:** repo, Rust skeleton, UCI lifecycle, CI, CPU selector, Windows
  universal-build ADR.
- **Phase 1:** oracle capture (`bench 12/16/18`, perft, 15 NNUE FENs), the public
  option catalogue and the 83 research levers.
- **Phase 2:** board, FEN, legal move generation, perft, copy-make position.
- **Phase 3:** v5 network loader, scheme-2 features, scalar accumulator,
  bit-exact evaluation on all captured FENs.

### Phase 4a — keys and shared tables

- Zobrist keys with the reference generator (`search`/`board/zobrist.rs`), key
  pinned by a golden test; the start position hashes to `0x48E86881DC3DFD16` in
  both engines.
- Lockless 16-byte TT entries, 64-byte buckets, aging, prefetch (`search/tt.rs`).
- Eval cache (`search/eval_cache.rs`) and triangular PV table (`search/pv.rs`).

### Phase 4b — ordering

- SEE (`search/see.rs`): reference values `{109, 403, 435, 679, 1242, 20000}`,
  swap list for ordering, boolean form for pruning, pins, king rule.
- Tiered picker (`search/move_picker.rs`), killers (`search/killers.rs`, four per
  ply), histories (`search/history.rs`) with reference gravity bounds, linear
  bonuses/maluses, bad-quiet malus lists and 7/8 ageing per iteration.

### Phase 4c — search (functionally complete, parity open)

- Iterative deepening, aspiration windows, PVS, TT probe/store, null move,
  reverse futility (formula + damped spill), razoring, LMP, futility, SEE capture
  pruning, check and single-response extensions, correction history, 50-move and
  twofold-repetition draws.
- Check-aware quiescence with TT probe, SEE filtering and delta pruning.
- Deterministic `bench [depth]` (Hash 256 MB, one thread, cleared table per
  position) and `go depth|nodes`.
- Tests: mate in one/two, playable PV, deterministic nodes, node limits, legal
  bench moves, reproducible bench totals.

### Phase 4d — time and options (complete)

- `search/time_manager.rs`: stateful manager — `movetime` (soft 95 % / hard
  100 %) and clock allocation (tournament share with the ten-move cap,
  sudden-death clock/30 + full increment, tactical floor, safety buffer,
  hard = min(4× allocation, clock/4), overhead-clamped soft limit);
  `go wtime/btime/winc/binc/movestogo/movetime/infinite` supported.
- Stopping: panic mode (1.5× on a collapsing score), the legacy
  PV-stability multipliers, and the stable-TM three-signal scaler
  (best-move ladder × score ladder × node-fraction term, clamped 0.12–2.45
  and to hard − overhead) with the five-iteration minimum-depth guard.
- One-sided time-advantage reinvestment and the two-sided clock-imbalance
  scaler (off by default, supersedes the advantage term).
- Root-move node attribution (`nodes_per_move`) feeds the node-fraction
  term; the UCI layer hands the manager a `TimeSetup` (fixed/clock/infinite).
- `search/options.rs`: declarative catalogue of the 12 public options plus
  the 39 research levers wired into the search (now including
  `UseStableTimeManagement`, `UseClockImbalance`,
  `ClockImbalanceAheadScale/BehindScale`, `UseLmrHistoryPrecision`,
  `UseRefSeeThresholds`, `UseNegativeExtensions`, `UseTrioHindsight`,
  `HistPruneMargin`). `uci` advertises them; `setoption` applies them.
  `Hash`, `Clear Hash` and `Move Overhead` are functional; the
  book/tablebase/`EvalFile`/`Contempt` names are accepted and inert by
  design.
- The first iteration always completes, so a timed search always reports at
  least `info depth 1` and a `bestmove`.

### Reference oracle (for the parity work)

- Reference source and a published Windows build are on the development host
  in a local checkout on the development host (commit `79b255a`).
- Per-position oracle: `tests/golden/search_parity.json`.
- Capture/compare tool: `tools/capture_bench.py`.
- The reference `bench` output is locale-formatted; the tool strips separators.
- The reference's per-iteration `info depth N ... nodes X` lines are a usable
  per-depth oracle: drive the binary with a kept-open stdin pipe, then read
  the info lines. This is what localised the first divergence to iteration 3.

---

## 2. Known problems and blockers (read this first)

Each item states the symptom, the evidence and the concrete next action.

### 2.1 Search-parity gate — resolved (ADR-0004)

Depths 1-4 exact at all four bench positions; `bench 6` -0.1 % (14,963 vs
14,979); the deep tree within ~2 % (`bench 18`: 6,797,622 vs 6,925,762). The
residual divergence (a learned-history-state seed from depth 5 on) is
accepted and documented in `docs/decisions/ADR-0004-search-parity-deviation.md`.
Do not reopen node-count forensics without a concrete defect symptom.

### 2.2 Remaining time-management gaps — resolved

Phase 4d is complete: allocation, panic mode, the stability rules, the
clock-imbalance weighting, the minimum-depth guard and per-move node
attribution are ported and unit-tested; the bench pins are unchanged.

Nothing pending here; the tournament-behaviour validation (does the new
allocation actually use the clock well?) is a Phase 7 tuning item, not a
correctness gap.

### 2.3 Lever exposure (44 of the 83 research levers are not exposed)

39 levers are wired and advertised (`search/options.rs`). The remaining 44
gate reference features that ship off or are not ported (ProbCut, SEE-reach,
LMR v2 and its terms, quiescence checks, book, tablebases); each lever is
exposed when its feature lands.

Next action: still open is a test that setting each lever changes the search
on a probe position (the C# suite has this pattern).

### 2.4 Threading, `stop`, `ponder` are not implemented

`go infinite` is accepted (the manager runs in infinite mode), but the UCI
loop is synchronous: `go` blocks until the search returns, and there is no
way to interrupt it. `Threads` is advertised and accepted but the engine
always searches on one thread.

Next action (Phase 5): search thread plus stop flag, `stop`/`ponderhit`,
Lazy SMP workers sharing the TT.

### 2.5 Inert public options

`Own Book`, `Book File`, `Book Depth`, `SyzygyPath`, `SyzygyRootProbe`,
`Syzygy50MoveRule`, `EvalFile`, `Contempt` are advertised and accepted but do
nothing. This is documented on purpose; do not report them as working.

### 2.6 Performance (Phase 7, not a correctness problem)

About 0.36 MNPS against the reference's ~1.4 MNPS on the same host. Causes:
full-refresh NNUE evaluation per node, copy-make per node, direction-walking
sliders, and a non-incremental accumulator. All are Phase 7 items; do not mix
them into other work.

### 2.7 CI: macOS hosted runners are starved

The `Build and test (macOS)` job can wait many minutes without being acquired
("The job was not acquired by Runner of type hosted"). Ubuntu and Windows pass in
1–2 minutes.

Do not use blocking `gh run watch` while a job is queued; it just waits. Use
`gh run view <id>` snapshots, or run a job again with
`gh run rerun <id> --job <job-id>`.

Options if this keeps happening: move the macOS job to a manual/tag-triggered
workflow, or accept late macOS results (do not weaken the other jobs).

### 2.8 Tooling hazard: PowerShell default encoding

`Set-Content` without `-Encoding utf8` writes non-ASCII characters in the local
code page and can corrupt Rust sources (this happened twice — the second time
through a `-replace | Set-Content` pipeline, see B17). Prefer the editor/write
tool for files containing non-ASCII, or write with
`[System.IO.File]::WriteAllText(path, text, [Text.UTF8Encoding]::new($false))`.

### 2.9 Documentation items that only close with features

- `migration-plan.md` Phase 5 checklists still open by design (threading is
  next).
- The audit's open items: `docs/audit/migration-audit.md` (only the
  ADR-0004-moot forensics remain).

---

## 3. Recommended next steps, in order

### 3.1 Phase 5 — UCI robustness and threading (next)

- Search thread with `stop`/`ponderhit`; `go ponder`.
- Lazy SMP workers sharing the TT, `setoption Threads`.
- UCI transcript tests, `info` fields (nps, time, hashfull).
- Optional follow-up from 4d: a probe-position test that every lever
  measurably changes the search.

### 3.2 Phase 7 — performance, then releases

- Incremental NNUE accumulator, SIMD kernels (NEON/AVX2/VNNI), slider and
  make/move optimizations; the 0.36 → ≥1.5 MNPS goal at the pinned node
  counts.
- Then Phase 8 (macOS/Linux/Windows artifacts) and Phase 6 (Syzygy/book) per
  the plan order.

---

## 4. How to verify (both machines)

### Windows (this host)

```powershell
cd C:\dev\knightbyte-unfinished
git pull --rebase
$env:KNIGHTBYTE_FEN_VERIFIER = "C:\dev\c#-chess-engines\stockfish\stockfish-windows-x86-64-universal.exe"
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release
```

### macOS

```bash
cd ~/dev/KnightByte
git pull --rebase
KNIGHTBYTE_FEN_VERIFIER=/opt/homebrew/bin/stockfish cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release
```

### Parity measurement

```powershell
# per-depth comparison against the captured oracle
python tools\capture_bench.py compare --engine .\target\release\knightbyte.exe `
    --baseline tests\golden\search_parity.json --depths 6,8,10,12,14
```

### Smoke transcript

```text
uci -> uciok, option lines
isready -> readyok
position startpos moves e2e4 e7e5
go depth 4 -> info depth ... bestmove
setoption name Hash value 32
go movetime 500 -> at least info depth 1 and a bestmove
quit
```

Expected `perft 3` on startpos: `Nodes: 8902`; `nnueeval` on startpos: `27`.

---

## 5. Git workflow

The code lives in a private GitHub repository. Each machine clones once, then
pulls and pushes; do not copy folders between machines.

```bash
git pull --rebase
git status
# ...work...
git add -A
git commit -m "short description"
git push            # if rejected: git pull --rebase, then push again
```

Useful: `git log --oneline -5`, `gh run list --limit 3`,
`gh run view <id>` (never `gh run watch` while a job is queued), and
`gh run view --log-failed` after a red run.

### Never commit

`target/`, the internal C# migration tree, `reference_engines/`, local
`session-*.md` notes, `.bundle` files. They are in `.gitignore`. The network and
the golden test data **are** part of the repository.

---

## 6. When to publish

The repository can stay private during development. CCRL requires publicly
available source and a license, so before the first submission either make the
repository public or publish a source release/mirror.

Private-repository Actions minutes are limited; macOS and Windows runners consume
more than Ubuntu. If the budget becomes a problem, keep the default CI
Ubuntu-only and run the macOS/Windows jobs manually or on tags (see §2.8).
