//! Search driver: state, iterative deepening and shared helpers.
//!
//! The driver owns everything that survives between iterations — the
//! transposition table, history tables, killers, principal-variation table and
//! the per-ply key stack — so a search needs no allocation after the
//! constructor. Negamax and quiescence are implemented in their own modules as
//! methods on [`Searcher`].
//!
//! The driver is synchronous: it searches to completion and reports every
//! completed iteration through a callback. A cooperative stop flag that a UCI
//! thread can raise arrives with the threading phase; the node limit already
//! exercises the same early-exit path and keeps the tables honest.

use crate::board::position::Position;
use crate::eval::nnue::evaluate_position;
use crate::search::MAX_PLY;
use crate::search::correction::CorrectionHistory;
use crate::search::eval_cache::EvalCache;
use crate::search::history::{History, Previous};
use crate::search::killers::Killers;
use crate::search::limits::Limits;
use crate::search::move_picker::PickerState;
use crate::search::params::SearchParams;
use crate::search::pv::PvTable;
use crate::search::time_manager::{TimeControl, TimeManager};
use crate::search::tt::{Bound, TranspositionTable, TtEntry, score_from_tt, score_to_tt};
use crate::types::color::{BLACK, Color, WHITE};
use crate::types::r#move::Move;
use crate::types::piece::{self, EMPTY};
use crate::types::piece::{BISHOP, KNIGHT, PAWN, QUEEN, ROOK};

/// Score of a forced mate at the root.
///
/// Mate scores decrease by one per ply, so a mate in one ply is
/// [`MATE_SCORE`] - 1. Values at or above [`MATE_LIMIT`] are treated as mates.
pub const MATE_SCORE: i32 = 31_000;

/// Boundary above which a score counts as a mate distance.
pub const MATE_LIMIT: i32 = 30_000;

/// Marker for "this node has no usable static evaluation".
///
/// The table's static-eval field is 16 bits; this sentinel cannot collide with
/// a real evaluation after clamping, and quiescence stores it for in-check
/// nodes where no stand-pat exists.
pub const NO_STATIC_EVAL: i32 = i16::MIN as i32;

/// A score larger than any real evaluation.
pub const INFINITE: i32 = 32_000;

/// Score of a drawn position.
pub const DRAW_SCORE: i32 = 0;

/// Per-depth report emitted after every completed iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchInfo {
    /// Completed depth in plies.
    pub depth: i32,
    /// Deepest ply reached by any line of this iteration.
    pub seldepth: usize,
    /// Score in centipawns from the side to move; mate distances use
    /// [`MATE_SCORE`].
    pub score: i32,
    /// Nodes visited so far.
    pub nodes: u64,
    /// Milliseconds elapsed since the search started.
    pub time_ms: u64,
    /// Nodes per second, derived from `nodes` and `time_ms` (guarded against
    /// a zero elapsed time).
    pub nps: u64,
    /// Table occupancy in permill, read once per iteration.
    pub hashfull: u16,
    /// Principal variation of the completed iteration.
    pub pv: Vec<Move>,
}

/// Final result of one search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    /// Best move found, or [`Move::NONE`] when no move is legal.
    pub best_move: Move,
    /// Score of the best move in centipawns from the side to move.
    pub score: i32,
    /// Last completed depth in plies.
    pub depth: i32,
    /// Deepest ply reached.
    pub seldepth: usize,
    /// Number of visited nodes.
    pub nodes: u64,
    /// Principal variation, which may be shorter than `depth` in a forced line.
    pub pv: Vec<Move>,
}

