# ADR-0011 - Windows build variants: a `v3` baseline, three PGO executables

**Date:** 2026-11-19
**Status:** accepted
**Amends:** ADR-0001 (the universal-executable prototype)

## Context

ADR-0001 chose "design A": one executable compiled for the `x86-64-v3` baseline
whose hot kernels are chosen at start-up, with a 2 % acceptance gate against
dedicated builds. The executable that was actually built never followed the first
half: `Cargo.toml` sets no `target-cpu`, so the compiler targeted plain x86-64
(no `popcnt`, no BMI, no `tzcnt`) outside the dispatched kernels. The gate was
therefore never measured. It is measured here on the Zen 4 host (Ryzen 7 260,
AVX-512 VNNI), the only Windows host available for this step.

## Measurements

All builds are PGO builds with their own profile (`tools/pgo_build.py`), the node
pins verified exact; speeds are balanced ABBA means of 12-16 runs at `bench 16`
unless stated (`tools/ab_bench.py`; the A/A control reads +0.1 %, the per-run
spread is 2-3 %). "Gain" is how much faster the second build is.

| Comparison | Gain |
| :--- | ---: |
| baseline x86-64 (as built before) -> dedicated `v3` | +2.1 % |
| baseline x86-64 -> dedicated `v4-vnni` | +4.1 % (`bench 16`), +2.4 % (`bench 18`) |
| baseline x86-64 -> `v2` baseline | +3.0 % |
| `v2` baseline -> dedicated `v3` | +1.0 % |
| `v2` baseline -> dedicated `v4-vnni` | +2.3 % (`bench 16`), +2.6 % (`bench 18`) |
| dedicated `v3` -> dedicated `v4-vnni` | +1.5 % (`bench 16`), +1.8 % (`bench 18`) |

The kernels are dispatched at run time in every build, so on this host all of
them run the AVX-512 kernels; the differences above are the compiler's code
generation for everything else (popcount, bit scans, `blsr`/`andn`, address
arithmetic). The baseline x86-64 executable misses the gate against both
dedicated builds, the `v2` baseline misses it against `v4-vnni`, and a `v3`
baseline meets it against `v4-vnni` (1.5-1.8 %). Statically linking the C
runtime costs nothing (+0.3 %, inside the noise).

## Decision

1. The **recommended Windows executable is the `v3` build**: compiled for
   `x86-64-v3` (AVX2, BMI1/2, FMA, `popcnt`, `lzcnt`), with the SIMD kernels and
   the PEXT/magic choice still selected once at start-up. It is within the 2 %
   gate of the dedicated `v4-vnni` build and runs on Haswell and newer and on
   Zen 1 and newer (Zen 1/2 use magic bitboards because their PEXT is
   microcoded).
2. The release ships **three executables**, each with its own PGO profile, built
   by one script (`tools/build_release.py`):

   | File | Compiled for | For |
   | :--- | :--- | :--- |
   | `knightbyte-v3.exe` | `x86-64-v3` | the default; mainstream CPUs since 2013, the rating-list host class |
   | `knightbyte-v4-vnni.exe` | `x86-64-v4` + `avx512vnni` | AVX-512 CPUs (Zen 4/5, recent Intel server): about 1.5 % faster |
   | `knightbyte-v2.exe` | `x86-64-v2` | the safety net: CPUs and virtual machines without AVX2 |

   A single self-contained "universal" file is not what ships: a binary compiled
   for `v3` cannot start on a CPU without AVX2, and a `v2`-compiled one costs
   1-2.6 %. The three-file layout is the ADR-0001 "design C" without a launcher,
   which ADR-0001 rejected only because of self-extracting launchers; plain
   executables have none of that cost. Design B (whole-engine multiversioning)
   is not needed.
3. A **start-up guard** (`platform::cpu::check_build_requirements`, called first
   in `main`) compares the features the binary was compiled to require with the
   CPU. A dedicated build started on an older machine prints which features are
   missing and to use the `v2` build, and exits with a failure code instead of
   dying with an illegal-instruction fault.
4. **`v3-vnni` is not built.** The only VNNI use would be an AVX-VNNI dot-product
   kernel; the SCReLU head squares each element, which VNNI cannot do without
   changing the arithmetic, so the `v3-vnni` rung runs the AVX2 kernel and a
   separate build would be a duplicate of `v3`. It returns when a kernel needs it.
5. **The C runtime is linked statically** (`-Ctarget-feature=+crt-static`, set in
   `.cargo/config.toml` and repeated by the PGO script, whose `RUSTFLAGS` replace
   the config). The first build imported `VCRUNTIME140.dll`, which a plain
   Windows 10 installation does not have unless the Visual C++ redistributable
   was installed. The executable now imports only `KERNEL32`, `ntdll`,
   `bcryptprimitives` (`ProcessPrng`) and the `api-ms-win-core-synch` forwarder
   (`WaitOnAddress`), all present on Windows 10; `tools/build_release.py` fails
   if any other DLL appears (`tools/pe_imports.py`).
6. Plain `cargo build --release` stays the portable development build (no
   `target-cpu`), so tests and CI behave the same on every host.

## Consequences

- The release script is the only supported way to produce the Windows artifacts:
  it builds, checks (`uci`, pinned `bench 6`, forced-scalar identity, import
  table), hashes and zips, and writes `SHA256SUMS.txt` and `manifest.json` with
  the commit, compiler and pins.
- The gate is measured on one host. The second Windows host (a mainstream AVX2
  CPU) should repeat the `v3` against `v2` comparison before the release; the
  `v4-vnni` build cannot run there.
- Adding a variant means a row in the table of `tools/build_release.py`, a row
  here and a gate measurement.
- The release and campaign tooling (`tools/build_release.py`, `tools/pgo_build.py`,
  `tools/sprt_arm.py`, `tools/lever_campaign.py`) is Python, not PowerShell. The
  first versions were PowerShell scripts; on a machine whose execution policy
  forbids scripts they could not even start, and Python is already required by
  the other tools in `tools/`. The Python versions are also portable to macOS and
  Linux, which the release engineering for those hosts will need.
