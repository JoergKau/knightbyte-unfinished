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
* **PGO** (`tools/pgo_build.py`: instrumented build, `bench 12` + `bench 16`
  profile runs, `llvm-profdata merge`, `-Cprofile-use` rebuild, node pins
  verified): `bench 16` median 2.16-2.21 M -> 2.36-2.42 M (+7-12 %). Pins
  exact at 6/12/16/18. The profile is per host and is never checked in
  (`*.profdata` / `*.profraw` and `target/pgo-work/` are git-ignored).

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

**Correction (same day, section 7.15):** the three-round table below used an
unbalanced back-to-back order and overstated the gain. The balanced ABBA
measurement is **+1.1 %** (2,222,125 -> 2,246,433 NPS, 12 runs each); the table
is kept as the record of what was first measured.

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

The gain is positive in every round (the spread is the machine state, not the
code). Balanced ABBA re-measurement (section 7.15): **+5.1 %** (2,269,895 ->
2,385,101 NPS, 12 runs each). Pins exact; every per-depth delta +0.
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
bit-exact: pins exact, per-depth deltas +0. Balanced ABBA re-measurement
(section 7.15): **+4.6 %** (2,311,972 -> 2,418,333 NPS, 12 runs each); the
unbalanced rounds above understated it.
`every_supported_head_kernel_matches_the_scalar_reference` runs each backend
the host supports against the scalar oracle on random data at widths 16, 48,
64, 512 and 1040 and on the overflow worst case at the maximum width (2048
saturated activations against `i16::MAX` and `i16::MIN` weights).


### 7.15 P4d - fused full refresh, and the balanced A/B method (2026-11-19)

**Method correction.** A back-to-back pair is biased: two byte-identical
binaries measured 1.7-4.5 % apart in four rounds, always in favour of the one
that ran first. `tools/ab_bench.py` replaces it: each round runs the engines in
the order A B B A, every run checks the node pin, and the change is the ratio of
the mean NPS over all runs. The A/A control (same binary on both sides, 8 runs
each) reads -0.76 % with a per-run spread of about 2 %, so a change under about
1.5 % needs more rounds before it is believed. From here on every speed claim
uses this tool; the earlier P1c, P4a and P4c figures were re-measured with it
(see 7.12-7.14).

**P4d.** `refresh_into` copied the bias and then made one load-modify-store pass
over the whole accumulator for each of the 20-30 active feature rows. The new
`kernels::refresh_rows` loops over vector chunks on the outside: each chunk
starts from the bias, collects the matching chunk of every row in a register and
is stored once. Row addition is wrapping `i16`, so the result is bit-identical;
SSE4.1, AVX2 and AVX-512 kernels plus the scalar reference exist, and `add_row`
(now unused) is removed. A refresh happens 156 K times in `bench 16` (own-king
moves and the root), against 1.89 M incremental pushes.

| Comparison (balanced ABBA, plain release) | Runs | A | B | Change |
| :--- | ---: | ---: | ---: | ---: |
| P4c -> P4d, `bench 16` | 12 + 12 | 2,399,267 | 2,442,504 | **+1.8 %** |
| P4c -> P4d, `bench 18` | 8 + 8 | 2,518,949 | 2,550,952 | **+1.3 %** |
| P1c -> P4a (fused push), `bench 16` | 12 + 12 | 2,269,895 | 2,385,101 | +5.1 % |
| P4a -> P4c (madd head), `bench 16` | 12 + 12 | 2,311,972 | 2,418,333 | +4.6 % |
| pre-P1c -> P4d (this session), `bench 16` | 12 + 12 | 2,195,128 | 2,514,758 | **+14.6 %** |

Pins exact and every per-depth delta +0.
`the_fused_refresh_matches_adding_the_rows_one_by_one` compares the dispatcher,
the scalar reference and every backend the host supports against adding the rows
one at a time (0 to 32 rows; widths 512, 48 and 37).

