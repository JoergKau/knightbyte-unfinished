# KnightByte baselines

This file records the performance numbers KnightByte is measured against. The
C# predecessor is the reference implementation; its full run logs stay in the
internal C# project and are summarized here.

**Status:** C# baselines captured for Apple M4 and Windows Zen 4. KnightByte
columns are filled in Phase 7 and updated for every release candidate.

## 1. Identity gates (fixed depth, compiled defaults, one thread)

These are behavioural gates, not speed measurements. They must match exactly.

| Run | Nodes | Meaning |
| :--- | ---: | :--- |
| `bench 6` | 15,528 | Search identity |
| `bench 12` | 456,308 | Search identity |
| `bench 16` | 2,815,799 | Search identity |
| `bench 18` | 6,892,874 | Search identity |

The `bench` set is four fixed positions (startpos, Kiwipete, an endgame and a
tactical middlegame), Hash 256 MB, one thread. The values are the reference
totals of the bug-fixed predecessor build (2026-11-18, ADR-0008); the earlier
totals 416,533 / 3,169,859 / 6,925,762 described the pre-fix build.

### KnightByte current totals (node identity, ADR-0005 and ADR-0008)

KnightByte `bench` against the bug-fixed reference build (2026-11-18 restart,
predecessor commits bffe98a and f274b39; the fixes for the former quirks
Q2-Q4 / predecessor bugs 147-149 are ported as the B35-B37 fixes),
Windows 11, Rust 1.100.0 release, Hash 256 MB, one thread:

| Run | Nodes | Reference | Delta |
| :--- | ---: | ---: | ---: |
| `bench 6` | 15,528 | 15,528 | 0 |
| `bench 12` | 456,308 | 456,308 | 0 |
| `bench 16` | 2,815,799 | 2,815,799 | 0 |
| `bench 18` | 6,892,874 | 6,892,874 | 0 |

Every per-position count of the captured depths (1-12, 14, 16, 18) is
identical to the reference as well (verified with
`tools/capture_bench.py compare`, all deltas zero). The pre-restart totals
against the OLD reference build (`bench 6` 14,979, `bench 12` 416,533,
`bench 16` 3,169,859, `bench 18` 6,925,762) described the quirk build and were
replaced when KnightByte adopted the reference's bug fixes (ADR-0008). The
first-round identity state (`bench 6` 14,963, `bench 12` 356,262, `bench 16`
3,077,942, `bench 18` 6,797,622) was traced to concrete porting differences
and removed; ADR-0005 supersedes it. The audit record is
`docs/audit/migration-audit.md`, the defects are `docs-new/new-bugs.md`
(B18-B28, B35-B37).

The `bench 6` total and the per-position counts of depths 1-8 are asserted in
`tests/search.rs`, so CI fails if any host or build mode produces a different
count. `tests/golden/bench.json` records all four totals.

## 2. Hosts

| Label | Host | CPU | OS | Role |
| :--- | :--- | :--- | :--- | :--- |
| `m4` | MacBook Air M4 | Apple M4, 10 cores | macOS 27.x | Primary development + macOS baseline |
| `zen4` | Windows 11 desktop | AMD Ryzen 7 260 (Zen 4) | Windows 11 | Windows baseline |

Never compare an `m4` run with a `zen4` run. Compare like with like.

### Reproduction

Use the local C# baseline binary for the host (the path is intentionally generic;
the predecessor checkout lives in the migration folder outside this repository):

```bash
# Scaling + quality gates + tactical mini-suite
python3 perf-test/perf_test.py \
  --engine "/path/to/csharp-baseline/engine-binary" \
  --threads 1,4,8 --movetime 5000 --hash 256 --tactics

# Head-to-head EBF
python3 perf-test/ebf_compare.py \
  --engine-a "/path/to/csharp-baseline/engine-binary" \
  --engine-b "/path/to/reference/engine" \
  --movetime 5000 --hash 256

# WAC screen
python3 perf-test/wac_test.py \
  --engine "/path/to/csharp-baseline/engine-binary" \
  --positions 300 --workers 1 --hash 128 --movetime 1000
```

Add `--json-out PATH` to `perf_test.py`/`ebf_compare.py` when recording a run.

## 3. C# baseline — Apple M4

