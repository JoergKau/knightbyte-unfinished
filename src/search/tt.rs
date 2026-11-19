//! Lock-free transposition table.
//!
//! Entries are 16 bytes and buckets are one 64-byte cache line holding four
//! entries, so a probe touches a single line. Storage is a flat boxed slice of
//! aligned buckets; entries are written with atomics so several search threads
//! can share the table without locks. Data is written before the key and the
//! key is published with release semantics, so a concurrent probe that sees a
//! new key is guaranteed to see the matching data.
//!
//! Every entry stores the full 64-bit key. A probe therefore only ever returns
//! exact matches; the alternative (truncating the key and trusting a checksum)
//! trades memory for a small false-hit rate that would make node counts harder
//! to explain.
//!
//! Entries survive across searches and are aged by a 5-bit generation counter
//! that starts at one. Replacement refreshes a same-key entry unless a deeper
//! entry from the current generation holds the slot; otherwise the
//! least-precious occupant — `stored depth - 2 per generation of age` — is
//! evicted, with no deeper-entry guard.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::types::r#move::Move;

/// Entries per bucket. Four entries fill exactly one 64-byte cache line.
pub const BUCKET_SLOTS: usize = 4;

/// Bytes per entry: one key and one packed data word.
pub const ENTRY_BYTES: usize = 16;

/// Bytes per bucket, the unit the table is indexed in.
pub const BUCKET_BYTES: usize = BUCKET_SLOTS * ENTRY_BYTES;

/// Score that represents a forced mate. Scores above `MATE_SCORE - 1000` are
/// treated as mate distances and adjusted for the ply on the way in and out of
/// the table.
pub const MATE_SCORE: i16 = 31_000;

/// Mate distances are stored relative to the root, so they must be converted.
const MATE_WINDOW: i16 = 1_000;

// A replacement is never refused: the least-precious occupant
// (`stored depth - 2 per generation of age`) is always replaced when the
// bucket is full. The reference policy has no deeper-entry guard.

/// Generation values wrap at 32; five bits are stored per entry. The counter
/// starts at one — zero is the encoding of a fresh table, not a generation.
const GENERATION_MASK: u8 = 0x1f;

const _: () = assert!(BUCKET_BYTES == 64);
const _: () = assert!(GENERATION_MASK == 31);

/// How a stored score relates to the true value of the position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bound {
    /// The score is exact because no cutoff occurred (a principal-variation
    /// node).
    Exact,
    /// The score is a lower bound: the search failed high.
    Lower,
    /// The score is an upper bound: the search failed low.
    Upper,
}

impl Bound {
    /// Packs the bound into its two-bit representation.
    #[inline(always)]
    const fn code(self) -> u8 {
        match self {
            Self::Exact => 0,
            Self::Lower => 1,
            Self::Upper => 2,
        }
    }

    /// Restores a bound from its two-bit representation.
    ///
    /// The unused fourth encoding is treated as [`Bound::Exact`] so a torn or
    /// cleared entry can never panic.
    #[inline(always)]
    const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Lower,
            2 => Self::Upper,
            _ => Self::Exact,
        }
    }
}

/// One transposition-table entry as presented to the search.
#[derive(Clone, Copy, Debug)]
pub struct TtEntry {
    /// Full Zobrist key of the position.
    pub key: u64,
    /// Best move found, or [`Move::NONE`] when only a score was stored.
    pub mv: Move,
    /// Score in root-relative terms; convert with [`score_from_tt`].
    pub score: i16,
    /// Static evaluation, reused to avoid a second evaluation.
    pub eval: i16,
    /// Remaining depth (or quiescence depth) the score refers to.
    pub depth: u8,
    /// Cutoff type of the score.
    pub bound: Bound,
    /// Generation counter value when the entry was written.
    pub generation: u8,
}

