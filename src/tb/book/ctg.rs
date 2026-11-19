//! Reader and probe for CTG `.ctg` opening books.
//!
//! A `.ctg` file is a sequence of 4 KiB pages. Page 0 is a header; every later
//! page holds up to a few hundred position entries. Each entry stores the
//! position signature, the book moves with their annotation bytes, and the
//! game statistics of the position. Only the `.ctg` file is required; the
//! editor's companion files are not used.
//!
//! The loader scans the file once and indexes every entry by the FNV-1a hash
//! of its signature. Probing then follows the canonical position signature,
//! decodes each stored move byte into a square pair, matches it against the
//! position's legal moves, and chooses by a weight derived from the statistics
//! of the child position: `2 * wins + draws`, scaled by the annotation.

use std::collections::HashMap;
use std::path::PathBuf;

use super::rng::BookRng;
use super::signature;
use crate::board::movegen::legal;
use crate::board::position::Position;
use crate::types::color::BLACK;
use crate::types::r#move::Move;
use crate::types::piece::{EMPTY, Piece, to_char};

/// Page size of the format.
const PAGE_SIZE: usize = 4096;

/// Move-byte glyph table (256 format constants).
const PIECE_CODE: &str = concat!(
    "PNxQPQPxQBKxPBRNxxBKPBxxPxQBxBxxxRBQPxBPQQNxxPBQNQBxNxNQQQBQBxxx",
    "xQQxKQxxxxPQNQxxRxRxBPxxxxxxPxxPxQPQxxBKxRBxxxRQxxBxQxxxxBRRPRQR",
    "QRPxxNRRxxNPKxQQxxQxQxPKRRQPxQxBQxQPxRxxxRxQxRQxQPBxxRxQxBxPQQKx",
    "xBBBRRQPPQBPBRxPxPNNxxxQRQNPxxPKNRxRxQPQRNxPPQQRQQxNRBxNQQQQxQQx"
);

/// Piece occurrence index per move byte (1-based; 0 and 9 are invalid).
#[rustfmt::skip]
const PIECE_INDEX: [u8; 256] = [
    5,2,9,2,2,1,4,9,2,2,1,9,1,1,2,1,9,9,1,1,8,1,9,9,7,9,2,1,9,2,9,9,
    9,2,2,2,8,9,1,3,1,1,2,9,9,6,1,1,2,1,2,9,1,9,1,1,2,1,1,2,1,9,9,9,
    9,2,1,9,1,1,9,9,9,9,8,1,2,2,9,9,1,9,1,9,2,3,9,9,9,9,9,9,7,9,9,5,
    9,1,2,2,9,9,1,1,9,2,1,0,9,9,1,2,9,9,2,9,1,9,9,9,9,2,1,2,3,2,1,1,
    1,1,6,9,9,1,1,1,9,9,1,1,1,9,2,1,9,9,2,9,1,9,2,1,1,1,1,3,9,1,9,2,
    2,9,1,8,9,2,9,9,9,2,9,2,9,2,2,9,2,6,1,9,9,2,9,1,9,2,9,5,2,2,1,9,
    9,1,2,1,2,2,2,7,7,2,2,6,2,1,9,4,9,2,2,2,9,9,9,1,2,1,1,1,9,9,5,1,
    2,1,9,2,9,1,4,1,1,1,9,4,1,1,2,1,2,1,9,2,2,2,0,1,2,2,2,2,9,1,2,9
];

/// Rank delta per move byte.
#[rustfmt::skip]
const FORWARD: [i8; 256] = [
    1,-1,9,0,1,1,1,9,0,6,-1,9,1,3,0,-1,9,9,7,1,1,5,9,9,1,9,6,1,9,7,9,9,
    9,0,2,6,1,9,7,1,5,0,-2,9,9,1,1,0,-2,0,5,9,2,9,1,4,4,0,6,5,5,9,9,9,
    9,5,7,9,-1,3,9,9,9,9,2,5,2,1,9,9,6,9,0,9,1,1,9,9,9,9,9,9,1,9,9,2,
    9,6,2,7,9,9,3,1,9,7,4,0,9,9,0,7,9,9,7,9,0,9,9,9,9,6,3,6,1,1,3,0,
    6,1,1,9,9,2,0,5,9,9,-2,1,-1,9,2,0,9,9,1,9,3,9,1,0,0,4,6,2,9,2,9,4,
    3,9,2,1,9,5,9,9,9,0,9,6,9,0,3,9,4,2,6,9,9,0,9,5,9,3,9,1,0,2,0,9,
    9,2,2,2,0,4,5,1,2,7,3,1,5,0,9,1,9,1,1,1,9,9,9,1,0,2,-2,2,9,9,1,1,
    -1,7,9,3,9,0,2,4,2,-1,9,1,1,7,1,0,0,1,9,2,2,1,0,1,0,6,0,2,9,7,3,9
];

