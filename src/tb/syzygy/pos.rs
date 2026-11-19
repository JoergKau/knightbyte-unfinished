//! A compact board used only by the tablebase prober.
//!
//! The Syzygy indexing code works on a material-and-placement representation
//! that is independent of the engine's [`Position`](crate::board::position::Position):
//! everything is expressed as eight bitboards, the fifty-move counter, the
//! en-passant square and the side to move. Keeping this representation local
//! makes the prober a self-contained unit that can be validated on its own.
//!
//! The move generator here is deliberately small: it produces captures (for
//! the WDL capture resolution) and all moves (for the DTZ/root walk). Legality
//! is tested by making a move and checking that the mover's king is not
//! attacked, exactly like the prober's reference algorithm.

use crate::board::attacks::{
    bishop_attacks, king_attacks, knight_attacks, pawn_attacks, queen_attacks, rook_attacks,
};
use crate::board::position::Position;
use crate::types::color::{BLACK, WHITE};

/// Maximum number of pieces a table may hold.
pub(crate) const TB_PIECES: usize = 7;
/// Maximum number of moves one position can produce (with four promotions).
pub(crate) const TB_MAX_MOVES: usize = 193;
/// Maximum number of capture moves one position can produce.
pub(crate) const TB_MAX_CAPTURES: usize = 64;

/// Tablebase colour value for Black.
pub(crate) const TB_BLACK: i32 = 0;
/// Tablebase colour value for White.
pub(crate) const TB_WHITE: i32 = 1;

/// Tablebase piece code: white pawn.
pub(crate) const W_PAWN: usize = 1;
/// Tablebase piece code: white knight.
pub(crate) const W_KNIGHT: usize = 2;
/// Tablebase piece code: white bishop.
pub(crate) const W_BISHOP: usize = 3;
/// Tablebase piece code: white rook.
pub(crate) const W_ROOK: usize = 4;
/// Tablebase piece code: white queen.
pub(crate) const W_QUEEN: usize = 5;
/// Tablebase piece code: black pawn.
pub(crate) const B_PAWN: usize = 9;
/// Tablebase piece code: black knight.
pub(crate) const B_KNIGHT: usize = 10;
/// Tablebase piece code: black bishop.
pub(crate) const B_BISHOP: usize = 11;
/// Tablebase piece code: black rook.
pub(crate) const B_ROOK: usize = 12;
/// Tablebase piece code: black queen.
pub(crate) const B_QUEEN: usize = 13;

/// Character of every tablebase piece code (index 0 and 7/8 are unused).
pub(crate) const PIECE_TO_CHAR: [u8; 15] = *b" PNBRQK  pnbrqk";

/// Tablebase promotion codes as they appear in the 16-bit move encoding.
pub(crate) const PROMOTES_NONE: u8 = 0;
/// Promotion to a queen.
pub(crate) const PROMOTES_QUEEN: u8 = 1;
/// Promotion to a rook.
pub(crate) const PROMOTES_ROOK: u8 = 2;
/// Promotion to a bishop.
pub(crate) const PROMOTES_BISHOP: u8 = 3;
/// Promotion to a knight.
pub(crate) const PROMOTES_KNIGHT: u8 = 4;

/// Bitboards and state of one position in the prober's representation.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Pos {
    /// All white pieces.
    pub(crate) white: u64,
    /// All black pieces.
    pub(crate) black: u64,
    /// Both kings.
    pub(crate) kings: u64,
    /// Both queens.
    pub(crate) queens: u64,
    /// Both rooks.
    pub(crate) rooks: u64,
    /// Both bishops.
    pub(crate) bishops: u64,
    /// Both knights.
    pub(crate) knights: u64,
    /// Both pawns.
    pub(crate) pawns: u64,
    /// Fifty-move counter, clamped to a byte.
    pub(crate) rule50: u8,
    /// En-passant target square, or zero for "none".
    pub(crate) ep: u8,
    /// `true` when White is to move.
    pub(crate) turn: bool,
}

