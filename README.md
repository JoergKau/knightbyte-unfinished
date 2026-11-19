# KnightByte

> **Work in progress.** This repository is an unfinished engine. The search is
> functional, tested and node-identical to the reference implementation; the
> remaining work is performance, cross-platform validation and search tuning
> (`next-steps.md`). History begins with the public snapshot of 2026-11-17.

KnightByte is a UCI chess engine written in Rust. It is a headless engine: it
speaks UCI on standard input/output and has no graphical interface.

## Status

**Phases 0–6 complete.** The engine has a validated board and legal move
generator, a perft suite, a v5 network loader with a bit-exact incremental
evaluation, Zobrist keys, a transposition table, static exchange evaluation, a
staged move picker with history tables, an iterative-deepening PVS search with
quiescence, full time management, a persistent Lazy SMP worker pool, the full
UCI option and lever surface, deterministic `bench`, Syzygy WDL/DTZ probing
with root DTZ ranking, and a `.ctg` opening book (off by default).

Exact node-count parity with the reference implementation holds at every
captured depth and position (`bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 /
6,892,874, `docs/SEARCH-PARITY.md`). Performance work and cross-platform
validation are tracked in `next-steps.md`. `EvalFile` and `Contempt` remain
accepted-but-inert options.

The production network is embedded in the binary from `net/jokausch.nnue`.
There is no external network file at runtime.

## Supported platforms

* macOS on Apple Silicon
* Ubuntu x64
* Windows 11 x64

The Windows release is planned as one universal executable with startup
instruction-set dispatch. The macOS and Linux releases are single generic
builds.

## Build

```bash
cargo build --release
```

The binary is written to `target/release/knightbyte` (`.exe` on Windows).

## Run

```bash
./target/release/knightbyte
```

Then type UCI commands:

```text
uci
isready
position startpos
perft 3
nnueeval
quit
```

`position` accepts `startpos` or a six-field `fen`; `perft` and `nnueeval` are
diagnostic commands for the current position.

## Repository workflow

Development happens in one public repository,
<https://github.com/JoergKau/knightbyte-unfinished> (branch `main`); there is
no private repository or second remote. Machines clone and pull instead of
copying directories.

* One-time setup on a new machine: install Git, authenticate (`gh auth login`
  or an SSH key), then
  `git clone https://github.com/JoergKau/knightbyte-unfinished.git`.
* Pull before a work session, commit after each completed step, push to
  `origin`. Everything pushed is public immediately.
* Do not copy or commit `target/`; it contains host-specific build artifacts.
* The predecessor tree and the migration files live in a separate migration
  folder outside this repository. Everything inside the repository is published
  during the migration; the publishing policy for predecessor material is
  decided at the end of the migration.
* CI runs on push. Windows/macOS artifacts are attached to tags or GitHub
  Releases rather than copied between machines.
* The source and the MIT licence are already public, which is what CCRL
  requires of a submission; what remains is a tagged release with the Windows
  x64 zip.

## Development workflow

Primary development happens on macOS Apple Silicon. The Rust code is portable;
move it with git rather than copying a working directory, and never copy
`target/` because it contains host-specific artifacts.

Windows development uses two roles: a Zen 4 machine for the AVX-512/VNNI path,
the universal-executable prototype and same-host baseline work; and a
mainstream AVX2 machine for the `v3` path and CCRL-proxy testing. PGO profiles
are generated per host and per ISA variant.

## Test

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Every FEN used by a test must be verified by an external UCI verifier at test
time. Set `KNIGHTBYTE_FEN_VERIFIER` to the verifier binary before running the
tests:

```bash
KNIGHTBYTE_FEN_VERIFIER=/path/to/uci-verifier cargo test
```

The verifier is a local test tool and is not distributed with KnightByte.

## License

MIT. See `LICENSE` and `THIRD-PARTY-NOTICES.md`.
