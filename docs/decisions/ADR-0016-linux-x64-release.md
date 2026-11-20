# ADR-0016 - Linux release: one x86-64 AVX2 build with PGO

**Date:** 2026-11-20
**Status:** accepted
**Related:** ADR-0001 (instruction-set dispatch), ADR-0011 (Windows variants), ADR-0015 (macOS)

## Context

Phase 8 asks for a release artifact for every supported OS family. macOS Apple
Silicon and Windows are finished; the Linux artifact is the last one, and it is
also the G6 cross-platform evidence the migration plan still wants: the same
bench node pins on macOS, Windows and Linux.

The supported Linux surface is deliberately narrow: **x86-64 only**, no ARM
(the migration plan's non-goals already exclude Linux ARM). The owner's
directive for this step is an AVX2 baseline and a tested PGO build, because PGO
was the largest single gain on both other platforms (+6.4 % at `bench 16` on
Windows, +6.0-7.5 % at `bench 12-18` on the M4).

## Decision

1. **One binary, `x86_64-unknown-linux-gnu`, compiled for `x86-64-v3`.** That
   baseline is AVX2, BMI1/BMI2, FMA and the earlier levels: Intel Haswell (2013)
   and newer, AMD Zen and newer. The CCRL host class (Haswell) is inside it.
   A pre-AVX2 CPU gets the start-up guard's message naming the missing features
   instead of an illegal-instruction fault. `--target-cpu x86-64-v2` produces the
   portable fallback for such machines; it is not part of the shipped release.
2. **The runtime kernel dispatch is unchanged.** The file is compiled for the
   AVX2 baseline, but the SIMD kernels are still selected once per process from
   the detected feature set (ADR-0001), so a CPU with AVX-512 runs the 512-bit
   kernels and every backend can be forced with `KNIGHTBYTE_FORCE_ISA`.
3. **PGO is part of the release, not an option.** `tools/pgo_build.py` runs the
   instrumented `bench 12` and `bench 16`, merges the profile, rebuilds with it
   and verifies the four bench node pins. The profile is generated on the build
   host, never committed and never reused across hosts. `--no-pgo` exists for
   the plain comparison build.
4. **One script owns the artifact.** `tools/build_release_linux.py` refuses to
   run off an x86-64 Linux host, runs the lever effect gate, builds (PGO by
   default), verifies `uciok`, all four bench pins, the forced scalar and forced
   AVX2 identity, and that the file is a 64-bit little-endian x86-64 ELF. It then
   writes `SHA256SUMS.txt` and `manifest.json` and packs the binary with
   `README.md`, `LICENSE` and `THIRD-PARTY-NOTICES.md` into a `tar.gz`.
5. **The tag workflow builds it on `ubuntu-22.04`.** That fixes the glibc floor
   at 2.35, so the artifact runs on Ubuntu 22.04/24.04 and Debian 12 and newer;
   the engine code itself is platform-neutral. `.github/workflows/release.yml`
   runs the script on every `v*` tag and attaches the archive next to the macOS
   one.
6. **No ARM artifact and no ARM kernel work.** Linux ARM stays a non-goal.

## Measurements (Ryzen 5 5600H, Linux, rustc 1.100.0, release build)

Host: AMD Ryzen 5 5600H (Zen 3, 6C/12T, AVX2/BMI2, no AVX-512), Linux, one
thread, compiled defaults, `bench` with a 256 MB hash. Absolute NPS is
host-relative. Medians of 5 (bench 12) or 3 (16/18) interleaved runs:

| Build | bench 12 | bench 16 | bench 18 |
| :--- | ---: | ---: | ---: |
| portable (no `target-cpu`) | 1,505,966 | 1,920,736 | 1,997,355 |
| `x86-64-v3`, plain | 1,584,402 | 1,935,257 | 2,017,820 |
| `x86-64-v3`, PGO (the release) | 1,683,793 | 2,131,566 | 2,186,131 |

Balanced ABBA pairs (`tools/ab_bench.py`; the A/A control reads +1.4 % at bench
16, which is this host's noise floor):

| Comparison | bench 12 | bench 16 | bench 18 |
| :--- | ---: | ---: | ---: |
| `v3` plain -> `v3` PGO | +5.0 % | +9.0 % | +8.9 % |
| portable -> `v3` plain | - | +0.9 % | - |

The `target-cpu=x86-64-v3` baseline is therefore close to free on this host:
the hot kernels were already dispatched to AVX2 at run time, so the flag only
changes code generation for everything else. The PGO gain is larger than on the
other platforms (+6.4 % Windows, +6.0-7.5 % macOS) and is the reason PGO is the
default here.

The kernel backends, forced on the release binary at `bench 12` (median of 5
interleaved runs, node pin exact in every run): scalar **427,254 NPS**, AVX2
**1,683,793 NPS**, a **3.94x** kernel gain. The node totals are identical, so
the AVX2 kernels change speed only.

All four pins are exact in the plain `v3` build, in the PGO build, and in the
packaged binary with the scalar and AVX2 kernels forced:
`bench 6/12/16/18` = **15,528 / 456,308 / 2,815,799 / 6,892,874**. The
unpacked archive reports `perft 3` = 8,902 and `nnueeval` = 27 on the start
position, like the other platforms.

## Consequences

- All three release platforms now reproduce the same node counts:
  `bench 6/12/16/18` = **15,528 / 456,308 / 2,815,799 / 6,892,874**, with the
  scalar and AVX2 kernels and in the packaged binary.
- The Linux binary needs AVX2 (and glibc 2.35 when the CI artifact is used).
  The README states both limits and points older CPUs at the `x86-64-v2` build
  and older distributions at building from source.
- The profile belongs to the profiling host; a profile generated on the CI
  runner guides code layout only, and the pins are verified after every build.
- `tools/build_release_linux.py` is the third and last per-OS packaging script;
  the shared PGO step stays in `tools/pgo_build.py`.
