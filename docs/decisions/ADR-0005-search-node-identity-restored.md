# ADR-0005 — Search node identity restored; ADR-0004 superseded

**Date:** 2026-11-17
**Status:** accepted
**Supersedes:** ADR-0004

## Context

ADR-0004 closed the search-parity gate as a documented deviation. It recorded
`bench 6` at 14,963 (reference 14,979) and `bench 18` at 6,797,622 (reference
6,925,762), and it attributed the residual to "learned-state values (the
correction and history tables' update magnitudes)" with "no logic error". It
also told future sessions not to reopen the forensics.

That attribution was wrong. A second audit round, driven by a per-node ledger
of both engines (the existing `KNIGHTBYTE_TRACE` ledger plus an instrumented
local build of the reference, diffed with `tools/trace_diff.py`), found that the
remaining gap was made of eleven ordinary porting differences (B18-B28 in
`docs-new/new-bugs.md`) and one predecessor quirk that had to be reproduced
(Q2, the table dropping castle and en-passant markers). Each showed up as a
concrete first-divergent node. None of them was learned-state drift: the
learned tables were different *because* an earlier decision (an ordering, a
reduction, a gate) was different.

The method that worked, in order:

1. compare per-position node counts per depth against the captured oracle to
   find the shallowest depth and position that differs;
2. dump both ledgers for that position and take the first divergent node;
3. print the decision inputs at that node on both sides (move-ordering scores,
   reduction terms, history cells, table hits) until one input differs;
4. find the reference code that decides that input, port it, repeat.

Each round moved the first divergence deeper (depth 5, then 7, 9, 13, 14, 16,
18) until there was none.

## Decision

1. The node-identity gate (G3) is **reinstated and passed**. At every captured
   depth (1-12, 14, 16, 18) and for every bench position the node count is
   identical to the reference. Totals: `bench 6` = 14,979, `bench 12` = 416,533,
   `bench 16` = 3,169,859, `bench 18` = 6,925,762.
2. The regression pin is no longer a KnightByte-only number: it is the
   reference total. `tests/search.rs` asserts `bench 6` = 14,979 and the
   reference per-position counts for depths 1-8; `tests/golden/bench.json`
   records the four totals.
3. The ADR-0004 rule "do not reopen node-count forensics" is withdrawn. The
   replacement rule: any search change must keep node identity, or it is a
   deliberate, documented search change that updates the pins in the same
   commit and records why (the original ADR-0004 note on pins stands).
4. Predecessor quirks that look wrong stay reproduced (new-bugs.md Q1-Q4).
   The most consequential one: singular extensions never fire, because the
   verification search takes a table cutoff on the entry that triggered it.
   Fixing that is a strength experiment, not a port step.

## Consequences

- Phase 4 is complete with the original gate, not with a waiver.
- Phase 7 (performance) can use "identical nodes" as the optimization check
  again: any optimization that changes a node count is a search change.
- The deviation table in `docs/SEARCH-PARITY.md` is history; the status is now
  "exact".
- The reference's own behaviour is the specification, including the quirks.
  The engines can only diverge again through new code or a changed option
  default; the per-position test makes that visible within the first eight
  depths.