Protocol: `perf_test.py`, four positions, `movetime 5000`, Hash 256 MB,
Threads 1/4/8, net `v5_poolv9_512_auxwdl-150`, hardware-intrinsics build
(13.11.2026). Tactics: 5 positions at 2000 ms. WAC: 300 positions at 1000 ms,
Hash 128 MB.

| Threads | Avg depth | Avg NPS | Speedup | Efficiency |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 22.2 | 1,656,154 | 1.00× | 100.0 % |
| 4 | 24.0 | 6,180,924 | 3.73× | 93.3 % |
| 8 | 23.8 | 7,821,579 | 4.72× | 59.0 % |

One-thread detail:

| Position | Depth | Seldepth | Margin | Nodes | EBF | NPS |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| startpos | 19 | 27 | 8 | 7,202,780 | 2.30 | 1,572,659 |
| kiwipete | 21 | 34 | 13 | 5,978,760 | 2.10 | 1,344,145 |
| middlegame | 23 | 39 | 16 | 6,343,246 | 1.98 | 1,683,005 |
| endgame | 26 | 38 | 12 | 6,351,821 | 1.83 | 2,024,807 |

Aggregate one-thread: total nodes **25,876,607**, average EBF **2.050**,
average selective margin **12.25**.

Other gates:

* Tactical mini-suite: **5/5**.
* WAC (300 positions, 1 s): **284/300 = 94.7 %** in 257.5 s.
* EBF head-to-head at 5 s: average depth 22.2, average EBF **2.05**; the
  reference engine reached 26.2 / 1.86 on the same run. Closing the EBF gap is
  a search-quality goal, not a porting gate.

For context, the pre-intrinsics build on the same host measured 1T NPS
1,557,912, average depth 22.0 and EBF 2.061; the intrinsics build is the
baseline to beat.

## 4. C# baseline — Windows Zen 4

Protocol: same as §3, Windows x64 hardware-intrinsics build (13.11.2026).

| Threads | Avg depth | Avg NPS | Speedup | Efficiency |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 22.0 | 1,569,444 | 1.00× | 100.0 % |
| 4 | 24.0 | 6,079,846 | 3.87× | 96.8 % |
| 8 | 24.0 | 10,767,216 | 6.86× | 85.8 % |

One-thread detail:

| Position | Depth | Seldepth | Margin | Nodes | EBF | NPS |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| startpos | 18 | 26 | 8 | 4,461,267 | 2.34 | 1,427,605 |
| kiwipete | 21 | 34 | 13 | 5,978,760 | 2.10 | 1,333,354 |
| middlegame | 23 | 39 | 16 | 6,343,246 | 1.98 | 1,617,349 |
| endgame | 26 | 38 | 12 | 6,351,821 | 1.83 | 1,899,468 |

Aggregate one-thread: total nodes **23,135,094**, average EBF **2.061**,
average selective margin **12.25**.

Other gates:

* Tactical mini-suite: **5/5**.
* WAC (300 positions, 1 s): **284/300 = 94.7 %** in 261.7 s.

## 5. Comparison rules

1. **Same host.** M4 numbers against M4 numbers; Zen 4 numbers against Zen 4
   numbers.
2. **Same protocol.** Four positions, 5 s/position, Hash 256 MB, Threads
   1/4/8, same net. Tactics 2 s; WAC 300 positions at 1 s and Hash 128 MB.
3. **Node identity uses fixed depth.** `movetime` results depend on NPS, so the
   total-node column is not comparable across hosts (the M4 reaches one extra
   depth on startpos). Use `perf_test.py --depth` or the `bench` command when
   node counts must match exactly.
4. **Record metadata.** Engine commit/binary hash, net SHA-256, host/CPU/OS,
   Hash, Threads, Syzygy/book state, depth or movetime, and the exact command.
5. **Cheap gates are not strength proof.** Confirm with `fastchess`/SPRT and
   `cutechess-cli` time-loss checks.
6. **Movetime variance.** A 5 s run on a busy or thermally throttled laptop can
   miss the last iteration (for example kiwipete depth 21 vs 20) even when the
   engine is correct. The recorded baseline is the reference; use fixed depth
   when comparing implementations, and alternate repeated runs for time-to-depth.

## 6. KnightByte results

Filled per release candidate. The 2026-11-19 rows are the current build
(B41 fix, incremental accumulator, SIMD kernels — SSE4.1/AVX2/AVX-512),
measured with `perf_test.py --depth 20` on the `zen4` host. WAC and the
tactical mini-suite were not re-run for these rows.