/// File delta per move byte.
#[rustfmt::skip]
const LEFT: [i8; 256] = [
    -1,2,9,-2,0,0,1,9,-4,-6,0,9,1,-3,-3,2,9,9,-7,0,-1,-5,9,9,0,9,0,1,9,-7,9,9,
    9,-7,2,-6,1,9,7,1,-5,-6,-1,9,9,-1,-1,-1,1,-3,-5,9,-1,9,-2,0,4,-5,-6,5,5,9,9,9,
    9,-5,7,9,-1,-3,9,9,9,9,0,5,-1,0,9,9,0,9,-6,9,1,0,9,9,9,9,9,9,-1,9,9,0,
    9,-6,0,7,9,9,3,-1,9,0,-4,0,9,9,-5,-7,9,9,7,9,-2,9,9,9,9,6,0,0,-1,0,3,-1,
    6,0,1,9,9,1,-7,0,9,9,-1,-1,1,9,2,-7,9,9,-1,9,0,9,-1,1,-3,0,0,0,9,0,9,4,
    0,9,-2,0,9,0,9,9,9,-2,9,6,9,-4,-3,9,0,0,6,9,9,-5,9,0,9,-3,9,0,-5,0,-1,9,
    9,-2,-2,2,-1,0,0,1,0,0,3,0,5,-2,9,0,9,1,-2,2,9,9,9,1,-6,2,1,0,9,9,1,1,
    -2,0,9,0,9,-4,0,-4,0,-2,9,-1,0,-7,1,-4,-7,-1,9,1,0,-1,0,2,-1,0,-3,-2,9,0,3,9
];

/// One position entry of a book page.
#[derive(Clone, Debug)]
struct CtgEntry {
    /// Canonical position signature.
    signature: Vec<u8>,
    /// `(move byte, annotation byte)` pairs.
    moves: Vec<(u8, u8)>,
    /// Won games.
    wins: u32,
    /// Drawn games.
    draws: u32,
    /// Lost games.
    losses: u32,
    /// Recommendation marker (`64` = avoid, `128` = preferred).
    recommendation: u8,
}

/// A loaded `.ctg` opening book.
#[derive(Default)]
pub struct CtgBook {
    index: HashMap<u64, Vec<CtgEntry>>,
    entry_count: usize,
    book_path: String,
}

impl CtgBook {
    /// Loads a `.ctg` file from disk. Returns `false` on any error.
    pub fn load(&mut self, path: &str) -> bool {
        match std::fs::read(path) {
            Ok(bytes) => {
                let loaded = self.load_bytes(bytes);
                if loaded {
                    self.book_path = path.to_owned();
                }
                loaded
            }
            Err(_) => {
                self.clear();
                false
            }
        }
    }

    /// Loads a book from memory. Returns `false` when no entry was found.
    pub fn load_bytes(&mut self, bytes: Vec<u8>) -> bool {
        self.clear();
        let pages = bytes.len() / PAGE_SIZE;
        let mut index: HashMap<u64, Vec<CtgEntry>> = HashMap::new();
        let mut count = 0;

        for page in 1..pages {
            let page_start = page * PAGE_SIZE;
            let page_end = (page_start + PAGE_SIZE).min(bytes.len());
            if page_start + 4 > page_end {
                break;
            }
            let num_positions =
                (u16::from(bytes[page_start]) << 8) | u16::from(bytes[page_start + 1]);
            // A real position page never holds more than about 117 entries.
            if num_positions == 0 || num_positions > 256 {
                continue;
            }

            let mut position = page_start + 4;
            for _ in 0..num_positions {
                if position + 1 >= page_end {
                    break;
                }
                let entry_size = (bytes[position] & 31) as usize;
                if entry_size < 1 || position + entry_size + 34 + 34 > page_end {
                    break;
                }
                let signature = bytes[position..position + entry_size].to_vec();
                position += entry_size;

                if position >= page_end {
                    break;
                }
                let move_list_size = bytes[position] as usize;
                if move_list_size < 1 || position + move_list_size + 33 > page_end {
                    break;
                }
                let num_moves = (move_list_size - 1) / 2;
                let mut moves = Vec::with_capacity(num_moves);
                for index in 0..num_moves {
                    moves.push((
                        bytes[position + 1 + 2 * index],
                        bytes[position + 2 + 2 * index],
                    ));
                }
                position += move_list_size;

                // Total games are not needed for the weight; skip the field.
                position += 3;
                let losses = read24(&bytes, position);
                position += 3;
                let wins = read24(&bytes, position);
                position += 3;
                let draws = read24(&bytes, position);
                position += 3;
                position += 4; // unknown counter
                position += 3; // average rating games
                position += 4; // average rating score
                position += 3; // performance rating games
                position += 4; // performance rating score
                let recommendation = bytes[position];
                position += 1;
                position += 2; // unknown/comment

                let entry = CtgEntry {
                    signature,
                    moves,
                    wins,
                    draws,
                    losses,
                    recommendation,
                };
                index
                    .entry(fnv1a64(&entry.signature))
                    .or_default()
                    .push(entry);
                count += 1;
            }
        }

        if count == 0 {
            return false;
        }
        self.index = index;
        self.entry_count = count;
        true
    }

