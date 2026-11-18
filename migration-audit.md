# Migration audit: Phases 1 to 4

**Date:** 2026-11-18
**Scope:** Phase 1 (oracle capture), Phase 2 (board, move generation, perft), Phase 3 (NNUE), Phase 4 (search and levers).
**Reference:** the predecessor's publication folder, at commit `bffe98a` of its local repository (that commit includes the three position fixes). Its binary is `publish\win-x64\PawnForge.UCI.exe`, rebuilt for this audit.
**KnightByte:** `target\release\knightbyte.exe`, built 18 Nov 2026 00:14. No source change since.

**Rule for this document:** it only records facts. Every item says how it was checked:

- **Run:** the same input was given to both binaries and the outputs compared.
- **Source:** the behaviour was read from the source of both sides, not run.
- **Not checked:** listed so the gap is visible.

No item in this document has been changed in the code.

---

## Summary

| Area | Verified identical | Deviation or missing feature |
| --- | --- | --- |
| Phase 1 golden files | bench 12/16/18, perft (5 positions), NNUE (15 FENs), lever defaults and ranges (83) | coverage only: 15 NNUE positions, golden UCI transcript is just the handshake |
| Phase 2 board and perft | perft counts on the 5 golden positions | none found in this audit |
| Phase 3 NNUE | 15 of 15 values, both engines | `EvalFile` inert; accumulator and SIMD not yet done (performance items) |
| Phase 4 search | bench 12/16/18 node totals | 47 of 83 levers missing; 4 KnightByte-only names; features off by default in the reference not ported; several UCI and default differences (below) |

---

## Phase 1: oracle capture

| # | Item | Result | How checked |
| --- | --- | --- | --- |
| 1.1 | `bench 12`, `bench 16`, `bench 18` totals | Identical: 416,533 / 3,169,859 / 6,925,762 | Run, both binaries |
| 1.2 | Perft on the 5 golden positions | Identical, all 5 | Run, both binaries |
| 1.3 | NNUE on the 15 golden FENs | Identical, all 15 | Run, both binaries |
| 1.4 | `levers.json`: 83 names, defaults and ranges | Identical to the reference source for all 83 | Source |
| 1.5 | Golden coverage of NNUE | 15 positions only; the plan asked for a larger set | Source (file contents) |
| 1.6 | Golden UCI transcript | Only the handshake; `go`, `stop`, `ponderhit` transcripts not captured (plan says Phase 5) | Source (file contents) |
| 1.7 | Harness modernisation (`--baseline`, timeouts, shared helper) | Open in the plan; not checked here | Not checked |

Note: the reference's `bench` output is locale-formatted (`3.169.859`); the numbers are the same.

---

## Phase 2: board, move generation, perft

| # | Item | Result | How checked |
| --- | --- | --- | --- |
| 2.1 | Perft counts | Identical (see 1.2) | Run |
| 2.2 | Zobrist, SEE, draw detection | Listed as open in Phase 2; done in Phase 4a/4c. The Phase 2 checklist was never ticked. | Source (plan text) |
| 2.3 | "No heap allocation in move generation" | Plausible from the code layout; not measured | Not checked |

---

## Phase 3: NNUE

| # | Item | Result | How checked |
| --- | --- | --- | --- |
| 3.1 | 15 golden evaluations | Identical | Run |
| 3.2 | `nnueeval` output format | Same form (`nnueeval <cp>`) | Run |
| 3.3 | `EvalFile` override | The reference loads a second net through the option (a bad file is a refusal). KnightByte accepts the name and ignores it. **Deviation (missing feature).** | Run and source |
| 3.4 | Incremental accumulator with lazy own-king refresh | The reference is incremental. KnightByte recomputes the accumulator per position. Results are identical; this is a speed item. | Source |
| 3.5 | SIMD kernels (AVX2, AdvSimd) | The reference has them; KnightByte uses the scalar path. Results must stay bit-exact. Speed item. | Source |
| 3.6 | Net checksum and loader refusal | Plan says implemented and tested; not re-run here | Not checked |

---

## Phase 4: search, time, levers

### 4.1 Search tree and defaults

| # | Item | Reference | KnightByte | Status | How checked |
| --- | --- | --- | --- | --- | --- |
| 4.1.1 | Node totals, bench 12/16/18 | 416,533 / 3,169,859 / 6,925,762 | same | identical | Run |
| 4.1.2 | Bare `go` (no limiter) searches to | depth 6 | depth 8 | **deviation** | Run |
| 4.1.3 | `bench` default depth | 13 | 12 | **deviation** | Source |
| 4.1.4 | `bench` arguments and output | `bench [depth] [threads]`, echoes `setoption` overrides, output `Total Nodes` / `Total Time` | `bench [depth]` only, output `Nodes` / `Time` / `NPS` | **deviation** | Source and run |
| 4.1.5 | Single legal move | returns at once with `info depth 0 score cp X pv move` (not while pondering) | searches it | **deviation** (missing feature) | Source |
| 4.1.6 | Stop before depth 1 completes | returns the first legal move with no `info` line | the plan says the same, but the code path was not checked | not verified | Not checked |
| 4.1.7 | Depth cap for `infinite` and `nodes` | 64 (256 in ponder) | 128 (`MAX_PLY`) | **deviation** | Source |
| 4.1.8 | Time management (soft and hard limits, stability, clock imbalance) | ported according to the plan | not re-checked | not verified | Not checked |

