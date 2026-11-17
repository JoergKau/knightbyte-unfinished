//! Time management.
//!
//! The manager converts the UCI clock inputs into soft and hard stop limits
//! and decides after every completed iteration whether to start the next
//! one. It is stateful: the multipliers that refine the base allocation
//! (best-move stability, score stability, panic mode, clock balance) change
//! while the search runs.
//!
//! Allocation (the reference's formulas):
//!
//! * tournament (`movestogo` given): remaining time divided by the moves to
//!   the next control, never more than a ten-move share;
//! * sudden death: a thirtieth of the clock plus the full increment;
//! * clock imbalance (two-sided, off by default, superseding the one-sided
//!   time-advantage multiplier): scale the allocation continuously by the
//!   clock ratio when both clocks are above five seconds;
//! * time advantage (on): up to 1.5x extra when clearly ahead on the clock
//!   in sudden death;
//! * tactical floor: with more than thirty seconds on the clock, at least
//!   three seconds or one percent of the clock;
//! * safety buffer: fifty milliseconds under a one-second clock, else five
//!   hundred;
//! * hard limit: at most four allocations and at most a quarter of the
//!   clock;
//! * overhead clamp (on): the soft limit also keeps the move-overhead margin
//!   intact;
//! * fixed `movetime`: soft = 95 %, hard = 100 % of the budget.
//!
//! Stopping:
//!
//! * the **soft** limit — the base allocation scaled by the stability and
//!   panic multipliers — stops between iterations and cuts off a runaway
//!   iteration mid-flight, but never before five completed iterations (the
//!   min-depth guard: a soft stop must never play a depth-1 blunder);
//! * the **hard** limit — a quarter of the clock — is the absolute backstop.
//!
//! The stable-TM model (on, timed searches only) replaces the
//! PV-stability/panic multipliers with the reference's three-signal scaler:
//! a best-move-stability ladder, a score-stability ladder and a
//! node-fraction term (the share of the search spent on the best root
//! move), clamped to 0.12..2.45 of the allocation and to the hard limit
//! minus the move overhead.
//!
//! All arithmetic on the limits is integer milliseconds; the multipliers are
//! computed in floating point exactly like the reference's.

use crate::search::limits::Limits;

/// Controls for one move, from the UCI clock inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeControl {
    /// Remaining time for the side to move, in milliseconds.
    pub time_left_ms: u64,
    /// The opponent's remaining time, in milliseconds (0 = unknown).
    pub opponent_time_ms: u64,
    /// Increment for the side to move, in milliseconds.
    pub increment_ms: u64,
    /// Moves left until the next time control; `None` = sudden death.
    pub moves_to_go: Option<u32>,
}

/// Per-move overhead subtracted from every budget, in milliseconds.
pub const DEFAULT_MOVE_OVERHEAD_MS: u64 = 50;

/// Iterations that must complete before a soft-limit stop is honoured.
///
/// A soft stop before five completed iterations can leave the engine on a
/// shallow blunder when a single iteration explodes; the hard limit stays
/// the backstop, so the guard can never cause a forfeit.
const MIN_ITERATION_DEPTH: i32 = 5;

/// The best-move-stability ladder (percent of the allocation), indexed by
/// the count of consecutive stable iterations.
const BEST_MOVE_STABILITY_FACTOR: [i64; 5] = [250, 120, 90, 80, 75];

/// The score-stability ladder (percent), indexed by the count of
/// consecutive iterations whose score stayed within ten centipawns.
const SCORE_STABILITY_FACTOR: [i64; 5] = [125, 115, 100, 94, 88];

/// The node-fraction term's base and scale, in hundredths: a larger share of
/// the search spent on the best move makes it more trustworthy and reduces
/// the extra time.
const NODE_TM_BASE_HUNDREDTHS: i64 = 165;
const NODE_TM_SCALE_HUNDREDTHS: i64 = 147;

