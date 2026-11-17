//! Recursive perft counting.

use crate::board::movegen::legal;
use crate::board::position::Position;

/// Counts the leaf nodes of the legal move tree at `depth`.
///
/// This is the correctness gate for move generation, castling, en passant,
/// promotions and copy-make state restoration.
pub fn perft(position: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = legal(position);
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0_u64;
    for &mv in moves.as_slice() {
        let next = position.make_move(mv);
        nodes += perft(&next, depth - 1);
    }
    nodes
}

/// Counts perft nodes split by root move.
///
/// Useful when a perft mismatch has to be narrowed to one branch without
/// instrumenting the recursive counter.
pub fn perft_divide(position: &Position, depth: u32) -> Vec<(String, u64)> {
    let moves = legal(position);
    moves
        .as_slice()
        .iter()
        .map(|&mv| {
            let next = position.make_move(mv);
            (mv.to_uci(), perft(&next, depth.saturating_sub(1)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::perft;
    use crate::board::position::Position;

    #[test]
    fn startpos_perft_one_to_three() {
        let position =
            Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        assert_eq!(perft(&position, 1), 20);
        assert_eq!(perft(&position, 2), 400);
        assert_eq!(perft(&position, 3), 8902);
    }

    #[test]
    fn kiwipete_perft_two_is_2039() {
        let position = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(perft(&position, 2), 2039);
    }
}
