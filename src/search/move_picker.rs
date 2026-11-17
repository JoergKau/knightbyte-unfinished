//! The staged move picker.
//!
//! The reference orders moves in lazy stages over separate per-stage buffers,
//! so the partial selection sort's swap only ever displaces a move inside its
//! own stage — the stage order and the generation order inside each stage are
//! both preserved exactly:
//!
//! 1. the transposition-table move (validated against the node's legal moves),
//! 2. good captures (`SEE >= 0`, sorted by material swing + exchange value +
//!    capture history),
//! 3. the four killers of the ply (validated against the quiet buffer),
//! 4. the countermove (same validation),
//! 5. quiet moves — including quiet promotions — sorted by the combined
//!    history score,
//! 6. bad captures (`SEE < 0`) in generation order.
//!
//! The stage state lives in [`PickerState`], one per search ply inside the
//! [`Searcher`](super::searcher::Searcher), so a move is scored lazily — when
//! its stage is reached — with the history tables in exactly the state the
//! reference sees them. The per-call borrows (position, history) keep the
//! state borrow-free while the search mutates the tables between yields.
//!
//! `skip_quiets` aborts the quiet stage the way the search's late-move and
//! futility pruners trigger it; bad captures are always served.

use crate::board::movegen;
use crate::board::position::Position;
use crate::search::history::{Continuations, History, Previous};
use crate::search::killers::{KILLER_SLOTS, Killers};
use crate::search::see::{see_ge, see_value};
use crate::types::r#move::{Move, flags};
use crate::types::piece::{PAWN, color_of, piece_type};

/// Material values used by the capture-ordering term (pawn 100 ... king
/// 20,000). These are the ordering weights, deliberately distinct from the
/// exchange values the search's SEE returns.
pub(crate) const ORDER_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 20_000];

/// Maximum number of moves a position can have; matches the generator's list.
pub const MAX_MOVES: usize = 256;

/// The picker's stages, in emission order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    TtMove,
    GenerateCaptures,
    GoodCaptures,
    Killers,
    Countermove,
    GenerateQuiets,
    Quiets,
    BadCaptures,
    Done,
}

/// Per-ply picker state: the stage buffers and cursors, borrow-free so the
/// search owns one per ply and passes the position and tables per call.
pub struct PickerState {
    tt_move: Move,
    counter_move: Move,
    killers: [Move; KILLER_SLOTS],
    cont: Continuations,
    qsearch: bool,
    captures_only: bool,
    caps: [Move; MAX_MOVES],
    cap_scores: [i32; MAX_MOVES],
    caps_count: usize,
    caps_index: usize,
    quiets: [Move; MAX_MOVES],
    quiet_scores: [i32; MAX_MOVES],
    quiets_count: usize,
    quiets_index: usize,
    bad: [Move; MAX_MOVES],
    bad_count: usize,
    bad_index: usize,
    stage: Stage,
    killer_index: usize,
    skip_quiets: bool,
}

impl Default for PickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl PickerState {
    /// Creates an empty state (safe to store before first use).
    pub fn new() -> Self {
        Self {
            tt_move: Move::NONE,
            counter_move: Move::NONE,
            killers: [Move::NONE; KILLER_SLOTS],
            cont: [None; 4],
            qsearch: false,
            captures_only: false,
            caps: [Move::NONE; MAX_MOVES],
            cap_scores: [0; MAX_MOVES],
            caps_count: 0,
            caps_index: 0,
            quiets: [Move::NONE; MAX_MOVES],
            quiet_scores: [0; MAX_MOVES],
            quiets_count: 0,
            quiets_index: 0,
            bad: [Move::NONE; MAX_MOVES],
            bad_count: 0,
            bad_index: 0,
            stage: Stage::TtMove,
            killer_index: 0,
            skip_quiets: false,
        }
    }

