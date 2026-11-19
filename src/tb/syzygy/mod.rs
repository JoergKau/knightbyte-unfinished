//! Syzygy tablebase probing.
//!
//! The prober answers three questions:
//!
//! * **WDL** — win, cursed win, draw, blessed loss or loss for the leaf
//!   positions the search visits.
//! * **DTZ** — distance to the next zeroing move, used to order won endgames
//!   instead of shuffling them into a fifty-move draw.
//! * **root** — the best move at the search root, with fifty-move awareness.
//!
//! Tables are optional. A `SyzygyPath` option builds a new [`Syzygy`] and
//! installs it as the process-wide table set; the search takes one reference
//! per search, so a reload only ever happens while no search is running. With
//! no tables loaded every probe returns `None` and the engine behaves exactly
//! as before.

mod index;
mod pairs;
mod pos;
mod probe;
mod table;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use table::Inner;

use crate::board::movegen::legal;
use crate::board::position::Position;
use crate::types::r#move::{Move, flags};

/// Score of a tablebase win.
///
/// It sits below the mate bound (so a proven mate always outranks it) and
/// above any material evaluation.
pub const TB_WIN_SCORE: i32 = 29_900;

/// Largest reduction of [`TB_WIN_SCORE`] used to order wins by DTZ.
pub const TB_WIN_SCORE_RANGE: i32 = 1_000;

/// Lowest score a DTZ-ordered tablebase win can take.
pub const TB_WIN_SCORE_MIN: i32 = TB_WIN_SCORE - TB_WIN_SCORE_RANGE;

/// Converts a WDL value (`-2..=2`) into a flat search score.
pub const fn wdl_to_score(wdl: i32) -> i32 {
    match wdl {
        2 => TB_WIN_SCORE,
        -2 => -TB_WIN_SCORE,
        1 => 1,
        -1 => -1,
        _ => 0,
    }
}

/// Converts a WDL value into a score graded by the distance to zeroing.
///
/// Cursed wins and blessed losses stay at `±1`: they are fifty-move draws in
/// all but name, and grading them would claim progress that does not exist.
pub fn wdl_to_score_graded(wdl: i32, dtz_abs: i32) -> i32 {
    if (wdl == 2 || wdl == -2) && dtz_abs > 0 {
        let graded = TB_WIN_SCORE - dtz_abs.min(TB_WIN_SCORE_RANGE);
        return if wdl == 2 { graded } else { -graded };
    }
    wdl_to_score(wdl)
}

/// The result of a root probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootResult {
    /// Best move, or [`Move::NONE`] for a terminal position or a failed probe.
    pub mv: Move,
    /// WDL class from the side to move (`-2..=2`).
    pub wdl: i32,
    /// Distance to the next zeroing move, unsigned.
    pub dtz: i32,
    /// The position is checkmate.
    pub is_checkmate: bool,
    /// The position is stalemate.
    pub is_stalemate: bool,
    /// The probe failed (not covered, castling rights, or no tables).
    pub failed: bool,
}

impl RootResult {
    /// A failed probe.
    const FAILED: Self = Self {
        mv: Move::NONE,
        wdl: 0,
        dtz: 0,
        is_checkmate: false,
        is_stalemate: false,
        failed: true,
    };
}

/// One immutable, thread-safe set of loaded tables.
pub struct Syzygy {
    inner: Mutex<Inner>,
    max_pieces: usize,
    wdl_count: usize,
}

impl Syzygy {
    /// Loads every table found under `path` (platform path-list separator).
    pub fn open(path: &str) -> Self {
        let inner = Inner::open(path);
        Self {
            max_pieces: inner.max_pieces,
            wdl_count: inner.wdl_count,
            inner: Mutex::new(inner),
        }
    }

    /// Returns the largest material combination covered (0 = no tables).
    pub fn max_pieces(&self) -> usize {
        self.max_pieces
    }

    /// Returns the number of WDL files discovered.
    pub fn wdl_tables(&self) -> usize {
        self.wdl_count
    }

    /// Returns whether this set can possibly answer for `position`.
    pub fn covers(&self, position: &Position) -> bool {
        self.max_pieces > 0
            && position.all_occupancy().count_ones() as usize <= self.max_pieces
            && position.castling_rights() == 0
    }

    /// Probes the WDL value (`-2..=2`) from the side to move's perspective.
    pub fn probe_wdl(&self, position: &Position) -> Option<i32> {
        if !self.covers(position) {
            return None;
        }
        let p = pos::to_pos(position, 0);
        if !pos::is_valid(&p) {
            return None;
        }
        let mut inner = self.lock();
        let value = probe::probe_wdl(&mut inner, &p);
        (value.success != 0).then_some(value.value)
    }

