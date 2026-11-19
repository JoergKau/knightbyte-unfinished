//! Compile-time counters for the performance composition report.
//!
//! Enabled with the `node-stats` cargo feature. Every counter is a relaxed
//! atomic: this build is for counting how often each path runs, not for
//! benchmarking it, so the added traffic is acceptable. The default build
//! contains no counters at all (the `crate::node_stat!` macro expands to
//! nothing).
//!
//! The counts answer "how often", not "how long". Multiply a count by the
//! matching `tests/perf_microbench.rs` rate to get the node-cost composition
//! without profiler or inlining bias. `bench` prints the counters when the
//! feature is enabled; use [`reset`] to clear them for one run.

use std::sync::atomic::{AtomicU64, Ordering};

/// Declares the counter enum, its stable report names and the storage array
/// from one list, so the three can never drift apart.
macro_rules! define_counters {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// One counter in the node-composition report.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(usize)]
        pub enum Counter {
            $(
                #[doc = concat!("Counts `", $name, "`.")]
                $variant,
            )+
        }

        /// Number of counters in [`Counter`].
        const COUNTER_COUNT: usize = [$($name),+].len();

        /// Stable report names, in declaration order.
        const COUNTER_NAMES: [&str; COUNTER_COUNT] = [$($name),+];
    };
}

define_counters! {
    NegamaxNodes => "negamax_nodes",
    QsearchNodes => "qsearch_nodes",
    LegalMoveGens => "legal_move_gens",
    NegamaxMoveLists => "negamax_move_lists",
    QsearchMoveLists => "qsearch_move_lists",
    PseudoMoves => "pseudo_moves",
    MovesTried => "moves_tried",
    MovesMade => "moves_made",
    IllegalMoves => "illegal_moves",
    TtProbes => "tt_probes",
    TtHits => "tt_hits",
    TtStores => "tt_stores",
    EvalCacheProbes => "eval_cache_probes",
    EvalCacheHits => "eval_cache_hits",
    EvalCacheStores => "eval_cache_stores",
    EvalHeadCalls => "eval_head_calls",
    FullRefreshes => "full_refreshes",
    AccumulatorPushes => "accumulator_pushes",
    SeeCalls => "see_calls",
}

/// The counter storage, one relaxed atomic per [`Counter`].
static COUNTERS: [AtomicU64; COUNTER_COUNT] = [const { AtomicU64::new(0) }; COUNTER_COUNT];

/// Adds one to `counter`.
#[inline]
pub fn bump(counter: Counter) {
    bump_by(counter, 1);
}

/// Adds `amount` to `counter`.
///
/// # Panics
///
/// Never panics for any `counter` value; the index is the enum's discriminant.
#[inline]
pub fn bump_by(counter: Counter, amount: u64) {
    COUNTERS[counter as usize].fetch_add(amount, Ordering::Relaxed);
}

/// Resets every counter to zero.
///
/// `bench` calls this before its run so one report covers exactly one workload.
pub fn reset() {
    for counter in &COUNTERS {
        counter.store(0, Ordering::Relaxed);
    }
}

/// Returns `(stable_name, value)` for every counter, in declaration order.
pub fn snapshot() -> Vec<(&'static str, u64)> {
    COUNTER_NAMES
        .iter()
        .zip(COUNTERS.iter())
        .map(|(name, counter)| (*name, counter.load(Ordering::Relaxed)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{COUNTER_COUNT, Counter, bump_by, snapshot};

    #[test]
    fn every_counter_has_a_unique_name() {
        let report = snapshot();
        assert_eq!(report.len(), COUNTER_COUNT);
        let mut names: Vec<&str> = report.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COUNTER_COUNT, "counter names must be unique");
        assert!(report.iter().all(|(name, _)| !name.is_empty()));
    }

    #[test]
    fn bump_by_is_visible_in_the_snapshot() {
        // Other tests may bump the same counter concurrently; only growth is
        // asserted, never an absolute value.
        let before = snapshot()[Counter::MovesTried as usize].1;
        bump_by(Counter::MovesTried, 3);
        let after = snapshot()[Counter::MovesTried as usize].1;
        assert!(after >= before + 3);
    }
}