/// The scaler's clamp, in hundredths of the allocation.
const STABLE_TM_SCALE_MIN_HUNDREDTHS: i64 = 12;
const STABLE_TM_SCALE_MAX_HUNDREDTHS: i64 = 245;

/// The node-fraction term's minimum completed depth.
const NODE_TM_MIN_DEPTH: i32 = 5;

/// Stateful time manager: allocation, the stop decisions and the stability
/// trackers.
pub struct TimeManager {
    start_ms: u64,
    hard_limit_ms: u64,
    soft_limit_ms: u64,
    use_time: bool,
    /// Moves to the next control; `-1` marks fixed-movetime mode.
    moves_to_go: i32,
    move_overhead_ms: u64,
    completed_depth: i32,
    pv_changes: i32,
    last_pv_move: i32,
    stable_iterations: i32,
    previous_score: i32,
    panic_mode: bool,
    first_score: bool,
    score_stability: i32,
    stable_tm_nodes: u64,
    stable_tm_best_move_nodes: u64,

    use_stable_tm: bool,
    use_panic_mode: bool,
    use_time_advantage: bool,
    use_overhead_soft_clamp: bool,
    use_clock_imbalance: bool,
    clock_imbalance_ahead_scale: i32,
    clock_imbalance_behind_scale: i32,
    use_stable_shrink: bool,
}

impl Default for TimeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeManager {
    /// Creates a manager in infinite mode (no time limits).
    pub fn new() -> Self {
        Self {
            start_ms: 0,
            hard_limit_ms: u64::MAX,
            soft_limit_ms: u64::MAX,
            use_time: false,
            moves_to_go: 0,
            move_overhead_ms: DEFAULT_MOVE_OVERHEAD_MS,
            completed_depth: 0,
            pv_changes: 0,
            last_pv_move: 0,
            stable_iterations: 0,
            previous_score: 0,
            panic_mode: false,
            first_score: true,
            score_stability: 0,
            stable_tm_nodes: 0,
            stable_tm_best_move_nodes: 0,

            use_stable_tm: true,
            use_panic_mode: true,
            use_time_advantage: true,
            use_overhead_soft_clamp: true,
            use_clock_imbalance: false,
            clock_imbalance_ahead_scale: 50,
            clock_imbalance_behind_scale: 50,
            use_stable_shrink: false,
        }
    }

    /// Sets the lever states the option layer carries.
    pub fn set_levers(
        &mut self,
        use_stable_tm: bool,
        use_clock_imbalance: bool,
        clock_imbalance_ahead_scale: i32,
        clock_imbalance_behind_scale: i32,
    ) {
        self.use_stable_tm = use_stable_tm;
        self.use_clock_imbalance = use_clock_imbalance;
        self.clock_imbalance_ahead_scale = clock_imbalance_ahead_scale;
        self.clock_imbalance_behind_scale = clock_imbalance_behind_scale;
    }

