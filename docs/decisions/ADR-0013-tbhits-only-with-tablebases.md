# ADR-0013 - `tbhits` appears in the `info` line only while tablebases are in use

**Date:** 2026-11-19
**Status:** accepted (the project owner took the engineering recommendation)
**Closes:** audit finding 5 after Phase 6 ("the plan wants `tbhits`, the predecessor never prints it")

## Context

The migration plan lists `tbhits` among the fields of the `info` line. The
predecessor's source contains no `tbhits` at all, so its `info` lines are
`depth seldepth score nodes nps hashfull time pv`, and the step-0 transcripts that
the replay tests compare against are in that form. GUIs and analysis tools do show
a tablebase-hit count when an engine reports one.

## Decision

1. `info` lines carry `tbhits <n>` between `hashfull` and `time` **only while a
   tablebase set is loaded and probing is enabled**. Without tablebases the line is
   field for field the predecessor's, so every transcript and parity check is
   unchanged.
2. A hit is a tablebase WDL value that the search used for a node (main search or
   quiescence) plus the successful root probe of the DTZ ranking. A DTZ probe of a
   position that was just counted by its WDL probe is not counted again.
3. Each searcher counts its own hits in a plain field (no atomic in the node loop;
   the increment sits inside the tablebase branch, which only runs when tables are
   loaded). Helper threads flush their counts in blocks next to their node blocks
   into one shared total, and the main searcher reports its own exact count plus
   that total, exactly as it does for nodes. The UCI layer clears the total before
   it launches a search.
4. The value never decreases within a search and is zero until the first hit.

## Consequences

- The two engines differ in their `info` lines only when Syzygy is on. That is
  documented here; the predecessor is not changed (it is the frozen reference, and
  this is an added field, not a fix).
- Speed without tablebases is unchanged (balanced A/B +0.46 %, noise); bench pins
  exact.
- Tests: `tbhits_joins_the_info_line_only_while_tablebases_are_in_use` (absent
  without a path; present on every line with one, non-decreasing, positioned
  between `hashfull` and `time`), `helper_threads_add_their_tablebase_hits`
  (three threads) and a unit test of the line format.
