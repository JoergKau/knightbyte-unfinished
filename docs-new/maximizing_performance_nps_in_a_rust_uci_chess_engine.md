# Maximizing Performance (NPS) in a Rust UCI Chess Engine — macOS Apple Silicon & Windows x64

**Scope:** headless UCI engine (no GUI), `rustc 1.99.1`, targets `aarch64-apple-darwin` (M1–M5) and `x86_64-pc-windows-msvc`.

**Updated revision.** This version keeps the useful parts of the original audit, corrects several claims that were wrong or overstated, and adds the items that usually dominate real NPS: TT prefetching, PGO, runtime-dispatch caveats, TT allocation/aging, persistent Lazy SMP threads, and platform-specific scheduling/paging details.

> **NPS is a proxy, not the goal.** A change that reduces node count (better move ordering, TT, pruning) can lower NPS while raising time-to-depth and Elo. Optimize **time-to-depth on a fixed benchmark set**; use NPS only to compare implementations of the same search.

---

## 1. Zero-Allocation Search & Stack-Only Data Structures

Any heap allocation (`Box`, `Vec::push`, `String`, `format!`) inside `alpha_beta()` or `quiescence()` costs more than an allocator lock: it touches new cache lines, may trigger a page fault, and prevents the optimizer from keeping data in registers. On .NET this was partly hidden by the GC/gen0 allocator; in Rust there is no GC, so a hot allocation is a direct `malloc`/`free` pair.

### Key Rust features

* **Fixed-capacity move lists.** Use `[Move; 256]` + a length, or `ArrayVec<Move, 256>`. The theoretical maximum number of legal moves is 218, and 256 slots comfortably cover pseudo-legal generation too. Keep the list per ply in the search stack, not on the heap.
* **`Copy` bit-packed primitives.** Encode moves as `u16`, squares/pieces as `u8`, scores as `i32`. Pass them by value. Add `#[repr(transparent)]` to newtypes for a guaranteed ABI/layout, but note that a normal single-field newtype is already zero-overhead in practice.
* **Avoid `Option<T>` where it doubles the footprint.** `Option<u64>` is **16 bytes** (no niche), and `Option<Move>` is **4 bytes** because `Move(u16)` has no invalid bit pattern. In a 256-entry move array that doubles the cache footprint. Use a sentinel such as `Move::NULL` (`0`, i.e. `a1a1`, which is never a legal move), or `Option<NonZeroU64>` where zero is not a valid payload.
* **Compile-time tables via `const fn` + `static`, not `const`.** A large `const` item may be duplicated/inlined at use sites; a `static` has one address and lands in `.rodata`. Write `static ROOK_ATTACKS: [u64; N] = build_rook_table();` where `build_rook_table` is `const fn`. If compile-time evaluation becomes too slow (the `long_running_const_eval` lint), generate the table in `build.rs` into `OUT_DIR` and `include!` it. For tables that must be computed at runtime, use `std::sync::LazyLock`/`OnceLock` and hoist the reference out of the hot loop — every access performs an atomic check.
* **Bounds checks: correct the expectation.** `get_unchecked` usually buys **0–3%**, not 5–15%; LLVM already removes many checks when indices come from `u8 & 63`. It matters most in NNUE kernels and table lookups where the compiler cannot prove the range. Use newtypes that guarantee `0..64`, `debug_assert!` in debug builds, and `unsafe { core::hint::assert_unchecked(cond) }` (stable since 1.81) when you need to feed the optimizer a proven invariant. `assert_unchecked` is a **soundness promise**, not a check: a false condition is UB.
* **`wrapping_sub` in bitboard loops.** `bb & (bb - 1)` panics in debug builds on `bb == 0` due to overflow checks; use `bb & bb.wrapping_sub(1)` so debug and release behave the same.
* **Flat arrays, not `Vec<Vec<T>>`.** History/continuation tables should be one flat `Box<[T]>` (or `Vec<T>` with a fixed length) indexed by `a * 64 + b`. `Box<[T]>` also saves the capacity word versus `Vec<T>` and prevents accidental reallocation. Use struct-of-arrays where the hot loop reads one field from many entries.
* **`#[repr(C)]` + size assertions for hot structs.** Rust's default layout may reorder fields. For a `Position`/`SearchStack`, use `#[repr(C)]`, put the hottest fields first, and assert the size: `const _: () = assert!(core::mem::size_of::<Position>() <= 256);`. This keeps the position inside a few cache lines and makes copy-make predictable.
* **Copy-make vs make/unmake.** With a compact `Position` (≤ 256 bytes), `#[derive(Copy, Clone)]` and `let mut child = pos;` is often faster than incremental make/unmake, which has many branches and read-modify-write updates. Top engines use both approaches; benchmark on your hardware. If you use make/unmake, keep the undo stack as `[Undo; MAX_PLY]`, never a `Vec` that grows in the search.
* **Stack budget.** `MAX_PLY = 128` with per-ply move lists, killer moves, and undo records is fine, but a recursive search can still overflow the default thread stack. Spawn search threads with `std::thread::Builder::new().stack_size(16 * 1024 * 1024)`. On Windows the main-thread stack may be 1 MB; run the search on a spawned thread or pass `-C link-arg=/STACK:16777216`.

