//! Tablebase access from the search.
//!
//! The searcher keeps one tablebase handle per search (refreshed from the
//! process-wide set in [`Searcher::search`](crate::search::searcher::Searcher::search)),
//! so a probe in the tree costs only the prober's own lock. These helpers keep
//! the decision logic out of the node functions: they answer "is this position
//! covered, and what is its exact WDL/DTZ value".

use crate::board::position::Position;

use super::searcher::Searcher;

impl Searcher {
    /// Returns the WDL value of `position` when it is inside the loaded tables.
    ///
    /// The master switch, the loaded material cardinality, castling rights and
    /// position legality are all checked here, so callers can treat `None` as
    /// "the search must decide this node normally".
    pub(crate) fn tb_wdl(&self, position: &Position) -> Option<i32> {
        if !self.tb_probing {
            return None;
        }
        self.tb.as_ref()?.probe_wdl(position)
    }

    /// Returns the DTZ value of `position` when it is inside the loaded tables.
    pub(crate) fn tb_dtz(&self, position: &Position) -> Option<i32> {
        if !self.tb_probing {
            return None;
        }
        self.tb.as_ref()?.probe_dtz(position)
    }
}
