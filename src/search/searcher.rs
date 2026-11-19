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

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::board::position::Position;
use crate::eval::nnue::Evaluator;
use crate::search::MAX_PLY;
use crate::search::correction::CorrectionHistory;
use crate::search::history::{History, Previous};
use crate::search::killers::Killers;
use crate::search::limits::Limits;
use crate::search::move_picker::PickerState;
use crate::search::params::SearchParams;
use crate::search::pv::PvTable;
use crate::search::time_manager::{TimeControl, TimeManager};
use crate::search::tt::{Bound, TtEntry, score_from_tt, score_to_tt};
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

/// What the search reports to its observer.
///
/// Two kinds exist, matching the reference's output exactly: a completed
/// iteration (one `info depth …` line) and the single `info currmove` report
/// of iteration 1's first root move (the step-0 transcripts show one such
/// line per search, before the depth-1 line — and none when the search never
/// reached iteration 1).
#[derive(Clone, Debug)]
pub enum SearchEvent<'a> {
    /// A completed iteration.
    Info(&'a SearchInfo),
    /// The first root move of iteration 1 is about to be searched: the move
    /// as the picker yields it, its move number, and the node count at that
    /// moment (1).
    CurrMove {
        /// The move the root's picker yields first.
        mv: Move,
        /// The move's index in the root move loop, counting from one.
        number: u64,
        /// The node count when the move begins.
        nodes: u64,
    },
}

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
    pub(crate) tables: crate::search::tables::SharedTables,
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
    /// The reusable full-refresh evaluator over the embedded net (its two
    /// accumulator buffers are allocated once per searcher, not per node).
    pub(crate) evaluator: crate::eval::nnue::evaluate::Evaluator<'static>,
    /// Start of the current search, used for the time budgets.
    pub(crate) start: std::time::Instant,
    /// Cooperative stop flag shared with the UCI loop. The loop sets it on
    /// `stop` (and on the join-first fallback); the search polls it every
    /// 1024 nodes and before every iteration. A searcher created without the
    /// UCI layer owns a flag that is never set.
    pub(crate) stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Step 6 (`go searchmoves`): the legal subset of the listed root moves
    /// for the current search. Empty means no restriction. Set once per
    /// search in [`Searcher::search`], read at every root node.
    pub(crate) root_filter: Vec<Move>,
    /// Step 8 (Lazy SMP): the shared node counter of a multi-threaded search.
    /// The main thread only READS it (its own count stays exact, which is
    /// what keeps the `Threads=1` node pins bit-identical); each helper
    /// thread WRITES its blocks to it (see `publishes_nodes`). `None` = no
    /// helpers (the single-threaded path).
    pub(crate) helper_nodes: Option<Arc<AtomicU64>>,
    /// Whether this searcher is a helper that publishes its node blocks to
    /// `helper_nodes` (bug B44: the main searcher used to publish its own
    /// count into the counter it reports from, and the helpers never
    /// published at all).
    pub(crate) publishes_nodes: bool,
    /// This searcher's node count already published to the shared counter
    /// (the block frontier; only helpers publish).
    pub(crate) published_nodes: u64,
    /// Whether iteration 1's `info currmove` report was emitted (step 10:
    /// exactly one per search).
    pub(crate) currmove_reported: bool,
    /// Number of completed iterations; the first one always runs to the end.
    pub(crate) completed_depth: i32,
    /// When the trace flag is on, every search node writes one ledger line to
    /// stderr. Enabled per process with `KNIGHTBYTE_TRACE`; it is the
    /// parity-hunt dump, never active in normal operation.
    pub(crate) trace: bool,
    /// Incremental NNUE accumulator pair per search ply: the pair at `ply`
    /// holds the white/black accumulators of the position searched there.
    /// Pushed from the parent pair for every made move; a perspective whose
    /// own king moved falls back to a full refresh (the feature rows depend
    /// on the own king's region and mirror).
    accumulator_stack: Vec<[Vec<i16>; 2]>,
}

