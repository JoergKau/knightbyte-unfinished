//! Pseudo-legal and legal move generation.
//!
//! Phase 2 generates pseudo-legal moves and filters them by making each move
//! on a copy and checking the moving side's king. This is the simplest
//! correctness-oriented design; it handles pins, discovered checks, en
//! passant and castling without a separate pin-detection layer. A later
//! performance phase can switch to staged legal generation behind the same
//! public functions.

use crate::board::attacks::{attacks, pawn_attacks};
use crate::board::bitboard::bits;
use crate::board::position::{CASTLE_BK, CASTLE_BQ, CASTLE_WK, CASTLE_WQ, Position};
use crate::types::color::{WHITE, flip};
use crate::types::r#move::{Move, MoveList, flags};
use crate::types::piece::{BISHOP, KING, KNIGHT, PAWN, QUEEN, ROOK, piece_type};
use crate::types::square::{NO_SQUARE, Square, bit, rank};

/// Generates all pseudo-legal moves for the side to move.
///
/// Castling is generated only when the rights, empty squares and attack
/// constraints are satisfied. The resulting moves may still leave the king in
/// check; use [`legal`] for playable moves.
pub fn pseudo_legal(position: &Position) -> MoveList {
    let mut moves = MoveList::new();
    let us = position.side();
    let them = flip(us);
    let own = position.occupancy(us);
    let all = position.all_occupancy();

    generate_pawns(position, us, them, &mut moves);
    generate_piece_moves(position, us, KNIGHT, own, all, &mut moves);
    generate_piece_moves(position, us, BISHOP, own, all, &mut moves);
    generate_piece_moves(position, us, ROOK, own, all, &mut moves);
    generate_piece_moves(position, us, QUEEN, own, all, &mut moves);
    generate_king_moves(position, us, own, &mut moves);
    generate_castles(position, us, them, &mut moves);

    moves
}

/// Generates all legal moves for the side to move.
pub fn legal(position: &Position) -> MoveList {
    let pseudo = pseudo_legal(position);
    let mut moves = MoveList::new();
    for &mv in pseudo.as_slice() {
        let next = position.make_move(mv);
        if !next.in_check(position.side()) {
            moves.push(mv);
        }
    }
    moves
}

/// Returns whether the side to move has at least one legal move.
pub fn has_legal_move(position: &Position) -> bool {
    let pseudo = pseudo_legal(position);
    for &mv in pseudo.as_slice() {
        let next = position.make_move(mv);
        if !next.in_check(position.side()) {
            return true;
        }
    }
    false
}

fn generate_pawns(position: &Position, us: u8, them: u8, moves: &mut MoveList) {
    let direction: i8 = if us == WHITE { 8 } else { -8 };
    let start_rank: u8 = if us == WHITE { 1 } else { 6 };
    let promotion_rank: u8 = if us == WHITE { 7 } else { 0 };
    let pawns = position.pieces(us, PAWN);
    let enemy = position.occupancy(them);
    let empty = !position.all_occupancy();

    for from in bits(pawns) {
        let one = (from as i8 + direction) as u8;
        if position.piece_at(one) == 0 {
            if rank(one) == promotion_rank {
                add_promotions(moves, from, one);
            } else {
                moves.push(Move::new(from, one, flags::NORMAL));
                if rank(from) == start_rank {
                    let two = (from as i8 + direction * 2) as u8;
                    if empty & bit(two) != 0 {
                        moves.push(Move::new(from, two, flags::DOUBLE_PUSH));
                    }
                }
            }
        }

        let mut targets = pawn_attacks(us, from) & enemy;
        while targets != 0 {
            let to = crate::board::bitboard::lsb(targets);
            targets &= targets.wrapping_sub(1);
            if rank(to) == promotion_rank {
                add_promotions(moves, from, to);
            } else {
                moves.push(Move::new(from, to, flags::NORMAL));
            }
        }

        let ep = position.en_passant();
        if ep != NO_SQUARE && pawn_attacks(us, from) & bit(ep) != 0 && position.piece_at(ep) == 0 {
            moves.push(Move::new(from, ep, flags::EN_PASSANT));
        }
    }
}

fn add_promotions(moves: &mut MoveList, from: Square, to: Square) {
    moves.push(Move::new(from, to, flags::PROMO_KNIGHT));
    moves.push(Move::new(from, to, flags::PROMO_BISHOP));
    moves.push(Move::new(from, to, flags::PROMO_ROOK));
    moves.push(Move::new(from, to, flags::PROMO_QUEEN));
}

fn generate_piece_moves(
    position: &Position,
    us: u8,
    piece_type_index: usize,
    own: u64,
    all: u64,
    moves: &mut MoveList,
) {
    let mut pieces = position.pieces(us, piece_type_index);
    while pieces != 0 {
        let from = crate::board::bitboard::lsb(pieces);
        pieces &= pieces.wrapping_sub(1);
        let mut targets = attacks(piece_type_index, us, from, all) & !own;
        while targets != 0 {
            let to = crate::board::bitboard::lsb(targets);
            targets &= targets.wrapping_sub(1);
            moves.push(Move::new(from, to, flags::NORMAL));
        }
    }
}