    /// Resets the state for the node at `ply`: no partition yet, the stage
    /// cursor back at the table move.
    ///
    /// `qsearch` selects the quiescence search's picker shape: no killer or
    /// countermove stages, no capture-history term and quiets ordered by
    /// generation only (the reference passes null tables there).
    /// `captures_only` drops quiet moves
    /// at partition time — the reference's captures-only generation, which
    /// the quiescence search uses when not in check.
    #[allow(clippy::too_many_arguments)]
    pub fn reset(
        &mut self,
        tt_move: Move,
        killers: &Killers,
        history: &History,
        ply: usize,
        previous: Option<Previous>,
        cont: &Continuations,
        qsearch: bool,
        captures_only: bool,
    ) {
        self.tt_move = tt_move;
        self.counter_move = if qsearch {
            Move::NONE
        } else {
            history.counter_move(previous)
        };
        self.killers = if qsearch {
            [Move::NONE; KILLER_SLOTS]
        } else {
            killers.get(ply)
        };
        self.cont = *cont;
        self.qsearch = qsearch;
        self.captures_only = captures_only;
        self.caps_count = 0;
        self.caps_index = 0;
        self.quiets_count = 0;
        self.quiets_index = 0;
        self.bad_count = 0;
        self.bad_index = 0;
        self.stage = Stage::TtMove;
        self.killer_index = 0;
        self.skip_quiets = false;
    }

    /// Returns the countermove captured when the picker was reset.
    ///
    /// The reference reads its countermove once, before the move loop, and
    /// every later use in the loop (the late-move-reduction test) sees that
    /// value. The live table can change underneath a long loop — a deeper
    /// node with the same previous move overwrites the cell — so the loop
    /// must use this snapshot and not the table.
    pub fn counter_move(&self) -> Move {
        self.counter_move
    }

    /// Aborts the quiet stage (late-move and futility pruning).
    pub fn skip_quiets(&mut self) {
        self.skip_quiets = true;
    }

    /// Writes the parity-picker trace line (the killers and the mode) with
    /// the caller's node number.
    pub fn trace_reset(&self, node: u64) {
        if !std::env::var_os("KNIGHTBYTE_PICKER_TRACE").is_some() {
            return;
        }
        let killers_text = self
            .killers
            .iter()
            .map(|mv| mv.to_uci())
            .collect::<Vec<_>>()
            .join(",");
        eprintln!(
            "P node {node} q {} caps_only {} killers {killers_text}",
            u8::from(self.qsearch),
            u8::from(self.captures_only),
        );
    }

    /// Partitions the legal move list into the stage buffers, generation
    /// order preserved inside each. The capture scores are computed here, in
    /// the capture-generation stage — which runs only after the table move
    /// (when there is one) has been searched, so the capture history already
    /// reflects that subtree. Quiescence orders captures without the capture
    /// history.
    fn partition(&mut self, position: &Position, history: &History, moves: &[Move]) {
        for &mv in moves {
            if !self.tt_move.is_none() && mv == self.tt_move {
                continue;
            }
            if movegen::is_capture(position, mv) {
                if see_ge(position, mv, 0) {
                    let victim = if mv.flag() == flags::EN_PASSANT {
                        PAWN
                    } else {
                        piece_type(position.piece_at(mv.to()))
                    };
                    let moving_type = piece_type(position.piece_at(mv.from()));
                    let history_term = if self.qsearch {
                        0
                    } else {
                        history.capture(moving_type, mv, victim)
                    };
                    self.caps[self.caps_count] = mv;
                    self.cap_scores[self.caps_count] =
                        capture_score(position, moving_type, mv, victim, history_term);
                    self.caps_count += 1;
                } else {
                    self.bad[self.bad_count] = mv;
                    self.bad_count += 1;
                }
            } else if !self.captures_only {
                self.quiets[self.quiets_count] = mv;
                self.quiets_count += 1;
            }
        }
    }

