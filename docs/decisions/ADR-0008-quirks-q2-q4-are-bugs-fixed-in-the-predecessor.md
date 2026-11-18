# ADR-0008 — Quirks Q2-Q4 are bugs: fixed in the predecessor, not reproduced

**Date:** 2026-11-18
**Status:** accepted

## Context

The migration rule separates two registers: a *quirk* is reproduced on purpose
and the migration goes on; a *bug* stops the migration until the predecessor is
fixed. The quirk register held Q2, Q3 and Q4 (besides Q1, decided in ADR-0006):

* **Q2** — the transposition-table move packing keeps only from/to/promotion,
  so a stored castling or en-passant best move compares unequal to every
  generated move: never tried first, never excluded in singular verification.
* **Q3** — quiescence classifies an en-passant capture as quiet (its target
  square is empty) and never searches it.
* **Q4** — the correction-history quiet gate tests the mover's own occupancy at
  the destination, which is never set for a legal move, so ordinary captures
  count as quiet and pollute the correction tables.

All three are deterministic, produce no wrong result, no illegal move and no
crash — the usual profile of the quirk register. The owner asked whether they
are really only quirks.

## Evidence

A source review found each one contradicts the predecessor's own intent:

* **Q2** is known and worked around inside the predecessor
  (`Search.IterativeDeepening.IsMoveLegal`: "TT packing loses
  IsEnPassant/IsCastle flags, so we match by coordinates only") — patched
  around in one place instead of fixed, which is not a design.
* **Q3** makes the engine contradict itself: the captures-only move generator
  explicitly generates en passant, and the move picker's capture test honours
  `IsEnPassant` — but the quiescence loop's own quiet test forgets the flag.
* **Q4**'s inline comment claims to mirror the reference's
  `board.isQuiet(bestMove)` at the same point, and the code does not do that —
  the wrong occupancy side is a porting error the comment papers over.

The predecessor's own history fixed this exact defect class as bugs: Bug 9
(read-but-never-written history tables) and Bug 10 (counter ordering) were
silent strength defects with no wrong result, and were fixed, not reproduced.

## Decision

1. **Q2, Q3, Q4 are reclassified as predecessor bugs 147, 148 and 149** and
   fixed in the predecessor first (commit `f274b39`: the TT probe re-stamps the
   flags from board context, quiescence honours `IsEnPassant`, the quiet gate
   tests the opponent occupancy). Regression tests live in
   `PredecessorBugs147to149Tests`; the full predecessor suite is 734/734.
2. **KnightByte stops reproducing them** when the migration restarts. The fixed
   predecessor is the specification from Phase 1 onward; the Q2/Q3/Q4
   reproduction is removed together with the re-pin.
3. **The Phase 4 node-identity pins are stale by decision, not by drift.** They
   describe the quirk build (14,979 / 416,533 / 3,169,859 / 6,925,762 for
   `bench 6/12/16/18`). The fixed reference measures 15,528 / 456,308 /
   2,815,799 / 6,892,874 (deterministic over two passes). `tests/search.rs` and
   `tests/golden/bench.json` stay pinned to the old build until KnightByte is
   re-pointed at the fixed reference, so the current binary's suite stays green;
   the re-capture is the first act of the Phase 1 restart.
4. **Q1 stays a quirk.** ADR-0006 already measured the fixed behaviour and found
   no gain (negative point estimate); nothing new emerged from this review.

## Consequences

* One extra oracle re-capture round is avoided: the predecessor was already on
  hold for bugs 143-145, so fixing 147-149 in the same window costs no
  additional stop.
* The parity discipline is sharpened: behaviour that looks wrong is recorded as
  a quirk first (the rule stands), but a review that finds code evidence against
  intentionality — a workaround comment, a self-contradiction, a comment that
  misdescribes its own line — reclassifies it as a bug.
* The Phase 1 restart must re-capture the oracle (bench totals, per-position
  splits, evaluation vectors are unaffected by 147-149 except through search
  behaviour) before any Phase 5 step runs.
