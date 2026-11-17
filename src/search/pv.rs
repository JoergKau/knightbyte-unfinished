//! Triangular principal-variation table.
//!
//! The PV of an iteration is a path, not a tree: every ply has at most one
//! continuation. The table exploits that by storing, for each ply, a line whose
//! first move is the ply's best move and whose remainder is the line reported
//! by the next ply. Updating a ply is therefore a copy of at most one line, and
//! reading the root PV is a single slice.
//!
//! The table is heap-allocated: at the maximum depth it is a few tens of
//! kilobytes, which is far too large for the main-thread stack on some
//! platforms.

use crate::types::r#move::Move;

use super::MAX_PLY;

/// A triangular table of principal variations indexed by ply.
pub struct PvTable {
    lines: Box<[[Move; MAX_PLY + 1]; MAX_PLY + 1]>,
    lengths: Box<[u8; MAX_PLY + 1]>,
}

impl Default for PvTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PvTable {
    /// Allocates an empty table.
    pub fn new() -> Self {
        Self {
            lines: Box::new([[Move::NONE; MAX_PLY + 1]; MAX_PLY + 1]),
            lengths: Box::new([0; MAX_PLY + 1]),
        }
    }

    /// Forgets the line starting at `ply`.
    ///
    /// Search calls this before the first child of a node is searched; a line
    /// only becomes longer when a better move is found.
    #[inline]
    pub fn clear(&mut self, ply: usize) {
        if let Some(length) = self.lengths.get_mut(ply) {
            *length = 0;
        }
    }

    /// Prepends `mv` to the line stored for `ply`.
    #[inline]
    pub fn update(&mut self, ply: usize, mv: Move) {
        if ply >= MAX_PLY {
            return;
        }
        let child_length = self.lengths[ply + 1] as usize;
        let (head, tail) = self.lines.split_at_mut(ply + 1);
        let current = &mut head[ply];
        current[0] = mv;
        current[1..=child_length].copy_from_slice(&tail[0][..child_length]);
        self.lengths[ply] = (child_length + 1) as u8;
    }

    /// Returns the line stored for `ply`.
    #[inline]
    pub fn line(&self, ply: usize) -> &[Move] {
        match self.lengths.get(ply) {
            Some(&length) => &self.lines[ply][..length as usize],
            None => &[],
        }
    }

    /// Forgets every line.
    pub fn reset(&mut self) {
        for length in self.lengths.iter_mut() {
            *length = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PvTable;
    use crate::search::MAX_PLY;
    use crate::types::r#move::{Move, flags};

    fn mv(from: u8, to: u8) -> Move {
        Move::new(from, to, flags::NORMAL)
    }

    #[test]
    fn update_builds_a_line_from_the_leaf_upwards() {
        let mut table = PvTable::new();
        // The leaf is updated first, then each parent prepends its own move.
        table.update(2, mv(12, 28));
        table.update(1, mv(6, 21));
        table.update(0, mv(4, 12));
        assert_eq!(table.line(0), [mv(4, 12), mv(6, 21), mv(12, 28)].as_slice());
        assert_eq!(table.line(1), [mv(6, 21), mv(12, 28)].as_slice());
        assert_eq!(table.line(2), [mv(12, 28)].as_slice());
        assert!(table.line(3).is_empty());
        assert!(table.line(3).is_empty());
    }

    #[test]
    fn clear_truncates_only_that_ply() {
        let mut table = PvTable::new();
        table.update(1, mv(1, 2));
        table.update(0, mv(0, 1));
        table.clear(1);
        table.clear(0);
        table.update(0, mv(5, 9));
        assert_eq!(table.line(0), [mv(5, 9)].as_slice());
        assert!(table.line(1).is_empty());
    }

    #[test]
    fn reset_drops_every_line() {
        let mut table = PvTable::new();
        table.update(3, mv(8, 16));
        table.update(2, mv(9, 17));
        table.reset();
        assert!(table.line(3).is_empty());
        assert!(table.line(2).is_empty());
    }

    #[test]
    fn out_of_range_plies_are_ignored() {
        let mut table = PvTable::new();
        table.update(MAX_PLY, mv(1, 2));
        table.clear(MAX_PLY + 5);
        assert!(table.line(MAX_PLY).is_empty());
        assert!(table.line(MAX_PLY + 5).is_empty());
    }

    #[test]
    fn deepest_line_fits_the_array() {
        let mut table = PvTable::new();
        for ply in (0..MAX_PLY).rev() {
            table.update(ply, mv((ply % 64) as u8, ((ply + 1) % 64) as u8));
        }
        assert_eq!(table.line(0).len(), MAX_PLY);
        assert_eq!(table.line(0)[0], mv(0, 1));
        assert_eq!(
            table.line(0)[MAX_PLY - 1],
            mv((MAX_PLY - 1) as u8 % 64, (MAX_PLY % 64) as u8)
        );
    }
}
