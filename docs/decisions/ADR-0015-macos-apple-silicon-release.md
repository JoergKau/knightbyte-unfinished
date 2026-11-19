# ADR-0015 - macOS release: one Apple Silicon build with NEON kernels

**Date:** 2026-11-20
**Status:** accepted
**Related:** ADR-0001 (instruction-set dispatch), ADR-0011 (Windows variants)

## Context

The migration plan requires a macOS Apple Silicon artifact, and the engine is
developed on an M4. The AArch64 target did not actually build:

- `KernelKind` had the x86-only variants (`Sse41`, `Avx2`, `Avx512`) with their
  dispatch arms behind `cfg(target_arch = "x86_64")`, so the three `match`
  statements were non-exhaustive on AArch64 (rustc E0004). The documented
  "scalar fallback" never existed as a compiling path.
- The cross-backend test used `is_x86_feature_detected!` without a platform
  guard, and the PEXT table static and its test-only ray helpers were dead code
  on AArch64.

At the same time the requirement is deliberately modest: **one** macOS binary
that runs on every Apple M processor (M1-M5), no Intel Mac support and no
per-chip optimization. That rules out the Windows-style multi-variant ladder:
there is nothing useful to dispatch between, because 128-bit NEON is part of the
AArch64 baseline.

## Decision

1. **NEON is the AArch64 backend.** A new `KernelKind::Neon` implements the same
   three kernels as the x86 backends: fused incremental row update, fused full
   refresh and the SCReLU output head. NEON is required by the AArch64 baseline,
   so the backend is selected at start-up without a runtime feature probe.
   `KNIGHTBYTE_FORCE_ISA=neon` and `KNIGHTBYTE_FORCE_ISA=scalar` keep the same
   A/B and parity testing surface as the x86 backends.
2. **Integer exactness is unchanged.** The row kernels use `vaddq_s16` /
   `vsubq_s16`, which wrap exactly like scalar `i16` arithmetic. The head
   clamps in 16-bit lanes, squares with `vmulq_s16` (the clamped square,
   255² = 65,025, fits an unsigned 16-bit lane), shifts right by eight bits,
   widens with `vmlal_s16` and folds the `i32` lanes into the `i64` total every
   16 vectors. The fold bound keeps the partial sum below `2^29`; the result is
   bit-identical to the scalar oracle.
3. **One artifact.** The release is built for `aarch64-apple-darwin` with
   `-Ctarget-cpu=apple-m1`. That baseline (NEON, DotProd, FP16, LSE, RCPC) is
   present on every Apple M processor, so a single binary covers M1-M5. There is
   no `apple-m2`/`apple-m4` artifact and no DotProd or i8mm kernel: the
   requirement is portability, and per-chip variants would add packaging and
   validation surface. The release is **profile-guided (PGO)** like the Windows
   one, with its own profile generated on the build host from `bench 12` and
   `bench 16`; `--no-pgo` produces the plain build. The profile changes code
   layout only, never the search: the four node pins are verified on the result.
4. **Intel macOS is not supported.** No `x86_64-apple-darwin` slice is built and
   no `lipo` universal file is produced. The release script refuses to run off
   an arm64 Mac and fails if the Mach-O architecture is anything but `arm64`.
5. **One script owns the artifact.** `tools/build_release_macos.py` runs the
   lever effect gate, builds, verifies (`uciok`, all four bench pins, forced
   scalar and forced NEON identical, arm64 only), hashes and packs the binary
   with the README, licence and third-party notices into a `tar.gz` with
   `SHA256SUMS.txt` and `manifest.json`. `.github/workflows/release.yml` runs it
   on every `v*` tag on an arm64 macOS runner.

## Measurements (Apple M4, macOS 27, rustc 1.100.0, release build)

`bench 12/16/18`, one thread, defaults, five (12) or three (16/18) interleaved
runs per backend; NEON is the selected backend and scalar is forced through
`KNIGHTBYTE_FORCE_ISA=scalar`.

| Bench | Scalar (median NPS) | NEON (median NPS) | Ratio |
| :--- | ---: | ---: | ---: |
| 12 | 440,027 | 1,721,916 | 3.91x |
| 16 | 435,612 | 1,888,530 | 4.33x |
| 18 | 427,412 | 1,875,611 | 4.39x |

PGO on the same host, interleaved three-run pairs against the plain
`apple-m1` build:

| Bench | Plain NPS | PGO NPS | Gain |
| :--- | ---: | ---: | ---: |
| 12 | 1,839,951 | 1,950,034 | +6.0 % |
| 16 | 1,868,479 | 2,002,701 | +7.2 % |
| 18 | 1,867,987 | 2,008,998 | +7.5 % |

The node totals are unchanged by the backend or by PGO: `bench 6/12/16/18` =
**15,528 / 456,308 / 2,815,799 / 6,892,874** for NEON, scalar, the plain build
and the PGO build. The macOS binary therefore searches the same tree as the
Windows artifacts.

## Consequences

- The AArch64 build compiles and is covered by the same kernel-parity tests as
  x86-64 (`tests/kernels_backends.rs` now includes a `neon` row, and the unit
  tests compare every NEON kernel against the scalar oracle at production
  widths, tail widths and the head's worst case).
- No start-up CPU guard is needed on macOS: every supported Apple M processor
  provides the `apple-m1` baseline. A future `apple-m2` variant would need one.
- The PGO profile is generated on the release host (M4 here) and never checked
  in. It guides code layout only; the binary remains an `apple-m1` baseline
  build that runs on M1-M5. A profile from a newer chip is valid on older ones,
  but the measured percentage belongs to the profiling host.
- The archive is ad-hoc signed by the toolchain but not notarized. macOS may
  require the user to allow the downloaded binary once; the README says so.
- Linux ARM stays unsupported, as the migration plan's non-goals state.