    /// Initializes time management from the UCI clock inputs. `now_ms` is
    /// the search's start on the monotonic clock; every limit is stored
    /// relative to it.
    pub fn init(&mut self, control: &TimeControl, overhead_ms: u64, now_ms: u64) {
        self.start_ms = now_ms;
        self.use_time = true;
        self.move_overhead_ms = overhead_ms;
        self.completed_depth = 0;
        self.reset_pv_stability();
        self.panic_mode = false;
        self.first_score = true;

        self.moves_to_go = control.moves_to_go.map_or(0, |moves| moves as i32);

        let my_time = control.time_left_ms.max(100);
        let their_time = control.opponent_time_ms;

        let mut alloc: u64;
        if self.moves_to_go > 0 {
            // Tournament: a share of the clock, never more than a ten-move
            // share for a single move.
            alloc = my_time / self.moves_to_go as u64;
            alloc = alloc.min(my_time / 10);
        } else {
            // Sudden death: a thirtieth of the clock plus the full increment.
            alloc = (my_time / 30).saturating_add(control.increment_ms);
        }

        // Two-sided clock-imbalance scaler; requires both clocks above five
        // seconds so near-zero clocks cannot distort the ratio.
        if self.use_clock_imbalance && their_time > 0 && my_time > 5_000 {
            let ratio = my_time as f64 / (my_time + their_time) as f64;
            let scale = if ratio >= 0.5 {
                1.0 + (ratio - 0.5) * 2.0 * (self.clock_imbalance_ahead_scale as f64 / 100.0)
            } else {
                1.0 - (0.5 - ratio) * 2.0 * (self.clock_imbalance_behind_scale as f64 / 100.0)
            };
            alloc = (alloc as f64 * scale) as u64;
        } else if self.use_time_advantage
            && self.moves_to_go == 0
            && their_time > 0
            && my_time > 30_000
        {
            // One-sided advantage: reinvest up to half the surplus.
            let surplus = my_time.saturating_sub(their_time);
            if surplus > 0 {
                let advantage = 1.0 + (surplus as f64 / my_time as f64).min(0.5);
                alloc = (alloc as f64 * advantage) as u64;
            }
        }

        // Tactical floor: sharp positions get at least three seconds or one
        // percent of the clock when there is time to spare.
        if my_time > 30_000 {
            let min_alloc = 3_000_u64.max(my_time / 100);
            alloc = alloc.max(min_alloc);
        }

        // Safety buffer: keep enough reserve to answer the next `go`.
        let safety_buffer: u64 = if my_time < 1_000 { 50 } else { 500 };
        if alloc > my_time.saturating_sub(safety_buffer) {
            alloc = my_time.saturating_sub(safety_buffer).max(10);
        }

        self.soft_limit_ms = now_ms + alloc;
        self.hard_limit_ms = now_ms + (alloc * 4).min(my_time / 4);
        if self.hard_limit_ms < self.soft_limit_ms {
            self.hard_limit_ms = self.soft_limit_ms;
        }

        // Overhead hardening: the soft stop also keeps the lag margin intact.
        if self.use_overhead_soft_clamp && self.moves_to_go != -1 {
            let max_soft = self.hard_limit_ms.saturating_sub(overhead_ms);
            if max_soft > now_ms && self.soft_limit_ms > max_soft {
                self.soft_limit_ms = max_soft;
            }
        }
    }

    /// Sets a fixed `movetime` budget: soft = 95 %, hard = 100 %.
    pub fn set_fixed_time(&mut self, time_ms: u64, overhead_ms: u64, now_ms: u64) {
        self.start_ms = now_ms;
        self.use_time = true;
        self.soft_limit_ms = now_ms + (time_ms as f64 * 0.95) as u64;
        self.hard_limit_ms = now_ms + time_ms;
        self.moves_to_go = -1;
        self.completed_depth = 0;
        self.reset_pv_stability();
        self.move_overhead_ms = overhead_ms;
    }

    /// Switches to infinite mode (only the stop signal ends the search).
    pub fn set_infinite(&mut self) {
        self.use_time = false;
    }

    /// True when no time limits apply.
    pub fn is_infinite(&self) -> bool {
        !self.use_time
    }

    /// Records that an iteration of `depth` plies completed.
    pub fn set_completed_depth(&mut self, depth: i32) {
        if depth > self.completed_depth {
            self.completed_depth = depth;
        }
    }

    /// Feeds the per-iteration node statistics for the node-fraction term:
    /// the nodes spent on the best root move and the search total.
    pub fn set_node_stats(&mut self, best_move_nodes: u64, total_nodes: u64) {
        self.stable_tm_best_move_nodes = best_move_nodes;
        self.stable_tm_nodes = total_nodes;
    }

    /// Feeds the per-iteration best-move result into the stability tracker.
    pub fn update_pv_stability(&mut self, best_move: i32, stable: bool) {
        if best_move != self.last_pv_move {
            self.pv_changes += 1;
            self.stable_iterations = 0;
            self.last_pv_move = best_move;
        } else if stable {
            self.stable_iterations += 1;
        }
    }

