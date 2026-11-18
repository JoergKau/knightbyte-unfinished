# KnightByte — next steps and open problems

**Repository:** https://github.com/JoergKau/knightbyte-unfinished (public, `main`)  
**History:** starts with the public snapshot of 2026-11-17. There is no private
repository or archive any more; everything lives in the repository above.  
**Last updated:** 2026-11-18 (Phase 5 steps 0–8 done — step 8 built the persistent Lazy SMP pool: `Threads` resizes the pool, helpers search the same job with their own persistent state, only the main thread reports the aggregate node counts; `Threads=1` pins unchanged)  
**Rule:** update this document at the end of every working session.

## 0. Current state in one paragraph

Phases 0–4 are complete and verified, and the search is **node-identical to the
bug-fixed reference**: `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 /
6,892,874 (the pins 14,979 / 416,533 / 3,169,859 / 6,925,762 described the
pre-fix build). The Phase 1 restart adopted the predecessor's bug fixes 143–149
(KnightByte side: B33–B37, all fixed) and re-captured the oracle. Phase 5 (UCI
robustness and threading) has started: **step 0** (oracle transcripts in
`tests/golden/transcripts/`, all five exit questions answered), **step 1**
(full command surface, case-insensitive parser, `GoLimits` structure with the
reference's parsing rules, diagnostics `d`/`eval`/`checkmoves`/`debug`, argv
with `param:Name=Value` overrides and one-shot command lines) and **step 2**
(all 83 reference levers advertised — the 47 unported ones as validated, inert
`wired: false` rows in `src/search/lever_table.rs` — full `setoption`
validation with one test per rule, the handshake's book/tablebase/eval-source
`info string` lines plus the `isready` eval-source line, and the removal of
`SingularExcludeTtCutoff`) are done. **Step 3** completed the `info` lines:
every iteration now reports `depth seldepth score nodes nps hashfull time pv`
in the captured order, with the nps zero-time guard and the occupancy read
once per iteration. The plan's `tbhits 0` is absent from the captured reference lines and is
omitted the same way until Syzygy lands. **Step 4** moved the search onto a
16 MB worker thread: the loop keeps reading commands while the search runs,
`stop` sets a shared flag (polled every 1024 nodes, no per-node clock read),
`quit`/EOF and every state-touching command stop and join first, a stop
before depth 1 finishes answers `info depth 0 score cp <eval> pv <first
legal move>` and exactly one `bestmove` per `go` comes from the worker. The
burst go+quit and one-shot argv semantics were verified against the
reference binary itself (both engines answer the depth-0 line). **Step 5**
implemented `go ponder`/`ponderhit`: the ponder search runs unbounded, a
`stop` without `ponderhit` answers `bestmove 0000` (Q5), and a
`ponderhit` applies the ponder move and starts a fresh search with the
clock beginning at the ponderhit moment; the zero-iteration and burst
cases were verified byte-for-byte against the reference binary. **Step 6**
implemented `go mate N` (stops at the first iteration proving a mate within
N moves, unbounded otherwise) and `go searchmoves` (root restriction; empty
and illegal-only lists are ignored; the restricted cold run is
node-identical to the reference binary). Every `go` limiter is now
implemented. **Step 7** moved the transposition table and the evaluation
cache behind the shared `Arc` handle (`src/search/tables.rs`) that the
step-8 workers will clone per search, and fixed **B38**: the table clear
zeroes in parallel, so even the first clear after a fresh 1 GB `Hash`
resize costs 59–64 ms. **Step 8** built the persistent Lazy SMP pool
(`src/search/pool.rs`): `Threads` resizes the pool without touching the
main searcher's state; every helper owns its search state (history,
correction, killers, PV, stacks, picker) and runs the same job — same root,
same limits minus the time budgets — publishing its node count in 2048-node
blocks; only the main thread runs the time manager and reports
(`reported_nodes` = own exact count + helpers' published blocks; the
`go nodes` aggregate check happens at the 1024-node poll). Measured:
`Threads=1` stays exact, `Threads=4` runs ~2.1× the single-threaded NPS on
the depth-12 startpos search. The step-0
capture found **predecessor
bug 150** (`setoption Hash` above the declared maximum crashed the reference
via an overflowing `TTable.Resize` allocation); it was fixed in the
predecessor the same day (commit `acaff1f`, full suite 745/745). KnightByte
was not affected (spin clamping). Next: Phase 5 step 9 (helper diversity and
the multi-thread stress gates).

Verification status on the last run: all 244 tests pass locally on Windows
with the Stockfish verifier from `C:\dev\c#-chess-engines\stockfish`;
fmt/clippy clean; all four bench pins exact and `capture_bench.py compare`
shows every per-position delta at +0. Node identity is verified on this
Windows host only; macOS and Linux still have to confirm it (migration-plan G6).

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

