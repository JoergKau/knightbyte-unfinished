# KnightByte

> **Work in progress.** This repository is an unfinished engine. The search is
> functional, tested and node-identical to the reference implementation; the
> Windows, macOS and Linux release work (speed, kernels, packaging) is done, and
> what remains is search tuning (`next-steps.md`). History
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
validation are tracked in `next-steps.md`. Every advertised option does
something: ten public options and sixty research levers.

The production network is embedded in the binary from `net/jokausch.nnue`.
There is no external network file at runtime.

## Supported platforms

* macOS on Apple Silicon (M1-M5); Intel Macs are not supported
* Linux on x86-64 with AVX2 (Intel Haswell 2013 and newer, AMD Zen and newer); Linux on ARM is not supported
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
Visual C++ runtime.

The macOS release is one **Apple Silicon** binary: `aarch64-apple-darwin` built
with `-Ctarget-cpu=apple-m1`, which is the baseline of every Apple M processor
(M1-M5). NEON is part of the AArch64 baseline, so there is no per-chip variant
and no start-up feature selection. Intel Macs are **not** supported and no
x86_64 slice is produced.

The Linux release is one **x86-64** binary: `x86_64-unknown-linux-gnu` built
with `-Ctarget-cpu=x86-64-v3`, the AVX2 baseline (Intel Haswell 2013 and newer,
AMD Zen and newer), profile-guided like the other releases. ARM is not
supported and no other architecture is produced. Inside the file the SIMD
kernels are still chosen at start-up, so a CPU with AVX-512 uses the wider
kernels anyway. A CPU without AVX2, BMI2 and FMA gets a clear message naming
the missing features instead of an illegal-instruction crash; `--target-cpu
x86-64-v2` builds the portable fallback for such machines. The artifact is
built on Ubuntu 22.04, so it needs glibc 2.35 or newer (Ubuntu 22.04/24.04,
Debian 12 and newer).

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

```bat
python tools\build_release.py
```

The Rust toolchain is pinned in `rust-toolchain.toml` (stable 1.100.0); the PGO
step additionally needs `rustup component add llvm-tools-preview`.

The macOS Apple Silicon artifact is produced by one script, which builds with
`-Ctarget-cpu=apple-m1` (profile-guided by default), runs the lever effect
gate, verifies the four bench node pins and the scalar/NEON kernel identity,
checks that the file is `arm64`-only, and writes the `tar.gz` with
`SHA256SUMS.txt` and `manifest.json`. It must run on an arm64 Mac; PGO needs
the LLVM tools once per machine:

```bash
rustup target add aarch64-apple-darwin      # once per machine
rustup component add llvm-tools-preview     # once per machine (PGO)
python3 tools/build_release_macos.py        # the release build (PGO)
python3 tools/build_release_macos.py --no-pgo --variant-suffix plain   # measurement build
```

The archive and its `.sha256` are the only files written to `dist/`. The whole
build — the lever gate, the compiler's cache and the unpacked package — runs in
one scratch tree under `target/` that is deleted afterwards, so a release does
not fill `target/` and never mixes into your normal `cargo build` cache. Pass
`--keep-cache` to keep it for a fast re-run and `--keep-binary` to also drop a
ready-to-run `dist/knightbyte` next to the archive; the executable inside the
archive is `knightbyte-<version>-macos-arm64/knightbyte`. `--no-pgo` is a
measurement variant, not a second release: it skips the profile and names its
artifact differently so it cannot be mistaken for the shipped build.

PGO gives about +6-7.5 % on the M4 with the node pins unchanged; the profile
is generated on the build host and is never committed.

The Linux x86-64 artifact is produced by one script, which builds with
`-Ctarget-cpu=x86-64-v3` (AVX2 baseline; profile-guided by default), runs the
lever effect gate, verifies the four bench node pins and the scalar/AVX2 kernel
identity, checks that the file is an x86-64 ELF, and writes the `tar.gz` with
`SHA256SUMS.txt` and `manifest.json`. It must run on an x86-64 Linux host; PGO
needs the LLVM tools once per machine:

```bash
rustup component add llvm-tools-preview     # once per machine (PGO)
python3 tools/build_release_linux.py        # the release build (PGO)
python3 tools/build_release_linux.py --no-pgo --variant-suffix plain   # measurement build
python3 tools/build_release_linux.py --target-cpu x86-64-v2            # older CPUs
```

As on macOS, the archive and its `.sha256` are the only files written to
`dist/`; the whole build runs in a scratch tree under `target/` that is deleted
afterwards (`--keep-cache` keeps it for a fast re-run, `--keep-binary` drops a
ready-to-run `dist/knightbyte` next to the archive).

The artifact is built on Ubuntu 22.04 in CI (glibc 2.35 floor). Processors
without AVX2/BMI2/FMA are not supported by the shipped binary; the start-up
guard names the missing features, and the `x86-64-v2` build is the fallback.

The archive is ad-hoc signed by the toolchain but not notarized, so macOS may
ask you to allow the downloaded binary once (System Settings → Privacy &
Security, or remove the quarantine attribute).

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

The `info` lines carry `tbhits <n>` (between `hashfull` and `time`) only while a
tablebase set is in use; without tablebases they are exactly
`depth seldepth score nodes nps hashfull time pv`.

Pondering is not advertised. The network is embedded, so there is no `EvalFile`,
and the engine has no `Contempt` option.

Every other option is one of exactly **sixty research levers**, named after the
feature they control (`Nmp...`, `Rfp...`, `Lmr...`, `Qs...`). All sixty act on the
search: the defaults are the shipped configuration, changing one is a tuning
experiment, and a name that is not one of the sixty is rejected as an unknown
option. `docs/LEVERS.md` lists every lever with its default, range, origin and
effect, and `docs/decisions/ADR-0014-the-sixty-levers.md` records how the set was
chosen.

Besides the standard commands the engine understands `bench [depth]` (the
deterministic node-count benchmark), `perft <depth>`, `nnueeval` and `d`, and
two hidden developer diagnostics, both off by default:

* `searchstats [reset|off]` counts, for the searches that follow, the share of
  quiescence nodes, the transposition-table probes and hits and the index of the
  cutoff move; a bare `searchstats` prints the last counted search.
* `nnueregions [reset|off]` counts accumulator refreshes per king region of the
  network; a bare `nnueregions` prints the histogram and its shares.

## Known limitations

* The network cannot be replaced at run time (it is embedded) and there is no
  contempt setting.
* Features of the reference that measured flat or negative (LMR v2, SEE reach,
  quiescence checks, verified null move, the extension trio) are not offered as
  levers; singular extensions never fire (a documented reference quirk) and have
  no lever.
* `go infinite` ends on its own at the engine's depth cap (128) or on a proven
  mate, instead of waiting for `stop`; this only happens in positions that are
  trivially drawn or already decided.
* The Windows x64 builds, the macOS Apple Silicon build and the Linux x86-64
  build have been through the release validation (build, node pins,
  forced-backend identity, packaging).

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
* CI runs on push. Windows/macOS/Linux artifacts are attached to tags or GitHub
  Releases rather than copied between machines.
* The source and the MIT licence are already public, which is what CCRL
  requires of a submission; what remains is a tagged release with the Windows
  x64 zip and the macOS and Linux archives.

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
