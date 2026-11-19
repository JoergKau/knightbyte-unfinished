# ADR-0012 - Search diagnostics are a second instantiation of the search

**Date:** 2026-11-19
**Status:** accepted

## Context

The reference engine has two hidden diagnostic commands that KnightByte had only
as placeholders: `searchstats` (how much of the tree is quiescence, how often the
transposition table is probed and hit, how early the cutoff move is found) and
`nnueregions` (which king region each accumulator refresh falls in). They are
developer tools, but they have to work in the binary that is shipped, because
that is the binary the measurements are about.

The first implementation followed the reference: a runtime switch, read in the
node loop. It cost speed. Five guarded counter sites (the node entry of
quiescence, both probes, both cutoffs) measured **-2.7 %** at `bench 16` in a
balanced A/B run (the `tools/ab_bench.py` noise floor is about 0.8 %), after a
whole session spent removing such costs.

## Decision

1. `negamax`, `quiescence` and `search_root` take a `const STATS: bool`. The
   counter sites are `if STATS { ... }`, which the compiler removes from the
   `false` instantiation, so the code that plays is the code that existed
   before the diagnostics. The `true` instantiation exists only for counting.
2. `Searcher::search` copies the process-wide switch once per search
   (`collect_stats`) and `search_root` picks the instantiation. The node loop
   never reads the switch.
3. Each searcher (the main thread and every Lazy SMP helper) keeps plain
   per-thread counters and merges them into one process-wide snapshot when its
   search ends. The UCI layer clears the snapshot before it launches a counted
   search, and the `searchstats` command waits for the helpers to go idle before
   it reads the snapshot.
4. `nnueregions` counters are atomics read on the refresh path only: a refresh
   already costs hundreds of nanoseconds, and they are about 8 % of the
   accumulator updates (156 K against 1.89 M in `bench 16`), so the one relaxed
   load is invisible.
5. The commands, their sub-commands (`reset`, `off`), their messages and the
   report formats are the reference's. The reference's `shares` line formats
   numbers with the machine's decimal separator (`0,547%` on a German system);
   KnightByte always prints a point.

## Verification

- `searchstats` after `go depth 12` on four positions (the start position,
  Kiwipete, a rook endgame and a promotion position) is **byte-identical** to the
  predecessor's: nodes, quiescence nodes, probes, hits and the cutoff-index
  percentages. With `Threads 4` the node count includes the helpers.
- Speed: the generic build against the build before the diagnostics, balanced
  ABBA: -0.07 % at `bench 18` (16 runs each), -1.2 % at `bench 16` (the A/B
  spread of that run was 2.9 %); a build with the sites disabled by hand reads
  -0.7 % against the same baseline, so the residue is code layout, not the
  counters. Pins exact.
- The tests (`tests/uci_smoke.rs`) cover the idle report, a counted search whose
  node total equals the search's own, `off` freezing the snapshot, and the
  histogram with `reset`, `off` and the shares line.

## What `nnueregions` counts

It counts KnightByte's accumulator refreshes: the root, every move of the own
king (the accumulator is rebuilt from scratch when the king changes region or
mirror) and every full evaluation outside the incremental path. The reference
refreshes lazily, at evaluation time and repeatedly, so its absolute counts and
its shares differ from KnightByte's on the same search (for example 11,292
against 5,311 refreshes on the Kiwipete position at depth 12). That is a
property of the two engines' accumulator schemes, not of the instrumentation.
Compare shares between runs of one engine, not between the engines.

## Consequences

- The release binary carries a second copy of the search that is only executed
  after `searchstats reset`.
- A new counter site is added in the search by wrapping it in `if STATS`; adding
  a runtime-checked counter to the node loop is not acceptable.
- The same pattern is available for any future per-node diagnostic.