### Phase 4c - search (complete, node-identical to the reference)

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

### 2.1 Search-parity gate — passed with exact node identity (ADR-0005)

`bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874, identical to
the bug-fixed reference (the Phase 1 restart pins; the pre-fix totals
14,979 / 416,533 / 3,169,859 / 6,925,762 described the old build), and every
captured per-position count matches. ADR-0004 (the
accepted deviation) is superseded: its "learned-state drift" explanation
was wrong. If a node count ever moves again, treat it as a search change
(update the pins and say why) and use the ledger method in
`docs/SEARCH-PARITY.md` §8 rule 6 to find the first divergent node.

Known and deliberately reproduced: singular extensions never fire in the
reference (the verification search takes a table cutoff on its own
trigger entry; `docs-new/new-bugs.md` Q1). It was tried behind the
KnightByte-only lever `SingularExcludeTtCutoff` and showed no gain over 904
games (Elo -18.7 +/- 15.2 at 800 games); the default stayed the reference
behaviour (ADR-0006). The lever was removed in Phase 5 step 2 (2026-11-18):
the reproduced behaviour is the only code path now, recorded in a comment at
the verification site (`src/search/negamax.rs`).

### 2.2 Remaining time-management gaps — resolved

Phase 4d is complete: allocation, panic mode, the stability rules, the
clock-imbalance weighting, the minimum-depth guard and per-move node
attribution are ported and unit-tested; the bench pins are unchanged.

Nothing pending here; the tournament-behaviour validation (does the new
allocation actually use the clock well?) is a Phase 7 tuning item, not a
correctness gap.

### 2.3 Lever exposure — the surface is complete, the wiring is not

All 83 reference levers are advertised under their reference names, defaults
and ranges (step 2, `src/search/lever_table.rs`), plus three KnightByte-only
research levers. 36 reference levers are wired into the search; the other 47
carry `wired: false`: they validate, are accepted, and stay inert until their
feature lands (ProbCut, SEE-reach, LMR v2 and its terms, quiescence checks,
book, tablebases). A golden-file test pins the whole catalogue against
`tests/golden/levers.json` and allows no name outside the 83 plus the marked
KnightByte-only three.

Next action: still open is a test that setting each *wired* lever changes the
search on a probe position (the C# suite has this pattern).

### 2.4 Threading, `stop`, `ponder` — the search thread landed; Lazy SMP and ponder are open

Step 4 (2026-11-18) is done: `go` runs on a 16 MB worker thread, the UCI
loop keeps reading commands while it searches, `stop` sets a shared flag
that the search polls every 1024 nodes (the exact `go nodes` limit stays a
per-node integer comparison), and `quit`/EOF plus every state-touching
command stop and join a running search first. Step 8 (2026-11-18) added the
Lazy SMP pool: `setoption Threads` resizes it, helpers persist with their
own search state, and the main thread's result is the answer.

Next action (Phase 5): step 9 stress gates (1, 2, 4, 8 threads, 500 random
cycles), then step 10-13.

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

### 2.10 CCRL readiness (status 2026-11-17)

The requirement list comes from the predecessor's CCRL notes: UCI, runs under
cutechess-cli/fastchess, handles `uci/isready/ucinewgame/position/go/quit`,
**no crashes, no illegal moves, no time losses**, own book off, ponder off, a
Windows x64 zip with readme and licence, stable public versions only.
CCRL's conditions: 40/15 and 40/4 and 2+1, Hash 128 or 256 MB, 3-4-5 piece
Syzygy optional.

| Item | KnightByte today |
| :--- | :--- |
| UCI handshake, `position`, `go` limiters (`depth`, `nodes`, `movetime`, clocks, `movestogo`, `infinite`, `ponder`, `mate`, `searchmoves`) | all done (Phase 5 steps 1–6; the ponder/mate/searchmoves behaviours verified against the reference binary) |
| No time losses | two real causes fixed (B30 clock restarted late; B32 silent `go`); a 216-search compliance grid (clocks 40 ms-10 s, increments 0/100/1000, `movestogo` none/1/3/40) shows no overrun. **Still open:** a 40/15 and a 40/4 run with zero timeouts on record |
| No illegal moves | `fastchess` flags them; all 904 finished games of the singular-extension experiment (8+0.08, ADR-0006) had no illegal move, time loss or crash. A dedicated self-play legality check of every PV and bestmove is not written yet |
| Repetitions of earlier game positions | fixed (B31); bench could never have shown it |
| Own book off, ponder off | trivially true: the book and ponder options are accepted and inert |
| Hash 128/256 MB | the `Hash` option accepts 1-1024 MB (default 256) |
| Threads | implemented (Phase 5 step 8, Lazy SMP pool; multi-thread play is stress-gated in step 9) |
| Syzygy | not implemented (optional for CCRL, Phase 6) |
| Windows x64 zip, README, licence | README and MIT licence exist; the release packaging, a tagged public release and a Linux build are Phase 8 |

The lesson of this round: node identity says nothing about *time* or *game
history*. Both bugs lived in code that bench never runs. Before a CCRL
submission, run real games at CCRL's time controls and read the `Timeouts` and
`Crashed` columns of the result table, not just the exit code.

### 2.9 Documentation items that only close with features

- `migration-plan.md` Phase 5 checklists still open by design (threading is
  next).
- The audit's open items: `docs/audit/migration-audit.md` (the forensics are
  closed; the SEE, time-management and option/lever rows were not line-by-line
  audited in the second round, and node identity is the evidence for them).

---

## 3. Recommended next steps, in order

**State (2026-11-18, end of session):** the predecessor bugs are fixed (143–149 at `bffe98a`/`f274b39`, 150 at `acaff1f`) and the migration has resumed. Phase 5 steps 0–7 are complete: oracle transcripts, the command surface and parser, the option surface with full `setoption` validation, the complete `info` lines, the search thread with the shared stop flag, `go ponder`/`ponderhit`, `go mate`/`searchmoves` (every `go` limiter implemented), and the shared-tables refactor with the parallel-zeroing bounded clear (B38 fixed). **Step 8 (persistent workers and `Threads`, the Lazy SMP core) is next.** Details in `migration-plan.md`, Phase 5.

### 3.1 Phase 5 — UCI robustness and threading (in progress, steps 0–8 done)

- ~~Step 2: option surface and `setoption` validation~~ — done (2026-11-18):
  full lever catalogue advertised and pinned against `levers.json`,
  catalogue-driven `setoption` validation with one test per rule, handshake
  `info string` lines and the `isready` eval-source line,
  `SingularExcludeTtCutoff` removed, `Clear Hash`/`ucinewgame` measured at
  `Hash 1024` (B38 logged; fix belongs to step 7).
- ~~Step 3: complete `info` lines~~ — done (2026-11-18): every iteration
  reports `depth seldepth score nodes nps hashfull time pv` in the captured
  order; `nps` guards the zero time; `hashfull` is read once per iteration.
  The plan's `tbhits 0` is absent from the captured reference lines and is
  omitted the same way until Syzygy lands. Recorded for step 12: with
  identical score/nodes, the reference's PV *tail* can differ from
  KnightByte's (startpos depth 2: `e2e4 c7c5` vs `e2e4 c7c6`) — derived
  reporting the reference refreshes between iterations; the search is
  node-identical.
- ~~Step 4: search thread with `stop`~~ — done (2026-11-18): 16 MB worker
  thread, shared stop flag polled every 1024 nodes (no per-node clock read),
  `quit`/EOF and state-touching commands stop and join first, the Q8
  single-legal-move fast path, and the stop-before-depth-1 depth-0 answer;
  the burst go+quit and one-shot argv semantics verified against the
  reference binary. The former burst-style smoke tests now use the
  interactive pattern (wait for `bestmove`, then `quit`).
- ~~Step 5: `go ponder`/`ponderhit`~~ — done (2026-11-18): unbounded ponder
  search printing its iterations, `stop`/burst-`quit` answer `bestmove 0000`
  (Q5, plus the interrupted-search depth-0 line — byte-identical to the
  reference binary), `ponderhit` applies the ponder move and starts the
  fresh search with the clock at the hit; with zero completed ponder
  iterations the reference re-searches the unchanged position (verified
  against it), and so does KnightByte.
- ~~Step 6: `go mate`/`searchmoves`~~ — done (2026-11-18): the mate limiter
  stops at the first iteration proving a mate within N (unbounded
  otherwise, other limits still apply); `searchmoves` restricts the root
  (empty and illegal-only lists are ignored, restricted cold run
  node-identical to the reference binary). Every `go` limiter is
  implemented; the step-1 fallback is gone.
- ~~Step 7: shared tables refactor~~ — done (2026-11-18): both tables moved
  behind the `SharedTables` Arc handle (`src/search/tables.rs`) for the
  step-8 workers; resize keeps its exclusive-borrow guarantee through the
  join-first rule. B38 fixed: parallel zeroing bounds every clear at
  59–64 ms (was ~190–275 ms for the first clear after a 1 GB resize).
- ~~Step 8: persistent workers and `Threads`~~ — done (2026-11-18): the
  pool grows and shrinks with `setoption Threads` without touching the main
  searcher's state; each helper owns its search state, runs the same job
  (limits minus the time budgets), and publishes nodes in 2048-node blocks;
  only the main thread runs the time manager and reports. Measured:
  `Threads=4` ≈ 2.1× the single-threaded NPS (depth 12 startpos);
  `Threads=1` pins unchanged.
- Steps 9–13: helper-diversity stress gates, output decision, lever
  independence, UCI transcript tests, match gates.
- Steps 10–13: output decision, lever independence, UCI transcript tests
  against the step-0 captures, match gates (1,000 games vs the reference).

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
# per-depth comparison against the captured oracle (every delta must be +0)
python tools\capture_bench.py compare --engine .\target\release\knightbyte.exe `
    --baseline tests\golden\search_parity.json --depths 1-12,14,16,18
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

All work goes to one public repository,
<https://github.com/JoergKau/knightbyte-unfinished> (branch `main`). There is no
private repository, private mirror or archive, and no second remote. Each
machine clones it once, then pulls and pushes; do not copy folders (or git
bundles) between machines.

```bash
git clone https://github.com/JoergKau/knightbyte-unfinished.git   # once per machine
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

### What is committed

Everything inside the repository is committed and pushed during the migration,
including the migration documents, the golden files and the notes. The only
exclusion is `target/` (host-specific build output, never committed). The
predecessor tree and the migration files live in the migration folder outside
the repository, not in the repository itself.

The repository is public, so everything pushed is public immediately: check
`git status` and `git diff --stat` before every commit. The publishing policy
for predecessor material is decided at the end of the migration (see
`project-rules.md`).

---

## 6. Publishing and CCRL

Nothing needs to be made public any more: the source and the MIT licence are
already published in the repository, which is what CCRL asks for. What is still
open for a submission is the release artifact (a tagged GitHub Release with the
Windows x64 zip, readme and licence; Phase 8) and the stability evidence in
§2.10.

GitHub Actions on a public repository is not minute-limited the way a private
one is. The macOS runner queue can still be slow (§2.7); that is a latency
problem, not a budget one.

---
