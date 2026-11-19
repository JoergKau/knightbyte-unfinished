//! SIMD kernels for the accumulator arithmetic and the output head.
//!
//! The evaluation calls three bulk operations per node: adding a weight row,
//! subtracting one, and running the SCReLU output head over a ready
//! accumulator pair. All three are integer-exact, so every instruction-set
//! backend must produce bit-identical results — the parity and bench tests
//! depend on it.
//!
//! The backend is selected once per process from [`crate::platform::cpu`]
//! (`Isa`), with the ADR-0001 `KNIGHTBYTE_FORCE_ISA` override for tests and
//! CI. The ladder currently maps `v3`, `v3-vnni` and `v4-vnni` to the 256-bit
//! AVX2 kernels: the SCReLU head squares each element before the dot product,
//! which cannot use the VNNI dot-product instruction without changing the
//! per-element semantics, and Zen 4 executes 256-bit AVX2 natively. AArch64
//! keeps the scalar fallback until the NEON kernels land with the macOS
//! validation phase.

use std::sync::OnceLock;

use crate::eval::nnue::net::{
    EVAL_SCALE_MUL, EVAL_SCALE_SHIFT, OUTPUT_CAP, OUTPUT_SHIFT, QA, SCRELU_SHIFT,
};
use crate::types::color::Color;

#[cfg(target_arch = "x86_64")]
use crate::platform::cpu::{Isa, detect_isa};

/// The instruction-set backend chosen once per process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KernelKind {
    /// Pure Rust loops (non-x86 builds and the forced test fallback).
    Scalar,
    /// 128-bit kernels over the x86-64 SSE4.1/SSE4.2 baseline.
    Sse41,
    /// 256-bit AVX2 kernels (the ladder's v3 and above).
    Avx2,
    /// 512-bit AVX-512 kernels (the ladder's v4-vnni rung).
    Avx512,
}

static KERNEL: OnceLock<KernelKind> = OnceLock::new();

/// Returns the selected backend, honouring `KNIGHTBYTE_FORCE_ISA`.
fn kernel() -> KernelKind {
    *KERNEL.get_or_init(|| {
        // ADR-0001: the override pins a backend so every variant can be
        // exercised on one machine in tests and CI.
        if let Ok(force) = std::env::var("KNIGHTBYTE_FORCE_ISA") {
            let pinned = match force.as_str() {
                "scalar" => Some(KernelKind::Scalar),
                "v2" | "sse41" => Some(KernelKind::Sse41),
                "v3" | "avx2" => Some(KernelKind::Avx2),
                "v3-vnni" => Some(KernelKind::Avx2),
                "v4-vnni" | "avx512" => Some(KernelKind::Avx512),
                _ => None,
            };
            if let Some(kernel) = pinned {
                return kernel;
            }
        }
        detected_kernel()
    })
}

/// Maps the detected instruction set to the backend (x86-64 hosts).
#[cfg(target_arch = "x86_64")]
fn detected_kernel() -> KernelKind {
    match detect_isa() {
        Isa::X86V2 => KernelKind::Sse41,
        Isa::X86V3 | Isa::X86V3Vnni => KernelKind::Avx2,
        Isa::X86V4Vnni => KernelKind::Avx512,
        Isa::Aarch64 => KernelKind::Scalar,
    }
}

/// AArch64 hosts have a single compile-time feature set (scalar until the
/// NEON kernels land).
#[cfg(target_arch = "aarch64")]
fn detected_kernel() -> KernelKind {
    KernelKind::Scalar
}

/// Adds (`add`) or subtracts the weight row `row` from `acc`.
///
/// The row width is `acc.len()`; the network validation guarantees a width
/// that is a multiple of 16, so the 128-bit and 256-bit kernels need no tail
/// loop.
pub(crate) fn add_row(acc: &mut [i16], weights: &[i16], row: usize, add: bool) {
    let l1 = acc.len();
    let base = row * l1;
    let row_weights = &weights[base..base + l1];
    match kernel() {
        KernelKind::Scalar => scalar::add_row(acc, row_weights, add),
        #[cfg(target_arch = "x86_64")]
        KernelKind::Sse41 => unsafe { wide::add_row_sse41(acc, row_weights, add) },
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx2 => unsafe { wide::add_row_avx2(acc, row_weights, add) },
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx512 => unsafe { wide::add_row_avx512(acc, row_weights, add) },
    }
}