/// Shared search state plus the iterative-deepening driver.
pub struct Searcher {
    pub(crate) tt: TranspositionTable,
    pub(crate) eval_cache: EvalCache,
    pub(crate) history: History,
    pub(crate) correction: CorrectionHistory,
    pub(crate) killers: Killers,
    pub(crate) pv: PvTable,
    pub(crate) params: SearchParams,
    pub(crate) limits: Limits,
    pub(crate) nodes: u64,
    pub(crate) seldepth: usize,
    pub(crate) stopped: bool,
    /// Position key per ply, used for repetition detection.
    pub(crate) keys: Box<[u64]>,
    /// Keys of the positions that preceded the root in the game, oldest first
    /// (the root itself is `keys[0]`). A repetition of one of them counts like
    /// a repetition inside the search.
    pub(crate) game_keys: Vec<u64>,
    /// Static evaluation per ply, used by the improving heuristic.
    pub(crate) evals: Box<[i32]>,
    /// The move played at each ply (`move_stack[ply]` is the move the node at
    /// `ply` made, leading to `ply + 1`). Indexes the continuation history.
    pub(crate) move_stack: Box<[Move]>,
    /// Legal moves searched so far at each ply; the parent's count gates the
    /// early transposition-table-cutoff history bonus in the child.
    pub(crate) move_counts: Box<[i32]>,
    /// The late-move reduction applied at each ply; the child's hindsight
    /// pair reads the parent's slot.
    pub(crate) reduction_stack: Box<[i32]>,
    /// One staged-picker state per ply (the reference's per-thread per-ply
    /// picker buffers).
    pub(crate) picker: Box<[PickerState]>,
    /// Stateful time manager: allocation, the stop decisions and the
    /// stability trackers.
    pub(crate) time_manager: TimeManager,
    /// Nodes spent per root move, indexed `from * 64 + to`; the time
    /// manager's node-fraction term reads the best move's share.
    pub(crate) nodes_per_move: Box<[u64; 64 * 64]>,
    /// Per-move time overhead subtracted from every budget, in milliseconds.
    pub(crate) move_overhead_ms: u64,
    /// Start of the current search, used for the time budgets.
    pub(crate) start: std::time::Instant,
    /// Cooperative stop flag shared with the UCI loop. The loop sets it on
    /// `stop` (and on the join-first fallback); the search polls it every
    /// 1024 nodes and before every iteration. A searcher created without the
    /// UCI layer owns a flag that is never set.
    pub(crate) stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Number of completed iterations; the first one always runs to the end.
    pub(crate) completed_depth: i32,
    /// When the trace flag is on, every search node writes one ledger line to
    /// stderr. Enabled per process with `KNIGHTBYTE_TRACE`; it is the
    /// parity-hunt dump, never active in normal operation.
    pub(crate) trace: bool,
}

impl Searcher {
    /// Creates a search state with a transposition table of `tt_megabytes`.
    ///
    /// Every per-ply array is heap allocated: a search stack sized for
    /// [`MAX_PLY`] must not live on the main thread's stack on platforms where
    /// that stack is only one megabyte.
    pub fn new(tt_megabytes: usize) -> Self {
        Self {
            tt: TranspositionTable::new(tt_megabytes),
            eval_cache: EvalCache::new(2),
            history: History::new(),
            correction: CorrectionHistory::new(),
            killers: Killers::new(),
            pv: PvTable::new(),
            params: SearchParams::default(),
            limits: Limits::default(),
            nodes: 0,
            seldepth: 0,
            stopped: false,
            keys: vec![0; MAX_PLY + 1].into_boxed_slice(),
            game_keys: Vec::new(),
            evals: vec![0; MAX_PLY + 1].into_boxed_slice(),
            move_stack: vec![Move::NONE; MAX_PLY + 1].into_boxed_slice(),
            move_counts: vec![0; MAX_PLY + 1].into_boxed_slice(),
            reduction_stack: vec![0; MAX_PLY + 1].into_boxed_slice(),
            picker: (0..=MAX_PLY).map(|_| PickerState::new()).collect(),
            time_manager: TimeManager::new(),
            nodes_per_move: Box::new([0; 64 * 64]),
            move_overhead_ms: crate::search::time_manager::DEFAULT_MOVE_OVERHEAD_MS,
            start: std::time::Instant::now(),
            stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            completed_depth: 0,
            trace: std::env::var_os("KNIGHTBYTE_TRACE").is_some(),
        }
    }

    /// Forgets every position-dependent table.
    ///
    /// `ucinewgame` and each benchmark position call this, which is what makes
    /// benchmark node counts reproducible from run to run.
    pub fn clear(&mut self) {
        self.tt.clear();
        self.eval_cache.clear();
        self.history.clear();
        self.correction.clear();
        self.killers.clear();
        self.pv.reset();
    }

    /// Clears the per-ply stacks and the countermove table at the start of a
    /// search.
    ///
    /// The history tables deliberately persist between searches; everything
    /// that describes the previous search's path does not. The stacks are read
    /// before they are written for the first time (a null-move child reads its
    /// parent's slot), so a stale value left by an earlier search would leak
    /// into this one.
    fn reset_stacks(&mut self) {
        self.keys.fill(0);
        self.evals.fill(0);
        self.move_stack.fill(Move::NONE);
        self.move_counts.fill(0);
        self.reduction_stack.fill(0);
        self.history.clear_counters();
    }

    /// Sets the positions that preceded the search root in the game, oldest
    /// first and excluding the root itself.
    ///
    /// The `position ... moves ...` command supplies them. Without them the
    /// search only sees repetitions along its own path and would walk into (or
    /// miss) a repetition of a position from earlier in the game. Only the
    /// positions since the last irreversible move can repeat, so a long history
    /// is trimmed to the last 100 plies.
    pub fn set_game_history(&mut self, keys: &[u64]) {
        let keep = keys.len().min(100);
        self.game_keys.clear();
        self.game_keys.extend_from_slice(&keys[keys.len() - keep..]);
    }