**Profile after P4c (`bench 18`, self time, main thread):** picker `next` +
`pick_best` 14.7 %, `negamax` 11.4 %, `evaluate_head` 9.7 % (before P4c),
`push_accumulators` 8.1 %, `make_move` 6.6 %, `legal` 5.7 %, `push_rows` 6.2 %,
`correction` 4.7 %, `add_row` 4.7 % (before P4d), SEE (`see_ge`,
`allowed_attackers`, `pinned_pieces`) 6.2 %.

### 7.16 P4e - lazy pins in SEE, and the close of P4 (2026-11-19)

SEE computed the pin masks of both colours up front for every exchange that
reached the attacker walk, and each mask used a `between` that walked the line
square by square. Most exchanges end at the first step because the opponent has
no recapture, so the pin question was asked and never used. Now:

* the pin mask of a colour is computed on first use (`LazyAllowed`), and only
  for a side that actually has an attacker. A colour's mask is only ever applied
  to that colour's own pieces and the position never changes during the walk,
  so this is exactly the old answer;
* `between` is the shared 64 x 64 table of `board::legality`; the old
  geometric walk stays in the test module as the oracle
  (`the_shared_between_table_matches_the_geometric_walk`, all 4,096 pairs);
* `attackers_to` receives the slider masks instead of rebuilding them.

| Comparison (balanced ABBA, plain release) | Runs | A (P4d) | B (P4e) | Change |
| :--- | ---: | ---: | ---: | ---: |
| `bench 16` | 12 + 12 | 2,493,721 | 2,579,408 | **+3.4 %** |
| `bench 18` | 8 + 8 | 2,412,218 | 2,456,943 | **+1.9 %** |

Pins exact, every per-depth delta +0 (SEE feeds pruning and ordering, so a wrong
answer anywhere would have moved a node count).

**P4 is closed.** Items of the plan: fused push (P4a), exact `madd` head (P4c),
fused refresh (P4d) and SEE (P4e) are done. The *contiguous accumulator pair*
is dropped: its purpose was one allocation and one 2 KB `memcpy` per make, and
the fused push already removed the copy (the child is written straight from the
parent while the rows are applied); only the null-move copy remains, which is a
rare path. The *dispatch hoist* is dropped as well: the kernel kind is a
process-wide `OnceLock` read once per call and the branch is perfectly
predicted; the kernels now run once per perspective per node instead of once
per row.


### 7.17 Same-session state after P1c and P4 (2026-11-19)

Same host (Ryzen 7 260), same session, plain release (no PGO), `perf_test.py
--depth 20 --threads 1 --hash 256`, engines interleaved K P P K K P P K, identical
tree for both (18,662,358 nodes, EBF 2.131):

| Engine | Runs (NPS) | Mean |
| :--- | :--- | ---: |
| KnightByte (P4e) | 2,186,574 / 2,170,190 / 2,215,064 / 2,174,246 | 2,186,519 |
| predecessor | 1,026,414 / 1,029,158 / 1,047,705 / 998,163 | 1,025,360 |

Ratio **2.13x**. The machine was in a slower state than during the earlier
depth-20 comparison (7.11: plain 2.35-2.38 M against 1.36-1.41 M, 1.67-1.74x),
so the absolute figures are lower and the ratio is not directly comparable to
that row; compare only within one session.

Single-thread speed of this session's steps, balanced ABBA, `bench 16`:
pre-P1c build 2,086,964 -> P4e 2,447,169 NPS (**+17.3 %**, 12 runs each). Steps:
P1c +1.1 %, P4a +5.1 %, P4c +4.6 %, P4d +1.8 %, P4e +3.4 %.

Open on this host: the match gate and a PGO build of the new code (the PGO
profile of 7.11 was recorded before these changes).



### 7.18 PGO rebuild of the new code (2026-11-19)

`tools/pgo_build.py` was re-run on the code after P1c and P4 (the profile of 7.11
predates the fused kernels and the madd head; profiles are per host and per
build, never reused). Pins verified exact by the script.

| Comparison | Runs | A | B | Change |
| :--- | ---: | ---: | ---: | ---: |
| plain P4e -> PGO, `bench 16`, balanced ABBA | 8 + 8 | 2,416,331 | 2,571,762 | **+6.4 %** |

