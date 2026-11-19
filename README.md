# KnightByte

> **Work in progress.** This repository is an unfinished engine. The search is
> functional, tested and node-identical to the reference implementation; the
> Windows work (speed, release builds, match gates) is done, and what remains
> is macOS and Linux validation and search tuning (`next-steps.md`). History
> begins with the public snapshot of 2026-11-17.

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
* Windows 10 and 11 x64

The Windows release contains three executables, each profile-guided for its
own instruction-set baseline (`docs/decisions/ADR-0011-windows-build-variants.md`):

| File | For |
| :--- | :--- |
| `knightbyte-v3.exe` | **the default**: any CPU with AVX2 (Intel Haswell 2013 and newer, AMD Zen 1 and newer) |
| `knightbyte-v4-vnni.exe` | CPUs with AVX-512 and VNNI (AMD Zen 4/5, recent Intel server): about 1.5 % faster |
| `knightbyte-v2.exe` | processors and virtual machines without AVX2 |

Inside each file the SIMD kernels and the sliding-attack method are still
chosen once at start-up. A build started on a CPU that lacks the instructions
it was compiled for prints which features are missing and exits; it does not
crash. The executables are plain files (no installer, no packer) and need no
Visual C++ runtime. The macOS and Linux releases are single generic builds.

## Build

```bash
cargo build --release
```

The binary is written to `target/release/knightbyte` (`.exe` on Windows). This
is the portable development build.

The Windows release artifacts are produced by one script, which builds every
variant with its own PGO profile (it must run on a machine that can execute the
most capable variant), checks each file (`uci`, the pinned `bench 6` node count,
a forced-scalar run with the same count, the import table), and writes the zip
with `SHA256SUMS.txt` and `manifest.json`:

```powershell
powershell -File tools\build_release.ps1
```

The Rust toolchain is pinned in `rust-toolchain.toml` (stable 1.100.0); the PGO
step additionally needs `rustup component add llvm-tools-preview`.

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

## UCI options

The engine answers `uci` with its option list. The public options are:

| Option | Default | Meaning |
| :--- | :--- | :--- |
| `Hash` | 256 | transposition table size in MB (1-1024) |
| `Threads` | 1 | search threads (Lazy SMP, 1-256) |
| `Move Overhead` | 50 | milliseconds reserved per move for communication lag |
| `Clear Hash` | button | empties the transposition table and the search history |
| `Own Book` | false | use the opening book |
| `Book File` | empty | path of a `.ctg` book; empty looks next to the executable and in `books/` |
| `Book Depth` | 16 | last ply at which the book is consulted |
| `SyzygyPath` | empty | directories of Syzygy tablebase files (`;` separates them on Windows, `:` elsewhere) |
| `SyzygyRootProbe` | false | rank root moves with DTZ when the position is in the tables |
| `Syzygy50MoveRule` | true | honour the 50-move rule in tablebase values |
| `EvalFile` | empty | accepted but ignored: the network is embedded |
| `Contempt` | 0 | accepted but ignored |

Pondering is not advertised. Every other option is a search-tuning lever named
after the feature it controls (`Nmp...`, `Rfp...`, `Lmr...`); the defaults are
the shipped configuration and changing them is for tuning experiments. A lever
whose feature has not been ported is accepted and has no effect.

Besides the standard commands the engine understands `bench [depth]` (the
deterministic node-count benchmark), `perft <depth>`, `nnueeval` and `d`.

## Known limitations

* `EvalFile` and `Contempt` do nothing; the network cannot be replaced at run
  time.
* Some research levers are accepted but not wired (the features they would
  switch on are not part of the shipped search).
* `info` lines do not carry `tbhits`.
* `go infinite` ends on its own at the engine's depth cap (128) or on a proven
  mate, instead of waiting for `stop`; this only happens in positions that are
  trivially drawn or already decided.
* Only the Windows x64 builds have been validated for the release; macOS and
  Linux builds exist and pass the tests but have not been through the release
  validation yet.

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