    /// Returns the current search parameters.
    pub fn params(&self) -> &SearchParams {
        &self.params
    }

    /// Returns mutable access to the search parameters.
    pub fn params_mut(&mut self) -> &mut SearchParams {
        &mut self.params
    }

    /// Returns the transposition-table occupancy in permill.
    pub fn hashfull(&self) -> u16 {
        self.tt.hashfull()
    }

    /// Resizes the transposition table to `megabytes`.
    pub fn set_hash(&mut self, megabytes: usize) {
        self.tt.resize(megabytes.clamp(1, 1024));
    }

    /// Sets the per-move time overhead in milliseconds.
    pub fn set_move_overhead(&mut self, milliseconds: u64) {
        self.move_overhead_ms = milliseconds;
    }

    /// Initializes the time manager for a clock-controlled search.
    ///
    /// The search clock starts here, when the `go` command is handled. The
    /// manager stores its limits as absolute milliseconds on that clock, so
    /// the clock must not be restarted afterwards: restarting it in
    /// [`Searcher::search`] shifted every limit by the time since the previous
    /// search began, which made timed searches overrun by about that much.
    pub fn init_clock(&mut self, control: &TimeControl) {
        self.sync_time_levers();
        self.start = std::time::Instant::now();
        self.time_manager
            .init(control, self.move_overhead_ms, self.now_ms());
    }

    /// Initializes the time manager for a fixed `movetime` search (the clock
    /// starts here, see [`Searcher::init_clock`]).
    pub fn init_movetime(&mut self, milliseconds: u64) {
        self.sync_time_levers();
        self.start = std::time::Instant::now();
        self.time_manager
            .set_fixed_time(milliseconds, self.move_overhead_ms, self.now_ms());
    }

    /// Switches the time manager to infinite mode.
    pub fn init_infinite(&mut self) {
        self.sync_time_levers();
        self.start = std::time::Instant::now();
        self.time_manager.set_infinite();
    }

    /// Pushes the time-management levers from the parameters into the
    /// manager. The clock-imbalance scales must be live before
    /// [`TimeManager::init`] runs, because the allocation reads them.
    fn sync_time_levers(&mut self) {
        self.time_manager.set_levers(
            self.params.use_stable_time_management,
            self.params.use_clock_imbalance,
            self.params.clock_imbalance_ahead_scale,
            self.params.clock_imbalance_behind_scale,
        );
    }

    /// Milliseconds elapsed since the search's start.
    pub(crate) fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// Returns the configured per-move overhead.
    pub fn move_overhead(&self) -> u64 {
        self.move_overhead_ms
    }

