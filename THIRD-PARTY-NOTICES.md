# Third-Party Notices

## Runtime

KnightByte has one third-party runtime dependency, `memmap2` (see "Runtime
dependencies" below). Apart from it, the release binary is built from the Rust
standard library and the project's own source code. The Windows executables link
the C runtime statically; no Visual C++ redistributable is needed.

## Embedded network

The neural-network file `net/jokausch.nnue` is part of this project and is
distributed with the engine.

## Test-only dependencies

The test suite uses the following crate; it is not linked into release builds.

| Crate | Version | License | Purpose |
| :--- | :--- | :--- | :--- |
| `sha2` | 0.10 | Apache-2.0 OR MIT | Verify the embedded network checksum in tests |

## Runtime dependencies

| Crate | Version | License | Purpose |
| :--- | :--- | :--- | :--- |
| `memmap2` | 0.9 | MIT OR Apache-2.0 | Read-only memory mapping of Syzygy tablebase files |
| `libc` | 0.2 | MIT OR Apache-2.0 | Used by `memmap2` on macOS and Linux only (not part of the Windows executables) |

The mapping is opened only when a `SyzygyPath` is configured; a build used
without tablebases never calls it.

## Test data

`perf-test/wacnew.epd` contains the public WAC tactical test positions used by
the WAC screen. The file is test data, not part of the release binary.

`tests/data/tb/` contains the three 3-man Syzygy WDL/DTZ table pairs (KQvK,
KRvK, KPvK). They are part of the publicly distributed Syzygy endgame
tablebases (public domain) and are used as test data only; they are not
embedded in the release binary and are not needed at runtime.

Opening-book tests build a synthetic `.ctg` fixture in memory. No third-party
book file is distributed with the engine.

## External test tools

The project's test and benchmark scripts may invoke an external UCI engine for
FEN verification. That tool is not distributed with KnightByte and is not a
runtime dependency.

`tools/tb_compare.py` imports the `chess` Python package (python-chess,
GPL-3.0) as an independent tablebase reader to cross-check probe results. It is
a developer-only script, is not part of the release package and is not linked
into the engine.