impl Pos {
    /// Returns the occupancy of one tablebase colour.
    #[inline]
    pub(crate) fn color(&self, color: i32) -> u64 {
        if color == TB_WHITE {
            self.white
        } else {
            self.black
        }
    }

    /// Returns the bitboard of one colour and piece type (types 1..=6).
    #[inline]
    pub(crate) fn pieces_by_type(&self, color: i32, piece_type: i32) -> u64 {
        let mask = self.color(color);
        match piece_type {
            1 => self.pawns & mask,
            2 => self.knights & mask,
            3 => self.bishops & mask,
            4 => self.rooks & mask,
            5 => self.queens & mask,
            _ => self.kings & mask,
        }
    }
}

/// Converts an engine position into the prober representation.
///
/// `rule50` is passed separately so the leaf probes can deliberately ignore
/// the real counter (the search treats tablebase values as exact), while the
/// root probe can honour it.
pub(crate) fn to_pos(position: &Position, rule50: u8) -> Pos {
    let white = position.occupancy(WHITE);
    let black = position.occupancy(BLACK);
    let group =
        |piece_type: usize| position.pieces(WHITE, piece_type) | position.pieces(BLACK, piece_type);
    let ep = position.en_passant();
    Pos {
        white,
        black,
        kings: group(5),
        queens: group(4),
        rooks: group(3),
        bishops: group(2),
        knights: group(1),
        pawns: group(0),
        rule50,
        ep: if ep < 64 { ep } else { 0 },
        turn: position.side() == WHITE,
    }
}

/// Returns the tablebase colour of a piece code.
#[inline]
pub(crate) fn color_of_piece(piece: usize) -> i32 {
    if piece >> 3 == 0 { TB_WHITE } else { TB_BLACK }
}

/// Returns the tablebase type (1..=6) of a piece code.
#[inline]
pub(crate) fn type_of_piece(piece: usize) -> usize {
    piece & 7
}

/// Returns the knight attacks from `square`.
#[inline]
fn knights(square: usize) -> u64 {
    knight_attacks(square as u8)
}

/// Returns the king attacks from `square`.
#[inline]
fn kings(square: usize) -> u64 {
    king_attacks(square as u8)
}

/// Returns the bishop attacks from `square` with `occupancy`.
#[inline]
fn bishops(square: usize, occupancy: u64) -> u64 {
    bishop_attacks(square as u8, occupancy)
}

/// Returns the rook attacks from `square` with `occupancy`.
#[inline]
fn rooks(square: usize, occupancy: u64) -> u64 {
    rook_attacks(square as u8, occupancy)
}

/// Returns the queen attacks from `square` with `occupancy`.
#[inline]
fn queens(square: usize, occupancy: u64) -> u64 {
    queen_attacks(square as u8, occupancy)
}

/// Returns the pawn attacks of a tablebase colour from `square`.
#[inline]
fn pawns(square: usize, color: i32) -> u64 {
    let color = if color == TB_WHITE { WHITE } else { BLACK };
    pawn_attacks(color, square as u8)
}

/// Builds the 16-bit prober move encoding.
#[inline]
pub(crate) fn make_move(promote: u8, from: usize, to: usize) -> u16 {
    ((promote as u16 & 7) << 12) | ((from as u16 & 0x3f) << 6) | (to as u16 & 0x3f)
}

/// Returns the source square of a prober move.
#[inline]
pub(crate) fn move_from(mv: u16) -> usize {
    ((mv >> 6) & 0x3f) as usize
}

/// Returns the target square of a prober move.
#[inline]
pub(crate) fn move_to(mv: u16) -> usize {
    (mv & 0x3f) as usize
}

/// Returns the promotion code of a prober move.
#[inline]
pub(crate) fn move_promotes(mv: u16) -> u8 {
    ((mv >> 12) & 0x7) as u8
}