    /// Runs iterative deepening and returns the best move.
    ///
    /// `observer` is called after every completed iteration; pass a closure
    /// that ignores its argument when no reporting is needed.
    pub fn search(
        &mut self,
        position: &Position,
        limits: Limits,
        observer: &mut dyn FnMut(&SearchInfo),
    ) -> SearchResult {
        // The search clock is started by the `init_*` call that configured the
        // time manager; it is deliberately not restarted here.
        self.limits = limits;
        self.completed_depth = 0;
        self.nodes = 0;
        self.seldepth = 0;
        self.stopped = false;
        self.pv.reset();
        self.killers.clear();
        self.reset_stacks();
        *self.nodes_per_move = [0; 64 * 64];
        self.tt.new_generation();

        let root_moves = crate::board::movegen::legal(position);
        if root_moves.is_empty() {
            let score = if position.in_check(position.side()) {
                mated_in(0)
            } else {
                DRAW_SCORE
            };
            return SearchResult {
                best_move: Move::NONE,
                score,
                depth: 0,
                seldepth: 0,
                nodes: 0,
                pv: Vec::new(),
            };
        }

        let max_depth = limits.depth_limit();
        // Reporting clock for the `info` lines: it starts with the search,
        // so every iteration publishes the elapsed time from the `go` (the
        // time manager keeps its own clock, started by the `init_*` call).
        let started = std::time::Instant::now();
        let mut best = SearchResult {
            best_move: root_moves.as_slice()[0],
            score: 0,
            depth: 0,
            seldepth: 0,
            nodes: 0,
            pv: Vec::new(),
        };
        let mut last_score = 0;

        for depth in 1..=max_depth {
            // The stop flag is honoured before each iteration too: a `stop`
            // that arrived between iterations must not start a new one. The
            // per-node polling in `should_stop` alone could miss it for up
            // to 1023 nodes into the next iteration.
            if self.stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let Some((score, pv)) = self.search_root(position, depth, last_score) else {
                break;
            };
            last_score = score;
            best.score = score;
            best.depth = depth;
            best.seldepth = self.seldepth;
            best.nodes = self.nodes;
            best.pv = pv;
            self.completed_depth = depth;
            if let Some(&first) = best.pv.first() {
                best.best_move = first;
            }
            let time_ms = started.elapsed().as_millis() as u64;
            observer(&SearchInfo {
                depth: best.depth,
                seldepth: best.seldepth,
                score: best.score,
                nodes: best.nodes,
                time_ms,
                nps: nps_of(best.nodes, time_ms),
                // Read once per completed iteration, not per node: the
                // occupancy is only published on the `info` line.
                hashfull: self.tt.hashfull(),
                pv: best.pv.clone(),
            });

            // Time management: record the completed depth, feed the node
            // statistics and the best-move stability, then decide whether
            // the next iteration starts. The mate score breaks first — a
            // proven mate needs no more depth.
            if score.abs() >= MATE_LIMIT {
                break;
            }
            self.time_manager.set_completed_depth(depth);
            let best_index =
                usize::from(best.best_move.from()) * 64 + usize::from(best.best_move.to());
            let best_nodes = self.nodes_per_move[best_index];
            self.time_manager.set_node_stats(best_nodes, self.nodes);
            self.time_manager
                .update_pv_stability(best_index as i32, true);
            if !self
                .time_manager
                .should_continue(self.now_ms(), best_index as i32, score)
            {
                break;
            }
            // History ages by 7/8 per completed iteration from depth 4 on,
            // matching the reference's saturation regulator.
            if depth >= 4 {
                self.history.age();
            }
        }

        best.nodes = self.nodes;
        best.seldepth = self.seldepth;
        best
    }

    /// Searches one iteration, optionally inside an aspiration window.
    ///
    /// Returns `None` when the iteration was interrupted by a limit; the caller
    /// then keeps the result of the previous depth.
    ///
    /// The window handling mirrors the reference: a fail-low halves the window
    /// (`beta = (alpha + beta) / 2`) and shifts alpha down by the current
    /// delta, a fail-high raises beta by the delta, and the delta widens
    /// geometrically by half of itself per cycle. A mate score or an exhausted
    /// cycle budget drops the loop back to the full window, which terminates
    /// the iteration.
    fn search_root(
        &mut self,
        position: &Position,
        depth: i32,
        previous_score: i32,
    ) -> Option<(i32, Vec<Move>)> {
        let params = self.params;
        let mut delta = params.aspiration_base;
        let mut alpha = -INFINITE;
        let mut beta = INFINITE;
        if depth >= params.aspiration_start_depth && previous_score.abs() < MATE_LIMIT {
            // No clamping on the widening arithmetic: the full-window bounds
            // stay far outside the mate range, and the reference widens in
            // plain `int` arithmetic without bounds.
            alpha = previous_score - delta;
            beta = previous_score + delta;
        }

        let mut cycles = 0;
        loop {
            let score = self.negamax(position, alpha, beta, depth, 0, None, false, Move::NONE, 0);
            if self.stopped {
                return None;
            }
            if score > alpha && score < beta {
                return Some((score, self.pv.line(0).to_vec()));
            }
            if score.abs() >= MATE_LIMIT || cycles >= params.aspiration_max_cycles {
                // One final search with the full window: any score lies
                // inside it, so the next pass terminates.
                alpha = -INFINITE;
                beta = INFINITE;
                delta = params.aspiration_base;
                continue;
            }
            if score <= alpha {
                // Fail-low: halve the window and shift alpha by the delta.
                beta = (alpha + beta) / 2;
                alpha -= delta;
            } else {
                // Fail-high: raise beta by the delta.
                beta += delta;
            }
            delta += delta / 2;
            cycles += 1;
        }
    }

