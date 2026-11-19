# Enhancements for KnightByte

**Status:** living list. Every idea that is worth doing in KnightByte, but must
not be done during the migration, is recorded here.

**Scope rule.** The migration reproduces the predecessor's behaviour exactly.
Once the migration is finished, the predecessor is frozen and KnightByte is the
only product. Everything below is therefore allowed after the migration, but
each item needs its own evidence before it lands:

- a change that moves node counts needs an SPRT (or the fixed-time time-to-depth
  benchmark, for time-related items) against the released KnightByte build;
- a protocol change needs a transcript test;
- every accepted change gets an ADR in `docs/decisions/` and a line in the
  feature list.

Items are grouped by area. "Gate" says how the item is judged.

---

## 1. Search

### 1.1 Helper-thread diversity for Lazy SMP
- **Idea:** helpers start their iterative deepening at different depths (a fixed
  skip table) so they fill the shared table with different work.
- **Why not in the migration:** the migration must not add deviations, and the
  predecessor's helpers all start at depth 1.
- **Gate:** SPRT at a fixed time control with 2, 4 and 8 threads. Node counts
  are not compared (multi-thread search is not deterministic).

### 1.2 Singular-extension verification without the triggering table entry
- **Idea:** the verification search skips the table cutoff on the entry that
  triggered it, so singular extensions can fire.
- **Evidence so far:** tried behind a lever, 904 games, no gain (ADR-0006).
- **Gate:** a new SPRT after the other search changes. Retire the idea if it
  still shows no gain.

### 1.3 Keep the move identity of castling and en-passant table moves
- **Idea:** the transposition table stores enough information to rebuild a
  castling or en-passant move, so such moves are tried first, excluded in
  verification, and can earn extensions.
- **Expected effect:** better move ordering; node counts change.
- **Gate:** SPRT.

### 1.4 En-passant captures in quiescence search
- **Idea:** quiescence generates en-passant captures.
- **Gate:** SPRT.

### 1.5 Correction-history quiet gate that sees real captures
- **Idea:** the quiet test for the correction-history update checks the capture
  target instead of the mover's own square.
- **Gate:** SPRT. Changes the learned state, so also check that node counts
  stay stable over the bench set.

### 1.6 Features that ship switched off
These are implemented in the migrated code but off by default. After the
migration each one is tested with SPRT, one at a time, and enabled only if it
gains:

- ProbCut (main and secondary layers);
- quiescence checks;
- SEE reach (the wider SEE pruning window);
- LMR v2 and its individual terms;
- verified null-move pruning;
- TT extension;
- trio extension.

### 1.7 Ablation of the three always-on features
- **Idea:** the KnightByte-only research levers `UseCorrectionHistory`,
  `UseCheckAwareQuiescence` and `UseSeeApproxPrefilter` switch off features that
  are on by default. Measure what each one is worth in the current engine.
- **Gate:** SPRT per lever. If a feature is worth nothing, remove it and the
  lever together.

### 1.8 Search constants
- **Idea:** tune the pruning and reduction constants with SPRT, once the golden
  gates are no longer needed to guard the port. Strength tuning starts only
  after the post-migration baseline is frozen.
- **Gate:** SPRT, with the baseline recorded first.

### 1.9 Lower effective branching factor (the search-tuning campaign; the LAST project step)
- **Evidence (2026-11-19, Zen 4, `perf-test/ebf_compare.py`, 5 s, 1 thread,
  256 MB).** KnightByte and the predecessor are node-identical at a fixed depth
  (identical EBF 2.131 over the four positions at depth 20), so KnightByte has no
  pruning advantage yet; the 2.07 vs 2.02 of the 5-second table only reflects the
  deeper end depth. A top engine of the same class reaches depth 26 on the start
  position with 4.2 M nodes (EBF 1.80) where KnightByte reaches depth 20 with
  9.3 M (EBF 2.23); average EBF 1.79 vs 2.01. The gap is largest in the opening
  and middlegame (1.80 / 1.89 / 1.84 vs 2.23 / 2.02 / 2.02) and small in the
  endgame (1.65 vs 1.76). That engine also spends about 2.2x more time per node,
  so at equal time the speed advantage is on KnightByte's side.
- **Likely sources:** a mild late-move-reduction schedule
  (`1 + ln(depth) * ln(move) / 1.75`), singular extensions that never fire (Q1,
  see 1.2), reference features that are off or not ported yet (1.6; 47 of the 83
  levers are still inert), and margins tuned for a less accurate evaluation. The
  network is fixed by the project rules, so its share of the gap cannot be closed.
- **Why it is not a migration task:** every change here moves the node pins. And
  EBF/depth is not strength: depth is counted differently by every engine and
  aggressive pruning can lose Elo. EBF, node counts and best-move agreement are
  guard rails only; **Elo under SPRT is the criterion**.
- **When:** after phase 6, after the performance plan is finished and after the
  macOS/Linux/M4 validation, with the baseline frozen and pinned on every
  platform. It is the last step of the project before the CCRL submission.