Same-session depth 20, identical tree (18,662,358 nodes), `perf_test.py --depth 20
--threads 1 --hash 256`, interleaved K P P K K P P K:

| Engine | Runs (NPS) |
| :--- | :--- |
| KnightByte, PGO | 2,318,749 / 2,313,811 / 2,352,137 / 2,322,170 |
| predecessor | 743,134 / 744,401 / 1,034,139 / 1,026,176 |

The predecessor's first two runs are slow outliers (a different machine state or
JIT warm-up in that stretch; its later pair is 1.03 M), so the steadier pair gives
**2.25x**, the outlier pair 3.1x. Compare within a session only; the plan's R3
mark (>= 2.43 M against the recorded 1.569 M baseline) is met in the same-session
form (>= 1.60 M) by a wide margin.



### 7.19 Match gate on Zen 4 (2026-11-19)

**1 thread, PGO build (7.18), 500 games vs the predecessor.** `fastchess` 1.8.3,
`8+0.08`, `Hash` 128 MB for both, UHO opening set (`order=random`, games
repeated in colour pairs), resign `movecount=3 score=600`, draw `movenumber=34
movecount=8 score=20`, 6 concurrent games, KnightByte 1 thread against the
single-threaded predecessor. Command and PGN are kept with the project notes
(`Step-14-match-gate/1-thread`).

| Result | Value |
| :--- | :--- |
| Games finished | 500 of 500 |
| Wins / losses / draws (KnightByte) | 281 / 30 / 189 |
| Score | 375.5 / 500 = **75.1 %** |
| Elo | **+191.8 +/- 20.6** (nElo +346 +/- 30), LOS 100 % |
| Pentanomial (0-2) | [0, 6, 45, 141, 58] |
| Time losses, illegal moves, disconnects, crashes | **none** (no such line in the output, empty log) |
| Total time | 32 min 38 s |

Endings: 306 wins by adjudication, 5 mates, 108 threefold repetitions, 40 draws
by adjudication, 24 by the fifty-move rule, 17 by insufficient material. The
same match before the move-path and eval work (3 x 100 games recorded in the
plan) scored 25 %: the speed work and B43/B44/B49/B50 fixes show up in play.
The score belongs to the PGO build of this session's code; it is a regression
and safety gate, not a strength measurement for the rating list.

**2, 4 and 8 threads.** The project owner ran the multi-thread matches against
the same predecessor and reports **no errors** in any configuration (no illegal
moves, time losses or crashes) and a strong result; the figures and PGNs stay
with the owner and are to be added here when supplied. A run that was started
here for the 2-thread case was stopped on the owner's instruction and discarded.



### 7.20 Windows build variants and the acceptance gate (2026-11-19)

Measured for ADR-0011 on the Zen 4 host: PGO builds with their own profile per
variant, node pins exact for all, balanced ABBA (`tools/ab_bench.py`), `bench 16`
unless stated, 12-16 runs per side, A/A control +0.12 %. "Gain" is how much
faster the second build is.

| A | B | Gain |
| :--- | :--- | ---: |
| baseline x86-64 (the build as it was) | dedicated `v3` | +2.1 % |
| baseline x86-64 | dedicated `v4-vnni` | +4.1 % (`bench 18`: +2.4 %) |
| baseline x86-64 | `v2` baseline | +3.0 % |
| `v2` baseline | dedicated `v3` | +1.0 % |
| `v2` baseline | dedicated `v4-vnni` | +2.3 % (`bench 18`: +2.6 %) |
| dedicated `v3` | dedicated `v4-vnni` | +1.5 % (`bench 18`: +1.8 %) |
| dynamic C runtime (`v3`) | static C runtime (`v3`) | +0.3 % (noise) |

Reading: the build that existed before this step (no `target-cpu`) loses 2.1-4.1 %
to the dedicated builds and misses the 2 % gate; a `v2` baseline still misses it
against `v4-vnni`; a `v3` baseline meets it (1.5-1.8 %). All builds run the same
AVX-512 kernels on this host (the dispatch is at run time), so the differences
are compiler code generation outside the kernels. Decision in ADR-0011: ship
`v3` (default), `v4-vnni` and `v2`, each with its own profile, built by
`tools/build_release.py`.