    /// Returns the static evaluation of `position`, optionally via the cache.
    pub(crate) fn evaluate(&mut self, position: &Position) -> i32 {
        let key = position.key();
        if self.params.use_eval_cache
            && let Some(cached) = self.eval_cache.probe(key)
        {
            return i32::from(cached);
        }
        let value = evaluate_position(position);
        self.eval_cache
            .store(key, value.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        value
    }

    /// Installs a fresh, cleared stop flag and returns its handle.
    ///
    /// The UCI loop keeps the handle and sets it on `stop` (and on the
    /// join-first fallback for protocol-breaking commands); the search polls
    /// it every 1024 nodes and before every iteration. Arming before every
    /// `go` also clears any flag left over from a previous search.
    pub fn arm_stop_flag(&mut self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let flag = Arc::new(AtomicBool::new(false));
        self.stop_flag = Arc::clone(&flag);
        flag
    }

    /// Returns whether the search must stop before visiting another node.
    ///
    /// Two checks stay on every node, and they are plain integer comparisons:
    /// the exact `go nodes` limit, and the remembered stop decision. The
    /// expensive inputs — the wall clock and the shared stop flag — are
    /// polled only every 1024 nodes (the plan's step-4 rule: no clock read
    /// and no atomic access per node).
    pub(crate) fn should_stop(&mut self) -> bool {
        use std::sync::atomic::Ordering;
        if self.stopped {
            return true;
        }
        if let Some(limit) = self.limits.nodes
            && self.nodes >= limit
        {
            self.stopped = true;
            return true;
        }
        // `nodes` was incremented before this call, so the polling window
        // fires once every 1024 visited nodes.
        if self.nodes & 1023 == 0
            && (self.stop_flag.load(Ordering::Relaxed)
                || self.time_manager.should_stop(self.now_ms()))
        {
            self.stopped = true;
        }
        self.stopped
    }

    /// Returns whether the current position repeats a position on the path or
    /// has exhausted the fifty-move clock.
    ///
    /// Twofold repetition scores as a draw because the search only needs to know
    /// that the side to move cannot make progress, which is exactly what the
    /// second occurrence proves. The lever keeps this switchable for parity
    /// work. Insufficient material (K vs K, K+N vs K, K+B vs K, and KB vs KB
    /// with same-coloured bishops) draws regardless of the lever.
    pub(crate) fn is_draw(&self, position: &Position, ply: usize) -> bool {
        if position.halfmove_clock() >= 100 || Self::insufficient_material(position) {
            return true;
        }
        if !self.params.use_twofold_repetition_draw {
            return false;
        }
        self.repetition_count(position, ply) > 0
    }

    /// Returns how many earlier positions with the same side to move equal
    /// `position`, looking back along the search path and then into the game
    /// history, as far as the fifty-move clock allows (a capture or pawn move
    /// makes every earlier position unreachable).
    pub(crate) fn repetition_count(&self, position: &Position, ply: usize) -> u32 {
        let key = position.key();
        let clock = usize::from(position.halfmove_clock());
        let reachable = clock.min(ply + self.game_keys.len());
        let mut count = 0;
        let mut back = 2;
        while back <= reachable {
            let found = if back <= ply {
                self.keys[ply - back] == key
            } else {
                // `beyond` plies before the root: 1 is the position just before it.
                let beyond = back - ply;
                self.game_keys[self.game_keys.len() - beyond] == key
            };
            if found {
                count += 1;
            }
            back += 2;
        }
        count
    }

    /// Returns whether neither side can deliver checkmate: king against
    /// king, king plus one knight against king, king plus one bishop against
    /// king, or king plus bishop against king plus bishop with
    /// same-coloured bishops. Any pawn, rook or queen on the board makes the
    /// position sufficient by definition (the reference's early gate —
    /// without it, K+P vs K+B would falsely read as KB vs KB).
    pub(crate) fn insufficient_material(position: &Position) -> bool {
        for piece in [PAWN, ROOK, QUEEN] {
            if position.pieces(WHITE, piece) != 0 || position.pieces(BLACK, piece) != 0 {
                return false;
            }
        }
        let knights = |color: Color| position.pieces(color, KNIGHT).count_ones();
        let bishops = |color: Color| position.pieces(color, BISHOP).count_ones();
        let white_knights = knights(WHITE);
        let black_knights = knights(BLACK);
        let white_bishops = bishops(WHITE);
        let black_bishops = bishops(BLACK);

        // King vs king.
        if white_knights == 0 && black_knights == 0 && white_bishops == 0 && black_bishops == 0 {
            return true;
        }
        // King + knight vs king.
        if (white_knights == 1 && white_bishops == 0 && black_knights == 0 && black_bishops == 0)
            || (black_knights == 1
                && black_bishops == 0
                && white_knights == 0
                && white_bishops == 0)
        {
            return true;
        }
        // King + bishop vs king.
        if (white_bishops == 1 && white_knights == 0 && black_knights == 0 && black_bishops == 0)
            || (black_bishops == 1
                && black_knights == 0
                && white_knights == 0
                && white_bishops == 0)
        {
            return true;
        }
        // King + bishop vs king + bishop on the same square colour.
        if white_knights == 0 && black_knights == 0 && white_bishops == 1 && black_bishops == 1 {
            let white_bishop = crate::board::bitboard::lsb(position.pieces(WHITE, BISHOP));
            let black_bishop = crate::board::bitboard::lsb(position.pieces(BLACK, BISHOP));
            let same_color = ((white_bishop as i32 / 8 + white_bishop as i32 % 8)
                - (black_bishop as i32 / 8 + black_bishop as i32 % 8))
                % 2
                == 0;
            return same_color;
        }
        false
    }

    /// Stores a search result in the transposition table.
    ///
    /// The generation is stamped here so every search-code caller records the
    /// same value the ageing policy expects.
    pub(crate) fn store_tt(&self, mut entry: TtEntry) {
        entry.generation = self.tt.generation();
        self.tt.store(entry);
    }

    /// Converts a stored score back to search terms.
    pub(crate) fn probe_score(entry: &TtEntry, ply: usize) -> i32 {
        i32::from(score_from_tt(entry.score, ply as u16))
    }

    /// Returns whether a stored score is consistent with the raw static
    /// evaluation, and therefore safe to reuse as the evaluation.
    pub(crate) fn can_use_tt_score(entry: &TtEntry, raw_static_eval: i32, ply: usize) -> bool {
        let score = Self::probe_score(entry, ply);
        match entry.bound {
            Bound::Exact => true,
            Bound::Lower => score >= raw_static_eval,
            Bound::Upper => score <= raw_static_eval,
        }
    }

    /// Returns whether the static evaluation improved over the same line.
    ///
    /// The grandparent's eval is the comparison point: two plies are enough to
    /// see whether the current side's position is trending up or down. The
    /// root and the first ply have no comparison point and count as *not*
    /// improving, which is the reference's convention — pruning margins there
    /// use their not-improving scales.
    pub(crate) fn is_improving(&self, ply: usize, eval: i32) -> bool {
        self.params.use_improving_flag && ply >= 2 && eval > self.evals[ply - 2]
    }

    /// Returns whether `color` has a piece other than pawns and the king.
    pub(crate) fn has_non_pawn_material(position: &Position, color: Color) -> bool {
        let pieces = position.pieces(color, KNIGHT)
            | position.pieces(color, BISHOP)
            | position.pieces(color, ROOK)
            | position.pieces(color, QUEEN);
        pieces != 0
    }
}

/// Everything a finished negamax node contributes to the shared tables.
///
/// Grouping the fields keeps the store and the correction-history update in one
/// place, so the two exits of the node function cannot drift apart.
pub(crate) struct NodeResult<'a> {
    /// Position of the node.
    pub position: &'a Position,
    /// Distance from the root.
    pub ply: usize,
    /// Remaining depth after extensions.
    pub depth: i32,
    /// Score returned by the node.
    pub score: i32,
    /// Raw, correction-free static evaluation.
    pub raw_eval: i32,
    /// Static evaluation including the correction, as used by the pruners.
    pub corrected_eval: i32,
    /// Bound type the score represents.
    pub bound: Bound,
    /// Best move found (or the inherited table move on an upper bound).
    pub best_move: Move,
    /// Whether the node was in check.
    pub in_check: bool,
    /// Move that produced the node, for the countermove correction term.
    pub previous: Option<Previous>,
    /// Whether the node is a singular-verification call; its result is a
    /// bound probe, never stored.
    pub excluded: bool,
}

