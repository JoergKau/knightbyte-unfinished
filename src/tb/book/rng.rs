//! A tiny deterministic random source for weighted book choices.
//!
//! Opening-book moves are chosen by weighted random selection so a book does
//! not always play the same line. A xorshift generator is more than enough
//! for that; it keeps the engine dependency-free and lets tests seed it.

use std::time::{SystemTime, UNIX_EPOCH};

/// A 64-bit xorshift generator.
#[derive(Clone, Debug)]
pub struct BookRng {
    state: u64,
}

impl BookRng {
    /// Creates a generator from an explicit seed (zero is replaced).
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// Creates a generator seeded from the clock and the process id.
    pub fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x1234_5678_9ABC_DEF0);
        Self::new(nanos ^ (u64::from(std::process::id()) << 32))
    }

    /// Returns the next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    /// Returns a value in `0..bound` (`bound == 0` returns 0).
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.next_u64() % bound
        }
    }
}
