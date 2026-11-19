# ADR-0009 — Tablebase and book access

**Date:** 2026-11-19
**Status:** accepted

## Context

Phase 6 adds the two optional knowledge sources the reference engine has:
Syzygy endgame tablebases (WDL and DTZ, 3-4-5 piece sets and larger) and
CTG `.ctg` opening books.

Three decisions are not obvious from the port itself:

1. **How table files are read.** A 3-4-5 set is about 1 GB; a 6-man set is
   larger than a typical machine's free RAM. Copying every file into the heap
   at load time would be either slow or impossible, and the engine must stay
   free of runtime dependencies in the search.
2. **When book files are looked for.** The reference scans the executable
   directory at startup. The migration plan asks for auto-detection only when
   it is explicitly requested, and no third-party book may be published.
3. **How the optional state is shared.** The engine probes from the search
   worker and, with `Threads > 1`, from several helpers at once. A reload must
   never race a running search.

## Decision

1. **Table files are memory-mapped, lazily.** `memmap2` (MIT OR Apache-2.0)
   provides the read-only mapping. The material list is enumerated by name at
   load time (a `metadata` call per candidate; no bytes are read), and a file
   is mapped and parsed on the first probe that needs it. An unreadable,
   truncated or corrupt file fails the probe for that material; it never
   panics and never aborts the engine.
   The dependency is the only runtime crate. It carries no search-path code,
   has no transitive dependencies of its own beyond `libc` on Unix, and the
   license is recorded in `THIRD-PARTY-NOTICES.md`.
2. **Book loading is explicit.** `setoption name Book File value <path>` loads
   that file; an empty value requests auto-detection next to the executable or
   in a `books/` subdirectory. Startup loads nothing, so a published build
   cannot silently pick up a book that happens to sit next to it. `Own Book`
   defaults to false, as CCRL requires.
3. **One process-wide table set, snapshotted per search.** `SyzygyPath` builds
   a new immutable `Syzygy` behind an `Arc` and swaps it into a process-wide
   slot; every search takes one clone at its start. The UCI loop joins a
   running search before any `setoption`, so a reload can never race a worker.
   Probes inside one table set are serialized by the set's own mutex, exactly
   like the reference's single prober lock; this is endgame-only traffic.
4. **The reference's coarser error flag is reproduced.** A material's hash
   slot carries one "file failed" flag for both WDL and DTZ. A missing or
   corrupt `.rtbz` therefore also disables WDL probing for that material until
   a reload. This is recorded as quirk Q9 in the migration register; the
   graceful-degradation behaviour (no panic, no crash) is the important part.
5. **Test data is small and public.** `tests/data/tb/` holds the three 3-man
   WDL/DTZ pairs (KQvK, KRvK, KPvK), which are part of the public Syzygy set.
   Book tests build a synthetic `.ctg` fixture in memory, so no third-party
   book is committed. The full 3-4-5 set is exercised locally through
   `KNIGHTBYTE_SYZYGY_PATH` (the DTZ-ranking tests skip when it is unset).

## Consequences

- A build without tablebases has no startup cost and no extra threads. The
  search consults one `Option<Arc<Syzygy>>` field, set once per search.
- Probing correctness is gated by the 3-man tests and, locally, the full
  set; the four bench pins are unchanged because the bench positions are far
  outside the loaded material range and the default configuration loads no
  tables.
- The error-flag coarseness is a known reference quirk, not a KnightByte
  regression.
- The release package must mention `memmap2` and the tablebase test data in
  `THIRD-PARTY-NOTICES.md`. No book file is part of the package.