`bench 16` on the shipped files, same session: `v3` 2.70 M NPS (balanced
against the previous dynamic-runtime `v3`). The release script's checks (all
three variants): `uci` answers `uciok`, `bench 6` = 15,528, a run forced to the
scalar kernels reports the identical count, the import table lists only
`KERNEL32`, `ntdll`, `bcryptprimitives` and `api-ms-win-core-synch`, the
SHA-256 of every file matches `SHA256SUMS.txt` after unpacking the zip.

Not measured: the second Windows host (mainstream AVX2 CPU, where the `v3` against
`v2` comparison should be repeated and the `v4-vnni` build cannot run), and a
clean Windows 10 machine without the Visual C++ redistributable.



### 7.21 Allocation audit of the search (2026-11-19)

`tests/alloc_audit.rs` installs a counting global allocator that counts only the
thread running the search, warms the process up (lazily built tables, the
network, the kernel selection allocate once per process), then searches the four
bench positions to depth 8 and to depth 12 with a cleared table and records the
allocations of each search.

| Position | Depth | Nodes | Iterations | Allocations | Bytes |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 8 | 29,604 | 8 | 16 | 108 |
| 0 | 12 | 305,691 | 12 | 24 | 188 |
| 1 | 8 | 7,832 | 8 | 16 | 92 |
| 1 | 12 | 59,503 | 12 | 24 | 216 |
| 2 | 8 | 4,237 | 8 | 16 | 180 |
| 2 | 12 | 38,151 | 12 | 24 | 280 |
| 3 | 8 | 9,686 | 8 | 16 | 164 |
| 3 | 12 | 52,963 | 12 | 24 | 284 |

Result: **exactly two allocations per completed iteration and none per node or
per search.** They are the principal-variation vectors of the iteration report
and the carried result, outside the node loop; a 305,691-node search allocates 24
times. The search, move generation, the picker, the evaluation, the accumulator
kernels, SEE and the transposition table therefore contain no `Vec` growth,
`Box`, `String` or `format!` on the node path.

The test asserts a budget of three per iteration plus a handful, and that the
deeper search (several times the nodes) allocates no more than its extra
iterations explain. Sensitivity: a `Box` added to the quiescence entry made it
fail. Not covered: the UCI output path (it formats one line per iteration, by
design), the helper threads' own `Searcher` construction (once per helper) and
the diagnostic `nnueeval` command (builds an evaluator per call).

### 7.22 Search diagnostics cost (2026-11-19)

The runtime-checked version of `searchstats` cost -2.7 % at `bench 16`; the
second-instantiation design of ADR-0012 costs nothing measurable (-0.07 % at
`bench 18`, 16 runs each, balanced ABBA). The counters are byte-identical to the
predecessor's on four positions.

### 7.23 P4b - the AArch64 NEON kernels and the macOS release (2026-11-20, Apple M4)

The macOS side starts from a build defect, not a performance number: the
AArch64 target did not compile (non-exhaustive `KernelKind` matches; bug B52),
so the documented scalar fallback had never run on an Apple machine. The NEON
backend (ADR-0015) is selected at start-up because NEON is part of the AArch64
baseline; one binary therefore covers every Apple M processor.

Host: MacBook Air M4, Apple M4, macOS 27.2.1, rustc 1.100.0, release profile.
`bench 12/16/18`, one thread, compiled defaults; 5 (12) or 3 (16/18) interleaved
runs per backend, NEON selected and `KNIGHTBYTE_FORCE_ISA=scalar` forced.

| Bench | Scalar median NPS | NEON median NPS | Ratio |
| :--- | ---: | ---: | ---: |
| 12 | 440,027 | 1,721,916 | 3.91x |
| 16 | 435,612 | 1,888,530 | 4.33x |
| 18 | 427,412 | 1,875,611 | 4.39x |