- **Method:** `Threads = 1`, short fast games (for example 8+0.08, many games per
  hour at the current speed), one change at a time; every accepted change gets a
  new set of bench pins, a line in `docs/baselines.md`, an ADR and a feature-list
  entry.
- **Experiments, cheapest first:**
  1. LMR constants: the divisor (1.75 up to roughly 2.2-2.8), the base term and
     the history-term scale.
  2. Pruning margins: reverse futility, futility, razoring, SEE-pruning
     thresholds, the null-move reduction base and divisor, late-move-pruning
     limits.
  3. Singular extensions, retested after step 1-2 (1.2; the earlier 904-game
     test showed no gain, so retire the idea if it still does not).
  4. The features that ship off (1.6), ported where needed and tested one at a
     time: ProbCut first, then LMR v2 terms, SEE reach, quiescence checks.
  5. A final SPSA pass over all margins together.
- **Expected effect (an estimate, not a measurement):** EBF about 2.0 -> 1.85-1.9
  and +50 to +150 Elo.
- **Gate:** SPRT per change against the frozen baseline; accept only on a pass.
  Also re-run the time-to-depth set, the 1/2/4/8-thread match gates and the
  stability checks of `perf_test.py`.

---

## 2. Protocol and UCI

### 2.1 Stdout banner
- **Idea:** remove the start-up banner from stdout. Protocol clients expect only
  protocol lines; CCRL asks for no stray stdout.
- **Gate:** transcript test. Shipped only after the migration is frozen.

### 2.2 `isready` warm-up
- **Idea:** no warm-up is needed in a compiled engine. Keep `readyok` immediate
  and drop the warm-up search, which writes table entries before the first
  search.
- **Gate:** node counts of the first search after `isready` should be identical
  with and without the warm-up; check it on the transcript set first.

### 2.3 Ponder stop reply
- **Idea:** decide whether a stopped ponder search should reply with the best
  move found so far instead of the null move. The null move is valid but gives
  the GUI nothing to use.
- **Gate:** transcript test with two GUI-style clients, plus a ponder match.

### 2.4 Persistent worker threads
- **Idea:** the migration plan already calls for persistent workers (Phase 5,
  step 8). Keep it; benchmark the start-up cost of a fresh thread per search to
  confirm the gain.

### 2.5 Engine identity and option wording
- **Idea:** the identity lines and option help texts are KnightByte's own. Write
  them once, in the feature list, and keep them stable from the first release.

---

## 3. Performance (from the performance audit, `maximizing_performance_nps_in_a_rust_uci_chess_engine.md`)

Most items are already in the migration plan under Phase 7. The list below adds
the items that the audit describes and the plan does not yet name.

### 3.1 Build and PGO
- Profile-guided optimisation per release artifact and per Windows ISA variant
  (plan §8.3). Gate: fixed-depth node identity plus NPS at the same host.
- `target-cpu` per artifact; `apple-m1` as the baseline, `apple-m2` for an
  i8mm build. Gate: node identity on each host.

### 3.2 Memory and OS
- Windows large pages for the transposition table, with the privilege check and
  a fallback to regular pages.
- Windows: turn off power throttling for the engine process and keep the
  search thread at above-normal priority; measure before shipping.
- macOS: set the QoS class of worker threads; no hard affinity is available.
- Keep the process awake during long searches on Windows, restored on exit.

### 3.3 Transposition table
- Generation-based `ucinewgame` (no synchronous clear of a large table).
- Cache-line aligned buckets with prefetch of the next bucket.
- Lazy page allocation.

### 3.4 Search hot path
- Zero-allocation audit with a counting allocator in tests.
- Copy-make against make/unmake, benchmarked on each host.
- `get_unchecked` and `assert_unchecked` only where a measurement shows a gain.
  The audit expects 0-3 %, so measure first.
- Keep the stop/time check at every 1024 nodes; no clock read per node.

### 3.5 NNUE
- Incremental accumulator with lazy own-king refresh (plan Phase 3 item,
  deferred).
- SIMD kernels per host. Each kernel must be bit-exact to the scalar path.

### 3.6 Sliders
- Compare magic bitboards with PEXT on each host. The audit warns that PEXT is
  slow on older AMD parts, so the choice is per CPU.

---

## 4. Tooling and evidence

- A time-to-depth harness on a fixed set of 20 positions (plan Phase 1 item,
  still open).
- A UCI oracle script that runs two engine builds on the same script and diffs
  node counts, best moves and option sets.
- A self-play legality check: every reported principal variation and best move
  is validated by replaying the game.
- Keep the performance notes in `docs-new/` up to date with the command, host
  and result for each run.

---

## 5. Decisions already taken that are not enhancements

Recorded here so they are not lost. None of these is an enhancement; each is a
migration decision and already reflected in the migration plan.

- Lever catalogue: 83 reference levers under their reference names, plus three
  KnightByte-only research levers (1.7). `SingularExcludeTtCutoff` is removed.
- The predecessor quirks are reproduced during the migration and checked again
  after it. The results of that check feed the items above.