    /// Probes the distance to zeroing in plies, signed from the side to move.
    pub fn probe_dtz(&self, position: &Position) -> Option<i32> {
        if !self.covers(position) {
            return None;
        }
        let p = pos::to_pos(position, 0);
        if !pos::is_valid(&p) {
            return None;
        }
        let mut inner = self.lock();
        let value = probe::probe_dtz(&mut inner, &p);
        (value.success != 0).then_some(value.value)
    }

    /// Probes the root position for a WDL-preserving, DTZ-aware best move.
    pub fn probe_root(&self, position: &Position) -> RootResult {
        if !self.covers(position) {
            return RootResult::FAILED;
        }
        let counter = if use_rule50() {
            position.halfmove_clock().min(100) as u8
        } else {
            0
        };
        let p = pos::to_pos(position, counter);
        if !pos::is_valid(&p) {
            return RootResult::FAILED;
        }

        let (probe_result, dtz) = {
            let mut inner = self.lock();
            let result = probe::probe_root(&mut inner, &p);
            (result, result.dtz)
        };
        if !probe_result.success {
            return RootResult::FAILED;
        }
        if probe_result.mv == probe::MOVE_CHECKMATE {
            return RootResult {
                is_checkmate: true,
                failed: false,
                ..RootResult::FAILED
            };
        }
        if probe_result.mv == probe::MOVE_STALEMATE {
            return RootResult {
                is_stalemate: true,
                failed: false,
                ..RootResult::FAILED
            };
        }
        if probe_result.mv == 0 {
            return RootResult::FAILED;
        }

        let from = pos::move_from(probe_result.mv);
        let to = pos::move_to(probe_result.mv);
        let promotion = pos::move_promotes(probe_result.mv);
        let wanted_flag = match promotion {
            1 => flags::PROMO_QUEEN,
            2 => flags::PROMO_ROOK,
            3 => flags::PROMO_BISHOP,
            4 => flags::PROMO_KNIGHT,
            _ => flags::NORMAL,
        };
        let mut chosen = Move::NONE;
        for &candidate in legal(position).as_slice() {
            if candidate.from() as usize != from || candidate.to() as usize != to {
                continue;
            }
            if promotion != 0 {
                if candidate.flag() != wanted_flag {
                    continue;
                }
            } else if candidate.is_promotion() {
                continue;
            }
            chosen = candidate;
            break;
        }
        if chosen.is_none() {
            return RootResult::FAILED;
        }

        let wdl = probe::dtz_to_wdl(i32::from(counter), dtz) - 2;
        RootResult {
            mv: chosen,
            wdl,
            dtz: dtz.unsigned_abs() as i32,
            is_checkmate: false,
            is_stalemate: false,
            failed: false,
        }
    }

    /// Locks the prober state, recovering from a poisoned mutex.
    ///
    /// A panic while probing must not permanently disable tablebases: the
    /// prober has no cross-probe invariant that a panic could break.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// The process-wide table set; `None` when no path is configured.
static CURRENT: OnceLock<RwLock<Option<Arc<Syzygy>>>> = OnceLock::new();
/// Master switch for probing during search; on by default.
static PROBING_ENABLED: AtomicBool = AtomicBool::new(true);
/// Whether the root probe honours the fifty-move counter.
static USE_RULE50: AtomicBool = AtomicBool::new(true);

/// Returns the global slot holding the current table set.
fn cell() -> &'static RwLock<Option<Arc<Syzygy>>> {
    CURRENT.get_or_init(|| RwLock::new(None))
}

/// Returns the current table set, if any.
///
/// The search takes one clone per search instead of calling this per node.
pub fn current() -> Option<Arc<Syzygy>> {
    cell()
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

/// Loads (or clears) the process-wide table set.
///
/// Returns `true`; a path with no tables is not an error, it simply leaves the
/// engine without tablebases.
pub fn initialize(path: &str) -> bool {
    let tables = if path.is_empty() || path == "<empty>" {
        None
    } else {
        Some(Arc::new(Syzygy::open(path)))
    };
    if let Ok(mut slot) = cell().write() {
        *slot = tables;
    }
    true
}

/// Releases the process-wide table set and drops its mappings.
pub fn release() {
    if let Ok(mut slot) = cell().write() {
        *slot = None;
    }
}

/// Enables or disables probing without unloading the tables.
pub fn set_probing_enabled(enabled: bool) {
    PROBING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Returns whether probing is enabled.
pub fn probing_enabled() -> bool {
    PROBING_ENABLED.load(Ordering::Relaxed)
}

/// Sets whether the root probe honours the fifty-move counter.
pub fn set_use_rule50(enabled: bool) {
    USE_RULE50.store(enabled, Ordering::Relaxed);
}

/// Returns whether the root probe honours the fifty-move counter.
pub fn use_rule50() -> bool {
    USE_RULE50.load(Ordering::Relaxed)
}

/// Returns `(WDL table count, largest covered piece count)` for the handshake.
pub fn status() -> (usize, usize) {
    current().map_or((0, 0), |tables| (tables.wdl_tables(), tables.max_pieces()))
}
