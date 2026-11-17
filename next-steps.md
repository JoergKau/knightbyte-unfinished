# KnightByte — next steps and open problems

**Repository:** https://github.com/JoergKau/knightbyte-unfinished (public, `main`)  
**History:** starts with the public snapshot of 2026-11-17; the earlier
development history is kept in the private archive `JoergKau/knightbyte-archive`.  
**Last updated:** 2026-11-17 (coupled search-parity port)  
**Rule:** update this document at the end of every working session.

## 0. Current state in one paragraph

Phases 0–3 are complete and verified. Phase 4 (search) is functionally
complete and the **coupled parity port has landed**: the reference's cut-node
threading, TT replacement (5-bit generation, `depth − 2` ageing), history
pruning, quiet SEE pruning, the LMR v1 term set with the re-search depth
shave, IIR, the hindsight pair, singular extensions with double/triple tiers,
negative extensions, the combined 1/2/4/6-ply quiet score with re-keyed
continuation tables, the classical capture-ordering values, the reference
aspiration widening and the early TT-cutoff history bonus. 115 unit tests
plus the integration suites pass. **The search-parity gate is still open but
materially closer**: node counts differ from the reference by −12.4 %
(`bench 6`), +2.2 % (`bench 8`), +3.1 % (`bench 12`) and +0.8 % (`bench 18`,
down from +22.8 %). The remaining divergence is seeded by single-node
differences at iteration 3 (see `docs/SEARCH-PARITY.md` §2 and §4a).

Verification status on the last run: Ubuntu and Windows CI green, macOS CI job
never acquired a runner (GitHub capacity, not a code failure), all tests pass
locally on Windows with the Stockfish verifier from
`C:\dev\c#-chess-engines\stockfish`.

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
- The reference's per-iteration `info depth N ... nodes X` lines are a usable
  per-depth oracle: drive the binary with a kept-open stdin pipe, then read
  the info lines. This is what localised the first divergence to iteration 3.

---

## 2. Known problems and blockers (read this first)

Each item states the symptom, the evidence and the concrete next action.

### 2.1 Search-parity gate is open (largest open item)

| Run | Reference | KnightByte | Delta |
| :--- | ---: | ---: | ---: |
| `bench 6` | 14,979 | 13,118 | −12.4 % |
| `bench 8` | 48,203 | 49,239 | +2.2 % |
| `bench 12` | 416,533 | 429,313 | +3.1 % |
| `bench 16` | 3,169,859 | 3,383,906 | +6.8 % |
| `bench 18` | 6,925,762 | 6,979,505 | +0.8 % |

Evidence: `docs/SEARCH-PARITY.md` §2 (divergence table), §4a (the coupled
port record).

Next action: hunt the single-node seeds at iteration 3 (endgame position: 273
vs 272 nodes) with per-node trace tooling; the deep tree is already close, the
mid-depth wobble cascades from these seeds. Do not tune toward the numbers.

### 2.2 The single-node seeds (cause of 2.1)

The trees at depths 1–2 of positions 1–3 are exact. Iteration 3 diverges by
one node (endgame) and +92 nodes (startpos), and by depth 4 the PVs diverge
(different best moves at the root), which swings the mid-depth totals while
the deep shape stays close. The pre-existing quiescence node-accounting
difference at depth 1 of the tactical position (371 vs 364) is a second,
older seed.

Next action: build a small trace tool that dumps a node-id + count ledger for
a fixed position/depth from both engines and diffs them, then read the
reference code at the first divergent node. No guessing.

### 2.3 54 of the 83 research levers are not exposed

Only 31 levers are wired and advertised (`search/options.rs`). The rest
control features that now exist or are pending; expose them after the parity
gate closes.

Next action: after the gate closes, extend `LEVERS` and `SearchParams`
together; the tests `lever_defaults_match_the_parameters` and
`every_lever_round_trips` keep the table honest.

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

### 3.1 (Highest value) Close the parity gate with trace tooling

Goal: close §2.1. The coupled port landed (§4a of SEARCH-PARITY.md); what
remains is the single-node hunt.

1. Build `tools/trace_nodes.py` (or a KnightByte `trace` command): replay a
   fixed position/depth and dump a per-node ledger (ply, move, window,
   depth, score, node id) from both engines; diff the ledgers.
2. Read the reference code at the first divergent node; port the deciding
   formula exactly.
3. Repeat until `bench 6/8/12/16/18` match; best move matches at every golden
   depth; any deliberate deviation recorded in an ADR.

Estimate: 1–2 focused sessions. Keep `main` green after every step.

### 3.2 (After the gate closes) Expose the levers, then Phase 5

- Extend `LEVERS` and `SearchParams` together for the newly ported features.
- Remaining 4d: clock imbalance, stability rules, best-move safety margins.
- Phase 5: search thread with `stop`/`ponderhit`/`go infinite`, Lazy SMP,
  `setoption Threads`, UCI transcript parity, `info` fields (nps, time,
  hashfull).
- Then Phase 6 (Syzygy/book) or Phase 7 (performance), in plan order.

### 3.3 Housekeeping

- When the gate closes, reconcile the bench goldens (§2.9) and update
  `docs/baselines.md`.
- Keep `docs-new/KnightByte-feature-list.md` and `migration-plan.md` in step
  with every merged slice.

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