impl Searcher {
    /// Stores the node result and folds its evaluation error into the
    /// correction history, then returns the score.
    ///
    /// A singular-verification node returns its score without storing: its
    /// window is artificial, so neither the table nor the correction history
    /// may learn from it.
    pub(crate) fn finish_node(&mut self, result: NodeResult<'_>) -> i32 {
        if result.excluded {
            return result.score;
        }
        self.store_tt(tt_entry(
            result.position,
            result.ply,
            result.depth,
            result.score,
            if result.in_check {
                NO_STATIC_EVAL
            } else {
                result.raw_eval
            },
            result.bound,
            result.best_move,
        ));

        // Reference update conditions: no mate score, not in check, and a quiet
        // best move — quiet in the correction-history sense: the destination is
        // not occupied by the opponent (no ordinary capture), and the move is
        // neither en passant nor a promotion. Bug B37 fix (reference bug 149):
        // the fixed reference tests the OPPONENT occupancy at the destination
        // (its pre-fix code tested the mover's own occupancy, which can never
        // fire, so ordinary captures wrongly passed as quiet and polluted the
        // correction tables); KnightByte's gate now classifies through
        // [`is_quiet_store_move`], which implements the corrected semantics.
        const TB_WIN_SCORE: i32 = 29_900;
        let trusted = match result.bound {
            Bound::Lower => result.raw_eval >= result.score,
            Bound::Upper => result.raw_eval <= result.score,
            Bound::Exact => false,
        };
        if self.params.use_correction_history
            && self.params.use_ref_correction_history
            && !result.in_check
            && result.score.abs() < MATE_LIMIT
            && result.score.abs() < TB_WIN_SCORE
            && (result.best_move.is_none()
                || is_quiet_store_move(result.position, result.best_move))
            && !trusted
        {
            self.correction.update(
                result.position,
                result.previous,
                result.score,
                result.corrected_eval,
                result.depth,
            );
        }
        result.score
    }

