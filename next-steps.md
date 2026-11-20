# KnightByte — next steps and open problems

**Repository:** https://github.com/JoergKau/knightbyte-unfinished (public, `main`)  
**History:** starts with the public snapshot of 2026-11-17. There is no private
repository or archive any more; everything lives in the repository above.  
**Last updated:** 2026-11-20 (Linux x64 release complete: one AVX2 PGO binary, packaging and tag job, ADR-0016; next: search tuning)
**Rule:** update this document at the end of every working session.

## ORDER OF WORK (decided 2026-11-19)

1. ~~**Phase 6: Syzygy (3-4-5 piece) and the opening book**~~ — **complete 2026-11-19** (see the session note below).
2. **Finish `knightbyte_perf_diagnosis.md`** on Zen 4: ~~P1c (early-exit forced-reply count)~~ done 2026-11-19 (+1.1 %), P4 eval path ~~(fused push, madd head, fused refresh, lazy-pin SEE)~~ done 2026-11-19 (docs/baselines.md 7.13-7.16), ~~the match gate~~ done (500 games at 1 thread: 75.1 %, no errors; multi-thread matches run by the owner: no errors, docs/baselines.md 7.19) (legality, time losses, 1/2/4/8 threads, same time control against the predecessor).
2a. **Windows release engineering - done 2026-11-19** (ADR-0011): three PGO executables built, checked and zipped by `tools/build_release.py`; the universal-build gate measured on the Zen 4 host; static C runtime (B51); start-up guard; README. Open on Windows only what needs another machine: the second Windows host (i7-10750H: `v3` against `v2`, smoke test, baseline), a clean Windows 10 machine, and the owner's multi-thread match figures.
3. ~~**macOS validation and release**~~ — **complete 2026-11-20** (AArch64 build fixed under B52, NEON kernels, one `apple-m1` binary for M1-M5, `tools/build_release_macos.py`, tag workflow; ADR-0015). ~~Remaining here: **Linux x64** release (P8) only.~~
4. **Last step: search tuning for a lower effective branching factor** (`docs-new/enhancements-for-knightbyte.md` 1.9): EBF comparison on Zen 4 showed KnightByte at 2.01 vs 1.79 for a top engine of its class (identical to the predecessor at a fixed depth, so no gain yet); SPRT-gated experiments (LMR constants, pruning margins, singular extensions retest, the features that ship off, SPSA) on the frozen and pinned baseline. Then tag and submit to CCRL.

## NEXT SESSION: search tuning; Windows, macOS and Linux are finished

**2026-11-20 (Linux x64 finish):** the last platform is done. One
`x86_64-unknown-linux-gnu` binary compiled with `-Ctarget-cpu=x86-64-v3` (AVX2
baseline, Intel Haswell 2013+/AMD Zen+) and PGO by default, packaged by
`tools/build_release_linux.py` and built on every `v*` tag by the
`ubuntu-22.04` job in `.github/workflows/release.yml` (glibc 2.35 floor).
Linux ARM is not supported. Bench 6/12/16/18 = **15,528 / 456,308 / 2,815,799 /
6,892,874** on the Ryzen 5 5600H in the packaged binary, with the forced scalar
and forced AVX2 kernels reporting the same totals. The PGO build is **+5.0 /
+9.0 / +8.9 %** against the plain `x86-64-v3` build at bench 12/16/18 (ABBA
pairs, A/A control +1.4 %); the AVX2 kernels are 3.94x the forced scalar
kernels at bench 12. Absolute: 1.68-2.19 MNPS at bench 12-18. See ADR-0016 and
`docs/baselines.md` 7.24.

**2026-11-20 (macOS Apple Silicon finish):** the AArch64 target did not compile at all (B52: per-target `KernelKind` variants left the dispatch non-exhaustive), so the macOS work started with a portability fix. The NEON backend (`KernelKind::Neon`) is now the single AArch64 kernel set — 128-bit row, refresh and SCReLU-head kernels, bit-exact against scalar and selected through `KNIGHTBYTE_FORCE_ISA=neon|scalar`. On the M4, bench 12/16/18 rose from 0.43-0.44 MNPS (scalar) to **1.72-1.89 MNPS** (3.9-4.4x) with all four bench pins exact. The release is deliberately one binary: `aarch64-apple-darwin` with `-Ctarget-cpu=apple-m1`, which runs on every Apple M processor (M1-M5); Intel Macs are unsupported and there are no per-chip variants (ADR-0015). PGO is on by default (`--no-pgo` for the plain build): interleaved three-run pairs on the M4 give **+6.0 / +7.2 / +7.5 %** at bench 12/16/18 (1.95-2.01 MNPS PGO against 1.84-1.87 plain), pins exact. `tools/build_release_macos.py` runs the lever gate, verifies `uciok`, the four pins, the forced scalar/NEON identity and the arm64-only slice, and packs the `tar.gz` with `SHA256SUMS.txt` and `manifest.json`; `.github/workflows/release.yml` does it on every `v*` tag. Verified on the M4: `cargo test --all-targets` (with the Stockfish verifier), fmt/clippy clean, `perft 3` = 8,902, `nnueeval` = 27, release archive unpacked and re-run. The M5 Pro smoke test and Linux x64 remain open.

