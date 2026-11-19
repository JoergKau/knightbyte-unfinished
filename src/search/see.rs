//! Static exchange evaluation.
//!
//! SEE answers a narrow question: if both sides keep recapturing on one square,
//! which side ends up ahead and by how much? Two entry points mirror the two
//! questions the search asks:
//!
//! * [`see_ge`] — "does this move win at least `threshold`?" — used for
//!   pruning and for splitting captures into good and bad.
//! * [`see_value`] — the exact value — used for ordering.
//! * [`see_approximate`] — the cheap material difference — used as a prefilter
//!   before the exact exchange.
//!
//! The walk keeps a local occupancy, removes the capturing piece each step and
//! recomputes slider attacks, so batteries and discovered recaptures appear
//! automatically. Pinned attackers are excluded unless the pin runs through the
//! target square, and a king never captures into a square the opponent still
//! attacks.
//!
//! Values and the `-1` tie-break in [`see_ge`] follow the ported reference; they
//! are not interchangeable with the evaluation's centipawn scale.

use crate::board::attacks::{
    bishop_attacks, king_attacks, knight_attacks, pawn_attacks, rook_attacks,
};
use crate::board::bitboard::lsb;
use crate::board::position::Position;
use crate::types::color::{BLACK, Color, WHITE, flip};
use crate::types::r#move::{Move, flags};
use crate::types::piece::{BISHOP, EMPTY, KING, KNIGHT, PAWN, QUEEN, ROOK, piece_type};
use crate::types::square::{Square, bit, file, rank};

/// Material values used by the exchange evaluation, in the reference scale.
pub const PIECE_VALUES: [i32; 6] = [109, 403, 435, 679, 1_242, 20_000];

/// Maximum recaptures tracked. A real position cannot sustain more.
const MAX_EXCHANGES: usize = 32;

/// Returns the material value of a piece type.
#[inline(always)]
pub fn piece_value(piece_type_index: usize) -> i32 {
    PIECE_VALUES[piece_type_index]
}

/// Returns the value the move captures under the reference convention.
///
/// An empty target reads as a pawn, matching the reference's index mapping. The
/// search only asks this question for captures, where the convention never
/// matters, but the value is reproduced faithfully for parity.
#[inline]
fn move_value(position: &Position, mv: Move) -> i32 {
    if mv.flag() == flags::EN_PASSANT {
        return piece_value(PAWN);
    }
    let capture = position.piece_at(mv.to());
    let capture_value = if capture == EMPTY {
        piece_value(PAWN)
    } else {
        piece_value(piece_type(capture))
    };
    if mv.is_promotion() {
        let promo = piece_value(promotion_type(mv));
        return capture_value + promo - piece_value(PAWN);
    }
    capture_value
}

/// Returns the exact static exchange value of `mv`.
///
/// Positive values mean the moving side wins material by playing the exchange
/// out. Castling returns zero.
pub fn see_value(position: &Position, mv: Move) -> i32 {
    if mv.flag() == flags::CASTLE {
        return 0;
    }

    let mut scores = [0_i32; MAX_EXCHANGES];
    let mut index = 0;

    let mut value = move_value(position, mv);
    let mut attacker_value = piece_value(piece_type(position.piece_at(mv.from())));
    if mv.is_promotion() {
        let promo = piece_value(promotion_type(mv));
        value += promo - piece_value(PAWN);
        attacker_value = promo;
    }
    scores[0] = value;
    index += 1;

    let from = mv.from();
    let to = mv.to();
    let mut occupied = position.all_occupancy();
    occupied &= !bit(from);
    occupied |= bit(to);
    if mv.flag() == flags::EN_PASSANT {
        occupied &= !bit(to ^ 8);
        scores[0] = piece_value(PAWN);
    }

    let mut attackers = attackers_to(position, to, occupied) & occupied;
    let mut side = flip(position.side());
    let diagonal = diagonal_sliders(position);
    let orthogonal = orthogonal_sliders(position);
    let allowed = allowed_attackers(position, to);

    loop {
        if index >= MAX_EXCHANGES {
            break;
        }
        let ours = attackers & allowed & position.occupancy(side);
        if ours == 0 {
            break;
        }
        let attacker_type = least_valuable_attacker(position, ours);
        if attacker_type == KING && attackers & position.occupancy(flip(side)) != 0 {
            break;
        }
        let candidates = position.pieces(side, attacker_type) & ours;
        let attacker_square = lsb(candidates);
        occupied &= !bit(attacker_square);

        scores[index] = attacker_value - scores[index - 1];
        index += 1;
        attacker_value = piece_value(attacker_type);
        side = flip(side);

        if matches!(attacker_type, PAWN | BISHOP | QUEEN) {
            attackers |= bishop_attacks(to, occupied) & diagonal;
        }
        if matches!(attacker_type, ROOK | QUEEN) {
            attackers |= rook_attacks(to, occupied) & orthogonal;
        }
        attackers &= occupied;
    }

    while index > 1 {
        index -= 1;
        scores[index - 1] = -((-scores[index - 1]).max(scores[index]));
    }
    scores[0]
}