```rust
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Move(u16);

impl Move {
    pub const NULL: Move = Move(0); // a1a1: never a legal move

    #[inline(always)]
    pub const fn new(from: u8, to: u8, flags: u8) -> Self {
        debug_assert!(from < 64 && to < 64 && flags < 16);
        Self((from as u16) | ((to as u16) << 6) | ((flags as u16) << 12))
    }

    #[inline(always)]
    pub const fn from(self) -> usize { (self.0 & 0x3f) as usize }

    #[inline(always)]
    pub const fn to(self) -> usize { ((self.0 >> 6) & 0x3f) as usize }

    #[inline(always)]
    pub const fn flags(self) -> u8 { (self.0 >> 12) as u8 }
}

const _: () = assert!(core::mem::size_of::<Move>() == 2);
const _: () = assert!(core::mem::size_of::<Option<Move>>() == 4); // no niche: prefer Move::NULL
```

```rust
pub struct MoveList {
    moves: [Move; 256],
    len: u16,
}

impl MoveList {
    #[inline(always)]
    pub fn push(&mut self, m: Move) {
        debug_assert!((self.len as usize) < self.moves.len());
        unsafe { *self.moves.get_unchecked_mut(self.len as usize) = m };
        self.len += 1;
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[Move] {
        unsafe { self.moves.get_unchecked(..self.len as usize) }
    }
}
```

---

## 2. Bitboard Intrinsics & Architecture-Specific Instructions

### 2.1 Hardware bit operations

* `bb.trailing_zeros()` → `TZCNT`/`BSF` on x86_64, `RBIT + CLZ` on AArch64.
* `bb.count_ones()` → `POPCNT` on x86_64, `CNT` on AArch64.
* `bb &= bb.wrapping_sub(1)` → `BLSR` on x86_64 with BMI1, otherwise a short `LEA`/`AND` sequence.

### 2.2 PEXT/BMI2 for sliding attacks

`_pext_u64(occ, mask)` is excellent on Intel Haswell+ and AMD Zen 3+, but it is **not automatically faster than magic bitboards**:

* Zen 1/Zen 2 implement PEXT in microcode; avoid it there.
* PEXT and fancy magics both read a large attack table; the table lookup, not the index calculation, often dominates.
* Benchmark PEXT vs magic on your exact CPU. Do not assume "one 3-cycle instruction" wins.

AArch64 has no PEXT. Do not emulate it; use plain/fancy magic bitboards (64-bit multiply + shift) or a kindergarten/black-magic scheme. Apple's integer multiplier is fast, so magics are a good fit.

### 2.3 Compile-time `cfg(target_feature)` is not runtime dispatch

This is the most common Rust engine portability bug:

