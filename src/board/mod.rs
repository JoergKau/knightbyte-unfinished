//! Board representation, attack generation, move generation and perft.

pub mod attacks;
pub mod bitboard;
pub mod fen;
pub mod legality;
pub mod movegen;
pub mod perft;
pub mod position;
pub mod zobrist;

pub use fen::FenError;
pub use position::{CASTLE_BK, CASTLE_BQ, CASTLE_WK, CASTLE_WQ, Position};