Node identity is exact for every backend: `bench 6/12/16/18` =
**15,528 / 456,308 / 2,815,799 / 6,892,874** with NEON, with the forced scalar
kernels, and in the release artifact. `perft 3` on startpos is 8,902 and
`nnueeval` on startpos is 27, as on Windows.

The release artifact (`tools/build_release_macos.py`, ADR-0015) is
`aarch64-apple-darwin` built with `-Ctarget-cpu=apple-m1`; the script checks
`uciok`, all four pins, the forced-scalar and forced-NEON identity and the
arm64-only Mach-O slice, then packs a `tar.gz` with `SHA256SUMS.txt` and
`manifest.json`. Measured for reference on the M4: 1.72-1.89 MNPS at bench
12-18, above the recorded C# M4 baseline (1.66 MNPS) but below the 2.48 MNPS
stretch goal, which remains search-tuning work.

**PGO on macOS (2026-11-20).** `tools/pgo_build.py` is host-independent, so the
same four-step build (instrumented run of `bench 12` + `bench 16`,
`llvm-profdata merge`, profile-use rebuild, pin verification) runs on the M4.
The macOS release uses it by default; `--no-pgo` gives the plain build. Same
session, interleaved three-run pairs:

| Bench | Plain NPS | PGO NPS | Gain |
| :--- | ---: | ---: | ---: |
| 12 | 1,839,951 | 1,950,034 | +6.0 % |
| 16 | 1,868,479 | 2,002,701 | +7.2 % |
| 18 | 1,867,987 | 2,008,998 | +7.5 % |

That is the same order as the Windows PGO result (+6.4 % in 7.18), and the
four pins stay exact. The profile is generated on the build host, is never
committed and does not change the instruction baseline: the binary still runs
on M1-M5.

### 7.24 Linux x86-64 release: AVX2 baseline, PGO and the last G6 row (2026-11-20, Ryzen 5 5600H)

The Linux artifact is one `x86_64-unknown-linux-gnu` binary compiled with
`-Ctarget-cpu=x86-64-v3` (AVX2, BMI2, FMA) and PGO by default (ADR-0016).
Host: AMD Ryzen 5 5600H (Zen 3, 6C/12T, no AVX-512), Linux, rustc 1.100.0,
release profile, one thread, 256 MB hash.

Medians of 5 (bench 12) or 3 (16/18) interleaved runs:

| Build | bench 12 NPS | bench 16 NPS | bench 18 NPS |
| :--- | ---: | ---: | ---: |
| portable (no `target-cpu`) | 1,505,966 | 1,920,736 | 1,997,355 |
| `x86-64-v3`, plain | 1,584,402 | 1,935,257 | 2,017,820 |
| `x86-64-v3`, PGO (the release) | 1,683,793 | 2,131,566 | 2,186,131 |

Balanced ABBA pairs (`tools/ab_bench.py`; A/A control +1.4 % at bench 16, the
noise floor of the host):

| Comparison | bench 12 | bench 16 | bench 18 |
| :--- | ---: | ---: | ---: |
| `v3` plain -> `v3` PGO | +5.0 % | +9.0 % | +8.9 % |
| portable -> `v3` plain | - | +0.9 % | - |

PGO is larger on this host than on Windows (+6.4 %) or the M4 (+6.0-7.5 %).
The `target-cpu` flag itself is close to free: with the kernels already
dispatched at run time, it mostly changes the rest of the code generation.

Kernel backends forced on the PGO binary at `bench 12` (median of 5 interleaved
runs, pin exact): scalar **427,254 NPS** vs AVX2 **1,683,793 NPS**, a **3.94x**
gain. This matches the macOS NEON ratio (3.91x) and confirms that the AVX2
kernels, not the compiler flags, carry the Linux speed.

Node identity across the platform: `bench 6/12/16/18` =
**15,528 / 456,308 / 2,815,799 / 6,892,874** in the plain `v3` build, the PGO
build and the packaged binary, with the scalar and AVX2 kernels forced. The
unpacked archive answers `uciok`/`readyok`, reports `perft 3` = 8,902 and
`nnueeval` = 27 on the start position. With this row the migration plan's G6
is verified on Windows x64, macOS Apple Silicon and Linux x86-64.