    /// Clears the stability tracking (a new search).
    pub fn reset_pv_stability(&mut self) {
        self.pv_changes = 0;
        self.last_pv_move = 0;
        self.stable_iterations = 0;
        self.score_stability = 0;
        self.stable_tm_nodes = 0;
        self.stable_tm_best_move_nodes = 0;
    }

    /// Hard-limit check: the search must stop immediately.
    pub fn is_time_up(&self, now_ms: u64) -> bool {
        self.use_time && now_ms >= self.hard_limit_ms
    }

    /// Periodic-stop decision for the node polling: the hard limit always
    /// applies; the soft limit waits for the minimum completed depth in real
    /// time controls.
    pub fn should_stop(&self, now_ms: u64) -> bool {
        if !self.use_time {
            return false;
        }
        if now_ms >= self.hard_limit_ms {
            return true;
        }
        if self.moves_to_go != -1 && self.completed_depth < MIN_ITERATION_DEPTH {
            return false;
        }
        now_ms >= self.effective_soft_limit()
    }

    /// The soft stop time for the current state: the base allocation scaled
    /// by the active multipliers. Shared by the mid-iteration stop and the
    /// between-iterations decision.
    pub fn effective_soft_limit(&self) -> u64 {
        if self.use_stable_tm && self.moves_to_go != -1 {
            let scale = self.best_move_stability_factor()
                * self.score_stability_factor()
                * self.node_tm_factor();
            let scale = scale.clamp(
                STABLE_TM_SCALE_MIN_HUNDREDTHS as f64 / 100.0,
                STABLE_TM_SCALE_MAX_HUNDREDTHS as f64 / 100.0,
            );
            let limit = self
                .start_ms
                .saturating_add(((self.soft_limit_ms - self.start_ms) as f64 * scale) as u64);
            let hard_minus_overhead = self.hard_limit_ms.saturating_sub(self.move_overhead_ms);
            return limit.min(hard_minus_overhead).max(self.start_ms);
        }

        let mut pv = self.pv_stability_multiplier();
        if self.panic_mode && self.use_panic_mode {
            pv = (pv * 1.5).min(1.5);
        }
        self.start_ms
            .saturating_add(((self.soft_limit_ms - self.start_ms) as f64 * pv) as u64)
    }

    /// The best-move-stability ladder value.
    fn best_move_stability_factor(&self) -> f64 {
        let index = (self.stable_iterations as usize).min(BEST_MOVE_STABILITY_FACTOR.len() - 1);
        BEST_MOVE_STABILITY_FACTOR[index] as f64 / 100.0
    }

    /// The score-stability ladder value.
    fn score_stability_factor(&self) -> f64 {
        let index = (self.score_stability as usize).min(SCORE_STABILITY_FACTOR.len() - 1);
        SCORE_STABILITY_FACTOR[index] as f64 / 100.0
    }

    /// The node-fraction term: less extra time when the best move already
    /// owns a large share of the search. Inert below the minimum depth.
    fn node_tm_factor(&self) -> f64 {
        if self.completed_depth < NODE_TM_MIN_DEPTH || self.stable_tm_nodes == 0 {
            return 1.0;
        }
        let fraction = self.stable_tm_best_move_nodes as f64 / self.stable_tm_nodes as f64;
        (NODE_TM_BASE_HUNDREDTHS as f64 / 100.0 - fraction)
            * (NODE_TM_SCALE_HUNDREDTHS as f64 / 100.0)
    }

    /// The PV-stability multiplier (the legacy model): 1.5x after three or
    /// more best-move changes, 1.2x after two, and (with the shrink lever)
    /// 0.85x/0.75x for a stable best move. Fixed movetime is never scaled.
    fn pv_stability_multiplier(&self) -> f64 {
        if self.moves_to_go == -1 {
            return 1.0;
        }
        if self.pv_changes >= 3 {
            1.5
        } else if self.pv_changes == 2 {
            1.2
        } else if self.stable_iterations >= 3 {
            if self.use_stable_shrink { 0.75 } else { 1.0 }
        } else if self.stable_iterations >= 2 {
            if self.use_stable_shrink { 0.85 } else { 1.0 }
        } else {
            1.0
        }
    }

