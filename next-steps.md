# KnightByte — next steps and open problems

**Repository:** https://github.com/JoergKau/knightbyte-unfinished (public, `main`)  
**History:** starts with the public snapshot of 2026-11-17; the earlier
development history is kept in the private archive `JoergKau/knightbyte-archive`.  
**Last updated:** 2026-11-17, commit `8cf8df4`  
**Rule:** update this document at the end of every working session.

## 0. Current state in one paragraph

Phases 0–3 are complete and verified. Phase 4 (search) is functionally complete:
Zobrist keys, transposition table, evaluation cache, PV table, SEE, move picker,
histories, negamax with quiescence, a deterministic `bench`, time management and
a UCI option catalogue all work and are covered by 143 tests. **The search-parity
gate is still open**: node counts differ from the reference by −2.0 % (`bench 6`),
−8.1 % (`bench 8`), −1.1 % (`bench 12`) and +22.8 % (`bench 18`). Exact parity
requires one coupled port of the remaining reference features (see §3.1); four
attempts to port them piecemeal were measured and reverted.

Verification status on the last run: Ubuntu and Windows CI green, macOS CI job
never acquired a runner (GitHub capacity, not a code failure), 143 tests pass
locally.

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

### Phase 4d — time and options (first slice complete)

- `search/time_manager.rs`: `movetime` and clock allocation with soft/hard
  budgets and move overhead; `go wtime/btime/winc/binc/movestogo` supported.
- `search/options.rs`: declarative catalogue of the 12 public options plus the 29
  research levers that are actually wired into the search. `uci` advertises them;
  `setoption` applies them. `Hash`, `Clear Hash` and `Move Overhead` are
  functional; the book/tablebase/`EvalFile`/`Contempt` names are accepted and
  inert by design.
- The first iteration always completes, so a timed search always reports at least
  `info depth 1` and a `bestmove`.

### Reference oracle (for the parity work)

- Reference source and a published Windows build are on the development host
  in a local checkout on the development host (commit `79b255a`).
- Per-position oracle: `tests/golden/search_parity.json`.
- Capture/compare tool: `tools/capture_bench.py`.
- The reference `bench` output is locale-formatted; the tool strips separators.

---

## 2. Known problems and blockers (read this first)

Each item states the symptom, the evidence and the concrete next action.

### 2.1 Search-parity gate is open (largest open item)

| Run | Reference | KnightByte | Delta |
| :--- | ---: | ---: | ---: |
| `bench 6` | 14,979 | 14,677 | −2.0 % |
| `bench 8` | 48,203 | 44,289 | −8.1 % |
| `bench 12` | 416,533 | 412,114 | −1.1 % |
| `bench 16` | 3,169,859 | 3,584,452 | +13.1 % |
| `bench 18` | 6,925,762 | 8,506,398 | +22.8 % |

Evidence: `docs/SEARCH-PARITY.md` §2 (divergence table), §3–§4 (four measured
attempts at isolated ports, all reverted).

Next action: one coupled port of the remaining reference features (§3.1), not
another piecemeal change. Do not tune toward the numbers.

### 2.2 Missing search features (cause of 2.1 and of most missing levers)

Not implemented yet: singular extensions (on by default in the reference), the
extension trio, history pruning, quiet SEE pruning, internal iterative reduction,
the reference LMR term set, the reference TT replacement (5-bit age, `depth − 2`),
ProbCut, and the reference staged picker with the combined 1/2/4/6-ply quiet
score.

Note: each of these was implemented and measured **alone**; every one widened the
tree at depth 12 and was reverted. They must land together.

### 2.3 54 of the 83 research levers are not exposed

Only 29 levers are wired and advertised (`search/options.rs`). The rest control
features from 2.2 and would be inert knobs today; exposing them is deliberately
deferred until their features exist.

Next action: after the coupled port, extend `LEVERS` and `SearchParams` together;
the tests `lever_defaults_match_the_parameters` and `every_lever_round_trips` keep
the table honest.

### 2.4 Threading, `stop`, `ponder`, `go infinite` are not implemented

The UCI loop is synchronous: `go` blocks until the search returns. `Threads` is
advertised and accepted but the engine always searches on one thread.

Next action (Phase 5): search thread plus stop flag, `stop`/`ponderhit`,
`go infinite`, Lazy SMP workers sharing the TT.

### 2.5 Time-management gaps

Implemented: soft/hard budgets from clocks and `movetime`, move overhead, the
first iteration always completes.

