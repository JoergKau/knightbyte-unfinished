//! v5 network file loader and immutable weight storage.
//!
//! The loader validates every architectural field, pins the quantization
//! constants to this build, checks the optional description bounds, verifies
//! the payload length and finally verifies the XOR checksum. A bad file is a
//! loud refusal; there is no silent fallback.

use std::fmt;

/// Network-file magic: `MCNN`.
pub const MAGIC: u32 = 0x4E4E_434D;
/// Supported format version.
pub const VERSION: u32 = 5;
/// Supported feature scheme.
pub const FEATURE_SCHEME: i32 = 2;
/// Number of feature slots per king region.
pub const SLOTS_PER_REGION: usize = 768;
/// Accumulator clamp.
pub const QA: i32 = 255;
/// SCReLU pre-scale shift.
pub const SCRELU_SHIFT: i32 = 8;
/// Post-SCReLU shift.
pub const OUTPUT_SHIFT: i32 = 0;
/// Output-layer clamp.
pub const OUTPUT_CAP: i32 = 255;
/// Centipawn scale multiplier.
pub const EVAL_SCALE_MUL: i64 = 1600;
/// Centipawn scale shift.
pub const EVAL_SCALE_SHIFT: i64 = 20;
/// Minimum accepted hidden width.
pub const MIN_L1: usize = 64;
/// Maximum accepted hidden width.
pub const MAX_L1: usize = 2048;
/// Maximum accepted description length.
pub const MAX_DESCRIPTION: usize = 4096;

/// A malformed or unsupported network file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetError(String);

impl NetError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NetError {}

/// A fully validated v5 network.
pub struct Net {
    region_table: [u8; 64],
    region_count: usize,
    l1: usize,
    description: String,
    feature_weights: Box<[i16]>,
    feature_biases: Box<[i16]>,
    output_weights: Box<[i16]>,
    output_bias: i32,
}

impl Net {
    /// Parses and validates a network file.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NetError> {
        if bytes.len() < 112 {
            return Err(NetError::new("network file is truncated"));
        }

        let mut reader = Reader::new(bytes);
        if reader.u32()? != MAGIC {
            return Err(NetError::new("bad network magic"));
        }
        if reader.u32()? != VERSION {
            return Err(NetError::new("unsupported network version"));
        }
        if reader.i32()? != FEATURE_SCHEME {
            return Err(NetError::new("unsupported feature scheme"));
        }
        let declared_regions = reader.i32()?;
        if !(1..=32).contains(&declared_regions) {
            return Err(NetError::new("region count out of bounds"));
        }

        let mut region_table = [0_u8; 64];
        region_table.copy_from_slice(reader.bytes(64)?);
        let table_max = region_table.iter().copied().max().unwrap_or(0);
        let region_count = table_max as i32 + 1;
        if region_count != declared_regions {
            return Err(NetError::new(
                "region count disagrees with the region table",
            ));
        }
        if region_count.count_ones() != 1 || !(1..=32).contains(&region_count) {
            return Err(NetError::new(
                "region count must be a power of two in 1..=32",
            ));
        }
        let mut used = [false; 32];
        for region in region_table {
            used[region as usize] = true;
        }
        for (region, is_used) in used.iter().take(region_count as usize).enumerate() {
            if !is_used {
                return Err(NetError::new(format!(
                    "region table contains unused region {region}"
                )));
            }
        }

        let l1 = reader.i32()? as usize;
        if !(MIN_L1..=MAX_L1).contains(&l1) || !l1.is_multiple_of(16) {
            return Err(NetError::new("hidden width is out of bounds or misaligned"));
        }
        let buckets = reader.i32()?;
        if buckets != 1 {
            return Err(NetError::new(
                "this build supports exactly one output bucket",
            ));
        }
        if reader.i32()? != QA
            || reader.i32()? != SCRELU_SHIFT
            || reader.i32()? != OUTPUT_SHIFT
            || reader.i32()? != EVAL_SCALE_MUL as i32
            || reader.i32()? != EVAL_SCALE_SHIFT as i32
        {
            return Err(NetError::new(
                "network quantization differs from this build",
            ));
        }

