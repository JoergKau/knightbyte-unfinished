# KnightByte

> **Work in progress.** This repository is an unfinished engine. The search is
> functional and tested, but it does not yet reproduce the reference node counts
> exactly (`bench 12` is within ~1 %); the remaining parity work, threading,
> tablebases and the opening book are tracked in `next-steps.md`.
> History begins with the public snapshot of 2026-11-17.

KnightByte is a UCI chess engine written in Rust. It is a headless engine: it
speaks UCI on standard input/output and has no graphical interface.

## Status

**Phases 0–3 complete. Phase 4 complete except the exact search-parity gate:**
search foundations, move ordering, alpha-beta search, quiescence, time
management and a UCI option catalogue are in place. The engine has a validated
board and legal move generator, a perft suite, a v5 network loader, the
scheme-2 feature mapping, a bit-exact scalar evaluation, Zobrist keys, a
transposition table, static exchange evaluation, a staged move picker with
history tables, and an iterative-deepening PVS search with quiescence, reachable
through `go depth`/`go nodes`/`go movetime`/clock limiters, with 12 public
options and 29 research levers, and the deterministic `bench` command.

Exact node-count parity with the reference implementation is still open: the
oracle is captured per position and depth, the gate is `bench 12` within 1.1 %
today, and the remaining work is one coupled port of the reference search
features (`docs/SEARCH-PARITY.md`, `next-steps.md` §3.1). Threading,
tablebases and the opening book are not implemented; the public book, Syzygy,
`EvalFile` and `Contempt` options are accepted and inert.

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

Development is centralized in a private git repository; machines clone and
pull instead of copying directories.

* One-time setup on a new machine: install Git, authenticate (`gh auth login`
  or an SSH key), then `git clone git@github.com:<owner>/knightbyte.git`.
* Pull before a work session, commit after each completed step, push to the
  private remote.
* Do not copy or commit `target/`; it contains host-specific build artifacts.
* The internal migration sources and reference trees are git-ignored. They
  never travel with the engine repository.
* CI runs on push. Windows/macOS artifacts are attached to tags or GitHub
  Releases rather than copied between machines.
* The repository can stay private during development. Before a CCRL
  submission, publish a source archive and license publicly, as CCRL requires
  publicly available source.

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