/// Runs the SCReLU output head over a ready accumulator pair and returns the
/// scaled evaluation in centipawns from the side-to-move perspective.
pub(crate) fn evaluate_head(
    stm: Color,
    pair: &[Vec<i16>; 2],
    output_weights: &[i16],
    output_bias: i32,
) -> i32 {
    let sum = match kernel() {
        KernelKind::Scalar => scalar::evaluate_head(stm, pair, output_weights, output_bias),
        #[cfg(target_arch = "x86_64")]
        KernelKind::Sse41 => unsafe {
            wide::evaluate_head_sse41(stm, pair, output_weights, output_bias)
        },
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx2 => unsafe {
            wide::evaluate_head_avx2(stm, pair, output_weights, output_bias)
        },
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx512 => unsafe {
            wide::evaluate_head_avx512(stm, pair, output_weights, output_bias)
        },
    };
    ((sum * EVAL_SCALE_MUL) >> EVAL_SCALE_SHIFT) as i32
}

/// SCReLU activation used by the output head.
#[inline(always)]
pub(crate) fn screlu(value: i16) -> i32 {
    let clamped = i32::from(value).clamp(0, QA);
    (clamped * clamped) >> SCRELU_SHIFT
}

/// Output-layer input transform (a no-op clamp for the production format).
#[inline(always)]
pub(crate) fn output(value: i32) -> i32 {
    (value >> OUTPUT_SHIFT).clamp(0, OUTPUT_CAP)
}

/// The exact scalar reference: every SIMD backend must produce these values
/// bit-for-bit, and the unit tests compare the dispatched backend against it.
pub(crate) mod scalar {
    use super::{output, screlu};
    use crate::types::color::Color;

    /// Adds or subtracts one weight row with wrapping `i16` arithmetic.
    pub(crate) fn add_row(acc: &mut [i16], row_weights: &[i16], add: bool) {
        for (index, value) in acc.iter_mut().enumerate() {
            if add {
                *value = value.wrapping_add(row_weights[index]);
            } else {
                *value = value.wrapping_sub(row_weights[index]);
            }
        }
    }

    /// The SCReLU output head: `sum` accumulates the exact `i64` products.
    pub(crate) fn evaluate_head(
        stm: Color,
        pair: &[Vec<i16>; 2],
        output_weights: &[i16],
        output_bias: i32,
    ) -> i64 {
        let l1 = pair[0].len();
        let stm_acc = &pair[stm as usize];
        let ntm_acc = &pair[(stm ^ 1) as usize];
        let mut sum = i64::from(output_bias);

        for index in 0..l1 {
            let a = stm_acc[index];
            if a > 0 {
                sum += i64::from(output(screlu(a))) * i64::from(output_weights[index]);
            }
            let b = ntm_acc[index];
            if b > 0 {
                sum += i64::from(output(screlu(b))) * i64::from(output_weights[l1 + index]);
            }
        }
        sum
    }
}

#[cfg(target_arch = "x86_64")]
pub(crate) mod wide {
    //! The 128-bit and 256-bit x86 kernels.
    //!
    //! The head computes per element `v = (clamp(a)²) >> 8` by widening the
    //! clamped lanes to `i32` (a plain square in `i16` would wrap), multiplying
    //! by the widened output weight and accumulating into `i32` lanes that
    //! fold into an `i64` total every few chunks. The fold bounds keep every
    //! `i32` partial sum far below overflow, so the result is bit-identical to
    //! the scalar `i64` accumulation.

    use std::arch::x86_64::*;

    use crate::types::color::Color;

    /// Adds or subtracts one weight row with 128-bit chunks.
    ///
    /// # Safety
    ///
    /// The host CPU must support SSE4.1 (the x86-64 v2 baseline).
    #[target_feature(enable = "sse4.1")]
    pub(super) unsafe fn add_row_sse41(acc: &mut [i16], row_weights: &[i16], add: bool) {
        let (acc_chunks, _) = acc.as_chunks_mut::<8>();
        let (row_chunks, _) = row_weights.as_chunks::<8>();
        for (acc_chunk, row_chunk) in acc_chunks.iter_mut().zip(row_chunks) {
            unsafe {
                let value = _mm_loadu_si128(acc_chunk.as_ptr() as *const __m128i);
                let row = _mm_loadu_si128(row_chunk.as_ptr() as *const __m128i);
                let result = if add {
                    _mm_add_epi16(value, row)
                } else {
                    _mm_sub_epi16(value, row)
                };
                _mm_storeu_si128(acc_chunk.as_mut_ptr() as *mut __m128i, result);
            }
        }
    }

