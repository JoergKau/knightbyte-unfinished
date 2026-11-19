# What to give back to PawnForgeCS

Written 2026-11-19, at the end of the Windows performance work. KnightByte (KB, Rust) now
searches the **identical tree** as PawnForgeCS (PF, C#) about **2.1-2.25x faster** on the
same host (same-session depth 20, `docs/baselines.md` 7.17 and 7.18). This file says
where that factor came from, and which parts are worth carrying back into the C#
engine, in order of value, each with the C# code that has to change.

The PF code references are to `C:\dev\PawnForgeCS.win.final\src\PawnForge.Core`, read on
2026-11-19. Nothing here has been tried in C# yet: every expected gain is an estimate
from the Rust measurements and must be measured in PF (section 5).

## 1. The honest decomposition

KB did not get 2.25x from clever tricks alone. It started **slower** than PF (0.63x), and
most of the first climb was KB repairing its own port defects. Reading the record of
`docs/baselines.md` 7.2-7.18 in order:

| Stage | Same-session depth 20, KB / PF | What happened |
| :--- | ---: | :--- |
| First port | 0.63x | KB slower than PF |
| make-free legality (P1) | 0.87-0.91x | KB's own defect: 89.8 % of its `make_move` calls were the legality filter. PF already generates check- and pin-aware moves. |
| hardware `pext` (P2) | 0.97-1.00x | KB's software fallback was in use. PF already calls `Bmi2.X64.ParallelBitExtract`. |
| TT prefetch (P5) | 1.08-1.11x | KB lacked it. PF already prefetches (`TranspositionTable.Prefetch`). |
| **B43: environment lookup per node** | **1.69-1.73x** | KB's own defect: 18-20 % of all samples. Not a gain over PF, a loss repaired. |
| fused kernels + forced-reply (P1c, P4a, P4d, P4e) | 1.17x on top (balanced) | **New work that PF does not have.** |
| PGO | +6.4 % on top (balanced) | Rust's profile-guided build. |

Rounded: **about 1.7x is Rust against .NET for the same algorithms** (no JIT, no GC, no
bounds checks, native code layout), **about 1.17x is new engineering**, **about 1.06x is
PGO**: 1.73 x 1.17 x 1.06 = 2.1, which matches the measured 2.13-2.25. (The stages were
measured in different sessions, so the product is a consistency check, not an exact
identity.)

What this means for PF: the 1.7x language factor cannot be given back. The 1.17x of new
engineering can, at least in part, and C# may also gain from things KB never needed to
do (section 3). A realistic target for PF is **single-digit percent to perhaps 10 %**
from section 2, not 2x.

## 2. Worth porting to C# (new in KB, absent in PF)

All of these are **behaviour-neutral**: PF's node counts must not move. KB's pins are
PF's own: `bench 6 / 12 / 16 / 18` = 15,528 / 456,308 / 2,815,799 / 6,892,874 nodes, and
depth 20 on the four-position set visits 18,662,358 nodes in both engines. Any change
that moves one of these numbers is a bug in the change.

### 2.1 Fused accumulator push, no parent copy (KB P4a: +5.1 % balanced)

**PF today** (`Evaluation/NnueAccumulatorV5.cs`): `Push()` copies both perspectives into
a stack slot with `Array.Copy` (2 x L1 shorts = 2 KB at L1 = 512), then `ApplyDirty` does
one load-modify-store pass over the live accumulator for **every** changed row
(`AddRow` / `SubtractRow`, two to four rows per move, `Vector<short>` chunks).

**KB**: `child = parent - sum(subtracted rows) + sum(added rows)` in **one pass**. Each
vector chunk is loaded once, updated in registers and stored once, whatever the row
count. The shape (rows added, rows subtracted) is a compile-time constant for the three
shapes a move produces (quiet 1/1, capture or promotion 1/2, castling 2/2), so the loop
unrolls; anything else takes the plain path. `i16` wrapping arithmetic is associative, so
the result is bit-identical to copy-then-apply.

How to do it in C#:

- Make the accumulator a per-ply array (`acc[ply]`) instead of "one live accumulator plus
  a saved copy". A make writes `acc[ply + 1]` straight from `acc[ply]`; an unmake is
  `ply--`. This removes the 2 KB `Array.Copy` in `Push` and the one in `Pop`.
- Keep the stale flags per ply exactly as `_stackStale` does today (a king move marks its
  own perspective stale; the unmake restores the flags of the ply you return to).
- The fused loop, for the quiet case:

```csharp
ref short parent = ref MemoryMarshal.GetArrayDataReference(acc[ply]);
ref short child  = ref MemoryMarshal.GetArrayDataReference(acc[ply + 1]);
ref short w      = ref MemoryMarshal.GetReference(weights);
for (int i = 0; i < l1; i += Vector256<short>.Count)
{
    Vector256<short> p = Vector256.LoadUnsafe(ref parent, (nuint)i);
    Vector256<short> a = Vector256.LoadUnsafe(ref w, (nuint)(addRow + i));
    Vector256<short> s = Vector256.LoadUnsafe(ref w, (nuint)(subRow + i));
    (p + a - s).StoreUnsafe(ref child, (nuint)i);
}
```

  Add the capture/promotion (1 add, 2 sub) and castling (2 add, 2 sub) variants. Write
  them as separate small methods rather than a generic loop over a row list: the JIT does
  not unroll a loop over a runtime count, and the unrolling is part of the gain.
- Test: the existing incremental == cold-start random walk (`NnueAccumulatorV5Tests`)
  already is the right oracle. Add widths that are not a multiple of the vector width.

### 2.2 Fused full refresh (KB P4d: +1.8 % balanced)

**PF today**: `RefreshPov` copies the bias, then makes one load-modify-store pass over
the whole accumulator for each of the 20-30 active feature rows.

**KB**: loop over vector chunks on the **outside**. Each chunk starts from the bias,
collects the matching chunk of every active row in a register, and is stored once. The
result is bit-identical (wrapping `i16`). A refresh is rare (about 156 K per `bench 16`
against 1.89 M incremental pushes), which is why the gain is small, but it is a few
lines.

```csharp
for (int i = 0; i < l1; i += Vector256<short>.Count)
{
    Vector256<short> sum = Vector256.LoadUnsafe(ref bias, (nuint)i);
    for (int k = 0; k < count; k++)
        sum += Vector256.LoadUnsafe(ref w, (nuint)(features[k] * l1 + i));
    sum.StoreUnsafe(ref acc, (nuint)i);
}
```

### 2.3 Forced-reply count without building the list (KB P1c: +1.1 % balanced)

**PF today** (`Search/Search.Negamax.cs`, the single-response extension, line 198):

```csharp
Span<Move> countBuf = stackalloc Move[256];
if (pos.GeneratePseudoLegalMoves(countBuf) == 1) { depth++; extensionsUsed++; }
```

This generates the whole move list at every eligible node (not in check, depth >= 3,
budget left) only to learn whether it holds exactly one move.

**KB** `has_exactly_one_pseudo_move`: knight, slider and king targets are only **counted**
(popcount of the attack set without own pieces); the answer is "no" as soon as a second
move is seen; only pawn moves and castles are built, and only when the pieces produced
fewer than two. Same answer, no list.

The test that matters: compare it against the full generator over a long walk of
positions **plus hand-made positions in which the side to move has exactly one pseudo-legal
move**. A random walk never reaches one (the king is walled in by its own pieces), so a
walk alone would pass a wrong implementation. KB's test fails if the king is dropped
from the count; copy that check.

### 2.4 Allocations in the C# move picker (a finding of the KB allocation audit, not yet measured in PF)

KB's audit found B40 (two accumulator vectors allocated per node) and fixed it; the final
state is two allocations per search iteration and none per node (`tests/alloc_audit.rs`).
Reading PF with the same question, three allocation sites sit in the capture scoring of
`Search/MovePicker.cs`, run once per **scored capture**:

| Site | Code | Fix |
| :--- | :--- | :--- |
| `Board/Position.See.cs` line 109, `SeeValue` | `int[] scores = new int[32];` (152 bytes) | `Span<int> scores = stackalloc int[32];` |
| `MovePicker.cs` line 632, `PieceValuesSafe` | `int[] vals = new int[] { 100, 320, ... };` (48 bytes), **twice per `ScoreCapture`** | `private static readonly int[] Values = { ... };` or a `ReadOnlySpan<int>` literal |
| `MovePicker.cs` line 538 | `ScoreCapture` calls `SeeValue` | (follows from the first row) |

That is about three heap allocations per scored capture, with Server GC enabled in both
`.csproj` files. Two cautions before counting on a gain: .NET 10 and later may already
remove non-escaping array allocations by escape analysis, so **measure the allocated
bytes per search first** (`GC.GetAllocatedBytesForCurrentThread()` before and after a
`bench`), and only then fix what is really allocating. The fix cannot move a node count.
The allocations may also matter for multi-thread scaling (section 3.1): every gen0
collection of Server GC pauses all search threads.

A smaller one in the same spirit: `Tb/Syzygy.cs` line 334, `DebugProbe =>
Environment.GetEnvironmentVariable("PF_TBDEBUG") == "1"` reads the environment at every
call (three call sites on the probe path). It only runs inside tablebase positions, so
the cost is small, but this is exactly KB's B43 (one environment lookup per node cost
18-20 % of all samples on Windows). Make it `static readonly bool`.