    /// Between-iterations decision: whether to start the next depth.
    ///
    /// Also updates the panic-mode and score-stability trackers from the
    /// completed iteration's score. (`best_move` is accepted for call-site
    /// symmetry with the tracker updates the search loop performs.)
    pub fn should_continue(&mut self, now_ms: u64, _best_move: i32, score: i32) -> bool {
        if !self.use_time {
            return true;
        }
        if now_ms >= self.hard_limit_ms {
            return false;
        }

        let elapsed = now_ms - self.start_ms;
        let total = self.hard_limit_ms - self.start_ms;

        // Fixed movetime: aggressive continuation to use the full budget.
        if self.moves_to_go == -1 {
            return elapsed < (total as f64 * 0.95) as u64;
        }

        // Panic mode: track the score trend.
        if !self.first_score && score.abs() < 20_000 && self.previous_score.abs() < 20_000 {
            if score < self.previous_score - 50 {
                self.panic_mode = true;
            } else if score > self.previous_score + 20 {
                self.panic_mode = false;
            }
        }
        // Score stability for the stable-TM model; must run before the
        // previous score is overwritten.
        if self.use_stable_tm
            && !self.first_score
            && score.abs() < 30_000
            && self.previous_score.abs() < 30_000
        {
            self.score_stability =
                if score >= self.previous_score - 10 && score <= self.previous_score + 10 {
                    self.score_stability + 1
                } else {
                    0
                };
        }

        self.previous_score = score;
        self.first_score = false;

        // Min-depth guard: keep iterating until the depth floor.
        if self.completed_depth < MIN_ITERATION_DEPTH {
            return true;
        }

        now_ms < self.effective_soft_limit()
    }

    /// The panic-mode state (diagnostics).
    pub fn is_panic_mode(&self) -> bool {
        self.panic_mode
    }

    /// The base soft limit (the unrefined allocation).
    pub fn soft_limit(&self) -> u64 {
        self.soft_limit_ms
    }

    /// The hard limit.
    pub fn hard_limit(&self) -> u64 {
        self.hard_limit_ms
    }

