//! WDL and DTZ probing with capture and pawn-move resolution.
//!
//! A table stores values for positions without en-passant captures and without
//! a pending capture that would change the fifty-move counter. The prober
//! therefore resolves such positions itself: a winning capture can make the
//! stored value irrelevant, and a winning non-capturing pawn move resets the
//! counter and must be played before a DTZ value can be used.
//!
//! The recursive capture resolution is bounded by [`MAX_PROBE_DEPTH`]; every
//! legal capture removes a piece, so a valid position can never get near the
//! cap, and corrupt state terminates instead of overflowing the stack.

use super::index::WDL_TO_DTZ;
use super::pairs::{DTZ, WDL};
use super::pos::{
    Pos, TB_MAX_CAPTURES, TB_MAX_MOVES, do_move, gen_captures, gen_legal, gen_moves, is_capture,
    is_en_passant, is_mate, legal_move, type_of_piece_moved,
};
use super::table::{Inner, TableValue, calc_key};

/// Maximum recursion depth of the capture resolver.
const MAX_PROBE_DEPTH: usize = 64;

/// Sentinel: no best move found.
const BEST_NONE: i32 = 0xFFFF;
/// Sentinel: an illegal move in the root list.
const SCORE_ILLEGAL: i32 = 0x7FFF;
/// Root sentinel: stalemate.
pub(crate) const MOVE_STALEMATE: u16 = 0xFFFF;
/// Root sentinel: checkmate.
pub(crate) const MOVE_CHECKMATE: u16 = 0xFFFE;

/// Probes the WDL table for `pos`, resolving captures.
///
/// `success` follows the reference: `1` ok, `2` a winning capture (or a lone
/// en-passant capture) is the best move, `0` failed.
pub(crate) fn probe_wdl(inner: &mut Inner, pos: &Pos) -> TableValue {
    let mut moves = [0_u16; TB_MAX_CAPTURES];
    let end = gen_captures(pos, &mut moves);
    let mut best_cap = -3;
    let mut best_ep = -3;

    for &mv in &moves[..end] {
        if !is_capture(pos, mv) {
            continue;
        }
        let (next, legal) = do_move(pos, mv);
        if !legal {
            continue;
        }
        let value = probe_ab(inner, &next, -2, -best_cap, 1);
        if value.success == 0 {
            return TableValue::FAILED;
        }
        if value.value > best_cap {
            if value.value == 2 {
                return TableValue {
                    value: 2,
                    success: 2,
                };
            }
            if !is_en_passant(pos, mv) {
                best_cap = value.value;
            } else if value.value > best_ep {
                best_ep = value.value;
            }
        }
    }

    let mut w = inner.probe_table(pos, 0, WDL);
    if w.success == 0 {
        return TableValue::FAILED;
    }

    if best_ep > best_cap {
        if best_ep > w.value {
            return TableValue {
                value: best_ep,
                success: 2,
            };
        }
        best_cap = best_ep;
    }

    if best_cap >= w.value {
        let success = 1 + i32::from(best_cap > 0);
        return TableValue {
            value: best_cap,
            success,
        };
    }

    // Stalemate / lone en-passant case.
    if best_ep > -3 && w.value == 0 {
        let mut all = [0_u16; TB_MAX_MOVES];
        let end = gen_moves(pos, &mut all);
        let mut index = 0;
        while index < end {
            if !is_en_passant(pos, all[index]) && legal_move(pos, all[index]) {
                break;
            }
            index += 1;
        }
        if index == end && !super::pos::is_check(pos) {
            return TableValue {
                value: best_ep,
                success: 2,
            };
        }
    }

    w.success = 1;
    w
}

/// Resolves captures for a WDL probe with an alpha-beta window.
fn probe_ab(inner: &mut Inner, pos: &Pos, mut alpha: i32, beta: i32, depth: usize) -> TableValue {
    if depth > MAX_PROBE_DEPTH {
        return TableValue::FAILED;
    }

    let mut moves = [0_u16; TB_MAX_CAPTURES];
    let end = gen_captures(pos, &mut moves);
    for &mv in &moves[..end] {
        if !is_capture(pos, mv) {
            continue;
        }
        let (next, legal) = do_move(pos, mv);
        if !legal {
            continue;
        }
        let value = probe_ab(inner, &next, -beta, -alpha, depth + 1);
        if value.success == 0 {
            return TableValue::FAILED;
        }
        let value = -value.value;
        if value > alpha {
            if value >= beta {
                return TableValue::ok(value);
            }
            alpha = value;
        }
    }

    let w = inner.probe_table(pos, 0, WDL);
    if w.success == 0 {
        return TableValue::FAILED;
    }
    if w.value > alpha {
        TableValue::ok(w.value)
    } else {
        TableValue::ok(alpha)
    }
}