impl Searcher {
    /// Creates a search state with a transposition table of `tt_megabytes`.
    ///
    /// Every per-ply array is heap allocated: a search stack sized for
    /// [`MAX_PLY`] must not live on the main thread's stack on platforms where
    /// that stack is only one megabyte.
    pub fn new(tt_megabytes: usize) -> Self {
        Self {
            tables: crate::search::tables::SharedTables::new(tt_megabytes),
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
            evaluator: Evaluator::new(crate::eval::nnue::embedded_net()),
            start: std::time::Instant::now(),
            stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            root_filter: Vec::new(),
            helper_nodes: None,
            publishes_nodes: false,
            published_nodes: 0,
            currmove_reported: false,
            completed_depth: 0,
            trace: std::env::var_os("KNIGHTBYTE_TRACE").is_some(),
            accumulator_stack: Self::accumulator_stack(),
        }
    }

    /// Allocates the per-ply accumulator pairs over the embedded network's
    /// first-layer width.
    fn accumulator_stack() -> Vec<[Vec<i16>; 2]> {
        let l1 = crate::eval::nnue::embedded_net().l1();
        (0..=MAX_PLY + 1)
            .map(|_| [vec![0_i16; l1], vec![0_i16; l1]])
            .collect()
    }

    /// Forgets every position-dependent table.
    ///
    /// `ucinewgame` and each benchmark position call this, which is what makes
    /// benchmark node counts reproducible from run to run.
    pub fn clear(&mut self) {
        self.tables.tt.clear();
        self.tables.eval_cache.clear();
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

    /// Returns the keys of the positions that preceded the last search's
    /// root, oldest first (see [`Self::set_game_history`]).
    ///
    /// The ponder path reads them to extend the history by one position
    /// when the ponder move is applied.
    pub fn game_history(&self) -> &[u64] {
        &self.game_keys
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
        self.tables.tt.hashfull()
    }

    /// Resizes the transposition table to `megabytes`.
    ///
    /// The exclusive borrow holds because `setoption` arrives when no search
    /// is active (the UCI loop's join-first rule), so no other thread holds
    /// a handle to the shared tables.
    pub fn set_hash(&mut self, megabytes: usize) {
        self.tables.resize(megabytes.clamp(1, 1024));
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
        observer: &mut dyn FnMut(SearchEvent),
    ) -> SearchResult {
        // The search clock is started by the `init_*` call that configured the
        // time manager; it is deliberately not restarted here.
        self.limits = limits;
        self.completed_depth = 0;
        self.nodes = 0;
        self.published_nodes = 0;
        self.seldepth = 0;
        self.stopped = false;
        self.currmove_reported = false;
        self.pv.reset();
        self.killers.clear();
        // The learned tables start cold every search, as in the predecessor:
        // its UCI layer builds a fresh per-thread state object for each `go`,
        // so history and correction history never survive a search (B41).
        // Only the transposition table persists.
        self.history.clear();
        self.correction.clear();
        self.reset_stacks();
        *self.nodes_per_move = [0; 64 * 64];
        // The predecessor's main thread advances the TT generation at the
        // start of every iterative-deepening search (NewSearch + _currentAge).
        // Keep that: the age blending in the replacement policy depends on it.
        self.tables.tt.new_generation();

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

        // Step 6 (`go searchmoves`): the root is restricted to the listed
        // moves. The filter holds the listed moves that are legal here; an
        // empty list — none given, or every entry illegal — searches
        // everything (the reference ignores an illegal-only list, verified
        // 2026-11-18).
        self.root_filter = self
            .limits
            .root_moves
            .iter()
            .filter(|mv| root_moves.as_slice().contains(mv))
            .copied()
            .collect();
        // The no-search fallback best move comes from the effective root
        // list: the first restricted move when the filter is active, the
        // first legal move otherwise.
        let fallback_move = if self.root_filter.is_empty() {
            root_moves.as_slice()[0]
        } else {
            self.root_filter[0]
        };

        let max_depth = self.limits.depth_limit();
        // Reporting clock for the `info` lines: it starts with the search,
        // so every iteration publishes the elapsed time from the `go` (the
        // time manager keeps its own clock, started by the `init_*` call).
        let started = std::time::Instant::now();
        let mut best = SearchResult {
            best_move: fallback_move,
            score: 0,
            depth: 0,
            seldepth: 0,
            nodes: 0,
            pv: Vec::new(),
        };
        let mut last_score = 0;
        // The root accumulator pair is refreshed once per search: every
        // deeper ply pushes its pair incrementally from the parent's.
        self.refresh_accumulators(position, 0);

        for depth in 1..=max_depth {
            // The stop flag is honoured before each iteration too: a `stop`
            // that arrived between iterations must not start a new one. The
            // per-node polling in `should_stop` alone could miss it for up
            // to 1023 nodes into the next iteration.
            if self.stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let Some((score, pv)) = self.search_root(position, depth, last_score, observer) else {
                break;
            };
            last_score = score;
            best.score = score;
            best.depth = depth;
            best.seldepth = self.seldepth;
            best.nodes = self.reported_nodes();
            best.pv = pv;
            self.completed_depth = depth;
            if let Some(&first) = best.pv.first() {
                best.best_move = first;
            }
            let time_ms = started.elapsed().as_millis() as u64;
            observer(SearchEvent::Info(&SearchInfo {
                depth: best.depth,
                seldepth: best.seldepth,
                score: best.score,
                nodes: best.nodes,
                time_ms,
                nps: nps_of(best.nodes, time_ms),
                // Read once per completed iteration, not per node: the
                // occupancy is only published on the `info` line.
                hashfull: self.tables.tt.hashfull(),
                pv: best.pv.clone(),
            }));

            // Time management: record the completed depth, feed the node
            // statistics and the best-move stability, then decide whether
            // the next iteration starts. A proven mate breaks first — a
            // proven mate needs no more depth. With a `go mate N` limiter the
            // break waits for a mate within N moves (a slower proven mate
            // keeps searching, per the step-0 capture); being mated always
            // ends it.
            let mated = score <= -MATE_LIMIT;
            let our_mate_moves = (MATE_SCORE - score + 1) / 2;
            let within_mate_bound = self
                .limits
                .mate
                .is_none_or(|bound| our_mate_moves <= bound as i32);
            if score >= MATE_LIMIT && within_mate_bound || mated {
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

        best.nodes = self.reported_nodes();
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
        observer: &mut dyn FnMut(SearchEvent),
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
            let score = self.negamax(
                position,
                alpha,
                beta,
                depth,
                0,
                None,
                false,
                Move::NONE,
                0,
                observer,
            );
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

    /// Returns the static evaluation of `position` from the accumulator pair
    /// at `ply`, optionally via the cache.
    ///
    /// The pair at `ply` must be current for `position` (pushed by the parent
    /// node or refreshed at the search root). The evaluation reuses this
    /// searcher's [`Evaluator`] for the output head.
    pub(crate) fn evaluate_stacked(&mut self, position: &Position, ply: usize) -> i32 {
        let use_cache = self.params.use_eval_cache;
        let key = position.key();
        if use_cache && let Some(cached) = self.tables.eval_cache.probe(key) {
            return i32::from(cached);
        }
        let value = crate::eval::nnue::evaluate::evaluate_pair(
            self.evaluator.net(),
            position.side(),
            &self.accumulator_stack[ply],
        );
        // The store is gated by the same lever as the probe: with the cache
        // off nothing ever reads it, so writing would only cost a shared
        // cache line per node (and ping it between threads).
        if use_cache {
            self.tables
                .eval_cache
                .store(key, value.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        }
        value
    }

    /// Full refresh of the accumulator pair at `ply` from `position`.
    ///
    /// Used at the search root and whenever a perspective's own king moved.
    pub(crate) fn refresh_accumulators(&mut self, position: &Position, ply: usize) {
        let net = self.evaluator.net();
        let pair = &mut self.accumulator_stack[ply];
        crate::eval::nnue::evaluate::refresh_into(net, position, WHITE, &mut pair[0]);
        crate::eval::nnue::evaluate::refresh_into(net, position, BLACK, &mut pair[1]);
    }

    /// Copies the validated pair of `ply` into `ply + 1` (the null-move child:
    /// same placement, only the side to move flips).
    pub(crate) fn copy_accumulators(&mut self, ply: usize) {
        let (head, tail) = self.accumulator_stack.split_at_mut(ply + 1);
        tail[0].clone_from(&head[ply]);
    }

    /// Computes the accumulator pair at `ply + 1` by applying `mv` to the
    /// validated pair at `ply`.
    ///
    /// A perspective whose own king moved falls back to a full refresh of
    /// `child`: its region and mirror may have changed, which invalidates
    /// every row. All other perspectives keep their frame (the parent's), so
    /// at most four rows change: the moved piece, the captured piece (its own
    /// square for en passant), the promotion piece and the castling rook.
    /// The delta application lives in [`crate::eval::nnue::evaluate`], next
    /// to the row kernels it calls.
    pub(crate) fn push_accumulators(
        &mut self,
        parent: &Position,
        child: &Position,
        mv: Move,
        ply: usize,
    ) {
        let net = self.evaluator.net();
        let (parent_stack, child_stack) = self.accumulator_stack.split_at_mut(ply + 1);
        crate::eval::nnue::evaluate::push_accumulators(
            net,
            parent,
            child,
            mv,
            &parent_stack[ply],
            &mut child_stack[0],
        );
    }

    /// Evaluates one child position through the incremental path.
    ///
    /// Diagnostic hook for the release microbench: it pushes the child
    /// accumulator pair from the pair at ply 0 and then runs the output head.
    /// The pair at ply 0 must already describe `position` (a completed search
    /// leaves it ready). Not used by the search itself.
    #[doc(hidden)]
    pub fn bench_incremental_eval(&mut self, position: &Position, mv: Move) -> i32 {
        let child = position.make_move(mv);
        self.push_accumulators(position, &child, mv, 0);
        crate::eval::nnue::evaluate::evaluate_pair(
            self.evaluator.net(),
            child.side(),
            &self.accumulator_stack[1],
        )
    }

    /// Installs a fresh, cleared stop flag and returns its handle.
    ///
    /// The UCI loop keeps the handle and sets it on `stop` (and on the
    /// join-first fallback for protocol-breaking commands); the search polls
    /// it every 1024 nodes and before every iteration. Arming before every
    /// `go` also clears any flag left over from a previous search.
    pub fn arm_stop_flag(&mut self) -> Arc<std::sync::atomic::AtomicBool> {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.stop_flag = Arc::clone(&flag);
        flag
    }

    /// Returns the shared tables (the step-7 handle).
    pub fn tables(&self) -> &crate::search::tables::SharedTables {
        &self.tables
    }

    /// Adopts `flag` as this searcher's stop flag instead of arming a fresh
    /// one.
    ///
    /// The worker pool uses this to give a helper the main search's flag, so
    /// the helper's iteration ends when the main thread's does.
    pub fn use_stop_flag(&mut self, flag: Arc<std::sync::atomic::AtomicBool>) {
        self.stop_flag = flag;
    }

    /// Replaces the shared tables (the step-7 handle).
    ///
    /// The worker pool swaps the shared handle in before every helper
    /// search, so all threads always probe the same table.
    pub fn set_tables(&mut self, tables: crate::search::tables::SharedTables) {
        self.tables = tables;
    }

    /// Releases the shared tables (bug B39 fix).
    ///
    /// A persistent helper must not keep the `SharedTables` clones between
    /// searches: the clones would keep the transposition table's `Arc` count
    /// above one forever, and every later `setoption Hash` resize would fail
    /// its exclusive-borrow check and kill the engine. After a search, the
    /// helper holds only its own minimal private tables until the next job
    /// arrives.
    pub fn release_tables(&mut self) {
        self.tables = crate::search::tables::SharedTables::new(1);
    }

    /// Attaches the shared node counter of a helper search.
    ///
    /// While attached, the reported node counts are this searcher's exact
    /// count plus the helpers' published blocks, and the node limit is also
    /// checked against that aggregate at the 1024-node poll.
    pub fn attach_helpers(&mut self, helper_nodes: Arc<AtomicU64>) {
        self.helper_nodes = Some(helper_nodes);
    }

    /// Detaches the shared node counter (back to the exact single-threaded
    /// reporting).
    pub fn detach_helpers(&mut self) {
        self.helper_nodes = None;
        self.publishes_nodes = false;
    }

    /// Makes this searcher a HELPER that publishes its node blocks to
    /// `counter` (the counter the main searcher reports from).
    pub fn attach_publisher(&mut self, counter: Arc<AtomicU64>) {
        self.helper_nodes = Some(counter);
        self.publishes_nodes = true;
    }

    /// Publishes this searcher's unpublished node block to the shared
    /// counter.
    ///
    /// Helpers call this from the 1024-node poll, where it publishes only
    /// full 2048-node blocks (no per-node atomic), and once with `force` when
    /// their search ends, which flushes the final partial block. The main
    /// thread never publishes (its count stays exact).
    pub fn publish_node_blocks(&mut self, force: bool) {
        if !self.publishes_nodes {
            return;
        }
        if let Some(counter) = &self.helper_nodes {
            let unpublished = self.nodes - self.published_nodes;
            if unpublished > 0 && (force || unpublished >= 2048) {
                counter.fetch_add(unpublished, Ordering::Relaxed);
                self.published_nodes = self.nodes;
            }
        }
    }

    /// Returns the node count this searcher reports: its own exact count
    /// plus, while helpers are attached, the helpers' published blocks.
    pub fn reported_nodes(&self) -> u64 {
        self.nodes
            + self
                .helper_nodes
                .as_ref()
                .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }

    /// Returns whether the search must stop before visiting another node.
    ///
    /// Two checks stay on every node, and they are plain integer comparisons:
    /// the exact `go nodes` limit against this thread's own count, and the
    /// remembered stop decision. The expensive inputs — the wall clock, the
    /// shared stop flag and the helpers' published node blocks — are touched
    /// only every 1024 nodes (the plan's step-4 rule: no clock read and no
    /// atomic access per node).
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
        if self.nodes & 1023 == 0 {
            // Step 8: helpers publish their counts in blocks; with helpers
            // attached, the node limit is enforced on the aggregate here.
            if self.publishes_nodes {
                self.publish_node_blocks(false);
            } else if self.helper_nodes.is_some()
                && let Some(limit) = self.limits.nodes
                && self.reported_nodes() >= limit
            {
                self.stopped = true;
                return true;
            }
            if self.stop_flag.load(Ordering::Relaxed)
                || self.time_manager.should_stop(self.now_ms())
            {
                self.stopped = true;
            }
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
        entry.generation = self.tables.tt.generation();
        self.tables.tt.store(entry);
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

    /// A tiny deterministic generator for the accumulator walk.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % n
        }
    }

    /// Plays `moves` from the start position with a fresh searcher, pushing
    /// the accumulator incrementally and comparing it against the full
    /// refresh after every move. Returns the castle and en-passant counts.
    fn assert_incremental_walk(moves: &[&str]) -> (u32, u32) {
        let mut searcher = Searcher::new(16);
        let mut position = Position::from_fen(STARTPOS_FEN).expect("startpos");
        searcher.refresh_accumulators(&position, 0);
        let mut castles = 0;
        let mut en_passants = 0;
        for (ply, uci) in moves.iter().enumerate() {
            let legal_moves = legal(&position);
            let mv = *legal_moves
                .as_slice()
                .iter()
                .find(|candidate| candidate.to_uci() == *uci)
                .unwrap_or_else(|| panic!("move {uci} is not legal at ply {ply}"));
            let next = position.make_move(mv);
            searcher.push_accumulators(&position, &next, mv, ply);
            let incremental = crate::eval::nnue::evaluate::evaluate_pair(
                searcher.evaluator.net(),
                next.side(),
                &searcher.accumulator_stack[ply + 1],
            );
            let full = searcher.evaluator.evaluate(&next);
            assert_eq!(
                incremental, full,
                "ply {ply} after {uci}: incremental eval diverges"
            );
            if mv.flag() == crate::types::r#move::flags::CASTLE {
                castles += 1;
            }
            if mv.flag() == crate::types::r#move::flags::EN_PASSANT {
                en_passants += 1;
            }
            position = next;
        }
        (castles, en_passants)
    }

    /// The incremental accumulator must reproduce the full refresh
    /// bit-exactly at every position of every walk. The deterministic
    /// segments cover the en-passant capture and castling for both sides; the
    /// seeded random walks add broad coverage (the promotion delta has its
    /// own test below).
    #[test]
    fn the_incremental_accumulator_matches_the_full_refresh() {
        // En passant: the white e-pawn captures the d5 pawn behind it.
        let (castles, en_passants) =
            assert_incremental_walk(&["e2e4", "a7a6", "e4e5", "d7d5", "e5d6"]);
        assert_eq!(en_passants, 1, "the en-passant capture must be walked");
        assert_eq!(castles, 0);

        // Both sides castle: the rook delta runs for white and for black.
        let (castles, en_passants) = assert_incremental_walk(&[
            "e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1", "f8c5", "d2d3", "e8g8",
        ]);
        assert_eq!(castles, 2, "both sides must have castled");
        assert_eq!(en_passants, 0);

        // Seeded random walks: 8 x 80 plies over the start position. The
        // identity check at every pushed position is the point; special move
        // deltas are pinned by the deterministic segments above.
        for seed in 1..=8u64 {
            let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut searcher = Searcher::new(16);
            let mut position = Position::from_fen(STARTPOS_FEN).expect("startpos");
            searcher.refresh_accumulators(&position, 0);
            for ply in 0..80usize {
                let moves = legal(&position);
                if moves.is_empty() {
                    break;
                }
                let mv = moves.as_slice()[rng.below(moves.len() as u64) as usize];
                let next = position.make_move(mv);
                searcher.push_accumulators(&position, &next, mv, ply);
                let incremental = crate::eval::nnue::evaluate::evaluate_pair(
                    searcher.evaluator.net(),
                    next.side(),
                    &searcher.accumulator_stack[ply + 1],
                );
                let full = searcher.evaluator.evaluate(&next);
                assert_eq!(
                    incremental,
                    full,
                    "seed {seed} ply {ply} after {}: incremental eval diverges",
                    mv.to_uci()
                );
                position = next;
            }
        }
        // Castle coverage comes from the deterministic segment above; random
        // games rarely castle, so no coverage assert here.
    }

    /// The promotion delta matches the full refresh. The FEN is verified
    /// against the external verifier at test time (project rule): the white
    /// b-pawn promotes on the empty b8 square.
    #[test]
    fn the_promotion_delta_matches_the_full_refresh() {
        // 6k1/1P6/8/8/8/8/8/6K1 w - - 0 1
        const PROMO_FEN: &str = "6k1/1P6/8/8/8/8/5PPP/6K1 w - - 0 1";
        verify_fen_externally(PROMO_FEN);

        let mut searcher = Searcher::new(16);
        let position = Position::from_fen(PROMO_FEN).expect("verified FEN");
        searcher.refresh_accumulators(&position, 0);
        let moves = legal(&position);
        let mv = *moves
            .as_slice()
            .iter()
            .find(|mv| mv.to_uci() == "b7b8q")
            .expect("the promotion must be legal");
        let child = position.make_move(mv);
        searcher.push_accumulators(&position, &child, mv, 0);
        let incremental = crate::eval::nnue::evaluate::evaluate_pair(
            searcher.evaluator.net(),
            child.side(),
            &searcher.accumulator_stack[1],
        );
        let full = searcher.evaluator.evaluate(&child);
        assert_eq!(
            incremental, full,
            "promotion delta diverges: incremental {incremental}, full {full}"
        );
    }

    /// Verifies one FEN through the configured external verifier, so the test
    /// FENs of this module obey the project rule without the integration-test
    /// helper module.
    fn verify_fen_externally(fen: &str) {
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let verifier = std::env::var_os("KNIGHTBYTE_FEN_VERIFIER")
            .expect("set KNIGHTBYTE_FEN_VERIFIER before running the tests");
        let mut child = Command::new(verifier)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cannot start the verifier");
        child
            .stdin
            .as_mut()
            .expect("verifier stdin")
            .write_all(
                format!("uci\nisready\nposition fen {fen}\nd\ngo depth 1\nquit\n").as_bytes(),
            )
            .expect("write verifier commands");
        let stdout = child.wait_with_output().expect("wait for verifier").stdout;
        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        assert!(!stdout.contains("Illegal"), "verifier rejected {fen}");
        assert!(stdout.contains("bestmove"), "verifier produced no bestmove");
        let reported = stdout
            .lines()
            .find_map(|line| line.strip_prefix("Fen: "))
            .expect("verifier did not report a FEN");
        let input: Vec<&str> = fen.split_whitespace().take(4).collect();
        let reported: Vec<&str> = reported.split_whitespace().take(4).collect();
        assert_eq!(input, reported, "FEN did not round-trip: {fen}");
    }

    #[test]
    fn the_eval_cache_lever_gates_the_cache_consult() {
        // Step 11: `UseEvalCache` is a pure memoisation — identical values,
        // identical node counts — so the node-count independence test cannot
        // see it. The behavioural proof: with the lever on, `evaluate`
        // returns the CACHED value (here a deliberately wrong one); with it
        // off, the cache is never consulted and the real evaluation comes
        // back.
        let mut searcher = Searcher::new(2);
        let position = Position::from_fen(STARTPOS_FEN).expect("the start FEN must parse");
        let key = position.key();
        searcher.tables.eval_cache.store(key, 12_345);
        searcher.refresh_accumulators(&position, 0);

        searcher.params_mut().use_eval_cache = true;
        assert_eq!(
            searcher.evaluate_stacked(&position, 0),
            12_345,
            "with the lever on the cache must be consulted"
        );
        searcher.params_mut().use_eval_cache = false;
        assert_ne!(
            searcher.evaluate_stacked(&position, 0),
            12_345,
            "with the lever off the cache must be ignored"
        );
    }

    #[test]
    fn the_eval_cache_is_not_written_while_the_lever_is_off() {
        // B42: the store used to run unconditionally, writing a cache that
        // nothing read. With the lever off the cache must stay untouched;
        // with it on the evaluation must be stored and then served.
        let mut searcher = Searcher::new(2);
        let position = Position::from_fen(STARTPOS_FEN).expect("the start FEN must parse");
        let key = position.key();
        searcher.refresh_accumulators(&position, 0);

        searcher.params_mut().use_eval_cache = false;
        let value = searcher.evaluate_stacked(&position, 0);
        assert_eq!(
            searcher.tables.eval_cache.probe(key),
            None,
            "an evaluation must not be stored while the lever is off"
        );

        searcher.params_mut().use_eval_cache = true;
        assert_eq!(searcher.evaluate_stacked(&position, 0), value);
        assert_eq!(
            searcher.tables.eval_cache.probe(key),
            Some(value as i16),
            "with the lever on the evaluation must be stored"
        );
    }

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
