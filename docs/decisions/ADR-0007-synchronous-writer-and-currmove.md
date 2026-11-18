# ADR-0007 — The synchronous writer stays; `info currmove` is emitted

**Date:** 2026-11-18
**Status:** accepted

## Context

The Phase 5 scope checklist carried over an idea from the early skeleton: a
dedicated writer thread with a bounded channel, so a slow GUI can never
stall the search. The output surface changed since then: a search prints one
`info` line per completed iteration (step 3) plus the reference's single
`info currmove` line of iteration 1 (step 10) and one `bestmove`.

The step-9 stress walk measured the output side: a search emits a handful of
lines in total (one per iteration), each a short locked `writeln` plus a
flush — microseconds per line, far below the 1024-node poll interval. Even a
GUI that reads slowly only blocks the worker thread, never the search
state: the worker owns its own `Output` writer, and the search drains via
the stop flag when the pipe breaks (step 4).

The step-0 transcripts show the reference emits exactly one
`info currmove <move> currmovenumber 1 nodes 1` line per search, before the
depth-1 line, and none when the search never reaches iteration 1's move
loop. Step 10's rule says the line is added only because the transcripts
show it — they do.

## Decision

1. **The synchronous writer stays.** No writer thread, no channel, no
   coalescing. The measured output volume (a few short lines per search)
   cannot block the search meaningfully, and the writer thread would add a
   queue, a thread and shutdown ordering to every platform for no measured
   gain.
2. **`info currmove` is emitted** exactly as the reference does: one line
   per search, when iteration 1's first root move begins, as the picker
   yields it (`info currmove a2a3 currmovenumber 1 nodes 1` on the start
   position). It is a report, not instrumentation — it adds no nodes and
   the bench pins are unchanged.

## Consequences

- If a later phase adds high-frequency output (`info currmove` per move at
  depth-per-node reporting, hashfull per node), this decision must be
  revisited with a measurement; the bounded-channel writer remains recorded
  in the checklist as the fallback design.
- The `currmove` value depends on the picker's state (a warm table changes
  it — the step-0 transcript's 4-thread run shows `d2d4` there). Transcript
  tests compare the line's structure, not its move value.
