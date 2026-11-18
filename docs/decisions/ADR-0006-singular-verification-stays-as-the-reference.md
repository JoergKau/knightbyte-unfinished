# ADR-0006 — Singular verification keeps the reference behaviour

**Date:** 2026-11-18
**Status:** accepted (the research lever was removed 2026-11-18, Phase 5 step 2)

## Context

ADR-0005 restored exact node identity with the reference engine and recorded
a predecessor quirk (`docs-new/new-bugs.md` Q1): the singular-extension
verification search probes the transposition table without excluding the entry
that triggered it, takes a cutoff on that very entry and therefore never
produces an extension. In `bench 14` the verification ran 2,810 times and never
extended.

Removing the quirk is a search change, not a port step, so it was tried behind
a switch before anything was decided: the KnightByte-only UCI option
`SingularExcludeTtCutoff` (default off). With it on, the verification node
skips the table cutoff and bound narrowing, so it really searches the
alternatives and extensions fire (92 of 925 verifications on the start position
at depth 14; 467 of 1,607 on Kiwipete). The tree grows a lot: Kiwipete at
depth 14 went from 195,860 to 733,346 nodes.

## Experiment

The same binary played itself, option on against option off:

* time control 8+0.08, one thread, Hash 64 MB, 12 games in parallel on a
  16-thread host;
* 1,000 random 8-ply openings, kept only when a depth-8 external evaluation was
  within 120 cp of equal, every opening played with colours swapped;
* SPRT, H0 = 0 Elo, H1 = +5 Elo, alpha = beta = 0.05, bounds LLR +/-2.94.

The run was stopped by the project owner after 904 finished games, before the
SPRT decided. The last full report, at 800 games:

| Games | Wins | Losses | Draws | Score | Elo | LLR |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 800 | 157 | 200 | 443 | 47.31 % | -18.7 +/- 15.2 | -1.79 (of -2.94) |

All 904 finished games ended normally: no time loss, illegal move, crash or
disconnection.

## Decision

1. The default stays **off**: KnightByte keeps the reference behaviour,
   including the quirk, and stays node-identical (ADR-0005). The experiment gave
   no evidence of a gain; the point estimate was negative, though the run did not
   reach a statistical decision.
2. The project is a migration, not a redesign. A search change needs a reason
   that outweighs fidelity, and this one has none so far.
3. The option exists only as a KnightByte research lever (it is not one of the
   reference's 95 option names). A regression test
   (`tests/search.rs`, `singular_verification_lever_is_off_by_default_and_...`)
   pins that the default reproduces the reference (depth 9 of the start position
   is 41,511 nodes) and that switching the lever on changes the search.
4. Whether to keep or remove the lever is a project decision; removing it does
   not touch the default behaviour.

**Resolution (2026-11-18, Phase 5 step 2):** the lever was removed. It was a
KnightByte-only name that the option-surface check would have to exempt forever,
and the experiment had shown no gain. The catalogue now holds exactly the 83
reference levers plus the three marked KnightByte-only research levers
(`src/search/lever_table.rs`); the verification search unconditionally takes its
own table cutoff (the code comment at the site records the reasoning), the
`SearchParams` field and the regression test are gone, and the node pins are
unchanged (the default was off).

## Consequences

- Nothing changes for the migration, the pins or the golden files.
- If singular extensions are revisited after release, the margins and the
  double/triple tiers were tuned for a search where they never fire, so any
  future experiment should expect to retune them and to use a longer
  time control and more games than this run.