    /// Forgets the book.
    pub fn clear(&mut self) {
        self.index.clear();
        self.entry_count = 0;
        self.book_path.clear();
    }

    /// Returns whether a book is loaded.
    pub fn is_loaded(&self) -> bool {
        self.entry_count > 0
    }

    /// Returns the number of indexed position entries.
    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// Returns the loaded file path (empty when none is loaded).
    pub fn book_path(&self) -> &str {
        &self.book_path
    }

    /// Picks a book move for `position`, or [`Move::NONE`] when the position is
    /// not in the book (or none of its moves is legal).
    pub fn pick_book_move(&self, position: &Position, rng: &mut BookRng) -> Move {
        if !self.is_loaded() {
            return Move::NONE;
        }
        let (buffer, length) = signature::compute(position);
        let Some(entry) = self.find_entry(&buffer[..length]) else {
            return Move::NONE;
        };
        if entry.moves.is_empty() {
            return Move::NONE;
        }

        let flip = position.side() == BLACK;
        let mirror = signature::is_mirrored(position);
        let legal_moves = legal(position);

        let mut candidates: Vec<(Move, i64, bool)> = Vec::with_capacity(entry.moves.len());
        let mut have_recommended = false;
        for &(move_byte, annotation) in &entry.moves {
            let Some((from, to)) = decode_move(move_byte, position, flip, mirror) else {
                continue;
            };
            let Some(matched) = legal_moves
                .as_slice()
                .iter()
                .find(|mv| mv.from() == from && mv.to() == to)
                .copied()
            else {
                continue;
            };

            let (mut weight, mut recommended) = self.child_weight(position, matched);
            weight = apply_annotation(weight, annotation, &mut recommended);
            if recommended {
                have_recommended = true;
            }
            if weight > 0 {
                candidates.push((matched, weight, recommended));
            }
        }

        if have_recommended {
            candidates.retain(|candidate| candidate.2);
        }
        if candidates.is_empty() {
            return Move::NONE;
        }

        let mut total = 0_i64;
        for candidate in &candidates {
            total = total.saturating_add(candidate.1);
        }
        if total <= 0 {
            return Move::NONE;
        }
        let mut choice = rng.below(total as u64) as i64;
        for candidate in &candidates {
            choice -= candidate.1;
            if choice < 0 {
                return candidate.0;
            }
        }
        candidates.last().expect("not empty").0
    }

    /// Finds the entry with exactly this signature.
    fn find_entry(&self, signature: &[u8]) -> Option<&CtgEntry> {
        self.index
            .get(&fnv1a64(signature))?
            .iter()
            .find(|entry| entry.signature == signature)
    }

    /// Weight of `mv` from the statistics of the child position.
    ///
    /// Returns `(weight, recommended)`. A child marked "avoid" weighs nothing;
    /// a child marked "preferred" reports `recommended`.
    fn child_weight(&self, position: &Position, mv: Move) -> (i64, bool) {
        let child = position.make_move(mv);
        let (buffer, length) = signature::compute(&child);
        let Some(entry) = self.find_entry(&buffer[..length]) else {
            return (0, false);
        };
        let games = i64::from(entry.wins) + i64::from(entry.draws) + i64::from(entry.losses);
        if games < 1 {
            return (0, false);
        }
        let weight = (2 * i64::from(entry.wins) + i64::from(entry.draws)) * 10_000 / games;
        match entry.recommendation {
            64 => (0, false),
            128 => (weight, true),
            _ => (weight, false),
        }
    }

