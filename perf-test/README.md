# perf-test — UCI benchmark harnesses

Small, engine-agnostic UCI tools used as **cheap gates** before a self-play match.
They detect breakage and rank changes; they do not prove strength. Self-play
(`fastchess`/`cutechess-cli`) remains the strength arbiter.

## Tools

| File | Purpose |
| :--- | :--- |
| `perf_test.py` | NPS, nodes, EBF, selective margin, late-iteration stability, optional tactical mini-suite, thread scaling |
| `ebf_compare.py` | Head-to-head depth/seldepth/nodes/EBF for two engines |
| `wac_test.py` | WAC tactical screen |
| `perf_test.py --depth N` | Machine-independent node/EBF baseline (`go depth N`) |
| `perf_test.py --movetime N` | Time-to-depth check on one host (`go movetime N`) |

## Standard commands

KnightByte (native run):

```bash
python3 perf-test/perf_test.py --engine target/release/knightbyte --depth 16 --threads 1
python3 perf-test/perf_test.py --engine target/release/knightbyte --threads 1,4,8 --movetime 5000
python3 perf-test/perf_test.py --engine target/release/knightbyte --tactics
```

C# baseline (internal checkout, not shipped with KnightByte):

```bash
python3 perf-test/perf_test.py \
  --engine "/path/to/csharp-baseline/engine" \
  --depth 16 --threads 1
```

Head-to-head:

```bash
python3 perf-test/ebf_compare.py \
  --engine-a target/release/knightbyte \
  --engine-b "/path/to/baseline/engine" \
  --name-a KnightByte --name-b baseline \
  --depth 18 --threads 1 --json-out /tmp/ebf-knightbyte.json
```

The default engine is `$KNIGHTBYTE_ENGINE`, then `$KNIGHTBYTE_BASELINE_ENGINE`, then
`target/release/knightbyte`. Always pass `--engine` explicitly for a recorded
baseline.

## Fixed depth vs movetime

Use **`--depth`** for any number that must reproduce on another machine. Node
counts and EBF are then hardware-independent; NPS and depth-at-fixed-time are
not. Use **`--movetime`** only for time-to-depth comparisons on the same host.

## Baselines

There are two C# baselines: **Apple M4** and **Windows Zen 4**. The consolidated
table is `docs/baselines.md`; the original C# run records are internal and are
not part of the KnightByte release.

Key one-thread targets from the latest C# build (four positions, 5 s, Hash 256):

| Host | Avg depth | Avg NPS | EBF | WAC |
| :--- | ---: | ---: | ---: | ---: |
| Apple M4 | 22.2 | 1,656,154 | 2.050 | 284/300 |
| Windows Zen 4 | 22.0 | 1,569,444 | 2.061 | 284/300 |

Compare KnightByte against the numbers for the **same host**: M4 against M4,
Zen 4 against Zen 4. Do not compare a macOS run to a Windows baseline and call
the difference a migration gain.

Record the engine commit, binary hash, `net/jokausch.nnue` SHA-256, host/CPU/OS,
Hash, Threads, Syzygy/book state, depth or movetime, position set and command.

## Positions file

Both tools accept `--positions-file PATH`:

```text
# one bare FEN or "name | FEN" per line
startpos | rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
kiwipete | r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1
```

This lets the four-position gate and the 20-position time-to-depth suite use the
same source.

## JSON output

Both tools accept `--json-out PATH` for CI and baseline storage. The payload
includes host metadata, engine command, settings, per-position rows, and (for
`perf_test.py`) the tactical gate result. `perf_test.py` also records the
SHA-256 of `net/jokausch.nnue` when the standard repository layout is present.

## Status and next upgrades

The planning pass already added `--depth` to both tools, `--positions-file`,
`--json-out` with host/engine metadata, the `KNIGHTBYTE_ENGINE` override and
this README. Remaining improvements (tracked in the migration plan; needed
before the performance phase):

* one shared UCI session helper instead of duplicated process handling;
* a `--baseline` diff mode (exact node identity, depth/EBF delta, NPS ratio,
  PASS/FAIL);
* robust timeouts/process cleanup so a silent engine cannot hang CI;
* a fake-UCI-engine fixture to test the harness itself;
* JSON schema/versioning and a stable baseline directory.