    /// Returns the path from the root to the node at `ply`: the moves played
    /// at plies `0..ply`. Used by the parity-trace ledger.
    pub(crate) fn path_to(&self, ply: usize) -> String {
        self.move_stack[..ply.min(self.move_stack.len())]
            .iter()
            .filter(|mv| !mv.is_none())
            .map(|mv| mv.to_uci())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Writes one parity-trace ledger line to stderr when the trace flag is
    /// on. `kind` is `n` for a negamax node, `q` for a quiescence node, `r`
    /// for a returned score.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn trace_node(
        &self,
        kind: char,
        ply: usize,
        depth: i32,
        alpha: i32,
        beta: i32,
        cut: bool,
        score: i32,
    ) {
        if !self.trace {
            return;
        }
        eprintln!(
            "T {kind} n {:>6} p {:>3} d {:>3} a {:>7} b {:>7} c {} s {:>7} m {}",
            self.nodes,
            ply,
            depth,
            alpha,
            beta,
            u8::from(cut),
            score,
            self.path_to(ply),
        );
    }
}

/// Builds a transposition-table entry, converting the score for `ply` and
/// clamping every field into its stored width.
pub(crate) fn tt_entry(
    position: &Position,
    ply: usize,
    depth: i32,
    score: i32,
    eval: i32,
    bound: Bound,
    mv: Move,
) -> TtEntry {
    TtEntry {
        key: position.key(),
        mv,
        score: score_to_tt(score.clamp(-MATE_SCORE, MATE_SCORE) as i16, ply as u16),
        eval: eval.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        depth: depth.clamp(0, i32::from(u8::MAX)) as u8,
        bound,
        // Replaced by `Searcher::store_tt`; kept here so the struct is complete.
        generation: 0,
    }
}

/// Returns the score of being mated in `ply` plies from the root.
#[inline]
pub(crate) const fn mated_in(ply: i32) -> i32 {
    -MATE_SCORE + ply
}

/// Derives nodes-per-second from a node count and an elapsed time.
///
/// An elapsed time of zero (an iteration that finished inside the clock
/// resolution) counts as one millisecond, so the division can never divide by
/// zero and a completed iteration is never reported as zero nps.
#[inline]
pub(crate) fn nps_of(nodes: u64, time_ms: u64) -> u64 {
    nodes * 1000 / time_ms.max(1)
}

/// Returns `true` when `mv` counts as quiet in the correction-history update
/// gate.
///
/// Bug B37 fix (reference bug 149): quiet means the destination is NOT occupied
/// by the opponent (no ordinary capture) and the move is neither an en-passant
/// capture nor a promotion — the fixed reference's `isQuiet` semantics,
/// evaluated on the node position (`position.side()` is the mover at the store
/// site). Extracted from the gate so the classification is directly unit-
/// testable: the pre-fix gate omitted the capture disjunct, so ordinary
/// captures folded their eval-error diff into the correction tables.
///
/// `Move::NONE` counts as quiet (the gate's "no best move" branch).
pub(crate) fn is_quiet_store_move(position: &Position, mv: Move) -> bool {
    if mv.is_none() {
        return true;
    }
    let target = position.piece_at(mv.to());
    let capture = target != EMPTY && piece::color_of(target) != position.side();
    !capture && mv.flag() != crate::types::r#move::flags::EN_PASSANT && !mv.is_promotion()
}

#[cfg(test)]
mod tests {
    use super::{Searcher, is_quiet_store_move};
    use crate::board::fen::STARTPOS_FEN;
    use crate::board::movegen::legal;
    use crate::board::position::Position;
    use crate::types::r#move::{Move, flags};

    /// Plays the UCI moves in order and returns every position reached,
    /// starting with the given one.
    fn play(start: &Position, moves: &[&str]) -> Vec<Position> {
        let mut positions = vec![*start];
        for text in moves {
            let current = *positions.last().expect("at least the start");
            let mv = legal(&current)
                .as_slice()
                .iter()
                .copied()
                .find(|mv| mv.to_uci() == *text)
                .unwrap_or_else(|| panic!("{text} is not legal"));
            positions.push(current.make_move(mv));
        }
        positions
    }

