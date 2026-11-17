# KnightByte — next steps and open problems

**Repository:** https://github.com/JoergKau/knightbyte-unfinished (public, `main`)  
**History:** starts with the public snapshot of 2026-11-17. There is no private
repository or archive any more; everything lives in the repository above.  
**Last updated:** 2026-11-17 (Phase 4 complete with exact node identity; the ADR-0004 deviation is gone)  
**Rule:** update this document at the end of every working session.

## 0. Current state in one paragraph

Phases 0–3 are complete and verified. Phase 4 (search) is **complete,
including 4d**, and the search is now **node-identical to the reference**:
`bench 6` = 14,979, `bench 12` = 416,533, `bench 16` = 3,169,859 and
`bench 18` = 6,925,762, plus every captured per-position count at depths
1–12, 14, 16 and 18. The earlier "accepted deviation" (ADR-0004) was wrong
about its cause: it was not learned-state drift but eleven concrete porting
differences (`docs-new/new-bugs.md` B18–B28) and one predecessor quirk (Q2),
found with per-node ledgers of both engines (ADR-0005 supersedes ADR-0004;
audit record `docs/audit/migration-audit.md`, first round B1–B17). The full
time-management port is in (allocation
with the tournament cap, sudden-death increment, tactical floor and safety
buffer; panic mode; the legacy PV-stability multipliers; the stable-TM
three-signal scaler with the five-iteration minimum-depth guard; one-sided
time advantage; two-sided clock imbalance; per-move node attribution), and
39 of the 83 research levers are wired and advertised. Next: Phase 5
(threading, `stop`/`ponderhit`, Lazy SMP).

Verification status on the last run: all 172 tests pass locally on Windows
with the Stockfish verifier from `C:\dev\c#-chess-engines\stockfish`;
fmt/clippy clean; `bench 6` pin = 14,979 (the reference total). Node
identity is verified on this Windows host only; macOS and Linux still have
to confirm it (migration-plan G6).

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

`bench 6/12/16/18` = 14,979 / 416,533 / 3,169,859 / 6,925,762, identical to
the reference, and every captured per-position count matches. ADR-0004 (the
accepted deviation) is superseded: its "learned-state drift" explanation
was wrong. If a node count ever moves again, treat it as a search change
(update the pins and say why) and use the ledger method in
`docs/SEARCH-PARITY.md` §8 rule 6 to find the first divergent node.

Known and deliberately reproduced: singular extensions never fire in the
reference (the verification search takes a table cutoff on its own
trigger entry; `docs-new/new-bugs.md` Q1). It was tried behind the
KnightByte-only lever `SingularExcludeTtCutoff` and showed no gain over 904
games (Elo -18.7 +/- 15.2 at 800 games); the default stays the reference
behaviour (ADR-0006). Whether to keep or remove the lever is still to be
decided.

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

### 2.10 CCRL readiness (status 2026-11-17)

The requirement list comes from the predecessor's CCRL notes: UCI, runs under
cutechess-cli/fastchess, handles `uci/isready/ucinewgame/position/go/quit`,
**no crashes, no illegal moves, no time losses**, own book off, ponder off, a
Windows x64 zip with readme and licence, stable public versions only.
CCRL's conditions: 40/15 and 40/4 and 2+1, Hash 128 or 256 MB, 3-4-5 piece
Syzygy optional.

| Item | KnightByte today |
| :--- | :--- |
| UCI handshake, `position`, `go` limiters (`depth`, `nodes`, `movetime`, clocks, `movestogo`, `infinite`) | done; `ponder`, `mate`, `searchmoves` are unsupported but now answered with a legal `bestmove` (B32) |
| No time losses | two real causes fixed (B30 clock restarted late; B32 silent `go`); a 216-search compliance grid (clocks 40 ms-10 s, increments 0/100/1000, `movestogo` none/1/3/40) shows no overrun. **Still open:** a 40/15 and a 40/4 run with zero timeouts on record |
| No illegal moves | `fastchess` flags them; all 904 finished games of the singular-extension experiment (8+0.08, ADR-0006) had no illegal move, time loss or crash. A dedicated self-play legality check of every PV and bestmove is not written yet |
| Repetitions of earlier game positions | fixed (B31); bench could never have shown it |
| Own book off, ponder off | trivially true: the book and ponder options are accepted and inert |
| Hash 128/256 MB | the `Hash` option accepts 1-1024 MB (default 256) |
| Threads | accepted, single-threaded until Phase 5 |
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

### Never commit

`target/`, the internal C# migration tree, `reference_engines/`, local
`session-*.md` notes, `.bundle` files. They are in `.gitignore`. The network and
the golden test data **are** part of the repository.

Because the repository is public, everything pushed is public immediately:
check `git status` and `git diff --stat` before every commit, and never add the
predecessor tree, reference-engine material, tool paths on this machine that
name other engines, or anything that is not meant to be published (see
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