/// Appends one move, or the four promotion moves, to `moves`.
fn add_move(moves: &mut [u16], at: usize, promotes: bool, from: usize, to: usize) -> usize {
    if !promotes {
        moves[at] = make_move(PROMOTES_NONE, from, to);
        at + 1
    } else {
        moves[at] = make_move(PROMOTES_QUEEN, from, to);
        moves[at + 1] = make_move(PROMOTES_KNIGHT, from, to);
        moves[at + 2] = make_move(PROMOTES_ROOK, from, to);
        moves[at + 3] = make_move(PROMOTES_BISHOP, from, to);
        at + 4
    }
}

/// Returns the type (1..=6) of the piece moving in `mv`, or 0.
pub(crate) fn type_of_piece_moved(pos: &Pos, mv: u16) -> i32 {
    let square = 1_u64 << move_from(mv);
    let side = if pos.turn { TB_WHITE } else { TB_BLACK };
    for piece_type in 1..=6 {
        if pos.pieces_by_type(side, piece_type) & square != 0 {
            return piece_type;
        }
    }
    0
}

/// Generates all legal captures (including promotions and en passant).
pub(crate) fn gen_captures(pos: &Pos, moves: &mut [u16]) -> usize {
    let occupancy = pos.white | pos.black;
    let us = if pos.turn { pos.white } else { pos.black };
    let them = if pos.turn { pos.black } else { pos.white };
    let mut at = 0;

    let king_bb = pos.kings & us;
    if king_bb == 0 {
        return 0; // Defensive: an illegal position without the side-to-move king.
    }
    let from_king = king_bb.trailing_zeros() as usize;
    let mut attacks = kings(from_king) & them;
    while attacks != 0 {
        let to = attacks.trailing_zeros() as usize;
        at = add_move(moves, at, false, from_king, to);
        attacks &= attacks - 1;
    }

    let mut pieces = us & pos.queens;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = queens(from, occupancy) & them;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.rooks;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = rooks(from, occupancy) & them;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.bishops;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = bishops(from, occupancy) & them;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.knights;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = knights(from) & them;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.pawns;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = pawns(from, if pos.turn { TB_WHITE } else { TB_BLACK });
        if pos.ep != 0 && attacks & (1_u64 << pos.ep) != 0 {
            at = add_move(moves, at, false, from, pos.ep as usize);
        }
        attacks &= them;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            let promotes = (to >> 3) == 7 || (to >> 3) == 0;
            at = add_move(moves, at, promotes, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }
    at
}

/// Generates all pseudo-legal moves.
pub(crate) fn gen_moves(pos: &Pos, moves: &mut [u16]) -> usize {
    let occupancy = pos.white | pos.black;
    let us = if pos.turn { pos.white } else { pos.black };
    let them = if pos.turn { pos.black } else { pos.white };
    let mut at = 0;

    let king_bb = pos.kings & us;
    if king_bb == 0 {
        return 0;
    }
    let from_king = king_bb.trailing_zeros() as usize;
    let mut attacks = kings(from_king) & !us;
    while attacks != 0 {
        let to = attacks.trailing_zeros() as usize;
        at = add_move(moves, at, false, from_king, to);
        attacks &= attacks - 1;
    }

    let mut pieces = us & pos.queens;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = queens(from, occupancy) & !us;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.rooks;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = rooks(from, occupancy) & !us;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.bishops;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = bishops(from, occupancy) & !us;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.knights;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let mut attacks = knights(from) & !us;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            at = add_move(moves, at, false, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }

    let mut pieces = us & pos.pawns;
    while pieces != 0 {
        let from = pieces.trailing_zeros() as usize;
        let next = if pos.turn {
            from + 8
        } else {
            from.wrapping_sub(8)
        };
        let mut attacks = pawns(from, if pos.turn { TB_WHITE } else { TB_BLACK });
        if pos.ep != 0 && attacks & (1_u64 << pos.ep) != 0 {
            at = add_move(moves, at, false, from, pos.ep as usize);
        }
        attacks &= them;
        if next < 64 && (1_u64 << next) & occupancy == 0 {
            attacks |= 1_u64 << next;
            let next2 = if pos.turn {
                from + 16
            } else {
                from.wrapping_sub(16)
            };
            let on_start_rank = if pos.turn {
                (from >> 3) == 1
            } else {
                (from >> 3) == 6
            };
            if on_start_rank && next2 < 64 && (1_u64 << next2) & occupancy == 0 {
                attacks |= 1_u64 << next2;
            }
        }
        while attacks != 0 {
            let to = attacks.trailing_zeros() as usize;
            let promotes = (to >> 3) == 7 || (to >> 3) == 0;
            at = add_move(moves, at, promotes, from, to);
            attacks &= attacks - 1;
        }
        pieces &= pieces - 1;
    }
    at
}

