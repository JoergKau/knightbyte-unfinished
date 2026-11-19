//! Search limits.
//!
//! A limit describes *why* the search stops. Depth and node limits are exact;
//! the time limits come from the time manager, which turns a clock into a soft
//! and a hard budget:
//!
//! * the **soft** limit stops the iterative-deepening loop before starting an
//!   iteration that is unlikely to finish;
//! * the **hard** limit aborts the current iteration, so the engine never
//!   flags.
//!
//! Every limit is optional: an unset limit never stops the search.

use crate::types::r#move::Move;

/// Stop conditions for one search.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Limits {
    /// Maximum nominal depth in plies.
    pub depth: Option<u32>,
    /// Maximum number of visited nodes.
    pub nodes: Option<u64>,
    /// Soft time budget in milliseconds; stops between iterations.
    pub soft_time_ms: Option<u64>,
    /// Hard time budget in milliseconds; aborts the current iteration.
    pub hard_time_ms: Option<u64>,
    /// `go mate N`: stop at the first completed iteration that proves a mate
    /// in at most N moves. A slower proven mate keeps searching; other
    /// limits still apply. Being mated always ends the search.
    pub mate: Option<u32>,
    /// `go searchmoves`: only these root moves are searched (empty = no
    /// restriction). The UCI layer fills this with the listed moves that are
    /// legal in the root position; a list whose every entry is illegal is
    /// ignored, exactly like the reference (verified 2026-11-18).
    pub root_moves: Vec<Move>,
    /// Whether this is a ponder search. The reference does not volunteer a
    /// tablebase root move while pondering, and neither does KnightByte.
    pub ponder: bool,
}

impl Limits {
    /// Searches exactly `depth` plies.
    pub fn depth(depth: u32) -> Self {
        Self {
            depth: Some(depth),
            ..Self::default()
        }
    }

    /// Stops after at most `nodes` visited nodes.
    pub fn nodes(nodes: u64) -> Self {
        Self {
            nodes: Some(nodes),
            ..Self::default()
        }
    }

    /// Stops after `milliseconds`, using the same value for both budgets.
    pub fn move_time(milliseconds: u64) -> Self {
        Self {
            soft_time_ms: Some(milliseconds),
            hard_time_ms: Some(milliseconds),
            ..Self::default()
        }
    }

    /// Returns the depth limit as a signed ply count, capped at the table bound.
    pub fn depth_limit(&self) -> i32 {
        self.depth
            .map(|depth| depth.min(super::MAX_PLY as u32) as i32)
            .unwrap_or(super::MAX_PLY as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::Limits;
    use crate::search::MAX_PLY;

    #[test]
    fn limits_are_independent() {
        let by_depth = Limits::depth(6);
        assert_eq!(by_depth.depth, Some(6));
        assert_eq!(by_depth.nodes, None);
        assert_eq!(by_depth.soft_time_ms, None);

        let by_nodes = Limits::nodes(1_000);
        assert_eq!(by_nodes.depth, None);
        assert_eq!(by_nodes.nodes, Some(1_000));

        let by_time = Limits::move_time(250);
        assert_eq!(by_time.soft_time_ms, Some(250));
        assert_eq!(by_time.hard_time_ms, Some(250));
    }

    #[test]
    fn depth_is_capped_at_the_table_bound() {
        assert_eq!(Limits::default().depth_limit(), MAX_PLY as i32);
        assert_eq!(Limits::depth(4).depth_limit(), 4);
        assert_eq!(Limits::depth(10_000).depth_limit(), MAX_PLY as i32);
    }
}