* `#[cfg(target_feature = "bmi2")]` is decided **when the crate is compiled**. If you build a generic binary, it is `false` even on a CPU that supports BMI2.
* Use `is_x86_feature_detected!("bmi2")` on x86_64/Windows, call it **once at startup**, and dispatch to `#[target_feature(enable = "bmi2")]` functions. Since Rust 1.86, `#[target_feature]` may be applied to safe functions; older code required `unsafe fn`.
* On macOS/AArch64, `is_aarch64_feature_detected!` is **not reliable**: the standard-library docs state that most features are only detected on Linux and the check returns `false` elsewhere. For `dotprod`/`i8mm` on Apple Silicon, either use compile-time `target-cpu` builds or query `sysctlbyname("hw.optional.arm.FEAT_DotProd")` / `("hw.optional.arm.FEAT_I8MM")`.
* The simplest maximum-performance strategy is the Stockfish one: **ship several binaries** (`v2`, `v3`, `v4-vnni`, `apple-m1`, `apple-m2`) and let the user/GUI choose. A single binary with function-pointer dispatch is more portable but adds an indirect call in hot paths.
* A `#[target_feature]` function cannot be inlined into a caller that lacks the feature. Keep the dispatch boundary at the top of the search, not around every attack lookup.

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
unsafe fn rook_attacks_pext(sq: usize, occ: u64) -> u64 {
    let mask = *ROOK_MASKS.get_unchecked(sq);
    let offset = *ROOK_OFFSETS.get_unchecked(sq);
    let index = core::arch::x86_64::_pext_u64(occ, mask) as usize;
    *ROOK_ATTACKS.get_unchecked(offset + index)
}
```

### 2.4 Apple Silicon feature levels (verified with rustc 1.99.1)

| `-C target-cpu` | Notable features | Safe on |
| :--- | :--- | :--- |
| `apple-m1` | NEON, `dotprod`, `fp16`, `lse`, `rcpc` | M1–M5 (baseline) |
| `apple-m2` | above + `i8mm`, `bf16` | M2–M5 |
| `apple-m4` | same feature set as `apple-m2` | M4–M5 |
| `apple-m5` | above + `mte` | M5 |

`apple-m1` is still accepted by rustc 1.99.1 even though `rustc --print target-cpus` may not list it (LLVM alias). Use `apple-m1` for a single universal Apple Silicon binary; add a separate `apple-m2` build if you want `i8mm` for NNUE. Note: `vdotq_s32` (DotProd) is stable since Rust 1.98; `vmmlaq_s32` (i8mm) is still unstable, so an i8mm kernel needs inline asm or nightly.

### 2.5 Prefetch the next TT entry (often a top-5 NPS win)

TT probes are random memory accesses; a cache miss costs hundreds of cycles. Prefetch the bucket immediately after computing the key, before the actual probe (and optionally prefetch the child position's TT bucket):

```rust
#[inline(always)]
unsafe fn prefetch_read<T>(p: *const T) {
    #[cfg(target_arch = "x86_64")]
    core::arch::x86_64::_mm_prefetch::<{ core::arch::x86_64::_MM_HINT_T0 }>(p as *const i8);

    // `core::arch::aarch64::_prefetch` is still unstable; use asm on stable.
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!("prfm pldl1keep, [{0}]", in(reg) p, options(nostack, preserves_flags));
}
```

Prefetch is a hint and cannot affect correctness. Measure with and without it; the gain depends on TT size and search depth.

---

## 3. SIMD & Auto-Vectorization for NNUE Evaluation

For an NNUE engine, accumulator updates and the clipped-ReLU/dot-product layers often consume 40–70% of CPU time.

### 3.1 Intrinsics by platform

* **Windows/x86_64:** `std::arch::x86_64` with `__m256i` (AVX2) or `__m512i` (AVX-512). Useful instructions: `_mm256_add_epi16`, `_mm256_sub_epi16`, `_mm256_min_epi16`, `_mm256_max_epi16`, `_mm256_maddubs_epi16`, `_mm256_madd_epi16`. AVX-512 VNNI uses `_mm512_dpbusd_epi32`; AVX-VNNI (256-bit) uses `_mm256_dpbusd_epi32`.
* **macOS/AArch64:** `int16x8_t`/`int8x16_t` NEON; `vdotq_s32` for DotProd; `vpadalq_s16`, `vmaxq_s16`, `vminq_s16`. Apple has multiple 128-bit NEON pipes per core, so 128-bit code can be competitive with 256-bit x86 code. There is **no SVE** on Apple Silicon.
* **Do not use `std::simd` for a production engine yet** — portable SIMD is still nightly-only. The `wide` crate is portable and convenient, but explicit `std::arch` intrinsics are faster when you care about the last few percent.

### 3.2 Alignment: the usual `#[repr(align(64))]` mistake

`#[repr(align(64))]` aligns the **type**, not every `Vec` allocation:

* `Vec<Accumulator>` where `Accumulator` is `#[repr(align(64))]` is fine; the allocator honors `align_of::<Accumulator>()`.
* `Vec<i16>` or `Box<[i16]>` is only 2-byte aligned. Adding `#[repr(align(64))]` to a struct that *contains* a `Vec<i16>` does **not** align the heap buffer.
* Use an aligned wrapper (`#[repr(C, align(64))] struct Align64<T>(T);`), `std::alloc::alloc` with `Layout::from_size_align(..., 64)`, or a custom `AlignedVec`.
* `_mm256_load_si256` requires 32-byte alignment and can fault (`vmovdqa`) if misaligned. `_mm256_loadu_si256` is safe and on modern CPUs is just as fast when the address happens to be aligned. Prefer `loadu` unless the type system proves alignment.
* On AArch64, `vld1q_s16` does **not** require alignment; alignment only helps avoid cache-line splits. The original claim that it "enables aligned NEON loads" is misleading.

### 3.3 Loop shape and auto-vectorization

`chunks_exact(16)` is not a magic vectorization switch. It can generate remainder/scalar code and may inhibit vectorization when combined with `zip`/iterators. More reliable:

