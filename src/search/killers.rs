//! Killer-move memory.
//!
//! A killer is a quiet move that caused a beta cutoff at the same ply in an
//! earlier branch. Quiet moves rarely transpose, so retrying the most recent
//! killers at a ply is a cheap and reliable ordering hint.
//!
//! Four killers are kept per ply, most recent first, matching the reference's
//! table width. The table is small enough to keep inline in the search state.

use super::MAX_PLY;
use crate::types::r#move::Move;

/// Number of killers remembered per ply.
pub const KILLER_SLOTS: usize = 4;

/// Four killer moves per ply.
pub struct Killers {
    slots: [[Move; KILLER_SLOTS]; MAX_PLY],
}

impl Default for Killers {
    fn default() -> Self {
        Self::new()
    }
}

impl Killers {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self {
            slots: [[Move::NONE; KILLER_SLOTS]; MAX_PLY],
        }
    }

    /// Forgets every killer.
    pub fn clear(&mut self) {
        self.slots = [[Move::NONE; KILLER_SLOTS]; MAX_PLY];
    }

    /// Returns the killers for `ply`.
    ///
    /// Out-of-range plies return no-moves instead of panicking, because a search
    /// that overruns its depth budget must not take the engine down.
    #[inline]
    pub fn get(&self, ply: usize) -> [Move; KILLER_SLOTS] {
        self.slots
            .get(ply)
            .copied()
            .unwrap_or([Move::NONE; KILLER_SLOTS])
    }

    /// Records `mv` as the most recent killer for `ply`.
    ///
    /// Re-recording the current first killer does not shift the others, so a
    /// single tactical move cannot fill the whole table.
    #[inline]
    pub fn record(&mut self, ply: usize, mv: Move) {
        let Some(slots) = self.slots.get_mut(ply) else {
            return;
        };
        if mv.is_none() || slots[0] == mv {
            return;
        }
        let mut index = KILLER_SLOTS - 1;
        while index > 0 {
            slots[index] = slots[index - 1];
            index -= 1;
        }
        slots[0] = mv;
    }

    /// Returns whether `mv` is one of the killers for `ply`.
    #[inline]
    pub fn contains(&self, ply: usize, mv: Move) -> bool {
        if mv.is_none() {
            return false;
        }
        self.get(ply).contains(&mv)
    }

    /// Returns whether `mv` is the first (most recent) killer for `ply`.
    #[inline]
    pub fn is_first(&self, ply: usize, mv: Move) -> bool {
        self.get(ply)[0] == mv && !mv.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::{KILLER_SLOTS, Killers};
    use crate::search::MAX_PLY;
    use crate::types::r#move::{Move, flags};

    fn mv(from: u8, to: u8) -> Move {
        Move::new(from, to, flags::NORMAL)
    }

    #[test]
    fn newest_killer_shifts_the_others() {
        let mut killers = Killers::new();
        killers.record(3, mv(0, 1));
        killers.record(3, mv(2, 3));
        assert_eq!(killers.get(3)[0], mv(2, 3));
        assert_eq!(killers.get(3)[1], mv(0, 1));
        assert!(killers.contains(3, mv(0, 1)));
        assert!(killers.is_first(3, mv(2, 3)));
        assert!(!killers.is_first(3, mv(0, 1)));
    }

    #[test]
    fn repeated_killer_does_not_shift_the_table() {
        let mut killers = Killers::new();
        killers.record(0, mv(4, 5));
        killers.record(0, mv(4, 5));
        assert_eq!(killers.get(0)[0], mv(4, 5));
        assert!(killers.get(0)[1..].iter().all(|mv| mv.is_none()));
    }

    #[test]
    fn table_holds_four_moves_and_drops_the_oldest() {
        let mut killers = Killers::new();
        for index in 0..KILLER_SLOTS as u8 {
            killers.record(1, mv(index, index + 10));
        }
        let slots = killers.get(1);
        assert_eq!(slots[0], mv(3, 13));
        assert_eq!(slots[KILLER_SLOTS - 1], mv(0, 10));
        killers.record(1, mv(7, 17));
        assert_eq!(killers.get(1)[0], mv(7, 17));
        assert!(!killers.contains(1, mv(0, 10)), "the oldest killer is gone");
    }

    #[test]
    fn plies_are_independent() {
        let mut killers = Killers::new();
        killers.record(1, mv(1, 2));
        assert!(killers.contains(1, mv(1, 2)));
        assert!(!killers.contains(2, mv(1, 2)));
    }

    #[test]
    fn clear_forgets_everything() {
        let mut killers = Killers::new();
        killers.record(5, mv(6, 7));
        killers.clear();
        assert!(killers.get(5).iter().all(|mv| mv.is_none()));
        assert!(!killers.contains(5, mv(6, 7)));
    }

    #[test]
    fn out_of_range_plies_are_ignored() {
        let mut killers = Killers::new();
        killers.record(MAX_PLY, mv(1, 2));
        killers.record(MAX_PLY + 9, mv(1, 2));
        assert!(killers.get(MAX_PLY).iter().all(|mv| mv.is_none()));
        assert!(killers.get(MAX_PLY + 9).iter().all(|mv| mv.is_none()));
        assert!(!killers.contains(MAX_PLY, mv(1, 2)));
    }

    #[test]
    fn no_move_is_never_a_killer() {
        let mut killers = Killers::new();
        killers.record(2, Move::NONE);
        assert!(!killers.contains(2, Move::NONE));
        assert!(killers.get(2).iter().all(|mv| mv.is_none()));
    }
}