    /// Returns the next move in stage order, or `None` when exhausted.
    pub fn next(&mut self, position: &Position, history: &History, moves: &[Move]) -> Option<Move> {
        loop {
            match self.stage {
                Stage::TtMove => {
                    self.stage = Stage::GenerateCaptures;
                    // The legal move list is the validation: the partition
                    // skips the table move, so membership in the original
                    // list decides whether it is legal here.
                    if !self.tt_move.is_none() && moves.contains(&self.tt_move) {
                        return Some(self.tt_move);
                    }
                }
                Stage::GenerateCaptures => {
                    self.partition(position, history, moves);
                    self.stage = Stage::GoodCaptures;
                }
                Stage::GoodCaptures => {
                    if self.caps_index < self.caps_count {
                        pick_best(
                            &mut self.cap_scores,
                            &mut self.caps,
                            self.caps_index,
                            self.caps_count,
                        );
                        let mv = self.caps[self.caps_index];
                        self.caps_index += 1;
                        return Some(mv);
                    }
                    self.stage = Stage::Killers;
                }
                Stage::Killers => {
                    while self.killer_index < KILLER_SLOTS {
                        let killer = self.killers[self.killer_index];
                        self.killer_index += 1;
                        if killer.is_none() {
                            continue;
                        }
                        if let Some(slot) = self.quiets[..self.quiets_count]
                            .iter()
                            .position(|&mv| mv.without_double_push() == killer)
                        {
                            // Mark the killer played so the quiet stage never
                            // re-emits it. The generated move is returned, not
                            // the remembered one: a killer recorded as a pawn
                            // double push can match another piece's move on
                            // the same squares, and only the generated move
                            // carries the right marker for the position.
                            let generated = self.quiets[slot];
                            self.quiets[slot] = Move::NONE;
                            return Some(generated);
                        }
                    }
                    self.stage = Stage::Countermove;
                }
                Stage::Countermove => {
                    self.stage = Stage::GenerateQuiets;
                    if !self.counter_move.is_none()
                        && let Some(slot) = self.quiets[..self.quiets_count]
                            .iter()
                            .position(|&mv| mv.without_double_push() == self.counter_move)
                    {
                        let generated = self.quiets[slot];
                        self.quiets[slot] = Move::NONE;
                        return Some(generated);
                    }
                }
                Stage::GenerateQuiets => {
                    if self.skip_quiets {
                        self.stage = Stage::BadCaptures;
                        continue;
                    }
                    // Compact the un-played quiets and score them — at the
                    // moment the first quiet is needed, exactly like the
                    // reference's lazy stage.
                    let mut kept = 0;
                    for slot in 0..self.quiets_count {
                        let mv = self.quiets[slot];
                        if !mv.is_none() {
                            let moving = position.piece_at(mv.from());
                            self.quiets[kept] = mv;
                            self.quiet_scores[kept] = if self.qsearch {
                                0
                            } else {
                                history.picker_score(color_of(moving), mv, &self.cont)
                            };
                            if std::env::var_os("KNIGHTBYTE_PICKER_TRACE").is_some() {
                                eprintln!("P quiet {} {}", mv.to_uci(), self.quiet_scores[kept]);
                            }
                            kept += 1;
                        }
                    }
                    self.quiets_count = kept;
                    self.quiets_index = 0;
                    self.stage = Stage::Quiets;
                }
                Stage::Quiets => {
                    if self.skip_quiets {
                        self.stage = Stage::BadCaptures;
                        continue;
                    }
                    if self.quiets_index < self.quiets_count {
                        pick_best(
                            &mut self.quiet_scores,
                            &mut self.quiets,
                            self.quiets_index,
                            self.quiets_count,
                        );
                        let mv = self.quiets[self.quiets_index];
                        self.quiets_index += 1;
                        return Some(mv);
                    }
                    self.stage = Stage::BadCaptures;
                }
                Stage::BadCaptures => {
                    if self.bad_index < self.bad_count {
                        let mv = self.bad[self.bad_index];
                        self.bad_index += 1;
                        return Some(mv);
                    }
                    self.stage = Stage::Done;
                }
                Stage::Done => return None,
            }
        }
    }
}