    /// Returns the move byte that encodes `mv` in `position`, if one exists.
    ///
    /// This is the inverse of [`decode_move`] by search over the 256 format
    /// constants; it exists for book-tooling tests and synthetic fixtures.
    pub fn encode_move_byte(position: &Position, mv: Move) -> Option<u8> {
        let flip = position.side() == BLACK;
        let mirror = signature::is_mirrored(position);
        (0..=255_u8)
            .find(|&byte| decode_move(byte, position, flip, mirror) == Some((mv.from(), mv.to())))
    }
}

/// Decodes one book move byte into a `(from, to)` square pair of the real board.
fn decode_move(byte: u8, position: &Position, flip: bool, mirror: bool) -> Option<(u8, u8)> {
    if byte == 107 {
        // O-O
        let rank = if flip { 7 } else { 0 };
        return Some((rank * 8 + 4, rank * 8 + 6));
    }
    if byte == 246 {
        // O-O-O
        let rank = if flip { 7 } else { 0 };
        return Some((rank * 8 + 4, rank * 8 + 2));
    }

    let glyph = PIECE_CODE
        .as_bytes()
        .get(byte as usize)
        .copied()
        .unwrap_or(b'x');
    if !matches!(glyph, b'P' | b'N' | b'B' | b'R' | b'Q' | b'K') {
        return None;
    }
    let nth = PIECE_INDEX[byte as usize] as usize;
    if nth == 0 || nth == 9 {
        return None;
    }

    let mut from_file = None;
    let mut from_rank = 0_usize;
    let mut count = 0_usize;
    'outer: for file in 0..8_u8 {
        for rank in 0..8_u8 {
            let source_file = if mirror { 7 - file } else { file };
            let source_rank = if flip { 7 - rank } else { rank };
            let mut piece: Piece = position.piece_at(source_rank * 8 + source_file);
            if flip && piece != EMPTY {
                piece = if piece <= 6 { piece + 6 } else { piece - 6 };
            }
            if to_char(piece) as u8 == glyph {
                count += 1;
                if count == nth {
                    from_file = Some(file);
                    from_rank = rank as usize;
                    break 'outer;
                }
            }
        }
    }
    let from_file = from_file?;

    let left = i32::from(LEFT[byte as usize]);
    let forward = i32::from(FORWARD[byte as usize]);
    let to_file = (from_file as i32 - left).rem_euclid(8) as u8;
    let to_rank = (from_rank as i32 + forward).rem_euclid(8) as u8;

    let from = (if flip {
        7 - from_rank as u8
    } else {
        from_rank as u8
    }) * 8
        + if mirror { 7 - from_file } else { from_file };
    let to =
        (if flip { 7 - to_rank } else { to_rank }) * 8 + if mirror { 7 - to_file } else { to_file };
    Some((from, to))
}

/// Applies a book annotation byte to a base weight.
fn apply_annotation(weight: i64, annotation: u8, recommended: &mut bool) -> i64 {
    match annotation {
        0x01 => weight * 8, // Good move.
        0x02 => {
            *recommended = false;
            0
        } // Dubious.
        0x03 => weight * 32, // Very good move.
        0x04 => {
            *recommended = false;
            0
        } // Very dubious.
        0x05 => {
            *recommended = false;
            weight / 2
        } // Interesting.
        0x06 => {
            *recommended = false;
            weight / 8
        } // Dubious but interesting.
        0x08 => i64::MAX,   // Only move.
        _ => weight,
    }
}

/// Reads a 24-bit big-endian counter.
fn read24(bytes: &[u8], position: usize) -> u32 {
    (u32::from(bytes[position]) << 16)
        | (u32::from(bytes[position + 1]) << 8)
        | u32::from(bytes[position + 2])
}

/// FNV-1a hash of a signature.
fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

/// Finds the first usable `.ctg` book next to the executable or in a `books/`
/// subdirectory (also checked relative to the working directory).
pub fn find_default_book_path() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        roots.push(parent.to_path_buf());
    }
    if let Ok(current) = std::env::current_dir() {
        roots.push(current);
    }

    for root in roots {
        for directory in [root.clone(), root.join("books")] {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            let mut candidates: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("ctg"))
                })
                .collect();
            candidates.sort();
            if let Some(path) = candidates.into_iter().next() {
                return Some(path);
            }
        }
    }
    None
}