/// Probes the DTZ table, resolving captures and non-capturing pawn moves.
pub(crate) fn probe_dtz(inner: &mut Inner, pos: &Pos) -> TableValue {
    let wdl_value = probe_wdl(inner, pos);
    if wdl_value.success == 0 {
        return TableValue::FAILED;
    }
    let wdl = wdl_value.value;
    if wdl == 0 {
        return TableValue::ok(0);
    }
    if wdl_value.success == 2 {
        return TableValue::ok(WDL_TO_DTZ[(wdl + 2) as usize]);
    }

    let mut moves = [0_u16; TB_MAX_MOVES];
    let mut end = 0;
    if wdl > 0 {
        // A winning non-capturing pawn move resets the counter and wins now.
        end = gen_legal(pos, &mut moves);
        for &mv in &moves[..end] {
            if type_of_piece_moved(pos, mv) != 1 || is_capture(pos, mv) {
                continue;
            }
            let (next, legal) = do_move(pos, mv);
            if !legal {
                continue;
            }
            let value = probe_wdl(inner, &next);
            if value.success == 0 {
                return TableValue::FAILED;
            }
            if -value.value == wdl {
                return TableValue::ok(WDL_TO_DTZ[(wdl + 2) as usize]);
            }
        }
    }

    let dtz = inner.probe_table(pos, wdl, DTZ);
    if dtz.success >= 0 {
        let base = WDL_TO_DTZ[(wdl + 2) as usize];
        return TableValue::ok(if wdl > 0 {
            base + dtz.value
        } else {
            base - dtz.value
        });
    }

    // The table could not be used for this side; resolve the move that keeps
    // the WDL value and probe the other side.
    let mut best;
    if wdl > 0 {
        best = i32::MAX;
    } else {
        best = WDL_TO_DTZ[(wdl + 2) as usize];
        end = gen_moves(pos, &mut moves);
    }

    for &mv in &moves[..end] {
        if is_capture(pos, mv) || type_of_piece_moved(pos, mv) == 1 {
            continue;
        }
        let (next, legal) = do_move(pos, mv);
        if !legal {
            continue;
        }
        let value = probe_dtz(inner, &next);
        if value.success == 0 {
            return TableValue::FAILED;
        }
        let value = -value.value;
        if value == 1 && is_mate(&next) {
            best = 1;
        } else if wdl > 0 {
            if value > 0 && value + 1 < best {
                best = value + 1;
            }
        } else if value - 1 < best {
            best = value - 1;
        }
    }
    TableValue::ok(best)
}

/// Converts a DTZ value and the fifty-move counter into a WDL class offset by two.
pub(crate) fn dtz_to_wdl(counter: i32, dtz: i32) -> i32 {
    let wdl = if dtz > 0 {
        if dtz + counter <= 100 { 2 } else { 1 }
    } else if dtz < 0 {
        if -dtz + counter <= 100 { -2 } else { -1 }
    } else {
        0
    };
    wdl + 2
}

/// Result of the root probe in the prober's own move encoding.
#[derive(Clone, Copy)]
pub(crate) struct RootProbe {
    /// Best move, or [`MOVE_CHECKMATE`]/[`MOVE_STALEMATE`]/0.
    pub(crate) mv: u16,
    /// Distance to zeroing, unsigned.
    pub(crate) dtz: i32,
    /// Whether the probe succeeded.
    pub(crate) success: bool,
}

/// Probes the root position and returns the best move (WDL-preserving, DTZ-aware).
pub(crate) fn probe_root(inner: &mut Inner, pos: &Pos) -> RootProbe {
    let first = probe_dtz(inner, pos);
    if first.success == 0 {
        return RootProbe {
            mv: 0,
            dtz: 0,
            success: false,
        };
    }
    let dtz = first.value;

    let mut moves = [0_u16; TB_MAX_MOVES];
    let end = gen_moves(pos, &mut moves);
    let mut scores = [0_i32; TB_MAX_MOVES];
    let mut draw_count = 0;

    for i in 0..end {
        let (next, legal) = do_move(pos, moves[i]);
        if !legal {
            scores[i] = SCORE_ILLEGAL;
            continue;
        }
        let value = if dtz > 0 && is_mate(&next) {
            1
        } else if next.rule50 != 0 {
            let probe = probe_dtz(inner, &next);
            if probe.success == 0 {
                return RootProbe {
                    mv: 0,
                    dtz: 0,
                    success: false,
                };
            }
            let value = -probe.value;
            if value > 0 {
                value + 1
            } else if value < 0 {
                value - 1
            } else {
                value
            }
        } else {
            let probe = probe_wdl(inner, &next);
            if probe.success == 0 {
                return RootProbe {
                    mv: 0,
                    dtz: 0,
                    success: false,
                };
            }
            WDL_TO_DTZ[(-probe.value + 2) as usize]
        };
        draw_count += i32::from(value == 0);
        scores[i] = value;
    }

    if dtz > 0 {
        // Winning (or a fifty-move draw): the fastest win keeps the WDL class.
        let mut best = BEST_NONE;
        let mut best_move = 0_u16;
        for i in 0..end {
            let value = scores[i];
            if value == SCORE_ILLEGAL {
                continue;
            }
            if value > 0 && value < best {
                best = value;
                best_move = moves[i];
            }
        }
        return RootProbe {
            mv: if best == BEST_NONE { 0 } else { best_move },
            dtz,
            success: true,
        };
    }
    if dtz < 0 {
        let mut best = 0;
        let mut best_move = 0_u16;
        for i in 0..end {
            let value = scores[i];
            if value == SCORE_ILLEGAL {
                continue;
            }
            if value < best {
                best = value;
                best_move = moves[i];
            }
        }
        return RootProbe {
            mv: if best == 0 { MOVE_CHECKMATE } else { best_move },
            dtz,
            success: true,
        };
    }

    if draw_count == 0 {
        return RootProbe {
            mv: MOVE_STALEMATE,
            dtz: 0,
            success: true,
        };
    }

    // Pick a draw-preserving move, using the material key as the selector.
    let mut count = calc_key(pos, !pos.turn) % draw_count as u64;
    for i in 0..end {
        let value = scores[i];
        if value == SCORE_ILLEGAL {
            continue;
        }
        if value == 0 {
            if count == 0 {
                return RootProbe {
                    mv: moves[i],
                    dtz: 0,
                    success: true,
                };
            }
            count -= 1;
        }
    }
    RootProbe {
        mv: 0,
        dtz: 0,
        success: true,
    }
}