| Host | Build | Threads | Avg depth | Avg NPS | Total nodes | EBF | WAC | Tactics |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| zen4 | 2026-11-19 | 1 | 20.0 | 810,424 | 18,662,358 | 2.131 | not run | not run |
| zen4 | 2026-11-19 | 4 | 20.0 | 1,417,620 | 20,594,684 | 2.14 | not run | not run |
| zen4 | 2026-11-19 | 8 | 20.0 | 894,971 | 20,037,205 | 2.11 | not run | not run |

Session context: the 2026-11-18 pre-B41/pre-SIMD build measured 1 T
516,458 / 4 T 993,497 / 8 T 773,020 on the same protocol — the incremental
accumulator and the SIMD kernels added +57 % single-threaded. The 4 T/8 T
efficiency (44 % / 14 %) remains the top profile target (§2.6 of
`next-steps.md`); the predecessor reaches 3.58× / 5.99× on the same host.
Multi-thread node totals are not compared (nondeterministic by design).

## 7. Performance rig and the 2026-11-19 same-session baseline

**Status:** P0 of the performance recovery plan is complete. The rig is now
reproducible, and the earlier cross-session ratio is superseded by the
same-session numbers below.

### 7.1 The rig

* `tools/perf_bench.py` - five fresh-process `bench` runs, node-pin check,
  median/min/max NPS and an optional JSON record. It reads both the engine's
  `Nodes:`/`NPS:` lines and a summary with `Total Nodes:`/`Average NPS:`.
* `perf-test/perf_test.py --depth 20 --threads 1 --hash 256` - the primary
  machine-independent comparison: identical nodes and EBF prove the same tree,
  NPS is the host readout.
* `perf-test/perf_test.py --movetime 5000 --threads 1 --hash 256` - the
  time-to-depth comparison.
* `cargo run --release --features node-stats -- bench N` - per-node composition
  counters (counts only, no timers).
* `cargo build --profile release-with-debug` plus a sampler - source-level
  profiling without changing the shipped release profile.

### 7.2 Same-session comparison (2026-11-19, Ryzen 7 260)

Both engines ran back-to-back in the same session, Hash 256 MB, one thread.

| Protocol | KnightByte | Predecessor | Ratio |
| :--- | ---: | ---: | ---: |
| `bench 12`, five-run median | 811,935 | 766,904 | 1.059x |
| `perf_test --depth 20` | 669,773 | 1,064,034 | 0.629x |
| `perf_test --movetime 5000` | 739,702 (avg depth 19.2) | 1,096,913 (avg depth 20.5) | 0.674x |

At depth 20 both engines visit **18,662,358 nodes** with the same per-position
counts and the same EBF (2.131). The tree is identical, so the NPS ratio is a
pure per-node cost ratio: **1.49 us vs 0.94 us (1.59x)**. The shallow bench-12
protocol flatters KnightByte - it is ~6 % faster there - and hides the
deep-search cost. Use depth 20 as the performance gate and bench 12 for node
identity.

**Protocol lesson:** the recorded predecessor baseline (Zen 4, 1,569,444 NPS)
came from a different session. Today's same-session predecessor run is
1,064,034 NPS at depth 20 and 1,096,913 NPS at 5 s. Cross-session comparisons
overstate the gap by roughly 40 %. Always run both engines in the same session
and record both numbers together.

### 7.3 Node-cost composition (`bench 20`, node-stats)

15,229,503 nodes, 22,061 ms, 690,336 NPS.

| Counter | Count | Per node |
| :--- | ---: | ---: |
| negamax nodes | 10,230,503 | 0.672 |
| qsearch nodes | 4,999,000 | 0.328 |
| legal move generations | 3,218,503 | 0.211 |
| pseudo moves | 97,729,208 | 6.42 |
| moves tried | 14,949,770 | 0.981 |
| moves made (search) | 11,080,400 | 0.728 |
| illegal moves rejected | 13,499,562 | 0.886 |
| TT probes / hits | 10,981,130 / 2,697,582 | 0.721 / 24.6 % |
| eval-cache probes / stores | 0 / 8,326,971 | 0 / 0.547 |
| eval head calls | 8,326,971 | 0.547 |
| full refreshes (perspective) | 1,222,454 | 0.080 |
| accumulator pushes | 11,080,400 | 0.728 |
| SEE calls | 17,761,133 | 1.166 |