/// Returns whether `mv` is an en-passant capture.
pub(crate) fn is_en_passant(pos: &Pos, mv: u16) -> bool {
    let from = move_from(mv);
    let to = move_to(mv);
    let us = if pos.turn { pos.white } else { pos.black };
    pos.ep != 0 && to == pos.ep as usize && (1_u64 << from) & us & pos.pawns != 0
}

/// Returns whether `mv` captures a piece (including en passant).
pub(crate) fn is_capture(pos: &Pos, mv: u16) -> bool {
    let to = move_to(mv);
    let them = if pos.turn { pos.black } else { pos.white };
    (them & (1_u64 << to)) != 0 || is_en_passant(pos, mv)
}

/// Returns whether the side that just moved left its own king safe.
fn is_legal(pos: &Pos) -> bool {
    let occupancy = pos.white | pos.black;
    let us = if pos.turn { pos.black } else { pos.white };
    let them = if pos.turn { pos.white } else { pos.black };
    let king = pos.kings & us;
    if king == 0 {
        return false;
    }
    let square = king.trailing_zeros() as usize;
    if kings(square) & (pos.kings & them) != 0 {
        return false;
    }
    let rook_attacks = rooks(square, occupancy);
    let bishop_attacks = bishops(square, occupancy);
    if rook_attacks & (pos.rooks & them) != 0
        || bishop_attacks & (pos.bishops & them) != 0
        || (rook_attacks | bishop_attacks) & (pos.queens & them) != 0
    {
        return false;
    }
    if knights(square) & (pos.knights & them) != 0 {
        return false;
    }
    if pawns(square, if pos.turn { TB_BLACK } else { TB_WHITE }) & (pos.pawns & them) != 0 {
        return false;
    }
    true
}

/// Returns whether the side to move is in check.
pub(crate) fn is_check(pos: &Pos) -> bool {
    let occupancy = pos.white | pos.black;
    let us = if pos.turn { pos.white } else { pos.black };
    let them = if pos.turn { pos.black } else { pos.white };
    let king = pos.kings & us;
    if king == 0 {
        return false;
    }
    let square = king.trailing_zeros() as usize;
    let rook_attacks = rooks(square, occupancy);
    let bishop_attacks = bishops(square, occupancy);
    if rook_attacks & (pos.rooks & them) != 0
        || bishop_attacks & (pos.bishops & them) != 0
        || (rook_attacks | bishop_attacks) & (pos.queens & them) != 0
    {
        return true;
    }
    if knights(square) & (pos.knights & them) != 0 {
        return true;
    }
    pawns(square, if pos.turn { TB_WHITE } else { TB_BLACK }) & (pos.pawns & them) != 0
}

/// Validates the prober representation before any indexing happens.
pub(crate) fn is_valid(pos: &Pos) -> bool {
    const FILE_EDGE: u64 = 0xFF00_0000_0000_00FF;
    if pos.kings.count_ones() != 2 {
        return false;
    }
    if (pos.kings & pos.white).count_ones() != 1 || (pos.kings & pos.black).count_ones() != 1 {
        return false;
    }
    if pos.white & pos.black != 0 {
        return false;
    }
    let types = pos.queens | pos.rooks | pos.bishops | pos.knights | pos.pawns;
    if pos.kings & types != 0 {
        return false;
    }
    if pos.queens & (pos.rooks | pos.bishops | pos.knights | pos.pawns) != 0 {
        return false;
    }
    if pos.rooks & (pos.bishops | pos.knights | pos.pawns) != 0 {
        return false;
    }
    if pos.bishops & (pos.knights | pos.pawns) != 0 {
        return false;
    }
    if pos.knights & pos.pawns != 0 {
        return false;
    }
    if pos.pawns & FILE_EDGE != 0 {
        return false;
    }
    if (pos.white | pos.black) != (pos.kings | types) {
        return false;
    }
    is_legal(pos)
}

