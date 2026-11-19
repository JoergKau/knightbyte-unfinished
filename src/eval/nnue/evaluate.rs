//! NNUE evaluation paths.
//!
//! The accumulator arithmetic and the output head run through the SIMD
//! kernels (`super::kernels`); this module owns the two call shapes: a full
//! refresh of one perspective from the board, and the incremental update of a
//! child accumulator from its parent. Both must produce bit-identical
//! results, which the parity tests assert over random walks.
//!
//! A reusable [`Evaluator`] keeps the two full-refresh weight vectors
//! allocated once, so evaluating many positions does not allocate per node.

use std::sync::OnceLock;

use super::kernels;
use crate::board::position::Position;
use crate::eval::nnue::features::{MAX_ACTIVE_FEATURES, active_features, feature_index};
use crate::eval::nnue::net::Net;
use crate::types::color::Color;
use crate::types::r#move::Move;
use crate::types::piece::{
    BISHOP, BP, BR, EMPTY, KNIGHT, Piece, QUEEN, ROOK, WP, WR, color_of, is_white, piece_type,
};
use crate::types::square::Square;

/// A reusable full-refresh evaluator over one network.
pub struct Evaluator<'a> {
    net: &'a Net,
    accumulators: [Vec<i16>; 2],
}

impl<'a> Evaluator<'a> {
    /// Creates an evaluator with two accumulator buffers of the network width.
    pub fn new(net: &'a Net) -> Self {
        let l1 = net.l1();
        Self {
            net,
            accumulators: [vec![0_i16; l1], vec![0_i16; l1]],
        }
    }

    /// Returns the network used by this evaluator.
    ///
    /// The lifetime is the network's, not the evaluator's: the search keeps
    /// the reference while mutating its own accumulator stack.
    pub fn net(&self) -> &'a Net {
        self.net
    }

    /// Evaluates a position in centipawns from the side-to-move perspective.
    ///
    /// This is the full-refresh path; the search uses the incremental
    /// accumulator stack instead and only refreshes when a king moves.
    pub fn evaluate(&mut self, position: &Position) -> i32 {
        let stm = position.side();
        let ntm = stm ^ 1;
        refresh_into(
            self.net,
            position,
            stm,
            &mut self.accumulators[stm as usize],
        );
        refresh_into(
            self.net,
            position,
            ntm,
            &mut self.accumulators[ntm as usize],
        );
        kernels::evaluate_head(
            stm,
            &self.accumulators,
            self.net.output_weights(),
            self.net.output_bias(),
        )
    }
}

/// Rebuilds one perspective's accumulator from the feature biases and every
/// active feature row of `position`.
///
/// The SIMD kernels add each row; a row add is exact wrapping `i16`
/// arithmetic, so the result is identical to the scalar oracle.
pub(crate) fn refresh_into(
    net: &Net,
    position: &Position,
    perspective: Color,
    accumulator: &mut [i16],
) {
    crate::node_stat!(FullRefreshes);
    accumulator.copy_from_slice(net.feature_biases());

    let mut features = [0_usize; MAX_ACTIVE_FEATURES];
    let count = active_features(net, position, perspective, &mut features);
    let weights = net.feature_weights();

    for &feature in &features[..count] {
        kernels::add_row(accumulator, weights, feature, true);
    }
}

