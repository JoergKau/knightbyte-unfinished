# ADR-0001: Windows universal executable

**Status:** accepted for the Phase 0 prototype  
**Date:** 2026-11-17

## Context

The Windows release must run well on the CCRL reference CPU class (Haswell,
`x86-64-v3`) and on the Zen 4 test host (AVX-512 VNNI) without shipping a pile
of executables. A single artifact is preferred for testers and for the CCRL
submission. Detection must happen once per process; a per-node feature test
would erase the gain.

## Decision

Adopt **design A** for the first prototype:

1. Compile the engine for `x86-64-v3` as the baseline. This covers Haswell,
   Comet Lake and Zen 3/4 for the non-vector code.
2. Multi-version the hot kernels:
   - NNUE head/eval: AVX2 baseline, AVX-VNNI, AVX-512-VNNI;
   - attack generation: PEXT/BMI2 or magic bitboards, selected by a predictable
     runtime flag;
   - additional bulk kernels only when a profile shows a benefit.
3. Select the backend once at startup through `src/platform/cpu.rs`.
4. Keep the selector ladder explicit: `v4-vnni` → `v3-vnni` → `v3` → `v2`.
5. Ship a `KNIGHTBYTE_FORCE_ISA` override so every backend can be exercised on
   one machine in tests and CI.

Design B (monomorphize the whole engine per ISA with `#[target_feature]`
wrappers) is the fallback if design A loses more than 2% NPS against a
dedicated build. Design C (separate executables plus a launcher) is the last
resort; a self-extracting launcher is undesirable for anti-virus and tester
friendliness.

## Variant ladder

| Variant | Build baseline | Intended hosts |
| :--- | :--- | :--- |
| `v2` | `x86-64-v2` | Optional safety net for pre-AVX2 machines and VMs |
| `v3` | `x86-64-v3` | Haswell/Zen 1+; CCRL reference and mainstream Intel |
| `v3-vnni` | `v3` + `+avxvnni` | Intel 12th gen and later |
| `v4-vnni` | `x86-64-v4` + `+avx512vnni` (or `znver4`) | Zen 4/5, Sapphire Rapids+ |

On AMD Excavator and Zen 1/2 the `v3` build is still selected, but the attack
backend must use magic bitboards because PEXT/PDEP are microcoded.

## Acceptance gate

* The universal executable is within **2% NPS** of the dedicated `v3` and
  `v4-vnni` builds on each Windows host at the same depth.
* Node counts are identical across every backend.
* Every backend can be forced and is exercised in CI.
* Each variant is PGO-profiled separately.

## Consequences

* The build script and its per-variant flags/PGO profiles become part of the
  engine, not local shell history.
* The selector must be unit-tested independently of the host CPU.
* Adding a new backend means adding a variant row, a dispatch branch, a
  forced-selection test, and a dedicated-build comparison.