    #[test]
    fn a_repetition_of_an_earlier_game_position_is_seen_by_the_search() {
        // The game returns to the start position after four plies, then the
        // search's root is that position and its path plays g1f3 g8f6 again.
        // The node two plies below the root equals the game position two plies
        // before the root, which only the game history can reveal.
        let start = Position::from_fen(STARTPOS_FEN).unwrap();
        let game = play(&start, &["g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6"]);
        let root = game[4];
        let node = game[6];

        let mut searcher = Searcher::new(1);
        searcher.keys[0] = root.key();
        searcher.keys[1] = game[5].key();

        assert_eq!(searcher.repetition_count(&node, 2), 0, "no history yet");
        assert!(!searcher.is_draw(&node, 2));

        let earlier: Vec<u64> = game[..4].iter().map(|position| position.key()).collect();
        searcher.set_game_history(&earlier);
        assert_eq!(searcher.repetition_count(&node, 2), 1);
        assert!(searcher.is_draw(&node, 2));
    }

    #[test]
    fn history_beyond_an_irreversible_move_cannot_repeat() {
        // A pawn move resets the fifty-move clock, so nothing before it counts
        // even when the key happens to match.
        let start = Position::from_fen(STARTPOS_FEN).unwrap();
        let game = play(&start, &["g1f3", "g8f6", "f3g1", "f6g8", "e2e4"]);
        let node = game[5];
        assert_eq!(node.halfmove_clock(), 0);

        let mut searcher = Searcher::new(1);
        searcher.keys[0] = node.key();
        let earlier: Vec<u64> = game[..5].iter().map(|position| position.key()).collect();
        searcher.set_game_history(&earlier);
        assert_eq!(searcher.repetition_count(&node, 0), 0);
    }

    #[test]
    fn game_history_is_trimmed_to_the_last_hundred_plies() {
        let mut searcher = Searcher::new(1);
        let keys: Vec<u64> = (0..250).collect();
        searcher.set_game_history(&keys);
        assert_eq!(searcher.game_keys.len(), 100);
        assert_eq!(searcher.game_keys[0], 150);
        assert_eq!(searcher.game_keys[99], 249);
        searcher.set_game_history(&[]);
        assert!(searcher.game_keys.is_empty());
    }

    #[test]
    fn the_correction_quiet_gate_accepts_quiet_moves_and_rejects_captures() {
        // Bug B37 fix (reference bug 149): an ordinary capture must not count
        // as quiet, so its eval-error diff no longer reaches the correction
        // tables. Built from the start position by play, so no new FEN
        // constant is introduced.
        let start = Position::from_fen(STARTPOS_FEN).unwrap();

        let quiet = legal(&start)
            .as_slice()
            .iter()
            .copied()
            .find(|mv| mv.to_uci() == "e2e3")
            .expect("e2e3 is legal in the start position");
        assert!(is_quiet_store_move(&start, quiet));
        assert!(is_quiet_store_move(&start, Move::NONE));

        // After 1.e4 d5 white's e4 pawn can capture the d5 pawn: a capture
        // must be rejected (the pre-fix gate accepted it).
        let after_e4_d5 = play(&start, &["e2e4", "d7d5"])[2];
        let capture = legal(&after_e4_d5)
            .as_slice()
            .iter()
            .copied()
            .find(|mv| mv.to_uci() == "e4d5")
            .expect("e4d5 is legal after 1.e4 d5");
        assert!(!is_quiet_store_move(&after_e4_d5, capture));

        // After 1.e4 a6 2.e5 d5 white's e5 pawn can capture en passant on d6:
        // also rejected, via the flag.
        let ep_position = play(&start, &["e2e4", "a7a6", "e4e5", "d7d5"])[4];
        let en_passant = legal(&ep_position)
            .as_slice()
            .iter()
            .copied()
            .find(|mv| mv.to_uci() == "e5d6")
            .expect("e5d6 en passant is legal after 1.e4 a6 2.e5 d5");
        assert_eq!(en_passant.flag(), flags::EN_PASSANT);
        assert!(!is_quiet_store_move(&ep_position, en_passant));

        // Promotion: classification only — the helper reads occupancy and
        // flags, not geometry, so the squares are arbitrary as long as the
        // target is empty (f6 here) and the flag says promotion.
        let promotion = Move::new(36, 53, flags::PROMO_QUEEN);
        assert!(!is_quiet_store_move(&ep_position, promotion));
    }
}