The decisive number: **89.8 % of all `make_move` calls are the legality filter**
(97.7 M filter makes vs 11.1 M search makes). The filter costs about 3.9 s of
the 22.1 s run (~17.5 %), and about **45-55 % of the same-session gap** to the
predecessor. This is the target of P1.

### 7.4 Building-block rates (release, P0 microbench)

| Block | Rate | Per call |
| :--- | ---: | ---: |
| full NNUE refresh | 1,246,235/s | 802 ns |
| pseudo-legal generation | 6,920,691/s | 144 ns |
| legal generation | 833,696/s | 1,200 ns |
| one copy-make | 20,222,205/s | 49.5 ns |
| `see_ge` on a capture | 12,788,481/s | 78 ns |
| incremental push + head | 3,737,928/s | 267.5 ns |

The earlier "copy-make" row called `legal()` inside its loop, so it measured
`legal + make`; the corrected rig measures one make alone at ~50 ns. The PEXT
software index is slower than the ray walk on this host (legal 1,200 ns vs
966 ns; pseudo 144 ns vs 118 ns), while the search-level difference is within
noise (bench 12 788 K vs 795 K). The hardware `pext` fix is P2.

### 7.5 Defect found by the counters

**B42** (`docs-new/new-bugs.md`): with `UseEvalCache=false` (the compiled
default) the eval cache is never probed but still stores every static
evaluation (8.33 M stores in `bench 20`). The predecessor performs no cache
instructions when the lever is off. The store is gated with the same lever in
P1.0; this is behaviour-neutral.

### 7.6 P1.0 result (B42 fixed, 2026-11-19)

Three interleaved pairs of `tools/perf_bench.py --depth 12 --runs 5` (same
host, same build class, before = the P0 commit, after = P1.0). Node pin exact
(456,308) in every run; `bench 6/16/18` = 15,528 / 2,815,799 / 6,892,874.

| Pair | Before median NPS | After median NPS | Ratio |
| :--- | ---: | ---: | ---: |
| 1 | 799,138 | 862,586 | 1.079 |
| 2 | 828,145 | 859,337 | 1.038 |
| 3 | 832,678 | 846,582 | 1.017 |

The spread between pairs is machine-state noise (a few percent); the after
build wins every pair, so the gain is real but modest (about +2-8 %, median
+3.8 %). The depth-20 same-session comparison is re-run after P1.

### 7.7 P1 result (make-free legality, 2026-11-19)

Change: `movegen::legal` filters the pseudo-legal list with `board::legality::Legality`
(checkers and pinned pieces computed once per node, no `make_move`), and the
search makes a move only after the pruning blocks (late-move, history and
futility pruning) have let it through. Pins exact: `bench 6/12/16/18` =
15,528 / 456,308 / 2,815,799 / 6,892,874; `capture_bench.py compare` all
deltas +0 at depths 1-12, 14, 16, 18.

**Same-session depth 20** (`perf_test.py --depth 20 --threads 1 --hash 256`,
interleaved, both engines visit 18,662,358 nodes):

| Run | KnightByte NPS | Predecessor NPS | Ratio |
| :--- | ---: | ---: | ---: |
| 1 | 1,266,762 | 1,396,130 | 0.907 |
| 2 | 1,251,896 | 1,442,185 | 0.868 |

(The machine state differs from the section 7.1 session - the predecessor ran
at 1.06 M there and 1.40-1.44 M here - so only the **ratio** is comparable:
0.629 before P1, 0.87-0.91 after.) `bench 12` median, interleaved against
the P0 build: 843-868 K -> 1,030-1,056 K (+22 %).

**Design note (why not "pseudo-legal list + make-then-test").** The first
attempt followed the plan literally: hand the picker the pseudo-legal list and
reject an illegal move after making it. It was fast but moved the node pins
(`bench 12/16/18` = 432,747 / 3,136,996 / 8,042,164): the picker's partial
selection sort swaps the best move into the cursor slot, which permutes the
order of equal-scored moves, so an illegal move sitting in the buffer changes
the order of the legal moves after it. Node identity therefore requires the
picker to see the exact legal list; the legality decision has to be made before
the picker, and it is made without making the move (P1b's pin/check-aware
test). The attempt was reverted.

