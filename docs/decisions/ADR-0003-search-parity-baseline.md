# ADR-0003: Search parity baseline and the oracle

**Status:** accepted, supersedes the "reference unavailable" assumption  
**Date:** 2026-11-17

## Context

The Phase 4 exit gate is exact: `bench 12/16/18` node totals must equal the
totals recorded from the predecessor implementation (`416,533`, `3,169,859`,
`6,925,762`; Hash 256 MB, one thread, four fixed positions). The gate proves the
search is a faithful port, not merely a working engine.

Phase 4c was first implemented without the reference at hand, so its formulas are
provisional and its node counts do not match. The reference source and a
published build are in fact available on the development host, outside this
repository. Its `bench` reproduces the recorded totals exactly, and a
per-position, per-depth capture now exists in `tests/golden/search_parity.json`
(reference commit `79b255a`).

## Decision

1. Keep the Phase 4c implementation, but treat every formula in it as
   **provisional** until ported.
2. Use the reference build as the oracle for all node-count work, through
   `tools/capture_bench.py`, and bisect per position and per depth
   (`docs/SEARCH-PARITY.md`).
3. Work in the documented order: quiescence gating, SEE values, correction
   history, IIR/TT rules, pruning formulas, LMR terms, extensions, aspiration,
   then constants. One feature per change; the depth-1 row is checked first.
4. Keep the gate closed, and do not tune or claim strength, until
   `bench 6/12/16/18` match exactly.
5. Record KnightByte's current totals as a **pre-parity determinism baseline**
   only. They remain useful because they are asserted in `tests/search.rs` and
   must be identical on every host and build mode.

## Consequences

* Cross-platform determinism is already enforced: integer-only arithmetic, a
  fixed Zobrist seed, and a CI assertion on the `bench 6` total. The assertion
  has passed on Ubuntu, macOS and Windows.
* The port now has a mechanical procedure: capture, compare, bisect, port one
  feature, repeat. No step depends on intuition about the reference.
* **Update 2026-11-17:** the "one feature at a time" procedure was applied four
  times (LMR/IIR twice, correction/history prerequisites, TT replacement) and
  each isolated port was measured and reverted; see `docs/SEARCH-PARITY.md`
  §3–§4. The remaining features are coupled, so the procedure is revised: port
  them as one slice behind a switch, measure the slice as a whole, then bisect
  inside it.
* Until parity lands, KnightByte searches fewer nodes than the reference at
  depths 6–12 and more at depth 18. Current deltas: −2.0 %, −8.1 %, −1.1 % and
  +22.8 %.
* The reference checkout must never be referenced by path in this repository.
  Commit hashes, feature names and measured numbers are fine.