fn generate_king_moves(position: &Position, us: u8, own: u64, moves: &mut MoveList) {
    let from = position.king_square(us);
    let mut targets = attacks(KING, us, from, position.all_occupancy()) & !own;
    while targets != 0 {
        let to = crate::board::bitboard::lsb(targets);
        targets &= targets.wrapping_sub(1);
        moves.push(Move::new(from, to, flags::NORMAL));
    }
}

fn generate_castles(position: &Position, us: u8, enemy: u8, moves: &mut MoveList) {
    let king_from = position.king_square(us);
    let rights = position.castling_rights();
    let rooks = position.pieces(us, ROOK);

    if us == WHITE {
        if rights & CASTLE_WK != 0
            && rooks & bit(7) != 0
            && position.piece_at(5) == 0
            && position.piece_at(6) == 0
            && !position.is_attacked(4, enemy)
            && !position.is_attacked(5, enemy)
            && !position.is_attacked(6, enemy)
        {
            moves.push(Move::new(king_from, 6, flags::CASTLE));
        }
        if rights & CASTLE_WQ != 0
            && rooks & bit(0) != 0
            && position.piece_at(1) == 0
            && position.piece_at(2) == 0
            && position.piece_at(3) == 0
            && !position.is_attacked(4, enemy)
            && !position.is_attacked(3, enemy)
            && !position.is_attacked(2, enemy)
        {
            moves.push(Move::new(king_from, 2, flags::CASTLE));
        }
    } else {
        if rights & CASTLE_BK != 0
            && rooks & bit(63) != 0
            && position.piece_at(61) == 0
            && position.piece_at(62) == 0
            && !position.is_attacked(60, enemy)
            && !position.is_attacked(61, enemy)
            && !position.is_attacked(62, enemy)
        {
            moves.push(Move::new(king_from, 62, flags::CASTLE));
        }
        if rights & CASTLE_BQ != 0
            && rooks & bit(56) != 0
            && position.piece_at(57) == 0
            && position.piece_at(58) == 0
            && position.piece_at(59) == 0
            && !position.is_attacked(60, enemy)
            && !position.is_attacked(59, enemy)
            && !position.is_attacked(58, enemy)
        {
            moves.push(Move::new(king_from, 58, flags::CASTLE));
        }
    }
}

/// Returns the UCI move list for diagnostics and search output.
pub fn uci_moves(position: &Position) -> Vec<String> {
    legal(position)
        .as_slice()
        .iter()
        .map(|mv| mv.to_uci())
        .collect()
}

/// Returns whether a move is a castling move.
#[inline]
pub fn is_castle(mv: Move) -> bool {
    mv.flag() == flags::CASTLE
}

/// Returns whether a move is an en-passant capture.
#[inline]
pub fn is_en_passant(mv: Move) -> bool {
    mv.flag() == flags::EN_PASSANT
}

/// Returns whether a move is a capture, including en passant.
pub fn is_capture(position: &Position, mv: Move) -> bool {
    is_en_passant(mv) || position.piece_at(mv.to()) != 0
}

/// Returns whether a move is a pawn move.
pub fn is_pawn_move(position: &Position, mv: Move) -> bool {
    piece_type(position.piece_at(mv.from())) == PAWN
}

/// Returns whether a move is a check, by making it and inspecting the opponent.
pub fn gives_check(position: &Position, mv: Move) -> bool {
    let next = position.make_move(mv);
    next.in_check(next.side())
}

#[cfg(test)]
mod tests {
    use super::{legal, pseudo_legal};
    use crate::board::position::Position;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn startpos_has_twenty_legal_moves() {
        let position = Position::from_fen(START).unwrap();
        assert_eq!(pseudo_legal(&position).len(), 20);
        assert_eq!(legal(&position).len(), 20);
    }

    #[test]
    fn kiwipete_has_forty_eight_legal_moves() {
        let position = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(legal(&position).len(), 48);
    }

    #[test]
    fn position_four_has_six_legal_moves() {
        let position =
            Position::from_fen("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1")
                .unwrap();
        assert_eq!(legal(&position).len(), 6);
    }

    #[test]
    fn en_passant_move_is_generated() {
        let position =
            Position::from_fen("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3")
                .unwrap();
        let moves: Vec<String> = legal(&position)
            .as_slice()
            .iter()
            .map(|mv| mv.to_uci())
            .collect();
        assert!(moves.contains(&"e5d6".to_owned()));
    }

    #[test]
    fn castle_is_generated_when_safe() {
        let position = Position::from_fen("4k3/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
        let moves = legal(&position);
        assert!(
            moves
                .as_slice()
                .iter()
                .any(|mv| mv.from() == 4 && mv.to() == 6)
        );
    }
}
