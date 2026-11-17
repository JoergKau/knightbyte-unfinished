# Third-Party Notices

## Runtime

KnightByte has no third-party runtime dependencies. The release binary is built
from the Rust standard library and the project's own source code.

## Embedded network

The neural-network file `net/jokausch.nnue` is part of this project and is
distributed with the engine.

## Test-only dependencies

The test suite uses the following crate; it is not linked into release builds.

| Crate | Version | License | Purpose |
| :--- | :--- | :--- | :--- |
| `sha2` | 0.10 | Apache-2.0 OR MIT | Verify the embedded network checksum in tests |

## Test data

`perf-test/wacnew.epd` contains the public WAC tactical test positions used by
the WAC screen. The file is test data, not part of the release binary.

## External test tools

The project's test and benchmark scripts may invoke an external UCI engine for
FEN verification. That tool is not distributed with KnightByte and is not a
runtime dependency.
