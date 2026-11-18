//! The tables shared between search threads.
//!
//! Both tables are lock-free inside: probe and store work through `&self`
//! with atomic operations, so an `Arc` handle is all a worker thread needs.
//! `resize` and `clear` run only when no search is active — the UCI loop's
//! join-first rule (step 4) guarantees no live search holds a handle; the
//! helpers' transient clones drain within milliseconds (see `resize`).

use std::sync::Arc;
use std::time::Duration;

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
    /// # The exclusive borrow
    ///
    /// The resize needs the table's `Arc` count to be one. Three things can
    /// transiently hold clones after a search: a helper draining its
    /// iteration (it aborts at its next 1024-node poll), a helper's final
    /// publish-and-release, and a queued job in a helper's channel (its stop
    /// flag is already set, so it aborts at its first poll). All of them
    /// drain within milliseconds, so the resize waits for them instead of
    /// failing; the UCI loop's join-first rule guarantees no LIVE search
    /// holds a clone.
    ///
    /// # Panics
    ///
    /// Panics when the table is still shared after five seconds — that is a
    /// defect (a handle holder that never releases), and it is named loudly.
    pub fn resize(&mut self, megabytes: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(table) = Arc::get_mut(&mut self.tt) {
                table.resize(megabytes);
                return;
            }
            if std::time::Instant::now() > deadline {
                panic!("a resize found a persistent handle holder for five seconds");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
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