/// Computes the accumulator pair at `ply + 1` by applying `mv` to the
/// validated pair at `ply`.
///
/// A perspective whose own king moved falls back to a full refresh of
/// `child`: its region and mirror may have changed, which invalidates
/// every row. All other perspectives keep their frame (the parent's), so
/// at most four rows change: the moved piece, the captured piece (its own
/// square for en passant), the promotion piece and the castling rook.
pub(crate) fn push_accumulators(
    net: &Net,
    parent: &Position,
    child: &Position,
    mv: Move,
    parent_pair: &[Vec<i16>; 2],
    child_pair: &mut [Vec<i16>; 2],
) {
    crate::node_stat!(AccumulatorPushes);
    let moving = parent.piece_at(mv.from());
    let mover_color = color_of(moving);
    let mut added_piece = moving;

    let mut removed = [(0 as Square, 0 as Piece); 3];
    let mut removed_count = 0;
    let mut added = [(0 as Square, 0 as Piece); 2];
    let mut added_count = 0;

    removed[removed_count] = (mv.from(), moving);
    removed_count += 1;
    if mv.flag() == crate::types::r#move::flags::EN_PASSANT {
        // The captured pawn is behind the target square, not on it.
        let captured_square = if is_white(moving) {
            mv.to() - 8
        } else {
            mv.to() + 8
        };
        let pawn = if is_white(moving) { BP } else { WP };
        removed[removed_count] = (captured_square, pawn);
        removed_count += 1;
    } else {
        let captured = parent.piece_at(mv.to());
        if captured != EMPTY {
            removed[removed_count] = (mv.to(), captured);
            removed_count += 1;
        }
    }
    if mv.is_promotion() {
        let promo_type = match mv.flag() {
            crate::types::r#move::flags::PROMO_QUEEN => QUEEN,
            crate::types::r#move::flags::PROMO_ROOK => ROOK,
            crate::types::r#move::flags::PROMO_BISHOP => BISHOP,
            _ => KNIGHT,
        };
        added_piece = if is_white(moving) {
            WP + promo_type as u8
        } else {
            BP + promo_type as u8
        };
    }
    added[added_count] = (mv.to(), added_piece);
    added_count += 1;
    if mv.flag() == crate::types::r#move::flags::CASTLE {
        let kingside = mv.to() > mv.from();
        let (rook_from, rook_to) = if kingside {
            (mv.to() + 1, mv.to() - 1)
        } else {
            (mv.to() - 2, mv.to() + 1)
        };
        let rook = if is_white(moving) { WR } else { BR };
        removed[removed_count] = (rook_from, rook);
        removed_count += 1;
        added[added_count] = (rook_to, rook);
        added_count += 1;
    }

    let king_moved = piece_type(moving) == crate::types::piece::KING;
    for perspective in 0..2_usize {
        if king_moved && mover_color == perspective as Color {
            refresh_into(
                net,
                child,
                perspective as Color,
                &mut child_pair[perspective],
            );
        } else {
            child_pair[perspective].clone_from(&parent_pair[perspective]);
            for &(square, piece) in &removed[..removed_count] {
                apply_row(
                    net,
                    &mut child_pair[perspective],
                    parent,
                    perspective as Color,
                    square,
                    piece,
                    false,
                );
            }
            for &(square, piece) in &added[..added_count] {
                apply_row(
                    net,
                    &mut child_pair[perspective],
                    parent,
                    perspective as Color,
                    square,
                    piece,
                    true,
                );
            }
        }
    }
}

/// Adds or subtracts the weight row of the piece on `square` from `acc`, seen
/// from `perspective`.
///
/// `frame` is the position whose king defines the perspective's region and
/// mirror. For an incremental update it is the parent position: the frame
/// only changes when the perspective's own king moves, and such a move falls
/// back to a full refresh instead.
pub(crate) fn apply_row(
    net: &Net,
    accumulator: &mut [i16],
    frame: &Position,
    perspective: Color,
    square: Square,
    piece: u8,
    add: bool,
) {
    let index = feature_index(net, frame, perspective, square, piece);
    kernels::add_row(accumulator, net.feature_weights(), index, add);
}

/// Runs the output head over a ready accumulator pair and returns the
/// evaluation in centipawns from the side-to-move perspective.
pub(crate) fn evaluate_pair(net: &Net, stm: Color, pair: &[Vec<i16>; 2]) -> i32 {
    crate::node_stat!(EvalHeadCalls);
    kernels::evaluate_head(stm, pair, net.output_weights(), net.output_bias())
}

static EMBEDDED_NET: OnceLock<Net> = OnceLock::new();

/// Returns the validated embedded production network.
///
/// # Panics
///
/// Panics only if the committed network asset fails validation; the checksum
/// test makes that a build-time packaging error rather than a runtime choice.
pub fn embedded_net() -> &'static Net {
    EMBEDDED_NET.get_or_init(|| {
        Net::from_bytes(crate::tools::net::bytes()).expect("embedded network must be valid")
    })
}

/// Evaluates one position with the embedded network.
///
/// This convenience function is intended for tests and diagnostics. The
/// search will reuse an [`Evaluator`] instead of allocating one per position.
pub fn evaluate_position(position: &Position) -> i32 {
    let net = embedded_net();
    let mut evaluator = Evaluator::new(net);
    evaluator.evaluate(position)
}
