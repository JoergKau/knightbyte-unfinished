//! Table discovery, lazy mapping and the index-table hash.
//!
//! A `.rtbw`/`.rtbz` file is mapped only when a position first needs it: the
//! engine enumerates all material combinations it can support by name at load
//! time (a stat test, no I/O beyond `metadata`), and the actual mapping plus
//! header parsing happens on the first probe of that material. The mapped
//! table's decompression tables are kept for the process lifetime and released
//! by a reload or a `SyzygyPath` reset.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::index::{EncInfo, EncShape, FILE_ENC, PIECE_ENC, init_enc_info, leading_pawn};
use super::pairs::{Bytes, DTZ, WDL, setup_pairs};
use super::pos::{
    B_BISHOP, B_KNIGHT, B_PAWN, B_QUEEN, B_ROOK, PIECE_TO_CHAR, Pos, TB_PIECES, TB_WHITE, W_BISHOP,
    W_KNIGHT, W_PAWN, W_QUEEN, W_ROOK,
};

/// Number of slots in the material hash (a power of two).
const TB_HASHBITS: usize = 12;
/// Number of material hash slots.
const TB_HASH_SIZE: usize = 1 << TB_HASHBITS;

/// File suffixes of the three table kinds (WDL, DTM, DTZ).
const TB_SUFFIX: [&str; 3] = [".rtbw", ".rtbm", ".rtbz"];
/// File magic numbers of the three table kinds.
const TB_MAGIC: [u32; 3] = [0x5d23e871, 0x88ac504b, 0xa50c66d7];

/// Magic numbers for the two kinds the engine reads.
const MAGIC_WDL: u32 = TB_MAGIC[0];
/// DTZ file magic number.
const MAGIC_DTZ: u32 = TB_MAGIC[2];

/// Material key primes per tablebase piece code.
#[rustfmt::skip]
const PRIMES: [u64; 15] = [
    0,
    17008651141875982339, // White pawn
    15202887380319082783, // White knight
    12311744257139811149, // White bishop
    10979190538029446137, // White rook
    11811845319353239651, // White queen
    0, 0, 0,
    11695583624105689831, // Black pawn
    13469005675588064321, // Black knight
    15394650811035483107, // Black bishop
    18264461213049635989, // Black rook
    15484752644942473553, // Black queen
    0,
];

/// One material combination ("tablebase") with its lazily mapped files.
pub(crate) struct TableEntry {
    /// Material key.
    pub(crate) key: u64,
    /// Number of pieces.
    pub(crate) num: usize,
    /// Whether the two colours' keys are identical.
    pub(crate) symmetric: bool,
    /// Whether the material contains pawns.
    pub(crate) has_pawns: bool,
    /// Whether the king-only two-king encoding applies.
    pub(crate) kk_enc: bool,
    /// Pawns of the leading colour.
    pub(crate) pawns0: usize,
    /// Pawns of the other colour.
    pub(crate) pawns1: usize,
    /// Whether a DTZ file exists for this material.
    pub(crate) has_dtz: bool,
    /// Mapped WDL table.
    pub(crate) wdl: Option<Loaded>,
    /// Mapped DTZ table.
    pub(crate) dtz: Option<Loaded>,
}

/// One mapped file with its parsed header.
pub(crate) struct Loaded {
    /// The mapped bytes.
    pub(crate) file: Bytes,
    /// Encoding information per side and table part.
    pub(crate) ei: Vec<EncInfo>,
    /// Offset of the DTZ value map, relative to the file.
    pub(crate) dtz_map_off: u64,
    /// Map offsets per WDL slot (byte offsets).
    pub(crate) dtz_map_idx: [u16; 16],
    /// Header flags per table part.
    pub(crate) dtz_flags: [u8; 4],
}