### 2.5 AVX-512 widths for the accumulator kernels (not separately measured)

PF's NNUE code is 256-bit throughout (`Vector256`, `Avx2`, and `Vector<short>`, which
is 256-bit by default in .NET even on an AVX-512 CPU). KB's kernels have SSE4.1, AVX2 and
AVX-512 versions chosen once at start, and the P4a/P4d numbers above were measured with
the AVX-512 kernel on a Zen 4 class CPU. KB did not measure AVX-512 against AVX2 on its
own, so the share of the +5.1 % that comes from the wider vector is unknown. In C# the
route is `Vector512<short>` guarded by `Vector512.IsHardwareAccelerated`, with the
256-bit path as the fallback. Do it **after** 2.1 and 2.2 and measure it as its own step.

## 3. Worth checking in PF (suspicion, not proof)

### 3.1 Thread scaling: PF 5.74x at 8 threads, KB 7.08x

`docs/baselines.md` 7.10, same session, same positions, depth 18:

| Threads | KB NPS | KB speedup | PF NPS | PF speedup |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2,316,332 | 1.00x | 1,337,563 | 1.00x |
| 2 | 4,792,734 | 2.07x | 2,578,258 | 1.93x |
| 4 | 9,286,033 | 4.01x | 5,044,434 | 3.77x |
| 8 | 16,393,170 | 7.08x | 7,683,494 | 5.74x |