/// Returns whether the exchange wins at least `threshold`.
///
/// This is the pruning form: it can stop as soon as the running balance proves
/// the threshold is met or missed, and it uses the reference's `-1` tie-break
/// so that an even exchange counts as a loss by one point.
pub fn see_ge(position: &Position, mv: Move, threshold: i32) -> bool {
    crate::node_stat!(SeeCalls);
    if mv.flag() == flags::CASTLE {
        return true;
    }

    let side = position.side();
    let mut balance = move_value(position, mv) - threshold;
    if balance < 0 {
        return false;
    }
    balance -= piece_value(piece_type(position.piece_at(mv.from())));
    if mv.is_promotion() {
        balance -= piece_value(promotion_type(mv));
    }
    if balance >= 0 {
        return true;
    }

    let from = mv.from();
    let to = mv.to();
    let mut occupied = position.all_occupancy();
    occupied &= !bit(from);
    occupied |= bit(to);
    if mv.flag() == flags::EN_PASSANT {
        occupied &= !bit(to ^ 8);
    }

    let mut attackers = attackers_to(position, to, occupied) & occupied;
    let mut stm = flip(side);
    let diagonal = diagonal_sliders(position);
    let orthogonal = orthogonal_sliders(position);
    let allowed = allowed_attackers(position, to);

    loop {
        let ours = attackers & allowed & position.occupancy(stm);
        if ours == 0 {
            break;
        }
        let attacker_type = least_valuable_attacker(position, ours);
        if attacker_type == KING && attackers & position.occupancy(flip(stm)) != 0 {
            break;
        }
        let candidates = position.pieces(stm, attacker_type) & ours;
        let attacker_square = lsb(candidates);
        occupied &= !bit(attacker_square);

        stm = flip(stm);
        balance = -balance - 1 - piece_value(attacker_type);
        if balance >= 0 {
            break;
        }

        if matches!(attacker_type, PAWN | BISHOP | QUEEN) {
            attackers |= bishop_attacks(to, occupied) & diagonal;
        }
        if matches!(attacker_type, ROOK | QUEEN) {
            attackers |= rook_attacks(to, occupied) & orthogonal;
        }
        attackers &= occupied;
    }

    stm != side
}

/// Returns the cheap material approximation used as a prefilter.
#[inline]
pub fn see_approximate(position: &Position, mv: Move) -> i32 {
    crate::node_stat!(SeeCalls);
    move_value(position, mv) - piece_value(piece_type(position.piece_at(mv.from())))
}

/// Returns the promoted piece type encoded in the move flag.
fn promotion_type(mv: Move) -> usize {
    match mv.flag() {
        flags::PROMO_KNIGHT => KNIGHT,
        flags::PROMO_BISHOP => BISHOP,
        flags::PROMO_ROOK => ROOK,
        _ => QUEEN,
    }
}

/// Returns every piece of either colour that attacks `square` under `occupied`.
fn attackers_to(position: &Position, square: Square, occupied: u64) -> u64 {
    let mut attackers = 0;
    attackers |= pawn_attacks(BLACK, square) & position.pieces(WHITE, PAWN);
    attackers |= pawn_attacks(WHITE, square) & position.pieces(BLACK, PAWN);
    attackers |=
        knight_attacks(square) & (position.pieces(WHITE, KNIGHT) | position.pieces(BLACK, KNIGHT));
    attackers |=
        king_attacks(square) & (position.pieces(WHITE, KING) | position.pieces(BLACK, KING));
    attackers |= bishop_attacks(square, occupied) & diagonal_sliders(position);
    attackers |= rook_attacks(square, occupied) & orthogonal_sliders(position);
    attackers
}