    /// Builds the time part of the search limits from the manager state.
    pub fn limits(&self) -> Limits {
        if self.use_time {
            Limits {
                soft_time_ms: Some(self.effective_soft_limit()),
                hard_time_ms: Some(self.hard_limit_ms),
                ..Limits::default()
            }
        } else {
            Limits::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_MOVE_OVERHEAD_MS, MIN_ITERATION_DEPTH, TimeControl, TimeManager};

    fn control(time: u64, inc: u64, mtg: Option<u32>) -> TimeControl {
        TimeControl {
            time_left_ms: time,
            opponent_time_ms: 0,
            increment_ms: inc,
            moves_to_go: mtg,
        }
    }

    #[test]
    fn fixed_movetime_splits_at_95_and_100_percent() {
        let mut manager = TimeManager::new();
        manager.set_fixed_time(1_000, 0, 0);
        assert_eq!(manager.soft_limit(), 950);
        assert_eq!(manager.hard_limit(), 1_000);
        // The stability multiplier never scales a fixed movetime.
        assert_eq!(manager.effective_soft_limit(), 950);
    }

    #[test]
    fn movetime_overhead_is_subtracted() {
        let mut manager = TimeManager::new();
        manager.set_fixed_time(1_000, DEFAULT_MOVE_OVERHEAD_MS, 0);
        assert!(manager.hard_limit() <= 1_000);
    }

    #[test]
    fn tournament_mode_splits_the_clock_and_caps_at_ten_moves() {
        let mut manager = TimeManager::new();
        manager.init(&control(30_000, 0, Some(30)), 0, 0);
        // 30,000 / 30 = 1,000 ms.
        assert_eq!(manager.soft_limit(), 1_000);
        // The hard limit is min(4x alloc, time/4).
        assert_eq!(manager.hard_limit(), 4_000);

        // A short control caps the single-move share at a tenth of the clock.
        let mut manager = TimeManager::new();
        manager.init(&control(10_000, 0, Some(5)), 0, 0);
        // 10,000 / 5 = 2,000, capped at 10,000 / 10 = 1,000.
        assert_eq!(manager.soft_limit(), 1_000);
    }

    #[test]
    fn sudden_death_adds_the_full_increment() {
        let mut manager = TimeManager::new();
        manager.init(&control(60_000, 1_000, None), 0, 0);
        // 60,000 / 30 + 1,000 = 3,000.
        assert_eq!(manager.soft_limit(), 3_000);
        assert_eq!(manager.hard_limit(), 12_000);
    }

    #[test]
    fn safety_buffer_caps_on_short_clocks() {
        let mut manager = TimeManager::new();
        manager.init(&control(600, 0, None), 0, 0);
        // Sudden death alloc = 600/30 = 20 (the buffer only caps, never
        // raises); the hard limit = min(4x alloc, time/4).
        assert_eq!(manager.soft_limit(), 20);
        assert_eq!(manager.hard_limit(), 80);
    }

    #[test]
    fn overhead_clamp_keeps_the_soft_limit_below_the_hard_limit() {
        let mut manager = TimeManager::new();
        manager.init(&control(1_000, 0, None), 100, 0);
        // The soft limit must keep the 100 ms overhead below the hard limit.
        assert!(manager.soft_limit() <= manager.hard_limit().saturating_sub(100));
    }

    #[test]
    fn time_advantage_scales_sudden_death_when_ahead() {
        let mut manager = TimeManager::new();
        let mut control = control(180_000, 0, None);
        control.opponent_time_ms = 60_000;
        manager.init(&control, 0, 0);
        // Base: 180,000/30 = 6,000; the advantage multiplier = 1 + 0.5 = 1.5.
        assert_eq!(manager.soft_limit(), 9_000);
    }

    #[test]
    fn clock_imbalance_scales_both_sides() {
        let mut manager = TimeManager::new();
        manager.set_levers(true, true, 50, 50);
        let mut ahead = control(90_000, 0, None);
        ahead.opponent_time_ms = 10_000;
        manager.init(&ahead, 0, 0);
        // ratio = 90/100 = 0.9 → scale = 1 + 0.4*2*0.5 = 1.4;
        // base = 90,000/30 = 3,000 → 4,200.
        assert_eq!(manager.soft_limit(), 4_200);

        let mut manager = TimeManager::new();
        manager.set_levers(true, true, 50, 50);
        let mut behind = control(10_000, 0, None);
        behind.opponent_time_ms = 90_000;
        manager.init(&behind, 0, 0);
        // ratio = 0.1 → scale = 1 - 0.8*0.5 = 0.6 → 333 * 0.6 = 199.
        assert_eq!(manager.soft_limit(), (10_000 / 30) * 3 / 5);
    }

    #[test]
    fn soft_stop_waits_for_the_minimum_depth() {
        let mut manager = TimeManager::new();
        manager.init(&control(10_000, 0, None), 0, 0);
        // Past the soft limit (333) but before the hard limit (1,333), the
        // min-depth guard keeps the search going.
        assert!(!manager.should_stop(1_000));
        manager.set_completed_depth(MIN_ITERATION_DEPTH);
        assert!(manager.should_stop(1_000));
    }

    #[test]
    fn hard_limit_stops_even_before_the_min_depth() {
        let mut manager = TimeManager::new();
        manager.init(&control(10_000, 0, None), 0, 0);
        // The hard limit = min(4x alloc, time/4) = 1,333.
        assert!(manager.should_stop(1_400));
    }

    #[test]
    fn should_continue_tracks_panic_and_stability() {
        let mut manager = TimeManager::new();
        // The legacy multiplier model (the stable-TM ladders off).
        manager.set_levers(false, false, 50, 50);
        manager.init(&control(60_000, 0, None), 0, 0);
        for depth in 1..MIN_ITERATION_DEPTH {
            manager.set_completed_depth(depth);
            assert!(
                manager.should_continue(10, 0, 40),
                "min-depth guard at {depth}"
            );
        }
        manager.set_completed_depth(MIN_ITERATION_DEPTH);
        // A collapsing score arms panic mode; the multiplier extends the
        // soft limit to 1.5x of the 3,000 ms allocation (tactical floor).
        assert!(manager.should_continue(1_000, 0, -80));
        assert!(manager.is_panic_mode());
        assert!(manager.should_continue(4_400, 0, -80));
        assert!(!manager.should_continue(4_600, 0, -80));
    }

    #[test]
    fn infinite_mode_never_stops_on_time() {
        let mut manager = TimeManager::new();
        manager.set_infinite();
        assert!(manager.is_infinite());
        assert!(!manager.should_stop(u64::MAX >> 2));
        assert!(manager.should_continue(u64::MAX >> 2, 0, 0));
    }

    #[test]
    fn movetime_continue_runs_to_95_percent() {
        let mut manager = TimeManager::new();
        manager.set_fixed_time(1_000, 0, 0);
        assert!(manager.should_continue(900, 0, 0));
        assert!(!manager.should_continue(960, 0, 0));
    }

    #[test]
    fn pv_stability_multiplies_the_soft_limit() {
        let mut manager = TimeManager::new();
        // The legacy multiplier model (the stable-TM ladders off).
        manager.set_levers(false, false, 50, 50);
        manager.init(&control(60_000, 0, None), 0, 0);
        // Base allocation 3,000 (tactical floor); after two best-move
        // changes the multiplier is 1.2x.
        manager.update_pv_stability(11, false);
        manager.update_pv_stability(22, false);
        for depth in 1..MIN_ITERATION_DEPTH {
            manager.set_completed_depth(depth);
            manager.should_continue(0, 22, 40);
        }
        manager.set_completed_depth(MIN_ITERATION_DEPTH);
        assert!(manager.should_continue(3_400, 22, 40));
        assert!(!manager.should_continue(3_600, 22, 40));
    }

    #[test]
    fn stable_tm_scales_by_the_ladders() {
        let mut manager = TimeManager::new();
        // The stable-TM model is on by default. Varying scores keep the
        // score-stability counter at zero, so the ladders sit at their
        // first rungs: 2.50 * 1.25 = 3.125, clamped to 2.45.
        manager.init(&control(60_000, 0, None), 0, 0);
        let scores = [40_i32, 55, 40, 55];
        for (depth, score) in (1..MIN_ITERATION_DEPTH).zip(scores) {
            manager.set_completed_depth(depth);
            manager.should_continue(0, 5, score);
        }
        manager.set_completed_depth(MIN_ITERATION_DEPTH);
        // 3,000 * 2.45 = 7,350, clamped by hard - overhead.
        assert!(manager.should_continue(7_300, 5, 40));
        assert!(!manager.should_continue(7_400, 5, 40));
    }

    #[test]
    fn stable_tm_score_stability_shortens_the_limit() {
        let mut manager = TimeManager::new();
        manager.init(&control(60_000, 0, None), 0, 0);
        // Five stable-best-move updates reach the last best-move rung and
        // four equal scores reach the last score rung: 0.75 * 0.88 = 0.66
        // → 3,000 * 0.66 = 1,980.
        for depth in 1..=MIN_ITERATION_DEPTH {
            manager.set_completed_depth(depth);
            manager.update_pv_stability(5, true);
            manager.should_continue(10, 5, 40);
        }
        manager.set_completed_depth(MIN_ITERATION_DEPTH);
        assert!(manager.should_continue(1_900, 5, 40));
        assert!(!manager.should_continue(2_000, 5, 40));
    }
}