* Use fixed-size arrays and `const` generics so the trip count is known (`[i16; HIDDEN]`, `const HIDDEN: usize`).
* Write the inner loop with `(0..N).step_by(16)` and `get_unchecked` in a small `#[inline(always)]` function.
* Keep the kernel in its own `#[target_feature(enable = "...")]` function; do not sprinkle intrinsics through generic code.
* Benchmark on both platforms; LLVM's auto-vectorizer is not a substitute for an explicit AVX2/NEON kernel in NNUE.

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn dot_i8_avx2(a: &[u8; 32], b: &[i8; 32]) -> i32 {
    use core::arch::x86_64::*;
    let va = _mm256_loadu_si256(a.as_ptr() as *const __m256i);
    let vb = _mm256_loadu_si256(b.as_ptr() as *const __m256i);
    let pairs = _mm256_maddubs_epi16(va, vb);
    let sums = _mm256_madd_epi16(pairs, _mm256_set1_epi16(1));
    let sums = _mm256_hadd_epi32(sums, sums);
    let sums = _mm256_hadd_epi32(sums, sums);
    let lo = _mm256_castsi256_si128(sums);
    let hi = _mm256_extracti128_si256(sums, 1);
    _mm_cvtsi128_si32(_mm_add_epi32(lo, hi))
}
```

### 3.4 NNUE-specific structural wins

* **Incremental accumulator updates.** Add/subtract feature columns on make/unmake; do a full refresh only on king moves or when the accumulator is marked dirty. A full refresh per node destroys NPS regardless of SIMD width.
* **Two perspectives.** Keep `acc[2]` (side-to-move and opponent) and combine only at eval time.
* **Quantization.** `i8` weights + `i16` accumulator + `i32` dot product is the standard; `maddubs`/`dpbusd`/`vdotq_s32` map directly to it.
* **No heap in the accumulator path.** `Box<[i16; N]>` or fixed arrays, not `Vec` that can reallocate.
* **Avoid per-node indirect calls.** If you must support multiple SIMD backends in one binary, monomorphize the search over a trait and dispatch once at the root; a `Box<dyn Backend>` call per node is measurable.

---

## 4. Lock-Free Concurrency (Lazy SMP) & Cache Locality

### 4.1 Lock-free TT

Two `AtomicU64` fields per entry, `Ordering::Relaxed`, and an XOR consistency check is the standard design. On x86_64 a relaxed load/store is a plain `mov`; on AArch64 it is `ldr`/`str` without barriers. Do **not** use `SeqCst` or a `Mutex`.

The XOR check detects an **inconsistent key/data pair** written by two threads; each 8-byte access is already atomic, so this is not about tearing a single `u64`.

```rust
use core::sync::atomic::{AtomicU64, Ordering};

#[repr(C, align(16))]
pub struct AtomicTTEntry {
    key: AtomicU64,   // stored as zobrist ^ data
    data: AtomicU64,
}