### 7.8 P2 (hardware PEXT) and P5-prefetch results (2026-11-19)

Both steps are node-neutral (pins exact, `capture_bench.py compare` all +0,
`cargo test --all-targets` green).

* **P2:** the PEXT index is now the single BMI2 `pext` instruction (inline
  asm behind the existing BMI2 / slow-BMI2 gate; the software loop stays as the
  portable fallback and test oracle, a new test compares both on random
  occupancies). `attacks.rs` is also cfg-gated so non-x86 targets compile
  without the x86 feature macro. `bench 12` median: 1.05 M (ray walk,
  `KNIGHTBYTE_DISABLE_PEXT=1`) -> 1.15 M; deeper searches gain more
  (`bench 18` 1.21 M -> 1.33 M).
* **P5 (prefetch only):** `tt.prefetch(next.key())` right after every
  `make_move` in negamax and quiescence and after `make_null`. Interleaved
  `bench 16` medians, P2 build vs P2 + prefetch: 1.34 M -> 1.49 M and
  1.30 M -> 1.47 M (+11-13 %).

**Same-session depth 20** (identical 18,662,358 nodes, interleaved):

| Build | KnightByte NPS | Predecessor NPS | Ratio |
| :--- | ---: | ---: | ---: |
| P1 | 1,251,896 - 1,266,762 | 1,396,130 - 1,442,185 | 0.87 - 0.91 |
| P1 + P2 | 1,392,732 - 1,401,984 | 1,402,460 - 1,429,129 | 0.97 - 1.00 |
| P1 + P2 + prefetch | **1,537,300 - 1,570,740** | 1,416,247 - 1,427,949 | **1.08 - 1.11** |

Milestone R2 (predecessor parity, same session) is reached: KnightByte is
~8-11 % faster per node than the predecessor at depth 20 on this host. R3 (the
1.5x project goal) and R4 (thread scaling) remain open.

### 7.9 P3 (board hot path) and the first sampling profile (2026-11-19)

**P3.1 incremental occupancy - measured, no gain, reverted.** Replacing the
six-iteration occupancy rebuild in `make_move` with moved/captured bit
updates was node-neutral and correct (a random-walk assertion compared it with
the recomputed union), but interleaved A/B runs showed no measurable gain:
`bench 16` medians 1.446 / 1.434 / 1.458 M (before) vs 1.440 / 1.477 / 1.445 M
(after), `bench 12` 1.250 / 1.220 M vs 1.240 / 1.230 M. After P1 the engine
makes one move per searched move (about 11 M makes in `bench 20`), so the
rebuild is well under 1 % of the run. Project rule: no gain, no merge.

**Profile-driven pivot.** The make/unmake decision of P3 needs a profile, so
the profiling rig was finished: `samply record --save-only
--unstable-presymbolicate` on the `release-with-debug` build, aggregated per
function by the new `tools/profile_report.py` (dbghelp symbol table of the
PDB; Windows). The first profile of `bench 18` (self time):

| Share | Function |
| ---: | :--- |
| 12.2 % | `add_row_avx512` (accumulator rows) |
| **7.1 %** | `std::sys::pal::windows::to_u16s` (environment name conversion) |
| 6.3 % | `evaluate_head_avx512` |
| 5.4 % | `movegen::pseudo_legal` |
| 5.4 % | `Searcher::negamax` |
| 4.9 % | `PickerState::next` |
| 3.7 % | `Position::make_move` |
| 2.8 % | `CorrectionHistory::correction` |
| **1.5 %** + ~8 % in ntdll/kernelbase | `std::sys::env::getenv` and its system calls |

