//! Shared search data structures.
//!
//! This module holds the pieces every search phase needs: position keys
//! ([`crate::board::zobrist`]), the transposition table, the static-evaluation
//! cache, the principal-variation table, static exchange evaluation and the
//! move-ordering state (killers and histories). The search itself — negamax,
//! quiescence and time management — is added in the following sub-phases and
//! builds on these structures.

pub mod correction;
pub mod eval_cache;
pub mod history;
pub mod killers;
pub mod lever_table;
pub mod limits;
pub mod move_picker;
pub mod negamax;
pub mod options;
pub mod params;
pub mod pv;
pub mod quiescence;
pub mod searcher;
pub mod see;
pub mod tables;
pub mod time_manager;
pub mod tt;

/// Maximum search depth in plies.
///
/// Every per-ply table is sized with this value. The search must stay inside
/// the bound; a ply beyond it is ignored by the tables instead of panicking, so
/// a runaway extension can never take the engine down.
pub const MAX_PLY: usize = 128;
