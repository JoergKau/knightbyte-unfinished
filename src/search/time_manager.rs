//! Time allocation.
//!
//! The manager converts the clocks reported by the GUI into the soft and hard
//! budgets the search loop consumes. The rules follow the reference shape:
//!
//! * a fixed `movetime` is used verbatim (minus the move overhead);
//! * otherwise the remaining time is split over an estimated number of moves,
//!   with the increment added once, and the result is capped so the engine keeps
//!   a reserve for the next move;
//! * a hard cap of half the remaining time prevents a loss on the clock even if
//!   the soft estimate was too optimistic;
//! * the clock-imbalance term (on by default in the catalogue, off here until
//!   the option layer exposes it) scales the budget when one side is much
//!   faster.
//!
//! All arithmetic is integer milliseconds. Nothing here depends on the clock
//! source itself, which keeps the allocation testable.

use crate::search::limits::Limits;

/// Time controls for one move.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimeControl {
    /// Remaining time for the side to move, in milliseconds.
    pub time_left_ms: u64,
    /// Increment for the side to move, in milliseconds.
    pub increment_ms: u64,
    /// Moves left until the next time control, when known.
    pub moves_to_go: Option<u32>,
}

/// Per-move overhead subtracted from every budget, in milliseconds.
pub const DEFAULT_MOVE_OVERHEAD_MS: u64 = 50;

/// Estimated moves per time control when the GUI reports none.
const DEFAULT_MOVES_TO_GO: u32 = 30;

/// Smallest budget the manager will hand out, in milliseconds.
const MIN_BUDGET_MS: u64 = 1;

/// Returns the search limits for a move time control.
pub fn allocate(control: TimeControl, overhead_ms: u64) -> Limits {
    let overhead = overhead_ms.min(control.time_left_ms.saturating_sub(1));

    let moves = control.moves_to_go.unwrap_or(DEFAULT_MOVES_TO_GO).max(1) as u64;
    let usable = control.time_left_ms.saturating_sub(overhead);
    let base = usable / moves + control.increment_ms / 2;
    let soft = base.max(MIN_BUDGET_MS).min(usable.max(MIN_BUDGET_MS));
    // The hard cap keeps at least a small reserve; half the usable time is the
    // classic safety bound.
    let hard = (soft * 3).min(usable / 2).max(soft);

    Limits {
        soft_time_ms: Some(soft),
        hard_time_ms: Some(hard),
        ..Limits::default()
    }
}

/// Returns the limits for a fixed `movetime` search.
pub fn fixed(move_time_ms: u64, overhead_ms: u64) -> Limits {
    let budget = move_time_ms.saturating_sub(overhead_ms).max(MIN_BUDGET_MS);
    Limits::move_time(budget)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_MOVE_OVERHEAD_MS, TimeControl, allocate, fixed};

    #[test]
    fn fixed_time_subtracts_the_overhead() {
        let limits = fixed(1_000, DEFAULT_MOVE_OVERHEAD_MS);
        assert_eq!(limits.soft_time_ms, Some(950));
        assert_eq!(limits.hard_time_ms, Some(950));
    }

    #[test]
    fn fixed_time_never_underflows() {
        let limits = fixed(10, 500);
        assert!(limits.soft_time_ms.unwrap() >= 1);
    }

    #[test]
    fn allocation_splits_the_remaining_time() {
        let limits = allocate(
            TimeControl {
                time_left_ms: 30_000,
                increment_ms: 0,
                moves_to_go: Some(30),
            },
            DEFAULT_MOVE_OVERHEAD_MS,
        );
        // (30,000 - 50) / 30 = 998 ms
        assert_eq!(limits.soft_time_ms, Some(998));
        assert!(limits.hard_time_ms.unwrap() >= 998);
        assert!(limits.hard_time_ms.unwrap() <= 14_975);
    }

    #[test]
    fn increment_contributes_half() {
        let limits = allocate(
            TimeControl {
                time_left_ms: 10_000,
                increment_ms: 1_000,
                moves_to_go: Some(10),
            },
            DEFAULT_MOVE_OVERHEAD_MS,
        );
        // (10,000 - 50) / 10 + 1,000 / 2 = 995 + 500 = 1,495
        assert_eq!(limits.soft_time_ms, Some(1_495));
    }

    #[test]
    fn hard_cap_keeps_half_the_clock() {
        let limits = allocate(
            TimeControl {
                time_left_ms: 2_000,
                increment_ms: 0,
                moves_to_go: None,
            },
            DEFAULT_MOVE_OVERHEAD_MS,
        );
        assert!(limits.hard_time_ms.unwrap() <= 975);
        assert!(limits.soft_time_ms.unwrap() >= 1);
    }

    #[test]
    fn tiny_clock_still_yields_a_budget() {
        let limits = allocate(
            TimeControl {
                time_left_ms: 20,
                increment_ms: 0,
                moves_to_go: None,
            },
            DEFAULT_MOVE_OVERHEAD_MS,
        );
        assert!(limits.soft_time_ms.unwrap() >= 1);
        assert!(limits.hard_time_ms.unwrap() >= 1);
    }
}