    /// Runs the output head with 128-bit chunks.
    ///
    /// # Safety
    ///
    /// The host CPU must support SSE4.1 (the x86-64 v2 baseline).
    #[target_feature(enable = "sse4.1")]
    pub(super) unsafe fn evaluate_head_sse41(
        stm: Color,
        pair: &[Vec<i16>; 2],
        output_weights: &[i16],
        output_bias: i32,
    ) -> i64 {
        let zero = _mm_setzero_si128();
        let qa = _mm_set1_epi16(crate::eval::nnue::net::QA as i16);
        let mut total = i64::from(output_bias);
        let mut lanes = _mm_setzero_si128();

        let stm_acc = &pair[stm as usize];
        let ntm_acc = &pair[(stm ^ 1) as usize];
        let l1 = stm_acc.len();
        for base in (0..l1).step_by(8) {
            // The stm rows and the ntm rows live in the two halves of the
            // output weights; each chunk pairs its lanes with its own half.
            for (acc_chunk, weight_base) in [(stm_acc, base), (ntm_acc, l1 + base)] {
                unsafe {
                    let a = _mm_loadu_si128(acc_chunk[base..].as_ptr() as *const __m128i);
                    let clamped = _mm_min_epi16(_mm_max_epi16(a, zero), qa);
                    // Widen to i32, square, shift: the per-element semantics
                    // of the scalar path (a pairwise `madd` would sum the
                    // squares of two lanes before the shift).
                    let wide_lo = _mm_cvtepi16_epi32(clamped);
                    let wide_hi = _mm_cvtepi16_epi32(_mm_unpackhi_epi64(clamped, clamped));
                    let sq_lo = _mm_mullo_epi32(wide_lo, wide_lo);
                    let sq_hi = _mm_mullo_epi32(wide_hi, wide_hi);
                    let v_lo = _mm_srai_epi32::<8>(sq_lo);
                    let v_hi = _mm_srai_epi32::<8>(sq_hi);
                    let w =
                        _mm_loadu_si128(output_weights[weight_base..].as_ptr() as *const __m128i);
                    let w_lo = _mm_cvtepi16_epi32(w);
                    let w_hi = _mm_cvtepi16_epi32(_mm_unpackhi_epi64(w, w));
                    lanes = _mm_add_epi32(lanes, _mm_mullo_epi32(v_lo, w_lo));
                    lanes = _mm_add_epi32(lanes, _mm_mullo_epi32(v_hi, w_hi));
                }
            }
            // Four chunks add at most 4 x 2 x 254 x 32767 = 66 M per lane,
            // far below `i32` overflow, so the fold is exact.
            if base % 32 == 24 {
                total += fold_lanes128(lanes);
                lanes = _mm_setzero_si128();
            }
        }
        total + fold_lanes128(lanes)
    }

    /// Adds or subtracts one weight row with 256-bit chunks.
    ///
    /// # Safety
    ///
    /// The host CPU must support AVX2 (the x86-64 v3 baseline).
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn add_row_avx2(acc: &mut [i16], row_weights: &[i16], add: bool) {
        let (acc_chunks, _) = acc.as_chunks_mut::<16>();
        let (row_chunks, _) = row_weights.as_chunks::<16>();
        for (acc_chunk, row_chunk) in acc_chunks.iter_mut().zip(row_chunks) {
            unsafe {
                let value = _mm256_loadu_si256(acc_chunk.as_ptr() as *const __m256i);
                let row = _mm256_loadu_si256(row_chunk.as_ptr() as *const __m256i);
                let result = if add {
                    _mm256_add_epi16(value, row)
                } else {
                    _mm256_sub_epi16(value, row)
                };
                _mm256_storeu_si256(acc_chunk.as_mut_ptr() as *mut __m256i, result);
            }
        }
    }