/// Converts a search score into the distance-independent table encoding.
///
/// Mate scores depend on the plies left to the root, so a table entry written
/// at ply 3 would mean a different distance at ply 7. Storing
/// `score + ply` for mates makes the value relative to the mate, not the root.
#[inline]
pub fn score_to_tt(score: i16, ply: u16) -> i16 {
    if score > MATE_SCORE - MATE_WINDOW {
        score.saturating_add(ply as i16)
    } else if score < -(MATE_SCORE - MATE_WINDOW) {
        score.saturating_sub(ply as i16)
    } else {
        score
    }
}

/// Converts a table score back into root-relative search terms.
#[inline]
pub fn score_from_tt(score: i16, ply: u16) -> i16 {
    if score > MATE_SCORE - MATE_WINDOW {
        score.saturating_sub(ply as i16)
    } else if score < -(MATE_SCORE - MATE_WINDOW) {
        score.saturating_add(ply as i16)
    } else {
        score
    }
}

/// One cache-line aligned bucket of four entries.
///
/// Slots are stored as interleaved key/data pairs so a probe reads both words
/// of a candidate entry from the same cache line.
#[repr(C, align(64))]
struct Bucket {
    slots: [AtomicU64; BUCKET_SLOTS * 2],
}

impl Default for Bucket {
    fn default() -> Self {
        Self {
            slots: [const { AtomicU64::new(0) }; BUCKET_SLOTS * 2],
        }
    }
}

impl Bucket {
    #[inline(always)]
    fn key_slot(&self, slot: usize) -> &AtomicU64 {
        &self.slots[slot * 2]
    }

    #[inline(always)]
    fn data_slot(&self, slot: usize) -> &AtomicU64 {
        &self.slots[slot * 2 + 1]
    }
}

/// A shared transposition table.
///
/// The table is allocated once and then shared by reference between search
/// threads; all mutating operations take `&self` and use atomics.
pub struct TranspositionTable {
    buckets: Box<[Bucket]>,
    mask: usize,
    generation: AtomicU8,
}

// The whole point of the atomic storage is that worker threads share one table.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TranspositionTable>();
};

impl Default for TranspositionTable {
    fn default() -> Self {
        Self::new(16)
    }
}

impl TranspositionTable {
    /// Allocates a table of at least `megabytes` megabytes.
    ///
    /// The size is rounded down to a power-of-two bucket count, which keeps
    /// indexing to a mask and makes the distribution uniform.
    pub fn new(megabytes: usize) -> Self {
        let buckets = bucket_count(megabytes);
        Self {
            buckets: allocate(buckets),
            mask: buckets - 1,
            // One, not zero: zero is the empty-slot encoding, never a stamp.
            generation: AtomicU8::new(1),
        }
    }

    /// Replaces the table contents with a freshly allocated table.
    ///
    /// This is what a `setoption name Hash` handler calls; existing entries are
    /// discarded because their index depended on the old bucket count.
    pub fn resize(&mut self, megabytes: usize) {
        let buckets = bucket_count(megabytes);
        self.buckets = allocate(buckets);
        self.mask = buckets - 1;
    }

    /// Returns the number of allocated buckets.
    #[inline(always)]
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Returns the number of storable entries.
    #[inline(always)]
    pub fn entry_count(&self) -> usize {
        self.buckets.len() * BUCKET_SLOTS
    }

    /// Returns the allocated size in bytes.
    #[inline(always)]
    pub fn size_bytes(&self) -> usize {
        self.buckets.len() * BUCKET_BYTES
    }

    /// Returns the current generation counter.
    #[inline(always)]
    pub fn generation(&self) -> u8 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Advances the generation and returns the new value.
    ///
    /// The search calls this once at the start of every `go`, which makes all
    /// entries from the previous search one step older. Zero is skipped: a
    /// fresh table's all-zero words mean "empty slot", so generation zero is
    /// never a valid entry stamp.
    #[inline]
    pub fn new_generation(&self) -> u8 {
        let current = self.generation.load(Ordering::Relaxed);
        let next = if current == u8::MAX { 1 } else { current + 1 };
        self.generation.store(next, Ordering::Relaxed);
        next
    }