/// Returns all bishops and queens of both colours.
#[inline]
fn diagonal_sliders(position: &Position) -> u64 {
    position.pieces(WHITE, BISHOP)
        | position.pieces(WHITE, QUEEN)
        | position.pieces(BLACK, BISHOP)
        | position.pieces(BLACK, QUEEN)
}

/// Returns all rooks and queens of both colours.
#[inline]
fn orthogonal_sliders(position: &Position) -> u64 {
    position.pieces(WHITE, ROOK)
        | position.pieces(WHITE, QUEEN)
        | position.pieces(BLACK, ROOK)
        | position.pieces(BLACK, QUEEN)
}

/// Returns the least valuable piece type present in `candidates`.
fn least_valuable_attacker(position: &Position, candidates: u64) -> usize {
    for piece_type_index in [PAWN, KNIGHT, BISHOP, ROOK, QUEEN] {
        if candidates
            & (position.pieces(WHITE, piece_type_index) | position.pieces(BLACK, piece_type_index))
            != 0
        {
            return piece_type_index;
        }
    }
    KING
}

/// Returns the attackers allowed to move without exposing their king.
///
/// A pinned piece may still capture on the pin line, which always includes the
/// target square; every other pin removes the piece from the exchange.
fn allowed_attackers(position: &Position, target: Square) -> u64 {
    let white = pinned_pieces(position, WHITE) & !between(position.king_square(WHITE), target);
    let black = pinned_pieces(position, BLACK) & !between(position.king_square(BLACK), target);
    !(white | black)
}

/// Returns the pieces of `color` that stand between their king and an enemy
/// slider on an open line.
fn pinned_pieces(position: &Position, color: Color) -> u64 {
    let king = position.king_square(color);
    let own = position.occupancy(color);
    if own == 0 {
        return 0;
    }
    // Own pieces are transparent for this query so sliders behind a blocker are
    // still found.
    let visible = position.all_occupancy() & !own;
    let them = flip(color);
    let diagonal = (position.pieces(them, BISHOP) | position.pieces(them, QUEEN))
        & bishop_attacks(king, visible);
    let straight =
        (position.pieces(them, ROOK) | position.pieces(them, QUEEN)) & rook_attacks(king, visible);

    let mut pinned = 0;
    for slider in crate::board::bitboard::bits(diagonal | straight) {
        let blockers = between(king, slider) & position.all_occupancy();
        if blockers.count_ones() == 1 && blockers & own != 0 {
            pinned |= blockers;
        }
    }
    pinned
}

/// Returns the squares strictly between two aligned squares, or zero.
fn between(first: Square, second: Square) -> u64 {
    let first_file = i32::from(file(first));
    let first_rank = i32::from(rank(first));
    let second_file = i32::from(file(second));
    let second_rank = i32::from(rank(second));

    let delta_file = second_file - first_file;
    let delta_rank = second_rank - first_rank;
    let aligned = delta_file == 0 || delta_rank == 0 || delta_file.abs() == delta_rank.abs();
    if !aligned || (delta_file == 0 && delta_rank == 0) {
        return 0;
    }

    let step_file = delta_file.signum();
    let step_rank = delta_rank.signum();
    let mut mask = 0;
    let mut current_file = first_file + step_file;
    let mut current_rank = first_rank + step_rank;
    while current_file != second_file || current_rank != second_rank {
        let square = (current_rank * 8 + current_file) as u8;
        mask |= bit(square);
        current_file += step_file;
        current_rank += step_rank;
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::{PIECE_VALUES, between};
    use crate::types::square::{from_algebraic, from_file_rank};

    #[test]
    fn values_match_the_reference_scale() {
        assert_eq!(PIECE_VALUES, [109, 403, 435, 679, 1_242, 20_000]);
    }

    #[test]
    fn between_covers_the_interior_squares() {
        let a1 = from_algebraic("a1").unwrap();
        let a4 = from_algebraic("a4").unwrap();
        assert_eq!(
            between(a1, a4),
            (1_u64 << from_file_rank(0, 1)) | (1_u64 << from_file_rank(0, 2))
        );
        let c3 = from_algebraic("c3").unwrap();
        let f6 = from_algebraic("f6").unwrap();
        assert_eq!(between(c3, f6).count_ones(), 2);
        assert_eq!(between(a1, from_algebraic("b2").unwrap()), 0);
    }
}