/// One slot of the material hash.
#[derive(Clone, Copy)]
struct HashSlot {
    /// Material key.
    key: u64,
    /// Whether the entry is a pawn table.
    pawn: bool,
    /// Index into the piece or pawn entry vector.
    index: usize,
    /// One bit per table kind (`1 << WDL`, `1 << DTZ`) whose file failed to
    /// load. The flag is per kind on purpose: a present but corrupt DTZ file
    /// must only fail DTZ probes, never switch off the healthy WDL table of the
    /// same material (a shared flag did exactly that and silently dropped the
    /// exact tablebase scores).
    error_mask: u8,
}

/// The result of one table read: value plus the reference success code.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TableValue {
    /// Probed value (`-2..=2` for WDL, a DTZ number otherwise).
    pub(crate) value: i32,
    /// `1` = ok (or WDL winning-capture code `2`), `0` = failed, `-1` = wrong side.
    pub(crate) success: i32,
}

impl TableValue {
    /// A failed read.
    pub(crate) const FAILED: Self = Self {
        value: 0,
        success: 0,
    };

    /// A successful read.
    pub(crate) const fn ok(value: i32) -> Self {
        Self { value, success: 1 }
    }
}

/// All loaded tables plus the material hash.
pub(crate) struct Inner {
    /// Directories searched for table files.
    paths: Vec<PathBuf>,
    /// Pawnless table entries.
    piece_entries: Vec<TableEntry>,
    /// Pawn table entries.
    pawn_entries: Vec<TableEntry>,
    /// Open-addressed material hash.
    hash: Vec<Option<HashSlot>>,
    /// Number of WDL files discovered.
    pub(crate) wdl_count: usize,
    /// Largest material combination covered.
    pub(crate) max_pieces: usize,
}

impl Inner {
    /// Opens the tables found under `path`.
    ///
    /// The path list separator follows the platform convention: `;` on
    /// Windows, `:` elsewhere. An empty path or the literal `<empty>` clears
    /// the tables.
    pub(crate) fn open(path: &str) -> Self {
        let mut inner = Self {
            paths: Vec::new(),
            piece_entries: Vec::new(),
            pawn_entries: Vec::new(),
            hash: vec![None; TB_HASH_SIZE],
            wdl_count: 0,
            max_pieces: 0,
        };

        if path.is_empty() || path == "<empty>" {
            return inner;
        }

        let separator = if cfg!(windows) { ';' } else { ':' };
        inner.paths = path
            .split(separator)
            .filter(|part| !part.is_empty())
            .map(PathBuf::from)
            .collect();

        let mut names = HashSet::new();
        inner.enumerate_names(&mut names);
        for name in names {
            inner.init_tb(&name);
        }
        inner
    }