**2026-11-19 (Windows finish):** the performance plan is complete on Zen 4 (P1c, P4a-P4e, PGO, the match gate: docs/baselines.md 7.12-7.20; whole session +17.3 % balanced, then PGO +6.4 %; same-session depth 20 about 2.25x the predecessor; 500 games at 1 thread 75.1 % with no time loss, illegal move or crash). `tools/ab_bench.py` is the balanced A/B tool (a back-to-back pair is biased by 2-4 %). The Windows release is built by `tools/build_release.py` (ADR-0011). `nnueregions`/`searchstats` are real now (ADR-0012) and the allocation audit is done (docs/baselines.md 7.21). `tbhits` is decided and done (ADR-0013: printed only while tablebases are in use). **The lever surface is decided and done (ADR-0014): `EvalFile` and `Contempt` are removed and exactly sixty levers are visible, all functional (`docs/LEVERS.md`); the owner tests them next.** No feature item is left on the Windows list. The tests of the sixty levers are prepared in `docs/audit-tests-KB.md` (60 arms, measured pre-flight totals, `tools/lever_campaign.py`); the campaign is the owner's and has not been run.

**2026-11-19 (Phase 6):** Syzygy and the opening book are ported and tested.
WDL/DTZ probing for 3-4-5 piece sets (and larger material names up to seven
pieces), depth-unconditional search and quiescence probes, 50-move-aware DTZ
values, root DTZ ranking (Kd2-e2 on the reported 4-man fixture; the toggle-off
lazy shuffle Kd2-c1 reproduces), and the `.ctg` reader with signature
validation, weighted move choice and annotations. `Own Book` defaults off;
`Book File` loads explicitly (empty value auto-detects next to the executable
or in `books/`); `SyzygyPath`/`SyzygyRootProbe`/`Syzygy50MoveRule` are
functional and report their state. Tests: 318 (the full-set tests skip when
`KNIGHTBYTE_SYZYGY_PATH` is unset, the real-book tests when no book is
present). Bench pins unchanged: 15,528 / 456,308 / 2,815,799 / 6,892,874. The
table files are mapped lazily through `memmap2` (ADR-0009); the 3-man test
tables are in `tests/data/tb/`; the book tests use a synthetic fixture plus a
local real book, so no third-party book is published. Quirk Q9 (a failed DTZ
load of a present but corrupt file poisons WDL for the same material) was
reproduced first, then fixed in both engines as Bug 151 / B49 (see the quirk
clean-up below).

**Quirk clean-up (2026-11-19, ADR-0010).** Q5-Q9 are fixed in both engines and
only Q1 remains in the register: a stopped ponder search answers its best move
(Q5); the banner never reaches stdout (Q6); the warm-up uses a private table
(Q7); the single-legal-move instant answer serves clock/`movetime` searches
only (Q8); one tablebase error flag per table kind (Q9 = Bug 151 = B49). The
Rust regression test also found B50 (a failed DTZ read was reported as a
success, so a WDL-only table set chose a bogus root move). Predecessor first:
755/755 tests, published binary rebuilt, `bench 6` unchanged. KnightByte: 318
tests, bench pins unchanged, step-0 transcripts re-captured from the fixed
predecessor, differentials clean. **Anything that says "reproduces the
predecessor" must now name the 2026-11-19 predecessor build.**

