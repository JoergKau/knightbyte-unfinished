//! Memory-mapped table files and Pairs decompression.
//!
//! A Syzygy table stores its values in "Pairs" blocks: an index table points
//! into a block-size table, and each block is a bit stream of Huffman symbols
//! whose leaves hold two value bytes. This module owns the byte-level access,
//! the one-time block-table setup and the decompression loop.
//!
//! All reads are checked: a truncated or corrupt file makes the probe fail
//! softly instead of panicking or spinning.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

/// Table kind: win/draw/loss.
pub(crate) const WDL: usize = 0;
/// Table kind: distance to zeroing.
pub(crate) const DTZ: usize = 1;

/// A read-only memory-mapped file with checked little- and big-endian reads.
#[derive(Clone)]
pub(crate) struct Bytes {
    map: Arc<Mmap>,
}

impl Bytes {
    /// Maps `path` read-only. Fails for unreadable or empty files.
    pub(crate) fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        // SAFETY: the mapping is read-only and private. The table files are
        // published artifacts that are not written while the engine runs; a
        // file that changes underneath the mapping would be a user error, and
        // every read below is bounds-checked against the mapped length.
        let map = unsafe { Mmap::map(&file)? };
        if map.is_empty() {
            return Err(std::io::Error::other("empty tablebase file"));
        }
        Ok(Self { map: Arc::new(map) })
    }

    /// Returns the file length in bytes.
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Reads one byte at `offset`, or `None` past the end.
    pub(crate) fn u8(&self, offset: u64) -> Option<u8> {
        self.map.get(offset as usize).copied()
    }

    /// Reads a little-endian `u16` at `offset`.
    pub(crate) fn u16(&self, offset: u64) -> Option<u16> {
        let index = offset as usize;
        let bytes = self.map.get(index..index.checked_add(2)?)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Reads a little-endian `u32` at `offset`.
    pub(crate) fn u32(&self, offset: u64) -> Option<u32> {
        let index = offset as usize;
        let bytes = self.map.get(index..index.checked_add(4)?)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads a big-endian `u64` at `offset` (the decompressor's bit-stream word).
    pub(crate) fn u64_be(&self, offset: u64) -> Option<u64> {
        let index = offset as usize;
        let bytes = self.map.get(index..index.checked_add(8)?)?;
        Some(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Reads a big-endian `u32` at `offset` (the decompressor's refill word).
    pub(crate) fn u32_be(&self, offset: u64) -> Option<u32> {
        let index = offset as usize;
        let bytes = self.map.get(index..index.checked_add(4)?)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

/// One decompression table of a table file.
#[derive(Clone)]
pub(crate) struct PairsData {
    /// The mapped file the offsets below refer to.
    pub(crate) file: Bytes,
    /// Offset of the 6-byte-per-entry index table.
    pub(crate) index_table: u64,
    /// Offset of the 2-byte-per-entry block-size table.
    pub(crate) size_table: u64,
    /// Offset of the compressed block data.
    pub(crate) data: u64,
    /// Huffman offset per code length.
    pub(crate) offset: Vec<u16>,
    /// Number of values below each symbol.
    pub(crate) sym_len: Vec<u8>,
    /// Three bytes per symbol (left child, packed right child, value pair).
    pub(crate) sym_pat: Vec<u8>,
    /// Log2 of the block size.
    pub(crate) block_size: u8,
    /// Log2 of the index granularity (0 = constant table).
    pub(crate) idx_bits: u8,
    /// Shortest Huffman code length.
    pub(crate) min_len: u8,
    /// Constant value for a constant-compressed table.
    pub(crate) const_value: [u8; 2],
    /// First code of each length.
    pub(crate) base: Vec<u64>,
}

/// Sizes of the three table regions produced by [`setup_pairs`].
pub(crate) type PairsSizes = [u64; 3];

/// Sets up one Pairs table at `*data_off` and advances the offset.
///
/// Returns the table, the region sizes (index table, block-size table, data)
/// and the header flags. `None` means the file is malformed.
pub(crate) fn setup_pairs(
    file: &Bytes,
    data_off: &mut u64,
    tb_size: u64,
    kind: usize,
) -> Option<(PairsData, PairsSizes, u8)> {
    let start = *data_off;
    let flags = file.u8(start)?;

    // A constant table stores one value for every index.
    if flags & 0x80 != 0 {
        let value = if kind == WDL { file.u8(start + 1)? } else { 0 };
        *data_off = start + 2;
        return Some((
            PairsData {
                file: file.clone(),
                index_table: 0,
                size_table: 0,
                data: 0,
                offset: Vec::new(),
                sym_len: Vec::new(),
                sym_pat: Vec::new(),
                block_size: 0,
                idx_bits: 0,
                min_len: 0,
                const_value: [value, 0],
                base: Vec::new(),
            },
            [0, 0, 0],
            flags,
        ));
    }

    let block_size = file.u8(start + 1)?;
    let idx_bits = file.u8(start + 2)?;
    if idx_bits >= 64 || block_size >= 64 {
        return None;
    }
    let real_num_blocks = file.u32(start + 4)?;
    let num_blocks = real_num_blocks + u32::from(file.u8(start + 3)?);
    let max_len = file.u8(start + 8)? as usize;
    let min_len = file.u8(start + 9)? as usize;
    if min_len == 0 || max_len < min_len {
        return None;
    }
    let height = max_len - min_len + 1;
    let num_syms = file.u16(start + 10 + 2 * height as u64)? as usize;
    if num_syms == 0 {
        return None;
    }

    let mut offset = vec![0_u16; height];
    for (index, slot) in offset.iter_mut().enumerate() {
        *slot = file.u16(start + 10 + 2 * index as u64)?;
    }

    let mut sym_pat = vec![0_u8; 3 * num_syms];
    let pattern_start = start + 12 + 2 * height as u64;
    for (index, slot) in sym_pat.iter_mut().enumerate() {
        *slot = file.u8(pattern_start + index as u64)?;
    }
    *data_off = pattern_start + 3 * num_syms as u64 + (num_syms as u64 & 1);

    let num_indices = (tb_size + (1_u64 << idx_bits) - 1) >> idx_bits;
    let sizes = [
        6 * num_indices,
        2 * u64::from(num_blocks),
        u64::from(real_num_blocks) << block_size,
    ];

    let mut table = PairsData {
        file: file.clone(),
        index_table: 0,
        size_table: 0,
        data: 0,
        offset,
        sym_len: vec![0_u8; num_syms],
        sym_pat,
        block_size,
        idx_bits,
        min_len: min_len as u8,
        const_value: [0, 0],
        base: vec![0_u64; height],
    };

    let mut seen = vec![0_u8; num_syms];
    for symbol in 0..num_syms {
        if seen[symbol] == 0 {
            calc_sym_len(&mut table, symbol, &mut seen);
        }
    }

    let height = table.base.len();
    table.base[height - 1] = 0;
    for index in (0..height - 1).rev() {
        let sum = table.base[index + 1]
            .wrapping_add(u64::from(table.offset[index]))
            .wrapping_sub(u64::from(table.offset[index + 1]));
        table.base[index] = sum / 2;
    }
    for (index, base) in table.base.iter_mut().enumerate().take(height) {
        *base = base.wrapping_shl(64 - (min_len + index) as u32);
    }

    Some((table, sizes, flags))
}

/// Fills `sym_len` for one Huffman symbol and its children.
fn calc_sym_len(table: &mut PairsData, symbol: usize, seen: &mut [u8]) {
    let w3 = 3 * symbol;
    let second = ((table.sym_pat[w3 + 2] as usize) << 4) | (table.sym_pat[w3 + 1] as usize >> 4);
    if second == 0x0fff {
        table.sym_len[symbol] = 0;
    } else {
        let first = (((table.sym_pat[w3 + 1] & 0xf) as usize) << 8) | table.sym_pat[w3] as usize;
        if seen[first] == 0 {
            calc_sym_len(table, first, seen);
        }
        if seen[second] == 0 {
            calc_sym_len(table, second, seen);
        }
        table.sym_len[symbol] = table.sym_len[first]
            .saturating_add(table.sym_len[second])
            .saturating_add(1);
    }
    seen[symbol] = 1;
}

/// Decompresses the value tuple for `index`; `None` means corrupt data.
pub(crate) fn decompress_pairs(table: &PairsData, index: u64) -> Option<(u8, u8)> {
    // Constant tables store the same value for every index.
    if table.idx_bits == 0 {
        return Some((table.const_value[0], table.const_value[1]));
    }

    let main_index = (index >> table.idx_bits) as u32;
    let mut literal =
        (index & ((1_u64 << table.idx_bits) - 1)) as i64 - (1_i64 << (table.idx_bits - 1));
    let mut block = table
        .file
        .u32(table.index_table + 6 * u64::from(main_index))?;
    literal += i64::from(
        table
            .file
            .u16(table.index_table + 6 * u64::from(main_index) + 4)?,
    );

    // The block-size table bounds the walk; garbage from a corrupt file
    // terminates here instead of spinning.
    let available = table.file.len().saturating_sub(table.size_table as usize);
    let max_block = (available / 2).saturating_sub(1) as u32;
    if block > max_block {
        return None;
    }

    if literal < 0 {
        while literal < 0 && block > 0 {
            block -= 1;
            literal += i64::from(table.file.u16(table.size_table + 2 * u64::from(block))?) + 1;
        }
    } else {
        while literal > i64::from(table.file.u16(table.size_table + 2 * u64::from(block))?)
            && block < max_block
        {
            literal -= i64::from(table.file.u16(table.size_table + 2 * u64::from(block))?) + 1;
            block += 1;
        }
    }

    let mut pointer = table.data + (u64::from(block) << table.block_size);
    let mut code = table.file.u64_be(pointer)?;
    pointer += 8;
    let mut bit_count = 0_u32;
    let mut symbol;

    loop {
        let mut length = table.min_len as usize;
        while length - (table.min_len as usize) < table.base.len()
            && code < table.base[length - table.min_len as usize]
        {
            length += 1;
        }
        if length - table.min_len as usize >= table.base.len() {
            return None;
        }
        let slot = length - table.min_len as usize;
        symbol =
            usize::from(table.offset[slot]) + ((code - table.base[slot]) >> (64 - length)) as usize;
        if symbol >= table.sym_len.len() {
            return None;
        }
        if literal < i64::from(table.sym_len[symbol]) + 1 {
            break;
        }
        literal -= i64::from(table.sym_len[symbol]) + 1;
        code = code.wrapping_shl(length as u32);
        bit_count += length as u32;
        if bit_count >= 32 {
            bit_count -= 32;
            let word = table.file.u32_be(pointer)?;
            pointer += 4;
            code |= u64::from(word) << bit_count;
        }
    }

    while table.sym_len[symbol] != 0 {
        let w3 = 3 * symbol;
        let first = (((table.sym_pat[w3 + 1] & 0xf) as usize) << 8) | table.sym_pat[w3] as usize;
        if literal < i64::from(table.sym_len[first]) + 1 {
            symbol = first;
        } else {
            literal -= i64::from(table.sym_len[first]) + 1;
            symbol =
                ((table.sym_pat[w3 + 2] as usize) << 4) | (table.sym_pat[w3 + 1] as usize >> 4);
        }
        if symbol >= table.sym_len.len() {
            return None;
        }
    }

    let offset = 3 * symbol;
    Some((*table.sym_pat.get(offset)?, *table.sym_pat.get(offset + 1)?))
}