    /// Enumerates every material name the engine can probe.
    fn enumerate_names(&self, names: &mut HashSet<String>) {
        let piece = |index: usize| PIECE_TO_CHAR[5 - index] as char;
        let mut add = |text: String| {
            names.insert(text);
        };

        // King and one piece versus king.
        for i in 0..5 {
            add(format!("K{}vK", piece(i)));
        }
        // One piece versus one piece.
        for i in 0..5 {
            for j in i..5 {
                add(format!("K{}vK{}", piece(i), piece(j)));
            }
        }
        // Two pieces versus king, and two versus one.
        for i in 0..5 {
            for j in i..5 {
                add(format!("K{}{}vK", piece(i), piece(j)));
                for k in 0..5 {
                    add(format!("K{}{}vK{}", piece(i), piece(j), piece(k)));
                }
            }
        }
        // Three pieces versus king, and three versus one.
        for i in 0..5 {
            for j in i..5 {
                for k in j..5 {
                    add(format!("K{}{}{}vK", piece(i), piece(j), piece(k)));
                    for l in 0..5 {
                        add(format!(
                            "K{}{}{}vK{}",
                            piece(i),
                            piece(j),
                            piece(k),
                            piece(l)
                        ));
                    }
                }
            }
        }
        // Two pieces each side (six pieces). The second group starts at the
        // first group's first piece, and at its second piece when the first
        // pieces are equal, so a mirrored material is listed only once.
        for i in 0..5 {
            for j in i..5 {
                for k in i..5 {
                    let start = if i == k { j } else { k };
                    for l in start..5 {
                        add(format!(
                            "K{}{}vK{}{}",
                            piece(i),
                            piece(j),
                            piece(k),
                            piece(l)
                        ));
                    }
                }
            }
        }
        // Four pieces versus king.
        for i in 0..5 {
            for j in i..5 {
                for k in j..5 {
                    for l in k..5 {
                        add(format!(
                            "K{}{}{}{}vK",
                            piece(i),
                            piece(j),
                            piece(k),
                            piece(l)
                        ));
                    }
                }
            }
        }
        // Seven-piece names (always enabled on 64-bit hosts).
        if usize::BITS == 64 {
            for i in 0..5 {
                for j in i..5 {
                    for k in j..5 {
                        for l in k..5 {
                            for m in l..5 {
                                add(format!(
                                    "K{}{}{}{}{}vK",
                                    piece(i),
                                    piece(j),
                                    piece(k),
                                    piece(l),
                                    piece(m)
                                ));
                            }
                        }
                    }
                }
            }
            for i in 0..5 {
                for j in i..5 {
                    for k in j..5 {
                        for l in k..5 {
                            for m in 0..5 {
                                add(format!(
                                    "K{}{}{}{}vK{}",
                                    piece(i),
                                    piece(j),
                                    piece(k),
                                    piece(l),
                                    piece(m)
                                ));
                            }
                        }
                    }
                }
            }
            for i in 0..5 {
                for j in i..5 {
                    for k in j..5 {
                        for l in 0..5 {
                            for m in l..5 {
                                add(format!(
                                    "K{}{}{}vK{}{}",
                                    piece(i),
                                    piece(j),
                                    piece(k),
                                    piece(l),
                                    piece(m)
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Registers one material name if its WDL file is present.
    fn init_tb(&mut self, name: &str) {
        if !self.test_tb(name, TB_SUFFIX[0]) {
            return;
        }

        let mut pcs = [0_u8; 16];
        let mut color = 0_usize;
        for character in name.chars() {
            if character == 'v' {
                color = 8;
            } else if let Some(piece_type) = char_to_piece_type(character) {
                pcs[piece_type | color] += 1;
            }
        }

        let key = calc_key_from_pcs(&pcs, false);
        let key2 = calc_key_from_pcs(&pcs, true);
        let has_pawns = pcs[W_PAWN] != 0 || pcs[B_PAWN] != 0;
        let num: usize = pcs.iter().map(|&count| count as usize).sum();

        let mut entry = TableEntry {
            key,
            num,
            symmetric: key == key2,
            has_pawns,
            kk_enc: false,
            pawns0: 0,
            pawns1: 0,
            has_dtz: self.test_tb(name, TB_SUFFIX[2]),
            wdl: None,
            dtz: None,
        };
        if has_pawns {
            let mut first = pcs[W_PAWN];
            let mut second = pcs[B_PAWN];
            if second != 0 && (first == 0 || first > second) {
                std::mem::swap(&mut first, &mut second);
            }
            entry.pawns0 = first as usize;
            entry.pawns1 = second as usize;
        } else {
            entry.kk_enc = pcs.iter().filter(|&&count| count == 1).count() == 2;
        }

        self.max_pieces = self.max_pieces.max(num);
        self.wdl_count += 1;

        let index = if has_pawns {
            self.pawn_entries.push(entry);
            self.pawn_entries.len() - 1
        } else {
            self.piece_entries.push(entry);
            self.piece_entries.len() - 1
        };
        self.add_to_hash(key, has_pawns, index);
        if key != key2 {
            self.add_to_hash(key2, has_pawns, index);
        }
    }

    /// Returns whether a readable file with the format size signature exists.
    fn test_tb(&self, name: &str, suffix: &str) -> bool {
        for directory in &self.paths {
            let path = directory.join(format!("{name}{suffix}"));
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.len() & 63 == 16 => return true,
                _ => {}
            }
        }
        false
    }

    /// Adds one key to the material hash.
    fn add_to_hash(&mut self, key: u64, pawn: bool, index: usize) {
        let mut slot = (key >> (64 - TB_HASHBITS)) as usize;
        let mut probed = 0;
        while self.hash[slot].is_some() && probed < TB_HASH_SIZE {
            slot = (slot + 1) & (TB_HASH_SIZE - 1);
            probed += 1;
        }
        if probed >= TB_HASH_SIZE {
            slot = (key >> (64 - TB_HASHBITS)) as usize;
        }
        self.hash[slot] = Some(HashSlot {
            key,
            pawn,
            index,
            error_mask: 0,
        });
    }

    /// Resolves the file path of `name` with `suffix` under the search paths.
    fn resolve(&self, name: &str, suffix: &str) -> Option<PathBuf> {
        self.paths
            .iter()
            .map(|directory| directory.join(format!("{name}{suffix}")))
            .find(|path| path.is_file())
    }

    /// Probes one value of `kind` for `pos`.
    ///
    /// `s` is the WDL class used to select the DTZ value map (ignored for WDL).
    pub(crate) fn probe_table(&mut self, pos: &Pos, s: i32, kind: usize) -> TableValue {
        let key = calc_key(pos, false);

        // The bare kings are always a draw and have no file.
        if kind == WDL && key == 0 {
            return TableValue::ok(0);
        }

        let mut slot_index = (key >> (64 - TB_HASHBITS)) as usize;
        let mut probed = 0;
        loop {
            match self.hash[slot_index] {
                Some(slot) if slot.key == key => break,
                Some(_) if probed < TB_HASH_SIZE => {
                    slot_index = (slot_index + 1) & (TB_HASH_SIZE - 1);
                    probed += 1;
                }
                _ => return TableValue::FAILED,
            }
        }
        let slot = self.hash[slot_index].expect("checked above");
        if slot.error_mask & (1 << kind) != 0 {
            return TableValue::FAILED;
        }

        let entry_index = slot.index;
        let has_pawns = slot.pawn;

        // Material without the needed file cannot answer this probe.
        {
            let entry = if has_pawns {
                &self.pawn_entries[entry_index]
            } else {
                &self.piece_entries[entry_index]
            };
            if kind == DTZ && !entry.has_dtz {
                return TableValue::FAILED;
            }
            let ready = if kind == WDL {
                entry.wdl.is_some()
            } else {
                entry.dtz.is_some()
            };
            if ready {
                return probe_loaded(entry, key, pos, s, kind);
            }
        }

        // Lazy first use: take the entry out so the path list can be borrowed
        // alongside it, map and parse the file, then put it back.
        let name = prt_str(pos, {
            let entry = if has_pawns {
                &self.pawn_entries[entry_index]
            } else {
                &self.piece_entries[entry_index]
            };
            // The canonical file name is produced from the position itself; a
            // flipped key means the mirrored side owns the material.
            entry.key != key
        });
        let suffix = TB_SUFFIX[if kind == WDL { 0 } else { 2 }];
        let Some(path) = self.resolve(&name, suffix) else {
            self.hash[slot_index]
                .as_mut()
                .expect("slot exists")
                .error_mask |= 1 << kind;
            return TableValue::FAILED;
        };
        let mut entry = if has_pawns {
            std::mem::replace(&mut self.pawn_entries[entry_index], empty_entry())
        } else {
            std::mem::replace(&mut self.piece_entries[entry_index], empty_entry())
        };
        let loaded = load_file(&path, &entry, kind);
        let (ready, error) = match loaded {
            Some(loaded) => {
                if kind == WDL {
                    entry.wdl = Some(loaded);
                } else {
                    entry.dtz = Some(loaded);
                }
                (true, false)
            }
            None => (false, true),
        };
        if has_pawns {
            self.pawn_entries[entry_index] = entry;
        } else {
            self.piece_entries[entry_index] = entry;
        }
        if error {
            self.hash[slot_index]
                .as_mut()
                .expect("slot exists")
                .error_mask |= 1 << kind;
            return TableValue::FAILED;
        }
        if !ready {
            return TableValue::FAILED;
        }
        let entry = if has_pawns {
            &self.pawn_entries[entry_index]
        } else {
            &self.piece_entries[entry_index]
        };
        probe_loaded(entry, key, pos, s, kind)
    }
}

/// An empty entry used only while a real entry is temporarily taken out.
fn empty_entry() -> TableEntry {
    TableEntry {
        key: 0,
        num: 0,
        symmetric: false,
        has_pawns: false,
        kk_enc: false,
        pawns0: 0,
        pawns1: 0,
        has_dtz: false,
        wdl: None,
        dtz: None,
    }
}

/// Reads the file header of one table kind.
fn load_file(path: &Path, entry: &TableEntry, kind: usize) -> Option<Loaded> {
    let file = Bytes::open(path).ok()?;
    let magic = file.u32(0)?;
    let expected = if kind == WDL { MAGIC_WDL } else { MAGIC_DTZ };
    if magic != expected {
        return None;
    }

    let mut loaded = Loaded {
        file: file.clone(),
        ei: vec![EncInfo::default(); 24],
        dtz_map_off: 0,
        dtz_map_idx: [0; 16],
        dtz_flags: [0; 4],
    };

    let split = kind != DTZ && (file.u8(4)? & 0x01) != 0;
    let data_start = 5_u64;
    let mut data_off = data_start;
    let num = num_tables(entry.has_pawns);
    let ei_base = first_ei_index(entry.has_pawns, kind);
    let enc = if !entry.has_pawns {
        PIECE_ENC
    } else {
        // Pawn tables (both WDL and DTZ) are split by pawn file. The rank
        // encoding exists only for the unused DTM tables.
        FILE_ENC
    };
    let shape = EncShape {
        num: entry.num,
        kk_enc: entry.kk_enc,
        pawns0: entry.pawns0,
        pawns1: entry.pawns1,
    };

    let mut tb_size = [[0_u64; 2]; 6];
    for t in 0..num {
        let (first, ei) = init_enc_info(&file, data_off, 0, t, enc, &shape)?;
        tb_size[t][0] = first;
        loaded.ei[ei_base + t] = ei;
        if split {
            let (second, ei) = init_enc_info(&file, data_off, 4, t, enc, &shape)?;
            tb_size[t][1] = second;
            loaded.ei[ei_base + num + t] = ei;
        }
        data_off += (entry.num + 1 + usize::from(entry.has_pawns && entry.pawns1 != 0)) as u64;
    }
    data_off += data_off & 1;

    let mut sizes = [[[0_u64; 3]; 2]; 6];
    for t in 0..num {
        let (pairs, size, flags) = setup_pairs(&file, &mut data_off, tb_size[t][0], kind)?;
        loaded.ei[ei_base + t].precomp = Some(pairs);
        sizes[t][0] = size;
        if kind == DTZ {
            if !entry.has_pawns {
                loaded.dtz_flags[0] = flags;
            } else {
                loaded.dtz_flags[t] = flags;
            }
        }
        if split {
            let (pairs, size, _) = setup_pairs(&file, &mut data_off, tb_size[t][1], kind)?;
            loaded.ei[ei_base + num + t].precomp = Some(pairs);
            sizes[t][1] = size;
        } else if kind != DTZ {
            loaded.ei[ei_base + num + t].precomp = None;
        }
    }

    if kind == DTZ {
        loaded.dtz_map_off = data_off;
        for t in 0..num {
            let flags = if entry.has_pawns {
                loaded.dtz_flags[t]
            } else {
                loaded.dtz_flags[0]
            };
            if flags & 2 != 0 {
                if flags & 16 == 0 {
                    for i in 0..4 {
                        let base = if entry.has_pawns { t * 4 } else { 0 };
                        loaded.dtz_map_idx[base + i] = (data_off + 1 - loaded.dtz_map_off) as u16;
                        data_off += 1 + u64::from(file.u8(data_off)?);
                    }
                } else {
                    data_off += data_off & 1;
                    for i in 0..4 {
                        let base = if entry.has_pawns { t * 4 } else { 0 };
                        loaded.dtz_map_idx[base + i] = (data_off - loaded.dtz_map_off) as u16;
                        data_off += 2 + 2 * u64::from(file.u16(data_off)?);
                    }
                }
            }
        }
        data_off += data_off & 1;
    }

    for t in 0..num {
        let pairs = loaded.ei[ei_base + t].precomp.as_mut()?;
        pairs.index_table = data_off;
        data_off += sizes[t][0][0];
        if split {
            let pairs = loaded.ei[ei_base + num + t].precomp.as_mut()?;
            pairs.index_table = data_off;
            data_off += sizes[t][1][0];
        }
    }
    for t in 0..num {
        let pairs = loaded.ei[ei_base + t].precomp.as_mut()?;
        pairs.size_table = data_off;
        data_off += sizes[t][0][1];
        if split {
            let pairs = loaded.ei[ei_base + num + t].precomp.as_mut()?;
            pairs.size_table = data_off;
            data_off += sizes[t][1][1];
        }
    }
    for t in 0..num {
        data_off = (data_off + 0x3f) & !0x3f_u64;
        let pairs = loaded.ei[ei_base + t].precomp.as_mut()?;
        pairs.data = data_off;
        data_off += sizes[t][0][2];
        if split {
            data_off = (data_off + 0x3f) & !0x3f_u64;
            let pairs = loaded.ei[ei_base + num + t].precomp.as_mut()?;
            pairs.data = data_off;
            data_off += sizes[t][1][2];
        }
    }
    if data_off > file.len() as u64 {
        return None;
    }

    Some(loaded)
}

/// Returns the number of table parts of one kind.
fn num_tables(has_pawns: bool) -> usize {
    if has_pawns { 4 } else { 1 }
}

/// Returns the first encoding-info slot for one kind.
fn first_ei_index(has_pawns: bool, kind: usize) -> usize {
    if has_pawns {
        if kind == WDL { 0 } else { 20 }
    } else if kind == WDL {
        0
    } else {
        4
    }
}

/// Probes an already-mapped table.
fn probe_loaded(entry: &TableEntry, key: u64, pos: &Pos, s: i32, kind: usize) -> TableValue {
    let loaded = if kind == WDL {
        entry.wdl.as_ref().expect("checked by the caller")
    } else {
        entry.dtz.as_ref().expect("checked by the caller")
    };

    let (flip, bside) = if !entry.symmetric {
        let flip = key != entry.key;
        (flip, pos.turn == flip)
    } else {
        (!pos.turn, false)
    };

    let ei_base = first_ei_index(entry.has_pawns, kind);
    let mut p = [0_i32; TB_PIECES];
    let mut flags = 0_u8;

    if !entry.has_pawns {
        if kind == DTZ {
            flags = loaded.dtz_flags[0];
            if ((flags & 1 != 0) != bside) && !entry.symmetric {
                return TableValue {
                    value: 0,
                    success: -1,
                };
            }
        }
        let ei = &loaded.ei[ei_base + if kind == DTZ { 0 } else { usize::from(bside) }];
        let mut i = 0;
        while i < entry.num {
            i = fill_squares(pos, &ei.pieces, flip, 0, &mut p, i);
        }
        let index = super::index::encode(&mut p, ei, &entry_shape(entry), PIECE_ENC);
        let Some(pairs) = ei.precomp.as_ref() else {
            return TableValue::FAILED;
        };
        let Some((w0, w1)) = super::pairs::decompress_pairs(pairs, index) else {
            return TableValue::FAILED;
        };

        if kind == WDL {
            return TableValue::ok(i32::from(w0) - 2);
        }
        let mut value = i32::from(w0) + (i32::from(w1 & 0x0f) << 8);
        if flags & 2 != 0 {
            let m = super::index::WDL_TO_MAP[(s + 2) as usize];
            let map_index = loaded.dtz_map_idx[m];
            if flags & 16 == 0 {
                let Some(byte) = loaded
                    .file
                    .u8(loaded.dtz_map_off + u64::from(map_index) + value as u64)
                else {
                    return TableValue::FAILED;
                };
                value = i32::from(byte);
            } else {
                let Some(word) = loaded
                    .file
                    .u16(loaded.dtz_map_off + u64::from(map_index) + 2 + 2 * value as u64)
                else {
                    return TableValue::FAILED;
                };
                value = i32::from(word);
            }
        }
        if (flags & super::index::PA_FLAGS[(s + 2) as usize] == 0) || (s & 1) != 0 {
            value *= 2;
        }
        return TableValue::ok(value);
    }

    // Pawn tables.
    let mut i = fill_squares(
        pos,
        &loaded.ei[ei_base].pieces,
        flip,
        if flip { 0x38 } else { 0 },
        &mut p,
        0,
    );
    let shape = entry_shape(entry);
    let t = leading_pawn(&mut p, entry.pawns0, FILE_ENC);
    if kind == DTZ {
        flags = loaded.dtz_flags[t];
        if ((flags & 1 != 0) != bside) && !entry.symmetric {
            return TableValue {
                value: 0,
                success: -1,
            };
        }
    }
    let ei = &loaded.ei[if kind == WDL {
        ei_base + t + 4 * usize::from(bside)
    } else {
        ei_base + t
    }];
    while i < entry.num {
        i = fill_squares(
            pos,
            &ei.pieces,
            flip,
            if flip { 0x38 } else { 0 },
            &mut p,
            i,
        );
    }
    let index = super::index::encode(&mut p, ei, &shape, FILE_ENC);
    let Some(pairs) = ei.precomp.as_ref() else {
        return TableValue::FAILED;
    };
    let Some((w0, w1)) = super::pairs::decompress_pairs(pairs, index) else {
        return TableValue::FAILED;
    };
    if kind == WDL {
        return TableValue::ok(i32::from(w0) - 2);
    }

    let mut value = i32::from(w0) + (i32::from(w1 & 0x0f) << 8);
    if flags & 2 != 0 {
        let m = super::index::WDL_TO_MAP[(s + 2) as usize];
        let map_index = loaded.dtz_map_idx[t * 4 + m];
        if flags & 16 == 0 {
            let Some(byte) = loaded
                .file
                .u8(loaded.dtz_map_off + u64::from(map_index) + value as u64)
            else {
                return TableValue::FAILED;
            };
            value = i32::from(byte);
        } else {
            let Some(word) = loaded
                .file
                .u16(loaded.dtz_map_off + u64::from(map_index) + 2 + 2 * value as u64)
            else {
                return TableValue::FAILED;
            };
            value = i32::from(word);
        }
    }
    if (flags & super::index::PA_FLAGS[(s + 2) as usize] == 0) || (s & 1) != 0 {
        value *= 2;
    }
    TableValue::ok(value)
}

/// Returns the shape descriptor of an entry.
fn entry_shape(entry: &TableEntry) -> EncShape {
    EncShape {
        num: entry.num,
        kk_enc: entry.kk_enc,
        pawns0: entry.pawns0,
        pawns1: entry.pawns1,
    }
}

/// Fills `p` with the squares of one piece group.
fn fill_squares(
    pos: &Pos,
    pieces: &[u8; TB_PIECES],
    flip: bool,
    mirror: usize,
    p: &mut [i32; TB_PIECES],
    mut i: usize,
) -> usize {
    let mut color = super::pos::color_of_piece(pieces[i] as usize);
    if flip {
        color = if color == TB_WHITE { 0 } else { 1 };
    }
    let mut board = pos.pieces_by_type(color, super::pos::type_of_piece(pieces[i] as usize) as i32);
    while board != 0 {
        let square = board.trailing_zeros() as usize;
        p[i] = (square ^ mirror) as i32;
        i += 1;
        board &= board - 1;
    }
    i
}

/// Builds the canonical material name of `pos` (optionally with sides swapped).
fn prt_str(pos: &Pos, flip: bool) -> String {
    let mut text = String::with_capacity(16);
    let mut color = if flip { 0 } else { TB_WHITE };
    for piece_type in (1..=6).rev() {
        let count = pos.pieces_by_type(color, piece_type).count_ones();
        for _ in 0..count {
            text.push(PIECE_TO_CHAR[piece_type as usize] as char);
        }
    }
    text.push('v');
    color ^= 1;
    for piece_type in (1..=6).rev() {
        let count = pos.pieces_by_type(color, piece_type).count_ones();
        for _ in 0..count {
            text.push(PIECE_TO_CHAR[piece_type as usize] as char);
        }
    }
    text
}

/// Maps a material name character to its tablebase piece type.
fn char_to_piece_type(character: char) -> Option<usize> {
    match character {
        'P' => Some(1),
        'N' => Some(2),
        'B' => Some(3),
        'R' => Some(4),
        'Q' => Some(5),
        'K' => Some(6),
        _ => None,
    }
}

/// Computes the material key of a position.
pub(crate) fn calc_key(pos: &Pos, mirror: bool) -> u64 {
    let (white, black) = if mirror {
        (pos.black, pos.white)
    } else {
        (pos.white, pos.black)
    };
    let mut key = 0_u64;
    let groups = [
        (white & pos.queens, W_QUEEN),
        (white & pos.rooks, W_ROOK),
        (white & pos.bishops, W_BISHOP),
        (white & pos.knights, W_KNIGHT),
        (white & pos.pawns, W_PAWN),
        (black & pos.queens, B_QUEEN),
        (black & pos.rooks, B_ROOK),
        (black & pos.bishops, B_BISHOP),
        (black & pos.knights, B_KNIGHT),
        (black & pos.pawns, B_PAWN),
    ];
    for (board, piece) in groups {
        key = key.wrapping_add((board.count_ones() as u64).wrapping_mul(PRIMES[piece]));
    }
    key
}

/// Computes a material key from a piece-count table.
fn calc_key_from_pcs(pcs: &[u8; 16], mirror: bool) -> u64 {
    let m = if mirror { 8 } else { 0 };
    let mut key = 0_u64;
    for (white, black) in [
        (W_QUEEN, B_QUEEN),
        (W_ROOK, B_ROOK),
        (W_BISHOP, B_BISHOP),
        (W_KNIGHT, B_KNIGHT),
        (W_PAWN, B_PAWN),
    ] {
        key = key.wrapping_add((pcs[(white ^ m) & 15] as u64).wrapping_mul(PRIMES[white]));
        key = key.wrapping_add((pcs[(black ^ m) & 15] as u64).wrapping_mul(PRIMES[black]));
    }
    key
}

#[cfg(test)]
mod tests {
    use super::Inner;

    /// Every material family the format defines must be registered when its
    /// file is present: a family missing from the name scan silently turns
    /// every position of that material into "not covered".
    #[test]
    fn the_name_scan_registers_every_material_family() {
        // Canonical names: strongest piece first, groups ordered Q R B N P.
        let names = [
            "KQvK",     // 3 pieces
            "KQvKR",    // 4 pieces, one each
            "KQRvK",    // 4 pieces, two on one side
            "KQRvKB",   // 5 pieces, 3 v 2
            "KQRBvK",   // 5 pieces, 4 v 1
            "KQNvKNP",  // 6 pieces, 3 v 3 with different first pieces
            "KQNvKQN",  // 6 pieces, 3 v 3 with equal first pieces
            "KQRvKQN",  // 6 pieces, 3 v 3 sharing the first piece
            "KQRBvKN",  // 6 pieces, 4 v 2
            "KQRBNvK",  // 6 pieces, 5 v 1
            "KQBNvKRN", // 7 pieces, 4 v 3
            "KQRBNvKN", // 7 pieces, 5 v 2
        ];
        let directory =
            std::env::temp_dir().join(format!("knightbyte-names-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("create temp directory");
        for name in names {
            // A table file's size is 16 modulo 64; the scan checks only that.
            std::fs::write(directory.join(format!("{name}.rtbw")), [0_u8; 16])
                .expect("write dummy table");
        }

        let inner = Inner::open(&directory.to_string_lossy());
        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(
            inner.wdl_count,
            names.len(),
            "every material family must be registered"
        );
        assert_eq!(
            inner.max_pieces, 7,
            "the seven-piece family must be covered"
        );
    }
}