Missing: clock-imbalance weighting (`UseClockImbalance`, scales 50/50), stable
time management, panic mode, best-move safety margins, and per-move node
attribution for the time manager.

### 2.6 Inert public options

`Own Book`, `Book File`, `Book Depth`, `SyzygyPath`, `SyzygyRootProbe`,
`Syzygy50MoveRule`, `EvalFile`, `Contempt` are advertised and accepted but do
nothing. This is documented on purpose; do not report them as working.

### 2.7 Performance (Phase 7, not a correctness problem)

About 0.36 MNPS against the reference's ~1.4 MNPS on the same host. Causes:
full-refresh NNUE evaluation per node, copy-make per node, direction-walking
sliders, and a non-incremental accumulator. All are Phase 7 items; do not mix
them into the parity work.

### 2.8 CI: macOS hosted runners are starved

The `Build and test (macOS)` job can wait many minutes without being acquired
("The job was not acquired by Runner of type hosted"). Ubuntu and Windows pass in
1–2 minutes.

Do not use blocking `gh run watch` while a job is queued; it just waits. Use
`gh run view <id>` snapshots, or run a job again with
`gh run rerun <id> --job <job-id>`.

Options if this keeps happening: move the macOS job to a manual/tag-triggered
workflow, or accept late macOS results (do not weaken the other jobs).

### 2.9 Golden-data duplication

`tests/golden/bench.json` contains the reference totals, while the CI pin
(`BENCH_DEPTH_6_NODES` in `tests/search.rs`) is our own baseline. They disagree by
design until parity is reached. When the gate closes, replace one with the other
and delete the duplicate.

### 2.10 Tooling hazard: PowerShell default encoding

`Set-Content` without `-Encoding utf8` writes non-ASCII characters in the local
code page and can corrupt Rust sources (this happened once and was repaired).
Prefer the editor/write tool for files containing non-ASCII, or write with
`[System.IO.File]::WriteAllText(path, text, [Text.UTF8Encoding]::new($false))`.

### 2.11 Documentation items that only close with features

- `docs-new/new-bugs.md` is empty: no open defects.
- `migration-plan.md` Phase 4 and Phase 5 checklists partially unticked by design.
- `docs/SEARCH-PARITY.md` §8 rules must be followed for any parity change.

---

## 3. Recommended next steps, in order

### 3.1 (Highest value) One coupled search-parity port

Goal: close §2.1. Do it as a single unit, behind one switch, then bisect inside.

Scope, in the reference's own order:

1. Thread the cut-node flag through the recursion.
2. Reference TT replacement: 5-bit age (zero reserved), `depth − 2` per
   generation, same-key refresh rule, no deeper-entry guard.
3. History pruning and the cut-node-aware NMP gate.
4. Reference LMR term set (base `1 + ln·ln/1.75`, history/opponent/cut/PV/killer
   terms, re-search depth shaving) and IIR.
5. Singular extensions with the double/triple tiers, negative extensions, the
   single-response extension (already present) and the per-path cap.
6. The reference staged picker with the combined 1/2/4/6-ply quiet score.
7. Then bisect inside the slice against `tests/golden/search_parity.json`, with
   `bench 12` first and `bench 18` as the deep check.

Definition of done: `bench 6/12/16/18` match exactly; best move matches at every
golden depth; any deliberate deviation recorded in an ADR.

Estimate: 2–3 focused sessions. Keep `main` green after every step; use a
temporary runtime switch for A/B and remove it before committing the winner.

### 3.2 (If parity is paused) Finish 4d and Phase 5 robustness

- Remaining 4d: clock imbalance, stability rules, best-move safety margins.
- Phase 5: search thread with `stop`/`ponderhit`/`go infinite`, Lazy SMP,
  `setoption Threads`, UCI transcript parity, `info` fields (nps, time,
  hashfull).
- Then Phase 6 (Syzygy/book) or Phase 7 (performance), in plan order.

### 3.3 Housekeeping

- When the gate closes, reconcile the bench goldens (§2.9) and update
  `docs/baselines.md`.
- Keep `docs-new/KnightByte-feature-list.md` and `migration-plan.md` in step with
  every merged slice.

---

## 4. How to verify (both machines)

### Windows (this host)

```powershell
cd C:\dev\knightbyte-unfinished
git pull --rebase
$env:KNIGHTBYTE_FEN_VERIFIER = (Get-ChildItem "C:\Program Files\ChessEngines" -Recurse -Filter "stockfish*.exe" | Select-Object -First 1).FullName
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