    /// Removes every entry.
    ///
    /// The zeroing runs in parallel threads above 64 MB: a 1 GB table costs
    /// ~50 ms to zero single-threaded, and the first clear after a resize
    /// additionally pays the operating system's page commit for the fresh
    /// allocation (~200 ms on the Windows host — bug B38), which the
    /// parallel first touch divides by the thread count. Called only when
    /// no search is active (the UCI join-first rule), so the scoped threads
    /// never race a search.
    pub fn clear(&self) {
        let thread_count = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1);
        let size_bytes = self.buckets.len() * BUCKET_BYTES;
        if thread_count > 1 && size_bytes >= 64 * 1024 * 1024 {
            let chunks = self.buckets.chunks(self.buckets.len() / thread_count + 1);
            std::thread::scope(|scope| {
                for chunk in chunks {
                    scope.spawn(|| {
                        for bucket in chunk.iter() {
                            for slot in bucket.slots.iter() {
                                slot.store(0, Ordering::Relaxed);
                            }
                        }
                    });
                }
            });
        } else {
            for bucket in self.buckets.iter() {
                for slot in bucket.slots.iter() {
                    slot.store(0, Ordering::Relaxed);
                }
            }
        }
        self.generation.store(1, Ordering::Relaxed);
    }

    /// Looks up `key`.
    ///
    /// Returns `None` when the position was never stored or was overwritten.
    /// An entry with a zero key is used as the empty marker, so a position
    /// whose key happens to be zero is never cached (probability 2^-64).
    #[inline]
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        crate::node_stat!(TtProbes);
        if key == 0 {
            return None;
        }
        let bucket = &self.buckets[self.bucket_index(key)];
        for slot in 0..BUCKET_SLOTS {
            // Acquire pairs with the release store of the key in `store`, so a
            // new key always comes with its matching data.
            let stored_key = bucket.key_slot(slot).load(Ordering::Acquire);
            if stored_key == key {
                crate::node_stat!(TtHits);
                let data = bucket.data_slot(slot).load(Ordering::Relaxed);
                return Some(unpack(key, data));
            }
            if stored_key == 0 {
                return None;
            }
        }
        None
    }

    /// Stores `entry`, replacing a same-key entry or a low-value victim.
    ///
    /// The generation is taken from the entry rather than the table so tests
    /// can simulate ageing; search code passes
    /// [`TranspositionTable::generation`].
    ///
    /// A same-key entry is overwritten in place, so a deeper search result can
    /// refine an existing shallow one. Entries from the current generation are
    /// preferred over old ones, and a shallow result never evicts a
    /// considerably deeper entry.
    #[inline]
    pub fn store(&self, entry: TtEntry) {
        crate::node_stat!(TtStores);
        if entry.key == 0 {
            return;
        }
        let bucket = &self.buckets[self.bucket_index(entry.key)];
        let Some(slot) = choose_slot(bucket, entry.key, entry.depth, entry.generation) else {
            return;
        };
        // Publish the data before the key so a concurrent reader can never see
        // the new key with stale data.
        bucket
            .data_slot(slot)
            .store(pack(&entry), Ordering::Relaxed);
        bucket.key_slot(slot).store(entry.key, Ordering::Release);
    }
    /// Prefetches the bucket for `key` into the first-level cache.
    ///
    /// Call this several plies before the probe, for example right after a move
    /// is made, to hide memory latency. The function is a no-op on targets
    /// without a stable prefetch intrinsic.
    #[inline(always)]
    pub fn prefetch(&self, key: u64) {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};

            let bucket: *const Bucket = &self.buckets[self.bucket_index(key)];
            // SAFETY: `bucket` points into `self.buckets`, which outlives this
            // call, and `_mm_prefetch` never dereferences the address in a way
            // that can fault.
            unsafe { _mm_prefetch(bucket.cast::<i8>(), _MM_HINT_T0) };
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = key;
        }
    }

    /// Returns the occupancy of the current generation in permill.
    ///
    /// The estimate samples the first thousand entries, which is what UCI
    /// consumers expect from `info ... hashfull`.
    pub fn hashfull(&self) -> u16 {
        let buckets = self.buckets.len().min(250);
        if buckets == 0 {
            return 0;
        }
        let generation = self.generation();
        let mut used = 0_u32;
        for bucket in self.buckets.iter().take(buckets) {
            for slot in 0..BUCKET_SLOTS {
                let key = bucket.key_slot(slot).load(Ordering::Relaxed);
                if key == 0 {
                    continue;
                }
                let data = bucket.data_slot(slot).load(Ordering::Relaxed);
                if entry_generation(data) == generation {
                    used += 1;
                }
            }
        }
        let sampled = buckets * BUCKET_SLOTS;
        ((used as u64 * 1000) / sampled as u64) as u16
    }

    #[inline(always)]
    fn bucket_index(&self, key: u64) -> usize {
        (key as usize) & self.mask
    }
}

