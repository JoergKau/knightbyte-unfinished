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
| `bench 12` | 416,533 | Search identity |
| `bench 16` | 3,169,859 | Search identity |
| `bench 18` | 6,925,762 | Search identity |

The `bench` set is four fixed positions (startpos, Kiwipete, an endgame and a
tactical middlegame), Hash 256 MB, one thread.

### KnightByte current totals (node identity, ADR-0005)

KnightByte `bench` after the second audit round (2026-11-17),
Windows 11 / Ryzen 7 260, Rust 1.100.0 release, Hash 256 MB, one thread:

| Run | Nodes | Reference | Delta |
| :--- | ---: | ---: | ---: |
| `bench 6` | 14,979 | 14,979 | 0 |
| `bench 12` | 416,533 | 416,533 | 0 |
| `bench 16` | 3,169,859 | 3,169,859 | 0 |
| `bench 18` | 6,925,762 | 6,925,762 | 0 |

Every per-position count of the captured depths (1-12, 14, 16, 18) is
identical to the reference as well. The earlier state (ADR-0004: `bench 6`
14,963, `bench 12` 356,262, `bench 16` 3,077,942, `bench 18` 6,797,622) was
traced to concrete porting differences and removed; ADR-0005 supersedes it.
The audit record is `docs/audit/migration-audit.md`, the defects are
`docs-new/new-bugs.md` (B18-B28).

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
the internal checkout is not published with KnightByte):

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

Filled in Phase 7 and at every release candidate.

| Host | Build | Threads | Avg depth | Avg NPS | Total nodes | EBF | WAC | Tactics |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| m4 | TBD | 1 | — | — | — | — | — | — |
| m4 | TBD | 4 | — | — | — | — | — | — |
| m4 | TBD | 8 | — | — | — | — | — | — |
| zen4 | TBD | 1 | — | — | — | — | — | — |
| zen4 | TBD | 4 | — | — | — | — | — | — |
| zen4 | TBD | 8 | — | — | — | — | — | — |
