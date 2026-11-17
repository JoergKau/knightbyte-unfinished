//! Direct-mapped cache for static evaluations.
//!
//! The evaluation is the most expensive leaf operation, and transpositions
//! reach the same position through different move orders. Caching the static
//! value per position key avoids evaluating the network twice for the same
//! board.
//!
//! Entries are eight bytes and are stored as a single atomic word: sixteen key
//! bits for verification, the score, and a six-bit generation used for
//! replacement. A false hit cannot corrupt the search tree, only the ordering
//! inside it, because the cached value is a score and not a bound.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Bytes per cache entry.
pub const ENTRY_BYTES: usize = 8;

/// Key bits kept for verification.
const KEY_BITS: u32 = 16;

/// Generation values wrap at 64; six bits are stored per entry.
const GENERATION_MASK: u8 = 0x3f;

/// A small direct-mapped evaluation cache.
pub struct EvalCache {
    entries: Box<[AtomicU64]>,
    mask: usize,
    generation: AtomicU8,
}

// Shared between search threads the same way the transposition table is.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EvalCache>();
};

impl Default for EvalCache {
    fn default() -> Self {
        Self::new(1)
    }
}

impl EvalCache {
    /// Allocates a cache of at least `megabytes` megabytes.
    ///
    /// The entry count is rounded down to a power of two so indexing needs only
    /// a mask.
    pub fn new(megabytes: usize) -> Self {
        let capacity = entry_count(megabytes);
        Self {
            entries: allocate(capacity),
            mask: capacity - 1,
            generation: AtomicU8::new(0),
        }
    }

    /// Reallocates the cache, dropping every entry.
    pub fn resize(&mut self, megabytes: usize) {
        let capacity = entry_count(megabytes);
        self.entries = allocate(capacity);
        self.mask = capacity - 1;
    }

    /// Returns the number of entries the cache can hold.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.entries.len()
    }

    /// Returns the allocated size in bytes.
    #[inline(always)]
    pub fn size_bytes(&self) -> usize {
        self.entries.len() * ENTRY_BYTES
    }

    /// Returns the current generation counter.
    #[inline(always)]
    pub fn generation(&self) -> u8 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Advances the generation and returns the new value.
    #[inline]
    pub fn new_generation(&self) -> u8 {
        let next = self.generation.load(Ordering::Relaxed).wrapping_add(1) & GENERATION_MASK;
        self.generation.store(next, Ordering::Relaxed);
        next
    }

    /// Removes every entry.
    pub fn clear(&self) {
        for entry in self.entries.iter() {
            entry.store(0, Ordering::Relaxed);
        }
        self.generation.store(0, Ordering::Relaxed);
    }

    /// Returns the cached static evaluation for `key`.
    ///
    /// The generation is deliberately not part of the hit condition: an
    /// evaluation never becomes wrong, so entries stay usable across searches.
    #[inline]
    pub fn probe(&self, key: u64) -> Option<i16> {
        if key == 0 {
            return None;
        }
        let stored = self.entries[self.index(key)].load(Ordering::Relaxed);
        if stored == 0 {
            // Zero marks an empty slot; a real entry with an all-zero score and
            // generation is not worth caching precisely.
            return None;
        }
        if stored_key(stored) == key_bits(key) {
            Some(entry_score(stored))
        } else {
            None
        }
    }

    /// Caches the static evaluation `score` for `key`.
    ///
    /// A slot occupied by an entry of the current generation is kept, because
    /// that value is the most likely to be reused; everything else is
    /// overwritten.
    #[inline]
    pub fn store(&self, key: u64, score: i16) {
        if key == 0 {
            return;
        }
        let index = self.index(key);
        let slot = &self.entries[index];
        let stored = slot.load(Ordering::Relaxed);
        if stored != 0 && entry_generation(stored) == self.generation() {
            return;
        }
        slot.store(pack(key, score, self.generation()), Ordering::Relaxed);
    }

    #[inline(always)]
    fn index(&self, key: u64) -> usize {
        (key as usize) & self.mask
    }
}