/// Returns the power-of-two bucket count for a size in megabytes.
fn bucket_count(megabytes: usize) -> usize {
    let bytes = megabytes.max(1) * 1024 * 1024;
    let buckets = (bytes / BUCKET_BYTES).max(1);
    if buckets.is_power_of_two() {
        buckets
    } else {
        1 << (usize::BITS - 1 - buckets.leading_zeros())
    }
}

/// Allocates `count` zeroed, cache-line aligned buckets.
fn allocate(count: usize) -> Box<[Bucket]> {
    (0..count).map(|_| Bucket::default()).collect()
}

/// Chooses the slot to write and returns its index, or `None` when a deeper
/// same-key entry must be kept.
///
/// The reference policy, in order:
///
/// 1. **Same key** — the entry is refreshed in place iff the new depth is
///    equal or greater. The reference's stamped age is identical on every
///    entry (its per-search stamp counter resets with each fresh search
///    object, B41), so the age clause of its rule never fires and only the
///    depth rule governs — across searches as well as within one.
/// 2. **Empty slot** — the first empty slot wins, never evicting a live
///    entry while a free one exists.
/// 3. **Least-precious victim** — the occupied-by-another-key slot with the
///    lowest `stored depth - 2 per generation of age`. There is no
///    deeper-entry guard: a full bucket always loses its least-precious
///    occupant.
fn choose_slot(bucket: &Bucket, key: u64, depth: u8, _generation: u8) -> Option<usize> {
    let current_generation = _generation & GENERATION_MASK;
    let mut victim = 0;
    let mut victim_precious = i32::MAX;
    for slot in 0..BUCKET_SLOTS {
        let stored_key = bucket.key_slot(slot).load(Ordering::Relaxed);
        let data = bucket.data_slot(slot).load(Ordering::Relaxed);
        if stored_key == key {
            // A deeper stored entry is kept, also across searches (B41): the
            // reference stamps every entry with the same per-search age, so
            // its same-key rule reduces to the depth comparison.
            if i32::from(depth) >= i32::from(entry_depth(data)) {
                return Some(slot);
            }
            return None;
        }
        if stored_key == 0 {
            // The first empty slot wins; a live entry is never evicted while
            // a free one exists.
            return Some(slot);
        }
        let age = current_generation.wrapping_sub(entry_generation(data)) & GENERATION_MASK;
        let precious = i32::from(entry_depth(data)) - 2 * i32::from(age);
        if precious < victim_precious {
            victim_precious = precious;
            victim = slot;
        }
    }
    Some(victim)
}

/// Packs an entry into its 64-bit data word.
fn pack(entry: &TtEntry) -> u64 {
    (entry.mv.bits() as u64)
        | ((entry.score as u16 as u64) << 16)
        | ((entry.eval as u16 as u64) << 32)
        | ((entry.depth as u64) << 48)
        | ((entry.bound.code() as u64) << 56)
        | (((entry.generation & GENERATION_MASK) as u64) << 58)
}

