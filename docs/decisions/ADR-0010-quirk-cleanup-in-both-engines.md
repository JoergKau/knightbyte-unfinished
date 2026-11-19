# ADR-0010 — The predecessor's quirks Q5-Q9 are fixed in both engines

**Date:** 2026-11-19
**Status:** accepted (decided by the project owner, who delegated the individual
design choices to the engineering side)

## Context

The migration reproduced the predecessor's behaviour on purpose, and recorded
the places where that behaviour looked wrong in a quirk register. Q2-Q4 were
reclassified as bugs and fixed in both engines at the Phase 1 restart
(ADR-0008). Q1 (singular extensions never fire) stays reproduced (ADR-0006).
Five entries were left:

| | Behaviour | Why it is wrong |
| :--- | :--- | :--- |
| Q5 | `stop` during a ponder search answered `bestmove 0000` | UCI requires a real best move after `stop`; some GUIs read `0000` as an illegal move. |
| Q6 | A four-line banner went to stdout before the first command | Stray output ahead of the handshake; CCRL asks for none. |
| Q7 | The first `isready` ran a throwaway search into the shared transposition table | State leak: the first real search depended on the speed of a warm-up. |
| Q8 | A position with one legal move was answered at once for every kind of `go` | `go infinite` must wait for `stop`; fixed-depth analysis lost its iteration lines. |
| Q9 | A corrupt DTZ file switched off the healthy WDL table of the same material | Silent loss of tablebase play. Reclassified as predecessor Bug 151 / KnightByte B49. |

Q5-Q8 are deliberate design choices in the predecessor (its comments say so), so
by the plan's own rule they are not bugs: they are behaviour changes. The owner
decided that all of them are made, identically, in both engines, so the two stay
equal and the migration's parity gates keep meaning something.

## Decision

1. **Q5.** A stopped ponder search answers the best move of the last completed
   iteration (the first legal move when none completed); the null move remains
   only the answer for a position with no legal move. A `ponderhit` is unchanged.
2. **Q6.** The banner is written to **stderr**, and only when stdin is an
   interactive console. A GUI or match runner never sees it on either stream.
   KnightByte never printed it, so it already conforms.
3. **Q7.** The warm-up runs the same code on a **private 1 MB table**: the JIT
   benefit is unchanged and no shared state is touched. KnightByte has no
   warm-up, so it already conforms.
4. **Q8.** The single-legal-move shortcut remains for **time-managed** searches
   (a clock or `go movetime`), where it saves the budget, and keeps its
   `info depth 0 ...` line there. `go infinite`, `go depth`, `go nodes`,
   `go mate` and pondering search normally. (An infinite search still ends on
   its own at the engine's depth cap or on a proven mate; that is separate,
   pre-existing behaviour, not part of this decision.)
5. **Q9 / Bug 151 / B49.** One error flag per table kind (WDL, DTZ).
6. **B50 (KnightByte only).** The regression test for B49 exposed a second
   defect in the Rust port: a DTZ probe that failed to read its table returned a
   made-up value (DTZ 1 or 101) as a success, so a WDL-only table set made the
   root ranking and the direct root probe choose a bogus "optimal" move. The
   predecessor lets the zero success flag flow out unchanged; the port now does
   too.

## Verification

- Predecessor: 755 of 755 tests (745 plus ten new regression tests). Each new
  test was shown to fail against the reverted fix before the fix was restored.
  The published binary was rebuilt; `bench 6` is unchanged at 15,528.
- KnightByte: 318 tests, `fmt` and `clippy -D warnings` clean, the four bench
  pins unchanged (15,528 / 456,308 / 2,815,799 / 6,892,874). The same
  revert-and-fail discipline was applied to Q5, Q8, Q9 and B50.
- The step-0 UCI transcripts were re-captured from the fixed predecessor and the
  replay suite (12 scenarios) passes against them. Only the ponder scenario's
  `bestmove` and the removed banner line changed shape.
- Differential runs on the rebuilt engines: 800 random 3-4-5 and 6-man
  tablebase positions and 193 book positions agree with the predecessor; 120
  random positions with a WDL-only table set give identical score and best move;
  the corrupt-DTZ position scores `cp 29900` in both engines (it was `cp 783`).

## Consequences

- The quirk register now holds only Q1. The oracle files in
  `tests/golden/transcripts/` describe the fixed predecessor.
- Search-level node counts are unaffected (the bench path touches none of the
  five); the pins did not move.
- Any future "reproduce the predecessor exactly" claim must name the build:
  the predecessor changed on 2026-11-19 (Q5-Q9) and on 2026-11-18 (bugs 143-150).