impl AtomicTTEntry {
    #[inline(always)]
    pub fn store(&self, zobrist: u64, data: u64) {
        self.key.store(zobrist ^ data, Ordering::Relaxed);
        self.data.store(data, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn probe(&self, zobrist: u64) -> Option<u64> {
        let data = self.data.load(Ordering::Relaxed);
        let key = self.key.load(Ordering::Relaxed);
        ((key ^ data) == zobrist).then_some(data)
    }
}
```

Practical TT details that matter more than the atomics:

* **Bucket the entries to cache lines.** Four 16-byte entries = one 64-byte line, or three entries + 16 bytes padding. Use `#[repr(C, align(64))]`. Probe the whole bucket while it is in L1.
* **Prefetch** the bucket after computing the key (Section 2.5).
* **Do not clear a multi-GB TT on `ucinewgame`.** Store a generation/age in each entry; increment the generation and treat old entries as empty. Clearing 16 GB can take seconds and makes `isready` look hung.
* **Allocate with zeroed/lazy pages.** `vec![AtomicU64::new(0); n]` writes every page. Prefer `std::alloc::alloc_zeroed` with a 64-byte-aligned layout, `VirtualAlloc` on Windows, or `mmap` on macOS. On Windows, `VirtualAlloc(MEM_COMMIT)` returns zeroed pages without touching them all; on macOS, anonymous `mmap` is zero-filled lazily.
* **Power-of-two table size** and index with `key as usize & mask` (or a multiply-high mapping). Avoid `%`.
* **`Option<u64>` is 16 bytes**, but returning it from `probe` is usually in registers; don't store millions of them. If it shows up in a profile, return a sentinel or `(u64, bool)`.

### 4.2 False sharing

* x86_64 cache lines are 64 bytes; Apple Silicon uses **128-byte** cache lines (confirmed: `hw.cachelinesize = 128` on M4).
* Align per-thread data with `#[repr(align(128))]` (or `crossbeam_utils::CachePadded`) so two workers' `nodes`/`seldepth` counters never share a line.
* Keep per-thread history/killer tables private; do not share them behind a `Mutex`. Share only the TT and read-only NNUE weights.

### 4.3 Threads: persistent, not spawned per `go`

`std::thread::scope` is convenient and avoids `Arc` for read-only search parameters, but spawning threads for every `go` costs tens to hundreds of microseconds and can dominate short searches. For Lazy SMP:

* Spawn workers once with `thread::Builder::new().stack_size(16 << 20)`.
* Use an atomic start/stop flag plus a `Condvar`/`Barrier`, or channels, to wake them.
* Keep `Arc<Shared>` for the TT/options; clone it once per worker, not per node.
* A `stop` atomic checked every N nodes is enough; `Ordering::Relaxed` is fine.

### 4.4 Counters and time checks

* **Do not `fetch_add(1)` on a shared atomic per node.** Each worker keeps a local `u64`; publish it to an `AtomicU64` every 1024 nodes (or only when `info` is printed). The main thread sums relaxed loads.
* **Do not call `Instant::now()` per node.** Check the clock every 512–2048 nodes. `Instant` uses `mach_absolute_time` on macOS and QPC on Windows, both fast, but the call and branch still add up.
* Use `core::hint::spin_loop()` in short spin-waits; do not busy-spin for long periods.

---

## 5. Inlining, Cold Paths & Branch Prediction

* **`#[inline(always)]` is not free.** Use it for tiny leaf functions (`Move::from`, `Bitboard::pop_lsb`, a 10-instruction SEE helper). Applying it to large functions (`NNUE::accumulate`, `alpha_beta`) increases code size, pressures the L1 instruction cache, and can make LLVM's inliner worse. Prefer `#[inline]` or let `lto = "fat"` handle cross-module inlining; benchmark both.
* **`#[cold]` belongs on rare paths, not on hot checks.** The original audit suggested marking time-up abort checks `#[cold]`; that is wrong if the check runs every N nodes. Mark the actual abort/return path and UCI/error printing as `#[cold] #[inline(never)]`, and keep the check itself in the hot function.
* **Stable branch hints are limited.** `core::intrinsics::likely`/`unlikely` are nightly. On stable, the pattern is: keep the likely path inline, put the unlikely path in a `#[cold] #[inline(never)]` function.
* **Branchless code is situational.** Modern branch predictors handle predictable chess branches (piece type, castling rights) well. Branchless tricks help only for genuinely unpredictable conditions; they can lengthen dependency chains and hurt. Do not rewrite move generation wholesale; measure with a profiler.
* **`assert_unchecked`/`unreachable_unchecked`** can remove bounds checks and impossible branches, but they are soundness promises. Keep them behind newtype invariants and a benchmark; use `debug_assert!` in debug builds so violations are caught during development.
* **Avoid `unwrap()` in hot paths.** `unwrap` adds a branch and a panic path; `unwrap_unchecked` is available when the invariant is proven. `panic = "abort"` removes landing pads but not the branch.

---

## 6. Compiler & Linker Configuration

### 6.1 Cargo profile

```toml
[profile.release]
opt-level = 3          # test 3 vs 2 on your code: 3 is not always faster
lto = "fat"            # "thin" builds much faster and is often within ~1-3%
codegen-units = 1      # maximum cross-module inlining; slower builds
panic = "abort"        # removes unwind tables/landing pads; disables catch_unwind
strip = "symbols"      # file size only; does NOT improve I-cache locality
overflow-checks = false
debug-assertions = false
incremental = false
```

Corrections to the previous audit:

* `strip = "symbols"` reduces the on-disk binary and symbol table, not the `.text` section or instruction-cache behavior. It is not an NPS optimization.
* `panic = "abort"` is fine for a release engine, but the process aborts on any panic. Keep unwinding in debug builds for diagnostics.
* Default `--release` does not leave "15–30%" on the table from Cargo flags alone; LTO + `codegen-units=1` typically gives a few percent to ~10%. PGO and `target-cpu` add more.

### 6.2 PGO (the biggest missing item in the previous audit)

Profile-guided optimization is usually worth **3–10% NPS**, sometimes more, because it improves branch layout in move generation and search.

```bash
# 1. Instrument
RUSTFLAGS="-C profile-generate=/tmp/pgo-data" cargo build --release

# 2. Run a representative workload
./target/release/engine bench        # or a scripted set of positions/depths

# 3. Merge profiles (llvm-profdata from `rustup component add llvm-tools-preview`)
llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data

# 4. Optimize with the profile
RUSTFLAGS="-C profile-use=/tmp/pgo-data/merged.profdata -C llvm-args=-pgo-warn-missing-function" \
  cargo build --release
```

* Use the **same** `target-cpu`/`target-feature` flags for steps 1 and 4; otherwise the profile and codegen disagree.
* The workload must resemble real games: opening/middlegame/endgame, several depths, not one position.
* `cargo-pgo` automates the loop if you prefer.
* PGO profiles are per-binary; regenerate after significant code changes.

### 6.3 Target-specific builds

**macOS Apple Silicon**

```bash
# Baseline for all Apple Silicon (M1–M5): NEON + DotProd, no i8mm
RUSTFLAGS="-C target-cpu=apple-m1" cargo build --release --target aarch64-apple-darwin

# Faster on M2+: adds i8mm/bf16; NOT safe on M1
RUSTFLAGS="-C target-cpu=apple-m2" cargo build --release --target aarch64-apple-darwin
```

* `apple-m1` is accepted by rustc 1.99.1 even though it may not appear in `--print target-cpus`.
* For a universal macOS binary, also build `x86_64-apple-darwin` and `lipo -create` the two binaries. If you only support Apple Silicon, skip the Intel slice.
* `-C target-cpu=native` is for local benchmarking only; it produces a binary tied to the build machine (on this host, `native` resolves to `apple-m4`, which includes `i8mm` and will crash on M1).

**Windows x86_64**

```bash
# AVX2/BMI1/BMI2/FMA/POPCNT; Haswell/Zen and later
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release --target x86_64-pc-windows-msvc
```

Important limits:

* `x86-64-v3` does **not** include AVX-512 or VNNI.
* `x86-64-v4` adds AVX-512F/BW/CD/DQ/VL, but **not** VNNI. Add `-C target-feature=+avx512vnni` or use `-C target-cpu=znver4`/`sapphirerapids`.
* Intel 12th–14th gen consumer CPUs have AVX-512 disabled; a v4 binary will fault. Ship separate binaries or runtime-detect.
* For modern Intel (12th gen+), `-C target-cpu=alderlake`/`raptorlake`/`meteorlake` enables **AVX-VNNI** (256-bit), which is a good NNUE win without AVX-512.
* For AMD Zen 4/5, `-C target-cpu=znver4`/`znver5` enables AVX-512 VNNI; note that Zen 4 executes 512-bit operations as two 256-bit halves, so measure AVX2 vs AVX-512.
* Some low-power Intel Atom/Celeron parts lack AVX2; keep a `x86-64-v2` or SSE4.2/POPCNT fallback binary if you distribute widely.
* `is_x86_feature_detected!` checks both CPU and OS support (`XCR0`); call it once at startup, never per node.

A practical distribution layout (Stockfish-style):

| Binary | Target | Runs on |
| :--- | :--- | :--- |
| `engine-v2.exe` | `x86-64-v2` | Any x86_64 with SSE4.2/POPCNT |
| `engine-v3.exe` | `x86-64-v3` | Haswell/Zen+ |
| `engine-vnni.exe` | `alderlake` / `+avxvnni` | Intel 12th gen+ |
| `engine-v4.exe` | `x86-64-v4 +avx512vnni` | Zen 4/5, Sapphire Rapids+ |
| `engine-m1` | `apple-m1` | All Apple Silicon |
| `engine-m2` | `apple-m2` | M2+ |

### 6.4 Other build notes

* `-C target-cpu=native` for local A/B tests; never for release artifacts.
* `-C target-feature=+avx2` alone changes features but not scheduling; `target-cpu` is better.
* Linker choice affects build time, not NPS: `lld` on Windows (`-C link-arg=-fuse-ld=lld`), `ld64`/`zld` on macOS.
* `#[global_allocator]` with mimalloc/jemalloc is optional. With no hot-path allocations, the gain is small; it can help NNUE loading and multi-threaded startup. It is not a substitute for zero-allocation search.
* Use `cargo asm`/`cargo llvm-lines`/`cargo bloat` to inspect codegen and find accidental monomorphization/code bloat.

---

## 7. Platform-Specific OS Optimizations

### Windows (`x86_64-pc-windows-msvc`)

1. **Large pages (2 MB) for the TT and NNUE.**
   * `MEM_LARGE_PAGES` requires `SeLockMemoryPrivilege`. Being an administrator is not always enough: grant "Lock pages in memory" in `secpol.msc` → Local Policies → User Rights Assignment, then log off/on.
   * Use `GetLargePageMinimum()` and round the size up to a multiple of it.
   * `VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES, PAGE_READWRITE)`; on failure (`ERROR_PRIVILEGE_NOT_HELD`, 1314), fall back to regular pages.
   * Large pages are non-pageable; a 16 GB TT on a 16 GB machine is a bad idea. Gains are typically a few percent, not a guaranteed 3–8%.
2. **P-core/E-core and processor groups.**
   * `SetThreadAffinityMask` only works for ≤ 64 logical CPUs in one group; use `SetThreadGroupAffinity`/`SetThreadSelectedCpuSets` for larger systems.
   * On Intel hybrid CPUs, `GetLogicalProcessorInformationEx(RelationProcessorCore, ...)` exposes `EfficiencyClass`: 0 = P-core, >0 = E-core. Pin some workers to P-cores; letting Lazy SMP use E-cores can still raise total NPS but may hurt latency-sensitive searches. Measure.
   * Disable EcoQoS/power throttling for the engine process: `SetProcessInformation(..., ProcessPowerThrottling, ...)` with `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` cleared in `StateMask`, and/or per-thread `SetThreadInformation(ThreadPowerThrottling, ...)`. Windows may otherwise treat a GUI-launched engine as background work.
   * `SetThreadPriority(THREAD_PRIORITY_ABOVE_NORMAL)` is usually enough; `HIGHEST` can starve the GUI.
3. **NUMA** (dual-socket): `VirtualAllocExNuma` and `SetThreadGroupAffinity` per node. Single-socket consumer machines do not need this.
4. **Keep the process awake** during long searches with `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`; restore on exit. `timeBeginPeriod(1)` is for timer resolution, not CPU throughput.
5. **Defender exclusion** for the engine directory and NNUE files improves load/startup time; it does not change search NPS.

### macOS (`aarch64-apple-darwin`)

1. **Superpages: correct the previous audit.** `VM_FLAGS_SUPERPAGE_SIZE_2MB` is a macOS **x86_64** feature. Apple Silicon has **no user-space superpage API**; the flag is not supported on arm64. Apple Silicon uses **16 KB base pages** (confirmed: `pagesize = 16384`), which already reduces TLB pressure relative to 4 KB pages. Do not rely on `madvise(MADV_HUGEPAGE)`; that is Linux-only.
2. **QoS classes, not affinity.**
   * At the start of each worker thread, call `pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0)` (value `0x21`). This tells macOS to prefer P-cores for that thread.
   * macOS has **no hard CPU affinity**. `thread_policy_set(THREAD_AFFINITY_POLICY)` is only a hint and is not a pinning mechanism on Apple Silicon. Do not promise deterministic P-core placement.
   * `QOS_CLASS_USER_INITIATED` is often sufficient; `USER_INTERACTIVE` is the most aggressive and can be inappropriate for a background engine.
3. **Feature detection.** `is_aarch64_feature_detected!` returns `false` for most features on macOS. Use `sysctlbyname` (`hw.optional.arm.FEAT_DotProd`, `hw.optional.arm.FEAT_I8MM`) or compile-time `target-cpu`.
4. **App Nap** generally does not throttle a command-line process, but a bundled/app-launched engine may be affected. `NSProcessInfo beginActivityWithOptions:` (via `objc2`) can prevent it; measure before adding the dependency.
5. **`mlock`/wiring** for TT/NNUE can reduce page-in latency but is subject to `RLIMIT_MEMLOCK` and can fail; treat it as best-effort.
6. **UCI output.** `println!` takes the stdout lock per call, and a GUI that stops reading the pipe can block the search. Use a dedicated writer thread with a bounded channel; drop/coalesce low-priority `info currmove` lines instead of blocking. Build each line once with `write!` into a reused `String`, then `write_all` + `flush` under `StdoutLock`.

### Both platforms: UCI I/O is not the hot path, but it can stall it

A search that writes `info` from inside the search can block on a full pipe. The writer-thread pattern isolates search from GUI back-pressure:

```rust
enum Out { Info(String), BestMove(String) }

let (tx, rx) = std::sync::mpsc::sync_channel::<Out>(1024);
std::thread::spawn(move || {
    use std::io::Write;
    while let Ok(msg) = rx.recv() {
        let mut out = std::io::stdout().lock();
        match msg {
            Out::Info(s) | Out::BestMove(s) => {
                let _ = out.write_all(s.as_bytes());
                let _ = out.write_all(b"\n");
                let _ = out.flush();
            }
        }
    }
});
// Search: tx.try_send(...); if full, drop optional info, never block.
```

---

## 8. Summary Checklist for Maximum Engine NPS

| Layer | Rust feature / technique | Primary benefit |
| :--- | :--- | :--- |
| **Measurement** | Fixed benchmark set, `hyperfine`/`samply`/Instruments/VTune, `cargo asm` | Prevents optimizing the wrong thing |
| **Memory** | Stack move lists, `Copy` `u16` moves, `Move::NULL` sentinel | Zero heap allocations in the search tree |
| **Bitboards** | `trailing_zeros`, `count_ones`, `wrapping_sub`, PEXT or magics | Single-cycle bit manipulation; avoid bounds/overflow checks |
| **TT** | Relaxed `AtomicU64` pairs, 64-byte buckets, prefetch, generation/age | Lock-free sharing without TLB/cache stalls or multi-GB clears |
| **NNUE SIMD** | AVX2/AVX-VNNI/AVX-512 on Windows; NEON/DotProd on macOS | 2–8× faster accumulator/eval kernels in practice |
| **Alignment** | `#[repr(align(64))]` on the *type*, aligned allocator, `loadu` | Avoids split loads and misaligned-load faults |
| **Lazy SMP** | Persistent threads, `#[repr(align(128))]` thread data, local counters | No spawn cost, no false sharing, no atomic per node |
| **Compiler** | `lto="fat"`, `codegen-units=1`, `target-cpu`, **PGO** | Cross-module inlining + profile-guided branch layout |
| **OS memory** | Windows large pages (privilege required); macOS 16 KB pages, no superpage API on ARM | Fewer TLB misses; correct platform expectations |
| **OS scheduling** | Windows P-core affinity/EcoQoS off; macOS QoS classes | Keeps workers on fast cores |
| **UCI I/O** | Dedicated writer thread, bounded queue | GUI back-pressure cannot stall search |

---

## 9. Notes for the C#/.NET → Rust Migration

Since this engine is moving from C#/.NET 11 to pure Rust, the biggest mindset changes are:

* **No JIT warmup, no tiered compilation.** Rust is AOT; the first search is already optimized. PGO replaces .NET's dynamic PGO/tiered PGO, but must be done at build time.
* **No GC.** Allocation is a direct allocator call; there are no gen0 pauses to hide it. This is why zero-allocation search matters more, not less.
* **`struct` vs `class`.** Use `Copy` structs for moves/squares/bitboards; avoid `Box`/`Rc` in hot data. A Rust `enum` is a value type, not a heap object.
* **`Span<T>`/`Memory<T>` → slices `&[T]`/`&mut [T]`.** Bounds checks exist in both; the same techniques (newtypes, `get_unchecked`, proof to the optimizer) apply.
* **`[MethodImpl(AggressiveInlining)]` → `#[inline(always)]`**, with the same caveat: small leaf functions only.
* **`[StructLayout(LayoutKind.Sequential)]` → `#[repr(C)]`**; .NET `Unsafe.As`/`fixed` → Rust `unsafe`/raw pointers.
* **`System.Numerics.Vector256` → `std::arch`** for maximum control; .NET's `Vector<T>` is closer to Rust's nightly `std::simd` (portable, not always optimal).
* **`ArrayPool<T>` → fixed arrays/`ArrayVec`**; no need to return buffers.
* **`ThreadPool` is not suitable for Lazy SMP.** Use persistent `std::thread` workers with explicit start/stop signaling.
* **`BenchmarkDotNet` → `criterion`/`hyperfine`**; for whole-engine NPS, use a fixed-depth UCI `bench` command and multiple runs.
* **`GC.TryStartNoGCRegion`/latency modes are gone.** The equivalent discipline is: no allocations, no locks, no syscalls in the search.

---

## Appendix: Corrections to the Previous Revision

| Previous claim | Correction |
| :--- | :--- |
| `get_unchecked` boosts NPS by 5–15% | Usually 0–3%; LLVM already elides many checks. Measure; use `assert_unchecked`/newtypes. |
| `#[repr(align(64))]` enables aligned SIMD loads | Only if the allocation itself is aligned. `Vec<i16>` is 2-byte aligned; use an `Align64` wrapper or `loadu`. |
| `vld1q_s16` benefits from alignment | NEON loads are unaligned-safe; alignment only avoids cache-line splits. |
| `strip = "symbols"` improves instruction-cache locality | False. It reduces file/symbol-table size, not `.text`. |
| macOS Apple Silicon can use `VM_FLAGS_SUPERPAGE_SIZE_2MB` | Only macOS **x86_64**; Apple Silicon has no user-space superpage API and uses 16 KB pages. |
| `apple-m1` gives full speed on M1–M4 | It is a safe baseline, but misses `i8mm`/`bf16` on M2+. Use `apple-m2` for an i8mm build. |
| `x86-64-v3` is enough for AVX-512/VNNI | v3 has AVX2/BMI2/FMA but no AVX-512/VNNI. v4 has AVX-512 but no VNNI. |
| `cfg(target_feature = "...")` can drive runtime dispatch | It is compile-time only. Use `is_x86_feature_detected!` or separate binaries. |
| `is_aarch64_feature_detected!` works on macOS | It returns `false` for most features outside Linux; use `sysctl` or compile-time features. |
| Branchless code avoids pipeline flushes | Only for unpredictable branches; branchless code can be slower and lengthen dependency chains. |
| `#[inline(always)]` on every hot function | Can bloat code and hurt I-cache; use it on small leaves, let LTO handle the rest. |
| Mark time-up abort checks `#[cold]` | Mark the rare abort/return path, not the hot check that runs every N nodes. |
| XOR trick detects torn reads | Each `u64` is atomic; the XOR detects an inconsistent key/data pair. |
| PEXT is always a single fast instruction | Zen 1/2 microcode it; PEXT vs magic is CPU-dependent. Benchmark. |
| Large pages give an immediate 3–8% | Often a few percent; Windows requires `SeLockMemoryPrivilege` and a size multiple of `GetLargePageMinimum()`. |
| Default release leaves 15–30% from Cargo flags | Flags alone are usually a few percent to ~10%; PGO + target-cpu add more. |
| `const fn` tables are automatically in `.rodata` | Use `static`, not `const`, for large tables; `const` may be duplicated at use sites. |
| `Option<u64>` is cheap | It is 16 bytes; `Option<Move>` is 4 bytes. Use sentinels/niches where it affects cache footprint. |
| `std::thread::scope` is ideal for Lazy SMP | It avoids `Arc`, but per-`go` spawning costs time; use persistent workers. |
| `std::simd` is ready for NNUE | Still nightly-only; use `std::arch` intrinsics for production. |

---

**Bottom line:** the largest wins for a Rust engine come from (1) no allocations/locks/syscalls in search, (2) incremental NNUE updates with explicit SIMD, (3) a prefetched, generation-based TT, (4) PGO + a correct `target-cpu` build for each CPU class, and (5) persistent Lazy SMP workers with no false sharing. Micro-optimizations like `get_unchecked` or `strip` are noise next to those.