/// Partial selection sort over `scores`/`moves` from `start` on: swaps the
/// highest remaining score into `start`.
///
/// The swap displaces the previously stored move into the picked slot — the
/// reference's buffer discipline (one buffer per stage) is what keeps that
/// displacement inside a single stage.
fn pick_best(scores: &mut [i32], moves: &mut [Move], start: usize, count: usize) {
    if start >= count {
        return;
    }
    let mut best = start;
    for candidate in start + 1..count {
        if scores[candidate] > scores[best] {
            best = candidate;
        }
    }
    if best != start {
        moves.swap(start, best);
        scores.swap(start, best);
    }
}

/// Returns the reference capture-ordering score: the classical material
/// swing, the exact exchange value and the raw capture-history cell
/// (`history_term`, zero where the picker has no capture history).
fn capture_score(
    position: &Position,
    moving_type: usize,
    mv: Move,
    victim: usize,
    history_term: i32,
) -> i32 {
    let material = ORDER_VALUES[victim] * 100 - ORDER_VALUES[moving_type];
    1_000_000 + material + see_value(position, mv) + history_term
}

/// Dumps the full picker order for a node — the test/tooling view of the
/// staged picker.
#[allow(clippy::too_many_arguments)]
pub fn debug_dump_picker_order(
    position: &Position,
    moves: &[Move],
    tt_move: Move,
    killers: &Killers,
    history: &History,
    ply: usize,
    previous: Option<Previous>,
    cont: &Continuations,
) -> Vec<Move> {
    let mut state = PickerState::new();
    state.reset(tt_move, killers, history, ply, previous, cont, false, false);
    let mut out = Vec::new();
    while let Some(mv) = state.next(position, history, moves) {
        out.push(mv);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{MAX_MOVES, debug_dump_picker_order};
    use crate::board::movegen;
    use crate::board::position::Position;
    use crate::search::history::{Continuations, History};
    use crate::search::killers::Killers;
    use crate::types::r#move::Move;

    const ENDGAME: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";

    #[test]
    fn picker_starts_with_the_capture_then_generation_order() {
        let position = Position::from_fen(ENDGAME).unwrap();
        let moves = movegen::legal(&position);
        let history = History::new();
        let killers = Killers::new();
        let cont: Continuations = [None; 4];
        let picked = debug_dump_picker_order(
            &position,
            moves.as_slice(),
            Move::NONE,
            &killers,
            &history,
            0,
            None,
            &cont,
        );
        let expected: Vec<String> = moves.as_slice().iter().map(|mv| mv.to_uci()).collect();
        let picked_uci: Vec<String> = picked.iter().map(|mv| mv.to_uci()).collect();
        // With every table empty the picker must preserve the generation
        // order exactly (the capture is generated last here, so it is also
        // picked last among non-quiets... it is the ONLY capture and wins
        // its stage, so it comes first).
        assert_eq!(picked_uci[0], "b4f4");
        let rest = &picked_uci[1..];
        let expected_rest: Vec<String> = expected
            .iter()
            .filter(|mv| mv.as_str() != "b4f4")
            .cloned()
            .collect();
        assert_eq!(rest, expected_rest, "quiets must keep generation order");
        let _ = MAX_MOVES;
    }

    #[test]
    fn every_move_is_emitted_exactly_once() {
        let position = Position::from_fen(ENDGAME).unwrap();
        let moves = movegen::legal(&position);
        let history = History::new();
        let killers = Killers::new();
        let cont: Continuations = [None; 4];
        let picked = debug_dump_picker_order(
            &position,
            moves.as_slice(),
            Move::NONE,
            &killers,
            &history,
            0,
            None,
            &cont,
        );
        let mut sorted: Vec<String> = picked.iter().map(|mv| mv.to_uci()).collect();
        sorted.sort();
        let mut expected: Vec<String> = moves.as_slice().iter().map(|mv| mv.to_uci()).collect();
        expected.sort();
        assert_eq!(sorted, expected);
    }
}