`make_move` is 3.7 %, below the 5 % threshold: **make/unmake is not worth
doing** (the plan's rule). The surprise was the environment lookup: the move
picker called `std::env::var_os("KNIGHTBYTE_PICKER_TRACE")` once per node and
once per scored quiet move, about **18-20 % of all samples** (bug B43).

**P3a fix:** the flag is read once into a `OnceLock<bool>`. Pins exact,
`capture_bench.py compare` +0, all tests green.

| Measurement | Before | After |
| :--- | ---: | ---: |
| `bench 12` median (interleaved) | 1.20 - 1.23 M | 1.68 - 1.74 M |
| `bench 16` single run | 1.46 M | 2.10 M |
| `bench 18` single run | 1.42 M | 2.28 M |

**Same-session depth 20** (identical 18,662,358 nodes, interleaved):

| KnightByte NPS | Predecessor NPS | Ratio |
| ---: | ---: | ---: |
| 2,431,939 | 1,408,798 | 1.73 |
| 2,381,957 | 1,408,108 | 1.69 |

This passes the same-session form of milestone R3 (>= 1.60 M, i.e. 1.5x the
predecessor) and is at the formal 2.43 M mark. R4 (thread scaling) and R5
(Apple Silicon) remain open.

### 7.10 P7.0 - thread scaling was a node-accounting defect (B44, 2026-11-19)

With B43 fixed the 8-thread NPS still looked collapsed. Independent
searchers in one process (a throw-away probe), with and without a shared TT,
scaled almost perfectly (8 threads: 13-15 M nodes/s), and a per-helper node
print showed each helper at ~2.6 M nodes/s while the reported total was low.
Cause: bug B44 (helpers never published their blocks; the main searcher
published itself into the counter it reports from). The earlier
"saturation/pool/TT" hypotheses of P7 were wrong; no change to the pool, the
TT or the eval cache was needed.

`perf_test.py --depth 18 --threads 1,2,4,8 --hash 256` (same session,
identical positions, aggregate NPS over all threads):

| Threads | KnightByte NPS | Speedup | PawnForge NPS | Speedup |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2,316,332 | 1.00x | 1,337,563 | 1.00x |
| 2 | 4,792,734 | 2.07x | 2,578,258 | 1.93x |
| 4 | 9,286,033 | 4.01x | 5,044,434 | 3.77x |
| 8 | 16,393,170 | **7.08x** | 7,683,494 | 5.74x |

Extra (`go movetime` on the start position, reported NPS): 12 threads
20.6 M, 16 threads 23.3 M (SMT region). R4 (>= 5x at 8 threads on 8 cores) is
reached. The 1-thread pins are unchanged.

### 7.11 P6 - PGO and target-cpu (2026-11-19)

* **`target-cpu=znver4`** (plain build): no gain (`bench 16` 2.17-2.18 M vs
  2.17-2.21 M), so no per-CPU rung is worth shipping for this host class.
* **PGO** (`tools/pgo_build.ps1`: instrumented build, `bench 12` + `bench 16`
  profile runs, `llvm-profdata merge`, `-Cprofile-use` rebuild, node pins
  verified): `bench 16` median 2.16-2.21 M -> 2.36-2.42 M (+7-12 %). Pins
  exact at 6/12/16/18. The profile is per host and is never checked in
  (`*.profdata` / `*.profraw` and `pgo-work/` are git-ignored).

Same-session depth 20 (identical 18,662,358 nodes, interleaved):

| Build | KnightByte NPS | Predecessor NPS | Ratio |
| :--- | ---: | ---: | ---: |
| plain release | 2,348,083 - 2,378,056 | 1,363,397 - 1,406,052 | 1.67 - 1.74 |
| **PGO release** | **2,511,531 - 2,614,226** | 1,363,397 - 1,406,052 | **1.79 - 1.92** |

The PGO build passes the formal R3 mark (>= 2.43 M) and the same-session form
(>= 1.60 M). Shipping the PGO binary is a packaging decision (the universal
Windows build and the per-ISA variants of ADR-0001 each need their own profile).

### 7.12 P1c - early-exit forced-reply count (2026-11-19)

The forced-reply extension asked `pseudo_legal(position).len() == 1`, which
builds the whole move list at every eligible node (not in check, depth >= 3,
extension budget left) just to learn whether it holds exactly one move.
`movegen::has_exactly_one_pseudo_move` gives the same answer without the list:
knight, slider and king targets are only counted (popcount of the attack set
without own pieces), the answer is "no" as soon as a second move is seen, and
only pawn moves and castles are built, and only when the pieces produced
fewer than two. The plan predicted under 1 %.

Interleaved A/B, plain release, `bench 16` median of three after a warm-up
(2,815,799 nodes both):

| Round | Before | After | Change |
| ---: | ---: | ---: | ---: |
| 1 | 2,196,411 | 2,261,685 | +3.0 % |
| 2 | 2,174,362 | 2,298,611 | +5.7 % |
| 3 | 2,161,012 | 2,287,407 | +5.9 % |

Pins 15,528 / 456,308 / 2,815,799 / 6,892,874 exact and every per-depth delta
+0 at depths 1-12, 14, 16, 18. The equality with the full generator is pinned
by `tests/legality.rs` (`the_early_exit_forced_reply_count_matches_the_full_generator`):
over 340,000 walked positions plus four hand-found positions in which the side
to move has exactly one pseudo-legal move (a random walk never reaches one:
the king is walled in by its own pieces). Dropping the king from the count
makes the test fail.

### 7.13 P4a - fused accumulator push (2026-11-19)

The incremental update copied the parent perspective into the child (a 1 KB
clone per perspective) and then made one load-modify-store pass over the child
for every changed row (two to four rows per move). `kernels::push_rows` now
computes `child = parent - sum(subtracted rows) + sum(added rows)` in a single
pass: each vector chunk is loaded once, updated in a register and stored once,
whatever the row count. The shape (rows added, rows subtracted) is a const
generic, so the three shapes moves produce (quiet 1/1, capture or promotion
1/2, castling 2/2) unroll; any other shape takes the scalar path. `i16`
wrapping arithmetic is associative, so the result is bit-identical to the old
copy-then-apply order. Kernels exist for SSE4.1, AVX2 and AVX-512 plus the
scalar reference; the old per-row `apply_row` helper is gone.

Interleaved A/B, plain release, median of three after a warm-up, previous
commit (P1c) against this one:

| Run | Before | After | Change |
| :--- | ---: | ---: | ---: |
| `bench 16` round 1 | 2,101,342 | 2,329,031 | +10.8 % |
| `bench 16` round 2 | 2,240,094 | 2,376,201 | +6.1 % |
| `bench 16` round 3 | 2,227,689 | 2,362,247 | +6.0 % |
| `bench 16` round 4 | 2,218,911 | 2,252,639 | +1.5 % |
| `bench 18` round 1 | 2,213,511 | 2,411,782 | +9.0 % |
| `bench 18` round 2 | 2,363,001 | 2,421,100 | +2.5 % |
| `bench 18` round 3 | 2,359,765 | 2,444,281 | +3.6 % |

The gain is positive in every round and about 4-6 % typically (the spread is
the machine state, not the code). Pins exact; every per-depth delta +0.
`tests/` and the kernel unit tests compare the fused kernels with
copy-then-apply for the production width, a width that is not a multiple of 32
and an awkward tail, for every backend the host supports.

### 7.14 P4c - exact `madd` output head (2026-11-19)

The three SIMD heads widened the clamped activations and the weights to `i32`
(two extractions and four widenings per vector), squared with `mullo_epi32`,
shifted, multiplied by the weight with `mullo_epi32` again and added. Because
the loader accepts only `QA = 255`, `clamp(a, 0, 255)^2` is at most 65,025 and
fits an unsigned 16-bit lane, so the head is now exact in 16-bit lanes:
`min`/`max` clamp, `mullo_epi16` squares, a logical shift by 8 leaves `v <=
254` (it fits a signed lane), and `madd_epi16` multiplies by the 16-bit weight
and adds adjacent pairs into 32-bit lanes with no widening. A lane gains at
most `2 x 254 x 32768` per iteration and the kernel folds into the `i64` total
every 32 iterations, which keeps every partial sum below 1.1 G. One macro
generates the SSE4.1, AVX2 and AVX-512 versions; a scalar tail handles widths
that are not a multiple of the vector (the old 512-bit head read past the end
for widths of 16 mod 32; the production width is 512, so it was latent).

Interleaved A/B, plain release, `bench 16` median of three after a warm-up,
previous commit (P4a) against this one: 2,336,762 -> 2,388,294 (+2.2 %),
2,366,217 -> 2,392,352 (+1.1 %), 2,400,510 -> 2,448,520 (+2.0 %),
2,358,290 -> 2,398,465 (+1.7 %). Small, but positive in every round, and
bit-exact: pins exact, per-depth deltas +0.
`every_supported_head_kernel_matches_the_scalar_reference` runs each backend
the host supports against the scalar oracle on random data at widths 16, 48,
64, 512 and 1040 and on the overflow worst case at the maximum width (2048
saturated activations against `i16::MAX` and `i16::MIN` weights).