/// Moves a piece on one bitboard.
#[inline]
fn do_bb_move(board: u64, from: usize, to: usize) -> u64 {
    (board & !(1_u64 << to) & !(1_u64 << from)) | (((board >> from) & 1) << to)
}

/// Applies `mv` to `pos0` and returns the next position and its legality.
pub(crate) fn do_move(pos0: &Pos, mv: u16) -> (Pos, bool) {
    let from = move_from(mv);
    let to = move_to(mv);
    let promotes = move_promotes(mv);
    let mut pos = *pos0;
    pos.turn = !pos0.turn;
    pos.white = do_bb_move(pos0.white, from, to);
    pos.black = do_bb_move(pos0.black, from, to);
    pos.kings = do_bb_move(pos0.kings, from, to);
    pos.queens = do_bb_move(pos0.queens, from, to);
    pos.rooks = do_bb_move(pos0.rooks, from, to);
    pos.bishops = do_bb_move(pos0.bishops, from, to);
    pos.knights = do_bb_move(pos0.knights, from, to);
    pos.pawns = do_bb_move(pos0.pawns, from, to);
    pos.ep = 0;

    if promotes != PROMOTES_NONE {
        pos.pawns &= !(1_u64 << to);
        match promotes {
            PROMOTES_QUEEN => pos.queens |= 1_u64 << to,
            PROMOTES_ROOK => pos.rooks |= 1_u64 << to,
            PROMOTES_BISHOP => pos.bishops |= 1_u64 << to,
            _ => pos.knights |= 1_u64 << to,
        }
        pos.rule50 = 0;
    } else if (1_u64 << from) & pos0.pawns != 0 {
        pos.rule50 = 0;
        if (from >> 3) == 1
            && (to >> 3) == 3
            && pawns(from + 8, TB_WHITE) & pos0.pawns & pos0.black != 0
        {
            pos.ep = (from + 8) as u8;
        } else if (from >> 3) == 6
            && (to >> 3) == 4
            && pawns(from - 8, TB_BLACK) & pos0.pawns & pos0.white != 0
        {
            pos.ep = (from - 8) as u8;
        } else if to == pos0.ep as usize {
            let ep_to = if pos0.turn { to - 8 } else { to + 8 };
            let mask = !(1_u64 << ep_to);
            pos.white &= mask;
            pos.black &= mask;
            pos.pawns &= mask;
        }
    } else if (1_u64 << to) & (pos0.white | pos0.black) != 0 {
        pos.rule50 = 0;
    } else {
        pos.rule50 = pos0.rule50.saturating_add(1);
    }

    let legal = is_legal(&pos);
    (pos, legal)
}

/// Returns whether `mv` is legal in `pos`.
pub(crate) fn legal_move(pos: &Pos, mv: u16) -> bool {
    do_move(pos, mv).1
}

/// Returns whether the side to move is checkmated.
pub(crate) fn is_mate(pos: &Pos) -> bool {
    if !is_check(pos) {
        return false;
    }
    let mut moves = [0_u16; TB_MAX_MOVES];
    let end = gen_moves(pos, &mut moves);
    for &mv in &moves[..end] {
        if do_move(pos, mv).1 {
            return false;
        }
    }
    true
}

/// Generates the legal moves of `pos`.
pub(crate) fn gen_legal(pos: &Pos, moves: &mut [u16]) -> usize {
    let mut pseudo = [0_u16; TB_MAX_MOVES];
    let end = gen_moves(pos, &mut pseudo);
    let mut at = 0;
    for &mv in &pseudo[..end] {
        if legal_move(pos, mv) {
            moves[at] = mv;
            at += 1;
        }
    }
    at
}