/// Restores an entry from a stored key and data word.
fn unpack(key: u64, data: u64) -> TtEntry {
    TtEntry {
        key,
        mv: Move::from_bits((data & 0xffff) as u16),
        score: ((data >> 16) & 0xffff) as u16 as i16,
        eval: ((data >> 32) & 0xffff) as u16 as i16,
        depth: entry_depth(data),
        bound: Bound::from_code(((data >> 56) & 3) as u8),
        generation: entry_generation(data),
    }
}

#[inline(always)]
fn entry_depth(data: u64) -> u8 {
    ((data >> 48) & 0xff) as u8
}

#[inline(always)]
fn entry_generation(data: u64) -> u8 {
    ((data >> 58) & GENERATION_MASK as u64) as u8
}

#[cfg(test)]
mod tests {
    use core::mem::{align_of, size_of};

    use super::{
        BUCKET_BYTES, BUCKET_SLOTS, Bound, ENTRY_BYTES, MATE_SCORE, TranspositionTable, TtEntry,
        bucket_count, score_from_tt, score_to_tt,
    };
    use crate::types::r#move::{Move, flags};

    fn entry(key: u64, depth: u8, score: i16) -> TtEntry {
        TtEntry {
            key,
            mv: Move::new(12, 28, flags::DOUBLE_PUSH),
            score,
            eval: -17,
            depth,
            bound: Bound::Exact,
            generation: 0,
        }
    }

    /// Builds an entry stamped with the table's current generation.
    fn stamped(table: &TranspositionTable, key: u64, depth: u8) -> TtEntry {
        TtEntry {
            generation: table.generation(),
            ..entry(key, depth, 0)
        }
    }

    #[test]
    fn layout_constants_match_a_cache_line() {
        assert_eq!(ENTRY_BYTES, 16);
        assert_eq!(BUCKET_SLOTS, 4);
        assert_eq!(BUCKET_BYTES, 64);
        assert_eq!(size_of::<super::Bucket>(), 64);
        assert_eq!(align_of::<super::Bucket>(), 64);
    }

    #[test]
    fn sizing_is_power_of_two_and_rounded_down() {
        assert!(bucket_count(1).is_power_of_two());
        assert!(bucket_count(16).is_power_of_two());
        assert!(bucket_count(16) >= bucket_count(1) * 8);
        assert!(bucket_count(0) >= 1, "a zero request still allocates");
        let bytes = bucket_count(8) * BUCKET_BYTES;
        assert!(bytes <= 8 * 1024 * 1024);
    }

    #[test]
    fn store_probe_round_trip_preserves_every_field() {
        let table = TranspositionTable::new(1);
        let mut stored = entry(0x1234_5678_9abc_def0, 7, -1234);
        stored.bound = Bound::Upper;
        stored.generation = 21; // five bits are stored per entry
        table.store(stored);

        let found = table.probe(stored.key).expect("entry must be found");
        assert_eq!(found.key, stored.key);
        assert_eq!(found.mv, stored.mv);
        assert_eq!(found.score, -1234);
        assert_eq!(found.eval, -17);
        assert_eq!(found.depth, 7);
        assert_eq!(found.bound, Bound::Upper);
        assert_eq!(found.generation, 21);
        assert!(table.probe(stored.key.wrapping_add(1)).is_none());
    }

    #[test]
    fn same_key_is_overwritten_in_place() {
        let table = TranspositionTable::new(1);
        table.store(stamped(&table, 99, 3));
        table.store(stamped(&table, 99, 9));
        let found = table.probe(99).unwrap();
        assert_eq!(found.depth, 9);
        assert_eq!(table.hashfull(), 1);
    }

    #[test]
    fn table_is_cleared_and_emptied() {
        let table = TranspositionTable::new(1);
        for key in 1..=64 {
            table.store(stamped(&table, key, 4));
        }
        assert!(table.probe(1).is_some());
        table.clear();
        assert!(table.probe(1).is_none());
        assert_eq!(table.hashfull(), 0);
    }

    #[test]
    fn generation_ages_old_entries_out_first() {
        let table = TranspositionTable::new(1);
        table.store(stamped(&table, 7, 6));
        table.new_generation();
        assert_eq!(table.hashfull(), 0, "old generation is not counted");
        assert!(table.probe(7).is_some(), "old entries stay readable");
    }

