//! The tables shared between search threads.
//!
//! Both tables are lock-free inside: probe and store work through `&self`
//! with atomic operations, so an `Arc` handle is all a worker thread needs.
//! `resize` and `clear` run only when no search is active — the UCI loop's
//! join-first rule (step 4) guarantees no thread holds a handle then, which
//! is what makes the exclusive-borrow resize safe.

use std::sync::Arc;

use crate::search::eval_cache::EvalCache;
use crate::search::tt::TranspositionTable;

/// The handle every search thread references: the transposition table and
/// the static-evaluation cache.
///
/// The owner (the main searcher) hands out `Arc` clones for the duration of
/// a search; the clones are dropped when the search joins, so a resize or a
/// clear always finds the tables unshared.
#[derive(Clone)]
pub struct SharedTables {
    /// The transposition table.
    pub tt: Arc<TranspositionTable>,
    /// The static-evaluation cache.
    pub eval_cache: Arc<EvalCache>,
}

impl SharedTables {
    /// Allocates both tables for a transposition table of `tt_megabytes`.
    pub fn new(tt_megabytes: usize) -> Self {
        Self {
            tt: Arc::new(TranspositionTable::new(tt_megabytes)),
            eval_cache: Arc::new(EvalCache::new(2)),
        }
    }

    /// Resizes the transposition table to `megabytes`.
    ///
    /// # Panics
    ///
    /// Panics when a search thread still holds a handle. That cannot happen:
    /// a resize arrives through `setoption`, and the join-first rule has
    /// already stopped and joined every search thread.
    pub fn resize(&mut self, megabytes: usize) {
        let table = Arc::get_mut(&mut self.tt).expect("a resize must find no other handle holder");
        table.resize(megabytes);
    }

    /// Forgets every entry in both tables.
    ///
    /// The transposition table zeroes its memory in parallel (see
    /// [`TranspositionTable::clear`]), which keeps `ucinewgame` and
    /// `Clear Hash` bounded at the maximum hash size.
    pub fn clear(&self) {
        self.tt.clear();
        self.eval_cache.clear();
    }
}