### 4.2 Info lines

| # | Item | Reference | KnightByte | Status | How checked |
| --- | --- | --- | --- | --- | --- |
| 4.2.1 | Iteration info line | `depth`, `seldepth`, `score`, `nodes`, `nps`, `hashfull`, `time`, `pv` | `depth`, `seldepth`, `score`, `nodes`, `pv` | **deviation** (missing `nps`, `hashfull`, `time`) | Run |
| 4.2.2 | `info currmove` | reported (interval setting, source) | not implemented | **deviation** (missing feature) | Source |

### 4.3 Levers and options

| # | Item | Result | How checked |
| --- | --- | --- | --- |
| 4.3.1 | Reference levers exposed under reference names, defaults and ranges | 36 of 83 exposed; all 36 match the reference | Source |
| 4.3.2 | Reference levers not exposed | 47 missing. Listed in the plan under Phase 4 carry-over. | Source |
| 4.3.3 | KnightByte-only names | `SingularExcludeTtCutoff` (to be removed, decided); `UseCorrectionHistory`, `UseCheckAwareQuiescence`, `UseSeeApproxPrefilter` (kept as research levers, decided) | Source |
| 4.3.4 | Lever visibility in the handshake | The reference lists only its 12 public options; its 83 levers are settable but hidden. KnightByte lists all 52 options. **Deviation.** | Run |
| 4.3.5 | Public options: names, defaults, ranges | Identical (`Book Depth` default 16 in both) | Source |
| 4.3.6 | `Threads` | Accepted, single-threaded in KnightByte. The reference runs a thread pool. **Missing feature.** | Source |
| 4.3.7 | `Contempt`, `Own Book`, `Book File`, `Book Depth`, `SyzygyPath`, `SyzygyRootProbe`, `Syzygy50MoveRule`, `EvalFile` | Accepted, inert in KnightByte (book, tablebases and `EvalFile` are later phases; `Contempt` has no effect yet). **Missing features.** | Source |
| 4.3.8 | Features off by default in the reference and not ported | ProbCut (main and secondary), quiescence checks, SEE reach, LMR v2 and its terms, verified null move, TT extension, trio extension, and others (plan lists them) | Source |

### 4.4 Parts of the UCI layer that Phase 4 touches

| # | Item | Reference | KnightByte | Status | How checked |
| --- | --- | --- | --- | --- | --- |
| 4.4.1 | Handshake `info string` lines (book, Syzygy, eval source) | printed | not printed | **deviation** | Run |
| 4.4.2 | Start-up banner on stdout | printed before `uci` | not printed | **deviation**; the rules say reproduce, so this needs a decision | Run |
| 4.4.3 | Diagnostic commands `d`, `eval`, `debug`, `nnueregions`, `searchstats`, `checkmoves` | present | `d`, `eval`, `debug` return "unknown command"; the rest not present | **deviation** | Run and source |
| 4.4.4 | argv: `param:Name=Value` overrides and one-shot command line | present | not present | **deviation** | Source |
| 4.4.5 | `ucinewgame` | resets the position to the start position | keeps the current position (predecessor bug 143 class) | **deviation**, tracked as KnightByte bug B34 | Run |
| 4.4.6 | `go` with no position | searches from the start position (after the predecessor fix at `bffe98a`) | answers `bestmove 0000` (fix B32) | **deviation** | Run |
| 4.4.7 | `position` with an invalid FEN or an illegal move | previous position kept, error line (after `bffe98a`) | previous position kept, error line | identical behaviour; message wording differs | Run |
| 4.4.8 | `isready` during a search | answers at once | answers at once | identical | Source |
| 4.4.9 | `isready` warm-up search | runs one short search on the first `isready` (predecessor quirk Q7) | none | **deviation** (quirk not reproduced; decision pending) | Source |
| 4.4.10 | `go ponder`, `ponderhit`, `go mate N`, `go searchmoves` | supported; a stopped ponder search answers `bestmove 0000` | not supported (falls back to a legal move) | **deviation** (missing features) | Source |
| 4.4.11 | `stop` and `quit` during a search | stop the search and answer | stop works for the synchronous loop; `Threads` and the search thread are not in place | **missing** (Phase 5) | Source |

---

## Items that need a decision before work continues

1. **Option visibility (4.3.4):** the reference hides its 83 levers from the handshake. KnightByte shows them. Decide whether KnightByte follows the reference or keeps them visible.
2. **Start-up banner (4.4.2) and `isready` warm-up (4.4.9):** both are predecessor quirks. The rules say reproduce now and review after the migration. Confirm that this is still the plan.
3. **`go` without a position (4.4.6):** the predecessor now starts from the start position; KnightByte still answers `0000`. Align KnightByte with the predecessor.
4. **Bare `go` depth (4.1.2) and `bench` default depth (4.1.3):** both differ from the reference. Align, or record as a deliberate deviation with a reason.
5. **Single legal move (4.1.5) and `info currmove` (4.2.2):** both are reference behaviour. Port them, or record the deviation.

## Not checked in this audit

- Phase 4 time management (4.1.8) and the stop-before-depth-1 path (4.1.6).
- The net checksum and loader refusals (3.6).
- Phase 1 harness items (1.7).
- Memory behaviour of move generation (2.3).
- Anything in Phases 5 to 9.