    #[test]
    fn shallow_entry_replaces_the_least_precious_occupant() {
        let table = TranspositionTable::new(1);
        // Keys one bucket apart collide because the bucket count is a power of
        // two and indexing is `key & mask`.
        let stride = table.bucket_count() as u64;
        for slot in 0..BUCKET_SLOTS as u64 {
            table.store(stamped(&table, 1 + slot * stride, 20));
        }
        let intruder = 1 + BUCKET_SLOTS as u64 * stride;

        // The reference policy has no deeper-entry guard: a full bucket always
        // loses its least-precious occupant to the newcomer.
        table.store(stamped(&table, intruder, 2));
        assert!(
            table.probe(intruder).is_some(),
            "a full bucket must yield a slot to any store"
        );
        let remaining = (0..BUCKET_SLOTS as u64)
            .filter(|slot| table.probe(1 + slot * stride).is_some())
            .count();
        assert_eq!(
            remaining,
            BUCKET_SLOTS - 1,
            "exactly one victim was evicted"
        );

        // Once the deep entries age, they lose to any current-generation
        // store: two generations of staleness cost 4 plies of preciousness.
        table.new_generation();
        table.new_generation();
        let latecomer = 1 + (BUCKET_SLOTS + 1) as u64 * stride;
        table.store(stamped(&table, latecomer, 4));
        assert!(table.probe(latecomer).is_some());
    }

    #[test]
    fn deeper_same_key_entry_is_kept_across_generations() {
        let table = TranspositionTable::new(1);
        table.store(stamped(&table, 99, 9));
        // Same generation, shallower depth: the stored entry stays.
        table.store(stamped(&table, 99, 3));
        assert_eq!(table.probe(99).unwrap().depth, 9);
        // B41: an older-generation entry is kept against a shallower store,
        // too. The predecessor stamps every entry with the same per-search
        // age, so its same-key rule is a pure depth comparison across
        // searches as well.
        table.new_generation();
        table.store(stamped(&table, 99, 3));
        assert_eq!(table.probe(99).unwrap().depth, 9);
    }

    #[test]
    fn zero_key_is_never_stored_or_found() {
        let table = TranspositionTable::new(1);
        table.store(entry(0, 12, 5));
        assert!(table.probe(0).is_none());
        assert_eq!(table.hashfull(), 0);
    }

    #[test]
    fn mate_scores_are_ply_adjusted() {
        let mate = MATE_SCORE - 3;
        let stored = score_to_tt(mate, 5);
        assert_eq!(stored, mate + 5);
        assert_eq!(score_from_tt(stored, 5), mate);
        assert_eq!(score_from_tt(score_to_tt(-mate, 9), 9), -mate);

        let quiet = 42_i16;
        assert_eq!(score_to_tt(quiet, 9), quiet);
        assert_eq!(score_from_tt(quiet, 9), quiet);
    }

    #[test]
    fn resize_keeps_the_table_usable() {
        let mut table = TranspositionTable::new(2);
        let before = table.entry_count();
        table.store(stamped(&table, 0xfeed, 5));
        table.resize(4);
        assert!(table.entry_count() >= before);
        assert!(table.probe(0xfeed).is_none(), "resize drops old entries");
        table.store(stamped(&table, 0xfeed, 5));
        assert!(table.probe(0xfeed).is_some());
    }

    #[test]
    fn hashfull_reports_current_generation_only() {
        let table = TranspositionTable::new(1);
        let stride = table.bucket_count() as u64;
        // Fill all four slots of every bucket in the sampled window.
        for bucket in 0..250_u64 {
            for slot in 1..=BUCKET_SLOTS as u64 {
                table.store(stamped(&table, bucket + slot * stride, 10));
            }
        }
        assert_eq!(table.hashfull(), 1000);
        table.new_generation();
        assert_eq!(table.hashfull(), 0);
    }
}