KB's pool, shared table and per-thread state are the same design as PF's, and KB needed
**no** pool or table change to reach 7.08x (the earlier "collapse" was KB's own node
accounting, B44, plus B43). So the cause of PF's weaker scaling is not known. Candidates,
cheapest first:

1. **GC pauses.** Server GC stops all threads at a collection; any per-node or per-capture
   allocation (section 2.4) makes that happen several times per second on 8 threads.
   Test: the same 8-thread run before and after removing the allocations, and with
   `DOTNET_gcServer=0` and `DOTNET_GCgen0size=` raised as a probe.
2. **False sharing** of per-thread counters or flags that sit next to each other.
3. **The shared evaluation cache and transposition table** (one cache line written by
   many threads).

Do one probe at a time and compare nodes per second of a single helper in isolation
against the same helper under 8 threads (KB's discovery method: print per-helper node
counts, do not trust the aggregate).

### 3.2 Node accounting with `Threads > 1`

KB's B44 was a pure reporting defect: helpers never published their node counts, and the
main thread published itself and then reported "own + counter". It made 8 threads look
slower than 1 and enforced `go nodes N` on the wrong aggregate. PF reports plausible
numbers (5.74x), so it probably does not have the bug, but it takes five minutes to
check: run `go movetime 4000` with `Threads 1` and `Threads 4`, print each helper's own
count, and confirm that the total equals their sum and that `go nodes N` stops at N in
total.

### 3.3 Levers that do nothing

KB's `bench` now honours configured levers, and a release-mode test
(`tests/lever_effect.rs`) checks that **every advertised lever changes the search**. That
caught levers that were wired to nothing. PF advertises about 80 levers and its own
audit found wiring defects of this class (Bugs 103 and 118 in its log). The same test is
cheap to write in C#: for each lever, set it to a non-default value, run a fixed-depth
bench, and require a different node total than the default. A lever that prints the
default total is not wired.

## 4. Already in PF (nothing to give back)

PF already has these and KB only had to catch up. They are listed so nobody re-implements
them:

- Hardware `pext` for the slider attacks (`Board/PextBitboards.cs`, `Bmi2.X64`).
- Transposition-table prefetch after every make (`TranspositionTable.Prefetch`, and the
  eval-cache prefetch).
- Check-mask and pin-aware move generation (`GenerateLegalMoves(..., useCheckMasks: true)`),
  i.e. no make-then-test legality filter.
- An exact 16-bit `MultiplyAddAdjacent` output head (`Evaluation/NnueNetV5.cs`, lines
  452-522). KB's first port widened to 32 bits and its P4c step only restored this.
