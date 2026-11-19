//! KnightByte engine library.
//!
//! The binary is intentionally thin: it starts the UCI loop, while the
//! reusable engine components live in this library so they can be unit-tested
//! without spawning a process.

/// Increments a compile-time performance counter.
///
/// Expands to nothing unless the `node-stats` feature is enabled, so the
/// default build carries no counter traffic. Both forms are statements, not
/// expressions: `node_stat!(TtProbes);` adds one, and
/// `node_stat!(PseudoMoves, count);` adds `count` (which must cast to `u64`).
///
/// The counters are declared in [`tools::node_stats`] and printed by `bench`.
#[macro_export]
#[doc(hidden)]
macro_rules! node_stat {
    ($counter:ident) => {
        #[cfg(feature = "node-stats")]
        $crate::tools::node_stats::bump($crate::tools::node_stats::Counter::$counter);
    };
    ($counter:ident, $amount:expr) => {
        #[cfg(feature = "node-stats")]
        $crate::tools::node_stats::bump_by(
            $crate::tools::node_stats::Counter::$counter,
            $amount as u64,
        );
    };
}

pub mod board;
pub mod eval;
pub mod output;
pub mod platform;
pub mod search;
pub mod tools;
pub mod types;
pub mod uci;