/// Returns the power-of-two entry count for a size in megabytes.
fn entry_count(megabytes: usize) -> usize {
    let bytes = megabytes.max(1) * 1024 * 1024;
    let entries = (bytes / ENTRY_BYTES).max(1);
    if entries.is_power_of_two() {
        entries
    } else {
        1 << (usize::BITS - 1 - entries.leading_zeros())
    }
}

/// Allocates a zeroed entry array.
fn allocate(count: usize) -> Box<[AtomicU64]> {
    (0..count).map(|_| AtomicU64::new(0)).collect()
}

/// Packs one cache entry into its 64-bit word.
fn pack(key: u64, score: i16, generation: u8) -> u64 {
    (key_bits(key) as u64)
        | ((score as u16 as u64) << KEY_BITS)
        | (((generation & GENERATION_MASK) as u64) << 32)
}

#[inline(always)]
fn key_bits(key: u64) -> u16 {
    (key >> (64 - KEY_BITS)) as u16
}

#[inline(always)]
fn stored_key(stored: u64) -> u16 {
    (stored & 0xffff) as u16
}

#[inline(always)]
fn entry_score(stored: u64) -> i16 {
    ((stored >> KEY_BITS) & 0xffff) as u16 as i16
}

#[inline(always)]
fn entry_generation(stored: u64) -> u8 {
    ((stored >> 32) & GENERATION_MASK as u64) as u8
}

#[cfg(test)]
mod tests {
    use super::{ENTRY_BYTES, EvalCache, entry_count, key_bits};

    #[test]
    fn sizing_is_power_of_two_and_rounded_down() {
        assert!(entry_count(1).is_power_of_two());
        assert_eq!(entry_count(1) * ENTRY_BYTES, 1024 * 1024);
        assert!(entry_count(0) >= 1);
        assert!(entry_count(4) >= entry_count(1) * 4);
    }

    #[test]
    fn store_probe_round_trip() {
        let cache = EvalCache::new(1);
        cache.store(0xabcd_ef01_2345_6789, -321);
        assert_eq!(cache.probe(0xabcd_ef01_2345_6789), Some(-321));
        assert_eq!(cache.probe(0xabcd_ef01_2345_6788), None);
    }

    #[test]
    fn zero_key_is_never_cached() {
        let cache = EvalCache::new(1);
        cache.store(0, 100);
        assert_eq!(cache.probe(0), None);
    }

    #[test]
    fn clear_drops_every_entry() {
        let cache = EvalCache::new(1);
        cache.store(7, 42);
        cache.clear();
        assert_eq!(cache.probe(7), None);
        assert_eq!(cache.generation(), 0);
    }

    #[test]
    fn current_generation_entries_survive_new_stores() {
        let cache = EvalCache::new(1);
        // Same index, different key: the first value must stay while its
        // generation is current.
        let first = 0x1111_u64 << 48;
        let second = 0x2222_u64 << 48;
        cache.store(first, 11);
        cache.store(second, 22);
        assert_eq!(cache.probe(first), Some(11));
        assert_eq!(cache.probe(second), None);
    }

    #[test]
    fn new_generation_allows_replacement() {
        let cache = EvalCache::new(1);
        let first = 0x1111_u64 << 48;
        let second = 0x2222_u64 << 48;
        cache.store(first, 11);
        cache.new_generation();
        cache.store(second, 22);
        assert_eq!(cache.probe(second), Some(22));
        assert_eq!(cache.probe(first), None);
    }

    #[test]
    fn key_bits_use_the_high_word() {
        assert_eq!(key_bits(0x1234_0000_0000_0000), 0x1234);
        assert_eq!(key_bits(0x0000_0000_0000_1234), 0);
        let cache = EvalCache::new(1);
        cache.store(0x1234_0000_0000_0005, 1);
        assert_eq!(
            cache.probe(0x1234_5678_0000_0005),
            Some(1),
            "only the high key bits are compared, the index stays the same"
        );
    }
}