- Incremental accumulators with a lazy own-king refresh and a push/pop stack.
- SEE with pins taken from the stored `pinned[]` arrays and the shared `BetweenTable`. KB's
  P4e (lazy pin masks, +3.4 %) fixed a KB-specific cost (pins recomputed per exchange);
  PF's version is two table lookups and is not worth changing before the rest.
- `TieredPGO`, so the .NET runtime already does the profile-guided step at run time. The
  Rust PGO's 6 % has no extra counterpart to build.

Not applicable to C#: B42 (eval cache stored while disabled: PF returns before any cache
instruction), B43 (PF reads its trace environment variable once, apart from the
`DebugProbe` above), B40, B51 (static C runtime), the per-ISA Windows variants and the
start-up CPU guard. The .NET JIT already picks the instruction set at run time; that is
one thing the JIT does better than a static build.

## 5. How to back-port safely

1. **Freeze the oracle.** Record `bench 6 / 12 / 16 / 18` and the per-depth node counts
   (KB's `tools/capture_bench.py compare` against `tests/golden/search_parity.json` works
   with any engine that prints the same `Nodes:` lines). Every step must keep every delta
   at +0.
2. **One step at a time.** Same order as KB: 2.3 (smallest, isolated), 2.1, 2.2, then 2.4,
   then 2.5. Measure each step before the next.
3. **Measure with the balanced method.** A back-to-back pair of byte-identical programs
   differed by 1.7-4.5 % in KB's tests, always in favour of the one that ran first.
   `tools/ab_bench.py` runs the order A B B A per round, checks the node pin on every run,
   and takes the ratio of the mean NPS. The A/A noise floor was about +/-0.8 %, so a
   change under about 1.5 % needs more rounds before it is believed. In C#, also run a
   warm-up first: a cold process spends its first seconds in the JIT (the predecessor's
   own `isready` warm-up exists for that, quirk Q7).
4. **Profile before guessing.** KB's two biggest findings (B43: 18-20 % of the samples in
   an environment lookup; B44: the thread collapse) were found by a per-function sampling
   profile and a per-helper count, after the written plan had guessed something else.
   For C#: a sampling profiler on a release build (for example `dotnet-trace`) and
   `GC.GetAllocatedBytesForCurrentThread()`, run on the same `bench 18`.
5. **Same session, same host.** The recorded predecessor baseline (1.57 M NPS, an older
   session) was 40 % above the same-session figure (1.06 M NPS). Compare two engines only
   when they ran in one session, and write both numbers down together.
6. **Gate before merging.** The full test suite, then a match against the previous build
   (at least a few hundred games at one thread; KB's gate was 500 games with no time
   loss, illegal move or crash) before anything is called finished.

## 6. Already given back

These were found while porting and have already been fixed in PF as well, so the two
engines stay equal (`quirks-found.md`, `PawnForgeCS-Files/new-bugs.md`):

- Bugs 147-149 (castling and en-passant table moves lose their marker, quiescence
  ignored en passant, the correction-history quiet gate).
- Q5-Q8 (ponder stop, start-up banner, `isready` warm-up writing into the shared table,
  single-move shortcut for infinite searches) and Bug 151 (a corrupt DTZ file switching
  off WDL probing for the same material).
- Still open in both: Q1, singular extensions never fire, because the verification search
  takes a table cutoff on the entry that triggered it. It is the largest open search item
  and a candidate for a joint test later (`docs/decisions/ADR-0006`).

## 7. Priority in one table

| # | Item | Expected in PF | Risk | Section |
| ---: | :--- | :--- | :--- | :--- |
| 1 | Allocation check, then fix of the picker allocations | unknown until measured; may also help threads | none | 2.4 |
| 2 | Forced-reply count without the list | about +1 % | low (test with hand positions) | 2.3 |
| 3 | Fused accumulator push, per-ply accumulators | the largest of the set, about +3-5 % | medium (stale flags) | 2.1 |
| 4 | Fused refresh | about +1-2 % | low | 2.2 |
| 5 | Why 5.74x and not 7x at 8 threads | up to +20 % at 8 threads | probe only | 3.1 |
| 6 | `Vector512` kernels | unknown | low (fallback exists) | 2.5 |
| 7 | Lever effect test | finds dead levers | none | 3.3 |
| 8 | `DebugProbe` as a static | tiny | none | 2.4 |

Sources for every number: `docs/baselines.md` 7.2-7.22, `docs-new/new-bugs.md` (B40, B42,
B43, B44), `docs/decisions/ADR-0011` (variants) and `ADR-0014` (levers).