    /// Runs the output head with 256-bit chunks.
    ///
    /// # Safety
    ///
    /// The host CPU must support AVX2 (the x86-64 v3 baseline).
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn evaluate_head_avx2(
        stm: Color,
        pair: &[Vec<i16>; 2],
        output_weights: &[i16],
        output_bias: i32,
    ) -> i64 {
        let zero = _mm256_setzero_si256();
        let qa = _mm256_set1_epi16(crate::eval::nnue::net::QA as i16);
        let mut total = i64::from(output_bias);
        let mut lanes = _mm256_setzero_si256();

        let stm_acc = &pair[stm as usize];
        let ntm_acc = &pair[(stm ^ 1) as usize];
        let l1 = stm_acc.len();
        for base in (0..l1).step_by(16) {
            // The stm rows and the ntm rows live in the two halves of the
            // output weights; each chunk pairs its lanes with its own half.
            for (acc_chunk, weight_base) in [(stm_acc, base), (ntm_acc, l1 + base)] {
                unsafe {
                    let a = _mm256_loadu_si256(acc_chunk[base..].as_ptr() as *const __m256i);
                    let clamped = _mm256_min_epi16(_mm256_max_epi16(a, zero), qa);
                    let wide_lo = _mm256_cvtepi16_epi32(_mm256_castsi256_si128(clamped));
                    let wide_hi = _mm256_cvtepi16_epi32(_mm256_extracti128_si256::<1>(clamped));
                    let sq_lo = _mm256_mullo_epi32(wide_lo, wide_lo);
                    let sq_hi = _mm256_mullo_epi32(wide_hi, wide_hi);
                    let v_lo = _mm256_srai_epi32::<8>(sq_lo);
                    let v_hi = _mm256_srai_epi32::<8>(sq_hi);
                    let w = _mm256_loadu_si256(
                        output_weights[weight_base..].as_ptr() as *const __m256i
                    );
                    let w_lo = _mm256_cvtepi16_epi32(_mm256_castsi256_si128(w));
                    let w_hi = _mm256_cvtepi16_epi32(_mm256_extracti128_si256::<1>(w));
                    lanes = _mm256_add_epi32(lanes, _mm256_mullo_epi32(v_lo, w_lo));
                    lanes = _mm256_add_epi32(lanes, _mm256_mullo_epi32(v_hi, w_hi));
                }
            }
            // Four iterations add at most 4 x 2 x 8 x 254 x 32767 = 531 M per
            // lane, far below `i32` overflow, so the fold is exact.
            if base % 64 == 48 {
                total += fold_lanes(lanes);
                lanes = _mm256_setzero_si256();
            }
        }
        total + fold_lanes(lanes)
    }

    /// Adds or subtracts one weight row with 512-bit chunks.
    ///
    /// # Safety
    ///
    /// The host CPU must support AVX-512 Foundation and BW (the x86-64 v4
    /// baseline).
    #[target_feature(enable = "avx512f,avx512bw,avx512vl")]
    pub(super) unsafe fn add_row_avx512(acc: &mut [i16], row_weights: &[i16], add: bool) {
        let (acc_chunks, _) = acc.as_chunks_mut::<32>();
        let (row_chunks, _) = row_weights.as_chunks::<32>();
        for (acc_chunk, row_chunk) in acc_chunks.iter_mut().zip(row_chunks) {
            unsafe {
                let value = _mm512_loadu_si512(acc_chunk.as_ptr() as *const __m512i);
                let row = _mm512_loadu_si512(row_chunk.as_ptr() as *const __m512i);
                let result = if add {
                    _mm512_add_epi16(value, row)
                } else {
                    _mm512_sub_epi16(value, row)
                };
                _mm512_storeu_si512(acc_chunk.as_mut_ptr() as *mut __m512i, result);
            }
        }
    }

    /// Runs the output head with 512-bit chunks.
    ///
    /// Each 32-lane chunk splits into two 256-bit halves, which widen to
    /// `i32` exactly like the 256-bit kernel's chunks; the products accumulate
    /// into 16 `i32` lanes and fold into the `i64` total on the same bound.
    ///
    /// # Safety
    ///
    /// The host CPU must support AVX-512 Foundation and BW (the x86-64 v4
    /// baseline).
    #[target_feature(enable = "avx512f,avx512bw,avx512vl")]
    pub(super) unsafe fn evaluate_head_avx512(
        stm: Color,
        pair: &[Vec<i16>; 2],
        output_weights: &[i16],
        output_bias: i32,
    ) -> i64 {
        let zero = _mm512_setzero_si512();
        let qa = _mm512_set1_epi16(crate::eval::nnue::net::QA as i16);
        let mut total = i64::from(output_bias);
        let mut lanes = _mm512_setzero_si512();

        let stm_acc = &pair[stm as usize];
        let ntm_acc = &pair[(stm ^ 1) as usize];
        let l1 = stm_acc.len();
        for base in (0..l1).step_by(32) {
            // The stm rows and the ntm rows live in the two halves of the
            // output weights; each chunk pairs its lanes with its own half.
            for (acc_chunk, weight_base) in [(stm_acc, base), (ntm_acc, l1 + base)] {
                unsafe {
                    let a = _mm512_loadu_si512(acc_chunk[base..].as_ptr() as *const __m512i);
                    let clamped = _mm512_min_epi16(_mm512_max_epi16(a, zero), qa);
                    // Two widening squares per 32-lane chunk (lo/hi 256).
                    let chunk_lo = _mm512_castsi512_si256(clamped);
                    let chunk_hi = _mm512_extracti64x4_epi64::<1>(clamped);
                    let v_lo = _mm512_srai_epi32::<8>(_mm512_mullo_epi32(
                        _mm512_cvtepi16_epi32(chunk_lo),
                        _mm512_cvtepi16_epi32(chunk_lo),
                    ));
                    let v_hi = _mm512_srai_epi32::<8>(_mm512_mullo_epi32(
                        _mm512_cvtepi16_epi32(chunk_hi),
                        _mm512_cvtepi16_epi32(chunk_hi),
                    ));
                    let w = _mm512_loadu_si512(
                        output_weights[weight_base..].as_ptr() as *const __m512i
                    );
                    let w_lo = _mm512_cvtepi16_epi32(_mm512_castsi512_si256(w));
                    let w_hi = _mm512_cvtepi16_epi32(_mm512_extracti64x4_epi64::<1>(w));
                    lanes = _mm512_add_epi32(lanes, _mm512_mullo_epi32(v_lo, w_lo));
                    lanes = _mm512_add_epi32(lanes, _mm512_mullo_epi32(v_hi, w_hi));
                }
            }
            // Four iterations add at most 4 x 2 x 16 x 254 x 32767 = 1.06 G per
            // lane, far below `i32` overflow, so the fold is exact.
            if base % 128 == 96 {
                total += fold_lanes512(lanes);
                lanes = _mm512_setzero_si512();
            }
        }
        total + fold_lanes512(lanes)
    }

