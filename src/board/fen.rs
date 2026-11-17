//! Forsyth-Edwards Notation parsing and serialization.

use std::fmt;

use crate::board::position::{CASTLE_BK, CASTLE_BQ, CASTLE_WK, CASTLE_WQ, Position};
use crate::board::zobrist;
use crate::types::color::{BLACK, WHITE};
use crate::types::piece::{EMPTY, KING, color_of, from_char, piece_type, to_char};
use crate::types::square::{NO_SQUARE, bit, from_algebraic, from_file_rank, to_algebraic};

/// Standard starting position in FEN form.
pub const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A malformed FEN string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FenError {
    message: String,
}

impl FenError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for FenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FenError {}

impl Position {
    /// Parses a six-field FEN string.
    ///
    /// # Errors
    ///
    /// Returns [`FenError`] when the field count, board geometry, side,
    /// castling rights, en-passant square or move counters are invalid, or
    /// when either side does not have exactly one king.
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let fields: Vec<&str> = fen.split_whitespace().collect();
        if fields.len() != 6 {
            return Err(FenError::new("FEN must contain exactly six fields"));
        }

        let mut board = [EMPTY; 64];
        let mut pieces = [[0_u64; 6]; 2];
        let mut king_sq = [NO_SQUARE; 2];
        let mut king_count = [0_u8; 2];

        let ranks: Vec<&str> = fields[0].split('/').collect();
        if ranks.len() != 8 {
            return Err(FenError::new("FEN board must contain eight ranks"));
        }

        for (index, rank_text) in ranks.iter().enumerate() {
            let rank_index = 7 - index as u8;
            let mut file_index = 0_u8;
            for character in rank_text.chars() {
                if let Some(empty) = character.to_digit(10) {
                    if empty == 0 {
                        return Err(FenError::new("FEN contains a zero empty-run"));
                    }
                    file_index = file_index
                        .checked_add(empty as u8)
                        .ok_or_else(|| FenError::new("FEN rank overflows"))?;
                } else {
                    let piece = from_char(character)
                        .ok_or_else(|| FenError::new(format!("unknown FEN piece '{character}'")))?;
                    if file_index >= 8 {
                        return Err(FenError::new("FEN rank is wider than eight files"));
                    }
                    let square = from_file_rank(file_index, rank_index);
                    board[square as usize] = piece;
                    let color = color_of(piece) as usize;
                    pieces[color][piece_type(piece)] |= bit(square);
                    if piece_type(piece) == KING {
                        king_sq[color] = square;
                        king_count[color] += 1;
                    }
                    file_index += 1;
                }
            }
            if file_index != 8 {
                return Err(FenError::new("FEN rank is not exactly eight files wide"));
            }
        }

        if king_count[WHITE as usize] != 1 {
            return Err(FenError::new("FEN must contain exactly one white king"));
        }
        if king_count[BLACK as usize] != 1 {
            return Err(FenError::new("FEN must contain exactly one black king"));
        }

        let side = match fields[1] {
            "w" => WHITE,
            "b" => BLACK,
            _ => return Err(FenError::new("FEN side to move must be 'w' or 'b'")),
        };

        let mut castling = 0_u8;
        if fields[2] != "-" {
            for character in fields[2].chars() {
                let rights = match character {
                    'K' => CASTLE_WK,
                    'Q' => CASTLE_WQ,
                    'k' => CASTLE_BK,
                    'q' => CASTLE_BQ,
                    _ => return Err(FenError::new("invalid FEN castling rights")),
                };
                if castling & rights != 0 {
                    return Err(FenError::new("duplicate FEN castling right"));
                }
                castling |= rights;
            }
        }

        let ep = if fields[3] == "-" {
            NO_SQUARE
        } else {
            from_algebraic(fields[3])
                .ok_or_else(|| FenError::new("invalid FEN en-passant square"))?
        };

        let halfmove = fields[4]
            .parse::<u16>()
            .map_err(|_| FenError::new("invalid FEN halfmove clock"))?;
        let fullmove = fields[5]
            .parse::<u16>()
            .map_err(|_| FenError::new("invalid FEN fullmove number"))?;
        if fullmove == 0 {
            return Err(FenError::new("FEN fullmove number must be at least 1"));
        }

        let occupancy = [
            pieces[0].iter().fold(0_u64, |all, board| all | *board),
            pieces[1].iter().fold(0_u64, |all, board| all | *board),
        ];

        let mut position = Self {
            board,
            pieces,
            occupancy,
            all: occupancy[0] | occupancy[1],
            side,
            castling,
            ep,
            halfmove,
            fullmove,
            king_sq,
            key: 0,
        };
        position.key = zobrist::compute(&position);
        Ok(position)
    }

    /// Serializes the position as a six-field FEN string.
    pub fn to_fen(&self) -> String {
        let mut board = String::new();
        for rank_index in (0_u8..8).rev() {
            let mut empty = 0_u8;
            for file_index in 0_u8..8 {
                let square = from_file_rank(file_index, rank_index);
                let piece = self.board[square as usize];
                if piece == EMPTY {
                    empty += 1;
                } else {
                    if empty > 0 {
                        board.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    board.push(to_char(piece));
                }
            }
            if empty > 0 {
                board.push(char::from(b'0' + empty));
            }
            if rank_index > 0 {
                board.push('/');
            }
        }

        let side = if self.side == WHITE { "w" } else { "b" };
        let mut castling = String::new();
        if self.castling & CASTLE_WK != 0 {
            castling.push('K');
        }
        if self.castling & CASTLE_WQ != 0 {
            castling.push('Q');
        }
        if self.castling & CASTLE_BK != 0 {
            castling.push('k');
        }
        if self.castling & CASTLE_BQ != 0 {
            castling.push('q');
        }
        if castling.is_empty() {
            castling.push('-');
        }
        let ep = if self.ep == NO_SQUARE {
            "-".to_owned()
        } else {
            to_algebraic(self.ep)
        };

        format!(
            "{board} {side} {castling} {ep} {} {}",
            self.halfmove, self.fullmove
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::position::Position;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn startpos_round_trips() {
        let position = Position::from_fen(START).expect("valid start position");
        assert_eq!(position.to_fen(), START);
    }

    #[test]
    fn parses_black_to_move_and_counters() {
        let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R b KQkq - 3 17";
        let position = Position::from_fen(fen).expect("valid FEN");
        assert_eq!(position.side(), 1);
        assert_eq!(position.halfmove_clock(), 3);
        assert_eq!(position.fullmove_number(), 17);
        assert_eq!(position.to_fen(), fen);
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(Position::from_fen("not a fen").is_err());
        assert!(Position::from_fen("8/8/8/8/8/8/8/8 w - - 0 1").is_err());
        assert!(
            Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1").is_err()
        );
    }
}