**Post-phase audit (2026-11-19): lesson and result.** The ported unit tests
passed while four real defects were present (B45-B48: capture sign, trailing
placement groups, the 3-v-3 name family, the book's en-passant capturer). They
were found by differential runs against an independent tablebase reader and
the predecessor binary: `tools/tb_compare.py` and `tools/book_compare.py`. Final
state: 3,900 random tablebase positions (3-4-5 and 6-man) and 315 book
positions agree. **Rule of thumb for later phases: a ported feature that reads
external data needs a differential run against the predecessor before it is
called done; unit tests on tiny fixtures are not a parity gate.** Open
decision from the audit: `tbhits` in the `info` line (the plan wanted it, the
predecessor never prints it) - decided and done 2026-11-19, ADR-0013.

**2026-11-19 (P6):** PGO (`tools/pgo_build.py`) gives +7-12 %: same-session depth 20 = **2.51-2.61 M vs 1.36-1.41 M NPS (1.79-1.92x)**, formal R3 (>= 2.43 M) reached; target-cpu=znver4 gave nothing (docs/baselines.md 7.11). The release workflow must run the PGO script per host/ISA variant.

**2026-11-19 (P7.0):** the thread-scaling collapse was bug **B44** (helpers never published nodes; the main searcher published itself) - fixed. `perf_test.py --depth 18`: 1/2/4/8 threads = 2.32 / 4.79 / 9.29 / 16.4 M NPS = **7.08x at 8T** (predecessor 5.74x). R4 reached; no pool/TT change needed (docs/baselines.md 7.10). Open: P4 (eval rows/head, SEE), P6 (PGO, target-cpu), P4b (NEON for the M4), P8 (cross-host validation). A real multi-thread strength check (Elo vs 1 T) belongs to the match gates.

**2026-11-19 (later):** the first sampling profile (`tools/profile_report.py`) found B43 - the move picker read an environment variable per node (18-20 % of samples). Fixed (P3a). Same-session depth 20: **2.38-2.43 M vs 1.41 M NPS = 1.69-1.73x** (docs/baselines.md 7.9). P3.1 (incremental occupancy) measured no gain and was reverted; make/unmake is rejected (make_move 3.7 % of samples). The profile now leads: `add_row_avx512` 12 %, `evaluate_head` 6 %, `pseudo_legal` 5 %, picker 5 %, `correction` 3 %. Next: P4 (eval rows/head, SEE), P6 (PGO / target-cpu), then P7 (thread scaling, the biggest open gap).

**2026-11-19 update:** P2 (hardware `pext`) and the P5 prefetch are also done. Same-session depth 20 (identical tree): KnightByte 1.54-1.57 M vs predecessor 1.42-1.43 M NPS = **1.08-1.11x** (docs/baselines.md 7.8). The parity milestone R2 is reached; the project goal (1.5x) and thread scaling (P7) remain.

**P1 is DONE (2026-11-19):** `movegen::legal` now uses the make-free `board::legality::Legality` test (equal to the making filter move for move, `tests/legality.rs`) and the search makes a move only after the pruning blocks. Pins exact; same-session depth-20 ratio vs the predecessor 0.629 -> 0.87-0.91 (details and the why-not-pseudo-list lesson: `docs/baselines.md` 7.7). Next: P1c (early-exit forced-reply count), P2 (hardware PEXT), P3 (incremental occupancy), P5 (TT prefetch), measured one step at a time.

### Historical: the P1 brief

**P0 is complete (2026-11-19).** The reproducible same-session baseline is:
KnightByte **669,773 NPS** at `perf_test.py --depth 20` and **739,702 NPS** at
5 s; the predecessor **1,064,034** and **1,096,913** on the same host in the
same session. At depth 20 both engines visit the same **18,662,358 nodes**
(identical tree), so the **1.59x** gap is pure per-node cost: 1.49 us vs
0.94 us. The shallow bench-12 protocol is not the performance gate - there
KnightByte is already ~6 % faster (811,935 vs 766,904 median) and it hides the
deep-search cost.

The counters name the dominant cost: **89.8 % of all make_move calls are the
`legal()` filter** (97.7 M filter makes vs 11.1 M search makes in `bench 20`).
The filter is ~17.5 % of the run and ~45-55 % of the same-session gap. **P1
replaces the full legal filter with the predecessor's one-pass move path**:
iterate the pseudo-legal list, make the move once, reject illegal moves before
any side effect, and keep every pruning condition and counter identical. The
gate is the four bench pins and every per-depth delta +0.

**P1.0 - DONE (2026-11-19):** **B42** fixed - the eval-cache store is gated with
`UseEvalCache` (it used to write 8.33 M never-read entries per `bench 20`).
Pins exact; `bench 12` median about +3.8 % (see `docs/baselines.md` 7.6). The
remaining work of this section is **P1 proper** (one-pass move path), next.

**After P1:** re-run `perf_test.py --depth 20` and the node-stats composition.
Expected ~0.80-0.85 MNPS at depth 20 (from 0.67), closing roughly half of the
remaining gap. Then P2 (hardware PEXT), P3 (board), P4 (eval/SEE), P5 (TT),
P6 (PGO), P7 (scaling), in the order of the performance plan.

**The goal line:** parity with the predecessor in the same session and protocol
(1.06 MNPS at depth 20) is the first milestone. The project target (1.5x the
predecessor, >=2.43 MNPS) is the long-term goal; the recorded 1,569 MNPS
predecessor number came from a different session (today's same-session
predecessor is 1,064 MNPS at depth 20), so cross-session ratios are no longer
used and the target is re-evaluated after P6 against a same-session baseline.

**Additional blueprints (added to `project-rules.md` by the project owner,
2026-11-19):** two reference engines live in `C:\dev\c#-chess-engines`
(`cinder`, `coda`) and are explicitly allowed as blueprints for improvement
ideas — use them alongside the predecessor source for the accumulator, the
head and the attack-table work. They are reference material only (the
copying rule of `migration-plan.md` §2 still applies: read them for
architecture, write our own code).

**Quick-and-dirty Cinder check (2026-11-19, this host):** Cinder ships
prebuilt `avx2` (`x86-64-v3`, tuned znver3) and `avx512` (`x86-64-v4`,
tuned znver4, plus GFNI/VBMI/VNNI/VPOPCNTQ) binaries. Depth-20 startpos,
deterministic per build: AVX2 = 990,208 nodes in ~1.4–1.9 s, AVX-512 =
**1,113,088 nodes in ~1.4–1.7 s** — the AVX-512 build searched a **12 %
larger tree in slightly less time** (per node ~17 % faster). Caveat found on
re-check: the two builds' NNUE heads round differently (their trees differ),
so the comparison mixes evaluation differences with speed — it is not a pure
ISA read.

**Quick-and-dirty Hobbes check (2026-11-19, this host):** Hobbes 4.0's two
prebuilt binaries search the **identical tree** (579,424 nodes, depth 20,
deterministic — bit-identical eval across their builds) and the AVX-512
build is only **~2–4 % faster** (787 K vs 818 K external). This is the clean
ISA read on Zen 4: the double-pumped 512-bit datapath adds a few percent,
nothing more. Together the two checks say: an AVX-512 build can gain ~20 %
on Zen 4 when it also changes the evaluation rounding (Cinder), but a
bit-identical NNUE gains only a few percent (Hobbes).

**Cinder source reconnaissance (2026-11-19):** Cinder 0.6.1 explains its own
avx512-vs-avx2 gap of ~20 % on this host — but also the limit of what we can
copy. Its NNUE is architecturally different from our fixed v5 net:

- **Width-generic `std::simd`** (nightly) with a compile-time `WIDTH` of 64
  bytes on the avx512 build: the whole NNUE — transformer, head, everything —
  processes 512-bit vectors, and the remaining scalar loops auto-vectorize
  because the crate is compiled for `x86-64-v4` with znver4 tuning.
- **Mixed quantization**: dense feature rows in `i16`, sparse feature sets in
  **`i8`** (half the memory traffic), with AVX-512 VNNI `dpbusd` for the
  `i8 × i8 → i32` updates.
- **Per-side king-bucket accumulator caches with attack-diff updates**: the
  cached placement and attack masks are diffed incrementally, so even a king
  move within the same bucket needs no full refresh.
- An **f32 network head** (no `i16`-quantized integer head at all).

Consequences for KnightByte: (a) the mixed quantization and the f32 head
belong to Cinder's own net format — our v5 net is fixed by project rule, so
these are **not portable**; (b) the attack-diff accumulator scheme is a real
technique worth adopting later (big surgery, own step); (c) building
KnightByte with AVX-512 target features so LLVM auto-vectorizes the
remaining scalar loops is a build-config change — but the measured
whole-binary `znver4` tuning showed no gain, so the expectation is low.
**The main migration gap is elsewhere:** the predecessor evaluates the same
positions with the same incremental design at 1.617 MNPS — 0.62 µs per node
against our 1.17 µs. With the evaluation now incremental and cheap, the
remaining cost sits in movegen, the TT, make-move and the picker: that is
what the line-by-line comparison of `MoveGenerator.cs`,
`TranspositionTable.cs` and `ThreadData.cs` must hunt down (lead 1 below).

**Status:** the migration-plan Phase 7 section records the same directive.

## 0. Current state in one paragraph

Phases 0–4 are complete and verified, and Phase 6 (Syzygy and the opening
book) completed on 2026-11-19, and the search is **node-identical to the
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
`stop` without `ponderhit` answers `bestmove 0000` (Q5; **superseded
2026-11-19**: it now answers the best move so far, ADR-0010), and a
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
the depth-12 startpos search. **Step 9** added the seeded 500-cycle
multi-thread stress walk (`tests/thread_stress.rs`: random
`position`/`go`/`stop`/`setoption`/`isready` cycles across `Threads`
1/2/4/8, every `bestmove` checked for legality against the mirrored
position, the engine's stderr captured). The walk found and fixed **B39**:
`setoption Hash` after a helper search killed the engine (the helpers held
their `SharedTables` clones; the resize's exclusive borrow failed). Fixed
in three layers (helper release + pool idle barrier + bounded resize
wait); the reference cannot have this bug (its .NET references never
constrain a resize — probed: it survives a mid-search `Hash` resize with
`Threads 4` attached). **Step 10** recorded `ADR-0007`: the synchronous
writer stays (a search emits a handful of short lines; the stress walk
measured the output side as microseconds per line), and the reference's
single `info currmove … currmovenumber 1 nodes 1` line of iteration 1 is
emitted exactly as the step-0 transcripts show it. The step-0
capture found **predecessor
bug 150** (`setoption Hash` above the declared maximum crashed the reference
via an overflowing `TTable.Resize` allocation); it was fixed in the
predecessor the same day (commit `acaff1f`, full suite 745/745). KnightByte
was not affected (spin clamping). **Step 12** replayed all eleven step-0
transcripts plus `bench` (`tests/uci_transcripts.rs`): line order and shape,
public option set, bestmove count and legality, cold fixed-depth node pins.
It found the perft `Time`/`NPS` gap (fixed) and the warm-TT node divergence
**B41** (found and fixed with the per-node ledger: the TT same-key rule is now
depth-only, and history/correction history reset every search). Step 11 was
dropped (see §2.3). Step 13 (match gates) is tracked in `migration-plan.md`,
where the project owner recorded 3 x 100 fastchess games.

Verification status on the last run (2026-11-19, after Phase 6): all 318 tests
pass locally on Windows (the full-set DTZ-ranking tests skip when
`KNIGHTBYTE_SYZYGY_PATH` is unset) using the Stockfish verifier from
`C:\dev\c#-chess-engines\stockfish`; fmt/clippy clean; all four bench pins
exact and `capture_bench.py compare` shows every per-position delta at +0.
Node identity is verified on Windows and, since 2026-11-20, on macOS Apple
Silicon (NEON, forced scalar and the release binary all report the four pins
exactly, ADR-0015) and on Linux x86-64 (AVX2, forced scalar and the packaged
PGO binary, ADR-0016), which closes migration-plan G6.

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
  `Hash`, `Clear Hash` and `Move Overhead` are functional; the book and
  tablebase names became functional in Phase 6, and `EvalFile`/`Contempt` stay
  accepted and inert by design.
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

Next action: none for now. The planned node-count lever test was dropped by
decision of the project owner (step 11, 2026-11-18): a lever that does not
move node counts on a probe position is not a defect, and lever strength is
measured after the migration — a dedicated tuning campaign with SPRT, one
lever at a time, on the final fast engine.

### 2.4 Threading, `stop`, `ponder` — the search thread landed; Lazy SMP and ponder are open

Step 4 (2026-11-18) is done: `go` runs on a 16 MB worker thread, the UCI
loop keeps reading commands while it searches, `stop` sets a shared flag
that the search polls every 1024 nodes (the exact `go nodes` limit stays a
per-node integer comparison), and `quit`/EOF plus every state-touching
command stop and join a running search first. Step 8 (2026-11-18) added the
Lazy SMP pool: `setoption Threads` resizes it, helpers persist with their
own search state, and the main thread's result is the answer.

Next action (Phase 5): steps 9 and 10 are done (stress gate, ADR-0007) and step
12 (transcript replay) is done; step 13 (match gates) is tracked in
`migration-plan.md`.

### 2.5 Public options

**Resolved 2026-11-19 (ADR-0014):** `EvalFile` and `Contempt` were removed; no
inert option is advertised any more. The ten public options (`Hash`, `Threads`,
`Move Overhead`, `Clear Hash`, `Own Book`, `Book File`, `Book Depth`,
`SyzygyPath`, `SyzygyRootProbe`, `Syzygy50MoveRule`) are all functional, and the
sixty research levers are listed in `docs/LEVERS.md`.

### 2.6 Performance (Phase 7) — decomposition after the incremental accumulator and the SIMD kernels

Measured on this host (release, `tests/perf_microbench.rs`, Ryzen 7 260; the
microbench now measures the real search rate per bench position instead of
the stale hardcoded 370 K):

| Building block | Rate | Per call |
| :--- | ---: | ---: |
| NNUE evaluation (full refresh, SIMD) | ~1.16M/s | 0.86 µs |
| Legal move generation | ~1.51M/s | 0.66 µs |
| Copy-make (one move) | ~1.49M/s | 0.67 µs |
| Search (bench 12, cleared tables) | ~856K nodes/s | 1.17 µs/node |

The search no longer pays the full refresh per node (the incremental
accumulator pushes deltas), so the full-refresh rate is only the **upper
bound** of the in-search evaluation cost — the microbench prints it as such.
Where the remaining 1.17 µs/node actually goes (TT, picker, the incremental
push, the output head) needs a real profile: Phase 7's first work item
("identify the top three costs. Do not guess.") is still open.

The Phase 7 attack order, with items 1–3 done:

1. ~~Wire the reusable `Evaluator`~~ (B40, done — pins unchanged).
2. ~~**Incremental accumulator**~~ — **done (2026-11-19)**: the searcher
   pushes a per-ply accumulator pair and only the changed weight rows are
   applied (own-king moves fall back to a full refresh); bit-exact against
   the full refresh on random walks and the deterministic segments, bench
   pins unchanged. Measured: bench 12 ~0.37 → **0.556 MNPS (+50 %)**,
   `go nodes 2000000` 386 K → 533 K at 1 T and 549 K → 923 K at 2 T.
3. ~~**SIMD kernels**~~ — **done for x86-64 (2026-11-19)**: SSE4.1 (128-bit)
   and AVX2 (256-bit) kernels for the accumulator row arithmetic and the
   SCReLU head, dispatched once per process with the ADR-0001
   `KNIGHTBYTE_FORCE_ISA` override; the 512-bit AVX-512 kernels for the
   `v4-vnni` rung landed the same day and measure identically to AVX2 on the
   Zen 4 host (double-pumped 512-bit datapath — the real gain belongs to
   Zen 5 / Intel 512-bit-datapath hosts). The VNNI rungs map to the AVX2
   kernels (the dot-product instruction cannot express the per-element
   square-shift semantics without changing them) and NEON lands with the
   macOS phase. Measured: bench 12 0.556 → **0.863 MNPS**, `go nodes 2000000`
   1 T 533 K → **856 K**. The 6–8 T degradation remains the next profile target.
4. ~~**Slider attacks**~~ — **done (2026-11-19)**: BMI2 PEXT tables
   (`src/board/attacks.rs`), filled from the ray-walking implementation
   (bit-identical by construction), selected once per process (BMI2 present,
   not a slow-BMI2 CPU, `KNIGHTBYTE_FORCE_PEXT`/`KNIGHTBYTE_DISABLE_PEXT`
   overrides). Verified against the ray walk for **every relevant subset of
   every square**. Honest result: no measurable search-level gain on this
   host (bench 12: 850K with PEXT vs 869K without — within noise); the
   predecessor's NPS advantage does not come from the slider attacks alone.
   The deadlock the first version introduced (the table fill re-entered the
   `pext_tables()` initialisation through the public dispatchers) is fixed —
   the fill uses the ray walks directly.
5. **`target-cpu` + PGO builds** (the ladder's per-variant artifacts).

Gate for every step: the bench pins stay exact (15,528 / 456,308 /
2,815,799 / 6,892,874) — a pin change is a search change and stops the
step. Target: at least PawnForgeCS's ~1.4 MNPS single-threaded on this
host (the plan's goal is higher: ≥1.5× the reference, ≥2.48 MNPS on the M4
baseline host).

**Step-13 session evidence (2026-11-18), entry item for the phase:** the
thread scaling caps at ~1 M aggregate NPS and *drops* beyond two threads
(external wall clock on `go nodes 2000000`: 1 T = 386 K, 2 T = 549 K,
4 T = 701 K, 6 T = 478 K, 8 T = 560 K), while the predecessor scales 5.99×
on the same 8-core host. Diagnostics: an 8-thread search keeps 7.98
core-equivalents fully busy (nothing blocks), the private-table variant
(no TT sharing) scales the same way, and — decisive — **8 concurrent
single-threaded processes also cap the host at ~390 K nodes/s aggregate**
(387 K / 381 K / 389 K / 390 K for 1 / 2 / 4 / 8 processes). The bottleneck
is therefore a shared hardware resource saturated by the per-node work
itself — the full 2 × 512-wide accumulator refresh dominates the node
budget — and **not** the Lazy SMP pool, the TT sharing or the node-counter
cadence. Consequence: the incremental accumulator and the SIMD kernels are
both the single-thread lever *and* the thread-scaling fix; profile
(ETW/samply) in front of every change. Match evidence from the same session
(KnightByte multi-thread vs the single-threaded predecessor, 100 games
each): 25.0 % (1 T), 17.0 % (2 T), 25.5 % (4 T) — consistent with the NPS
deficit, and no crash, illegal move or time loss in any game
(`fastchess --compliance` passes for both engines). Phase 6 was deferred by
the project owner on the same day (deferral, not cancellation).

**Update after the SIMD kernels (2026-11-19):** the same
`go nodes 2000000` probe now reads 1 T = 856 K and 2 T = 1,293 K (from
386 K / 549 K at the start of the phase) — the per-node traffic shrank and
the cap moved up. The 6–8 T degradation remains (8 T now reads *below* 1 T)
and is the top profile target of the next session.

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
| Own book off, ponder off | `Own Book` defaults false (Phase 6) and the `Ponder` option is not advertised |
| Hash 128/256 MB | the `Hash` option accepts 1-1024 MB (default 256) |
| Threads | implemented (Phase 5 step 8, Lazy SMP pool; multi-thread play is stress-gated in step 9) |
| Syzygy | implemented (Phase 6): WDL/DTZ probing, 3-4-5 and larger sets, root DTZ ranking; optional, off until `SyzygyPath` is set |
| Windows x64 zip, README, licence | README and MIT licence exist; the Windows zip and the macOS/Linux archives are produced by the release scripts (ADR-0011, ADR-0015, ADR-0016). A tagged public release is still open |

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

**State (2026-11-18, end of session):** the predecessor bugs are fixed (143–149 at `bffe98a`/`f274b39`, 150 at `acaff1f`) and the migration has resumed. Phase 5 steps 0–10 are complete: oracle transcripts, the command surface and parser, the option surface with full `setoption` validation, the complete `info` lines, the search thread with the shared stop flag, `go ponder`/`ponderhit`, `go mate`/`searchmoves` (every `go` limiter implemented), the shared-tables refactor with the parallel-zeroing bounded clear (B38 fixed), the persistent Lazy SMP pool behind `Threads`, the 500-cycle multi-thread stress gate (B39 found and fixed), and the output decision ADR-0007 with the `info currmove` line. **Step 11 was dropped by decision of the project owner** (no node-count lever test; strength measurement of all levers happens after the migration as a dedicated SPRT tuning campaign). **Step 12 (UCI transcript tests) is done:** the replay suite checks all eleven
captures plus `bench`, found the missing perft `Time`/`NPS` lines (fixed) and
the warm-TT node divergence **B41** (found and fixed with the per-node ledger:
the TT same-key rule is now depth-only, and history/correction history reset
every search — details in `docs-new/new-bugs.md`). **Next when we continue:
step 13 (match gates).** Details in `migration-plan.md`, Phase 5.

### 3.1 Phase 5 — UCI robustness and threading (in progress, steps 0–10 done)

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
  single-legal-move fast path (**narrowed 2026-11-19** to time-managed
  searches, ADR-0010), and the stop-before-depth-1 depth-0 answer;
  the burst go+quit and one-shot argv semantics verified against the
  reference binary. The former burst-style smoke tests now use the
  interactive pattern (wait for `bestmove`, then `quit`).
- ~~Step 5: `go ponder`/`ponderhit`~~ — done (2026-11-18): unbounded ponder
  search printing its iterations, `stop`/burst-`quit` answer `bestmove 0000`
  (Q5, **superseded 2026-11-19**: now the best move so far; plus the interrupted-search depth-0 line — byte-identical to the
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
- ~~Step 9: helper diversity and the stress gate~~ — done (2026-11-18): the
  seeded 500-cycle walk (`tests/thread_stress.rs`) passes; it found and
  fixed B39 (the resize's exclusive borrow vs the helpers' table clones —
  no reference counterpart, probed and documented).
- ~~Step 10: output decision~~ — done (2026-11-18): `ADR-0007` — the
  synchronous writer stays (a search emits a handful of short lines; the
  stress walk measured microseconds per line); `info currmove
  <move> currmovenumber 1 nodes 1` is emitted once per search, when
  iteration 1's first root move begins, exactly as the step-0 transcripts
  show it.
- ~~Step 11: lever independence~~ — dropped (the lever strength measurement
  is a dedicated SPRT campaign after the migration).
- ~~Step 12: UCI transcript tests~~ — done (2026-11-18): `tests/uci_transcripts.rs`
  replays all 11 step-0 captures plus `bench 6`; it checks line order/shape,
  the public option set, one `bestmove` per search with legality, cold
  fixed-depth node pins and the perft output. The perft `Time`/`NPS` gap was
  fixed, and the warm-TT node divergence **B41** was found and fixed with the
  per-node ledger (TT same-key rule now depth-only; history/correction reset
  every search — the re-captured warm transcript node counts match exactly).
  264 tests pass, fmt/clippy clean, bench deltas all +0.
- Step 13: match gates — see `migration-plan.md` step 13 (the project owner
  recorded 3 x 100 fastchess games there: KnightByte scored 25.0 % at 1
  thread, 17.0 % at 2 threads and 25.5 % at 4 threads against the
  single-threaded predecessor; `fastchess --compliance` passes for both
  engines, no crashes, illegal moves or time losses in any game). The G5
  1,000-game cutechess gate is not re-run by step 12.

### 3.2 The performance plan: reach ≥1.617 MNPS (100 % of the predecessor)

**The baseline (Zen 4, same host, same protocol, identical node counts):**
the predecessor runs 1,617 K nodes/s single-threaded (0.62 µs/node);
KnightByte runs **863 K** (1.17 µs/node) after the incremental accumulator
and the SIMD kernels. The gap to close is **1.87×**. The project goal is
≥1.5× the predecessor = **≥2.43 MNPS**. Every step keeps the bench pins
exact (node identity is the gate — this is speed work, never search
behaviour) and is measured with the same protocol: bench 12 NPS and
`perf_test.py --depth 20` on the `zen4` host, compared against
`docs/baselines.md` §4.

**P0 - Measurement rig (half a day).** Done 2026-11-19. Added the
`release-with-debug` profile; fixed the microbench (legal, pseudo, make, SEE,
incremental push+head and search measured separately); added the `node-stats`
feature (19 counters printed by `bench`) and `tools/perf_bench.py` (five-run
median, node-pin check, JSON). Same-session result: depth 20 669,773 vs
1,064,034 NPS (1.59x, identical 18,662,358 nodes); bench 12 811,935 vs
766,904. The `bench 20` counters found **B42** (eval cache stores while
disabled) and show that 89.8 % of all makes are the legal filter.

**P1 - One-pass move path (1-2 days, the decisive step).** Search and
quiescence iterate the pseudo-legal list, make the move once, reject illegal
moves before any side effect, and keep every pruning condition and counter
identical. The forced-reply extension keeps its pseudo-count predicate.
Predicted +30-60 %; gate: all four pins exact and every per-depth delta +0.

**P2 - Hardware PEXT (half a day).** Call `_pext_u64` from the BMI2 branch;
keep the software loop as the table-fill oracle and the non-BMI2 fallback;
hoist the per-call `OnceLock`. Gate: the subset test, perft, pins.

**P3 - Board (1 day).** Incremental occupancy (unconditional). Make/unmake
and staged generation only if the post-P1 profile still justifies them.

**P4 - Evaluation (1-2 days).** One contiguous 2 KB accumulator pair per
ply; an exact `madd_epi16` head variant; hoist the kernel dispatch. Gate:
the 15 vectors, the random-walk identity, the bench pins.

**P4b - AArch64 NEON — done 2026-11-20 (ADR-0015).** The Apple build no longer
evaluates in scalar: the row, refresh and SCReLU-head kernels run on 128-bit
NEON (the AArch64 baseline), with the same parity gates on the M4. Bench 12-18
1.72-1.89 MNPS against 0.43-0.44 MNPS scalar. PGO, now the default for the
macOS release, adds +6.0-7.5 % (1.95-2.01 MNPS at bench 12-18). The >= 2.48
MNPS M4 goal is not reached yet; it remains search-tuning work.

**P4c - Linux x86-64 release — done 2026-11-20 (ADR-0016).** One
`x86_64-unknown-linux-gnu` binary with the `x86-64-v3` (AVX2) baseline and PGO
by default; `tools/build_release_linux.py` and the tag workflow build and
verify it. On the Ryzen 5 5600H the release reads 1.68-2.19 MNPS at bench
12-18; PGO adds +5.0-9.0 % over the plain build, and the AVX2 kernels are
3.94x the forced scalar kernels. Pins exact with every backend.

**P5 - TT and caches (1 day).** Prefetch after every make and null move;
advance the shared generation once per `go` in the main thread; keep the
replacement policy exactly.

**P6 - PGO and `target-cpu` (half a day to a day).** Per-host, per-variant
profiles; `v3`/`znver4`/`apple-m1` rungs; `native` is local only. Gate: node
counts and perft unchanged.

**P7 - Thread scaling (2-3 days).** Re-run the N-process saturation test,
then per-thread eval caches, single generation ownership, helper diversity
and false-sharing checks. Gate: the stress walk and the match gates; the
1 T pins stay exact.

**The checkpoint rule:** measure after every step (bench 12 NPS,
`perf_test.py --depth 20`, the predecessor comparison). If a step does not
move the number, revert it and record why. When 1.617 MNPS is crossed, the
goal rises to ≥1.5× the predecessor (≥2.43 MNPS) per the migration plan.

**Phase 6 (Syzygy/book) is complete (2026-11-19)** and no longer blocks the
performance work: both features are optional, load nothing by default, and
leave the bench pins untouched. The remaining performance items are the ones
listed above.

**What is explicitly NOT portable (recorded so nobody re-tries):** Cinder's
`i8` sparse feature rows and `f32` network layers belong to its own net
format; the KnightByte v5 net is fixed by project rule. The predecessor's
accumulator design (dirty-piece stack, stale flags, lazy refresh) is already
ported (B41 and the incremental accumulator).

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

### macOS (Apple Silicon)

```bash
cd ~/dev/knightbyte-unfinished
git pull --rebase
KNIGHTBYTE_FEN_VERIFIER=/opt/homebrew/bin/stockfish cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release
python3 tools/build_release_macos.py   # release archive, pins verified
```

### Linux (x86-64)

```bash
cd ~/dev/knightbyte-unfinished
git pull --rebase
KNIGHTBYTE_FEN_VERIFIER=/usr/games/stockfish cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
python3 tools/build_release_linux.py   # PGO archive, pins verified
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
§2.10. The macOS and Linux archives are produced by the tag workflow; the
Windows zip still comes from the Windows host.

GitHub Actions on a public repository is not minute-limited the way a private
one is. The macOS runner queue can still be slow (§2.7); that is a latency
problem, not a budget one.

---