    /// Horizontal sum of the sixteen `i32` lanes into the `i64` total.
    fn fold_lanes512(lanes: __m512i) -> i64 {
        let mut out = [0_i32; 16];
        unsafe { _mm512_storeu_si512(out.as_mut_ptr() as *mut __m512i, lanes) };
        out.iter().map(|value| i64::from(*value)).sum()
    }

    /// Horizontal sum of the eight `i32` lanes into the `i64` total.
    fn fold_lanes(lanes: __m256i) -> i64 {
        let mut out = [0_i32; 8];
        unsafe { _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i, lanes) };
        out.iter().map(|value| i64::from(*value)).sum()
    }

    /// Horizontal sum of the four `i32` lanes into the `i64` total.
    fn fold_lanes128(lanes: __m128i) -> i64 {
        let mut out = [0_i32; 4];
        unsafe { _mm_storeu_si128(out.as_mut_ptr() as *mut __m128i, lanes) };
        out.iter().map(|value| i64::from(*value)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::{add_row, evaluate_head, scalar};
    use crate::eval::nnue::net::{EVAL_SCALE_MUL, EVAL_SCALE_SHIFT};
    use crate::types::color::{BLACK, WHITE};

    /// A tiny deterministic generator for the kernel parity tests.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % n
        }

        /// Mixed magnitudes with saturated and negative values, so the clamps
        /// and the sign handling are exercised.
        fn i16(&mut self) -> i16 {
            let value = self.below(65536) as u16;
            (value >> 1) as i16
        }
    }

    #[test]
    fn the_dispatched_backend_matches_the_scalar_reference() {
        let mut rng = Lcg(0x5EED);
        let l1 = 512;
        let weights: Vec<i16> = (0..4 * l1).map(|_| rng.i16()).collect();

        // Row add and row subtract against the scalar reference.
        let mut acc: Vec<i16> = (0..l1).map(|_| rng.i16()).collect();
        let reference = acc.clone();
        let row = 3;
        add_row(&mut acc, &weights, row, true);
        for (index, value) in acc.iter().enumerate() {
            assert_eq!(
                *value,
                reference[index].wrapping_add(weights[row * l1 + index]),
                "row add diverges at {index}"
            );
        }
        add_row(&mut acc, &weights, row, false);
        for (index, value) in acc.iter().enumerate() {
            assert_eq!(*value, reference[index], "row subtract diverges at {index}");
        }

        // The output head over random pairs, both perspectives.
        let output_weights: Vec<i16> = (0..2 * l1).map(|_| rng.i16()).collect();
        for stm in [WHITE, BLACK] {
            let pair = [
                (0..l1).map(|_| rng.i16()).collect::<Vec<i16>>(),
                (0..l1).map(|_| rng.i16()).collect::<Vec<i16>>(),
            ];
            let dispatched = evaluate_head(stm, &pair, &output_weights, -1_234);
            let reference = scalar::evaluate_head(stm, &pair, &output_weights, -1_234);
            let scaled = ((reference * EVAL_SCALE_MUL) >> EVAL_SCALE_SHIFT) as i32;
            assert_eq!(
                dispatched, scaled,
                "the head diverges for side {stm}: dispatched {dispatched}, reference {reference}"
            );
        }
    }
}