        let description_length = reader.i32()?;
        if description_length < 0 || description_length as usize > MAX_DESCRIPTION {
            return Err(NetError::new("network description length is out of bounds"));
        }
        let description_bytes = reader.bytes(description_length as usize)?;
        let description = std::str::from_utf8(description_bytes)
            .map_err(|_| NetError::new("network description is not UTF-8"))?
            .to_owned();

        let feature_rows = SLOTS_PER_REGION * region_count as usize;
        let feature_weights = reader.i16s(feature_rows * l1)?;
        let feature_biases = reader.i16s(l1)?;
        let output_weights = reader.i16s(2 * l1)?;
        let output_bias = reader.i32()?;

        let expected_length = 108
            + 4
            + description_length as usize
            + feature_rows * l1 * 2
            + l1 * 2
            + 2 * l1 * 2
            + 4
            + 4;
        if bytes.len() != expected_length {
            return Err(NetError::new(
                "network file has an unexpected payload length",
            ));
        }

        let checksum_offset = bytes.len() - 4;
        let stored = reader.u32_at(checksum_offset)?;
        let actual = xor_word_checksum(bytes, 108, checksum_offset);
        if actual != stored {
            return Err(NetError::new("network checksum mismatch"));
        }

        Ok(Self {
            region_table,
            region_count: region_count as usize,
            l1,
            description,
            feature_weights: feature_weights.into_boxed_slice(),
            feature_biases: feature_biases.into_boxed_slice(),
            output_weights: output_weights.into_boxed_slice(),
            output_bias,
        })
    }

    /// Returns the raw king-region table.
    #[inline(always)]
    pub fn region_table(&self) -> &[u8; 64] {
        &self.region_table
    }

    /// Returns the number of king regions.
    #[inline(always)]
    pub fn region_count(&self) -> usize {
        self.region_count
    }

    /// Returns the hidden width.
    #[inline(always)]
    pub fn l1(&self) -> usize {
        self.l1
    }

    /// Returns the network description from the file header.
    pub fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn feature_weights(&self) -> &[i16] {
        &self.feature_weights
    }

    pub(crate) fn feature_biases(&self) -> &[i16] {
        &self.feature_biases
    }

    pub(crate) fn output_weights(&self) -> &[i16] {
        &self.output_weights
    }

    pub(crate) fn output_bias(&self) -> i32 {
        self.output_bias
    }
}

fn xor_word_checksum(bytes: &[u8], start: usize, end: usize) -> u32 {
    let mut checksum = 0_u32;
    let mut offset = start;
    while offset + 4 <= end {
        checksum ^= u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;
    }
    checksum
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], NetError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| NetError::new("network offset overflow"))?;
        if end > self.bytes.len() {
            return Err(NetError::new("network file is truncated"));
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, NetError> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u32_at(&self, offset: usize) -> Result<u32, NetError> {
        if offset + 4 > self.bytes.len() {
            return Err(NetError::new("network file is truncated"));
        }
        Ok(u32::from_le_bytes([
            self.bytes[offset],
            self.bytes[offset + 1],
            self.bytes[offset + 2],
            self.bytes[offset + 3],
        ]))
    }

    fn i32(&mut self) -> Result<i32, NetError> {
        Ok(self.u32()? as i32)
    }

    fn i16s(&mut self, count: usize) -> Result<Vec<i16>, NetError> {
        let bytes = self.bytes(count * 2)?;
        let mut values = Vec::with_capacity(count);
        for chunk in bytes.as_chunks::<2>().0 {
            values.push(i16::from_le_bytes(*chunk));
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::Net;
    use crate::tools::net::bytes;

    #[test]
    fn embedded_network_loads() {
        let net = Net::from_bytes(bytes()).expect("embedded network must load");
        assert_eq!(net.l1(), 512);
        assert_eq!(net.region_count(), 8);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = bytes().to_vec();
        data[0] ^= 0xff;
        assert!(Net::from_bytes(&data).is_err());
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut data = bytes().to_vec();
        data[4] = 4;
        assert!(Net::from_bytes(&data).is_err());
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let mut data = bytes().to_vec();
        let last = data.len() - 1;
        data[last] ^= 0xff;
        assert!(Net::from_bytes(&data).is_err());
    }

    #[test]
    fn rejects_truncated_payload() {
        let data = bytes();
        assert!(Net::from_bytes(&data[..data.len() - 8]).is_err());
    }
}
