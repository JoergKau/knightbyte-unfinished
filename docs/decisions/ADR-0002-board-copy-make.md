# ADR-0002: Copy-make position for the correctness phase

**Status:** accepted for Phases 2–3  
**Date:** 2026-11-17

## Context

The rules layer must be proven against the reference perft suite before search
or performance work starts. The board is small: a mailbox, twelve bitboards,
castling/en-passant state, clocks and two king squares. The network
accumulator is much larger and is deliberately kept outside the board.

## Decision

Use a compact `Copy` position and **copy-make** during the correctness phase:

* `Position` is `Copy`, with a compile-time size assertion (`<= 256` bytes).
* A move returns the next position by value; there is no mutable undo stack.
* Legal moves are generated pseudo-legally and filtered by making the move and
  checking the mover's king. Castling transit squares are checked during
  generation.
* The network evaluator owns its accumulator buffers; it does not live inside
  `Position`.

## Consequences

* The rules code is small and auditable, and perft isolates move-generation
  errors without make/unmake state-restoration bugs.
* Search will pay a position copy per node. That is acceptable for the parity
  phase; Phase 7 benchmarks copy-make against make/unmake and switches only if
  the measurement justifies it.
* Incremental accumulator updates need an explicit make/unmake or dirty-piece
  stream when the search lands. The current full-refresh evaluator is the
  bit-exact oracle that the incremental path must match.
* Zobrist keys, SEE and draw detection are orthogonal and can be added without
  changing the position ownership model.
