//! SIMD kernels for the accumulator arithmetic and the output head.
//!
//! The evaluation calls three bulk operations: the fused incremental update
//! of a child accumulator from its parent (`push_rows`), the full refresh of
//! one perspective (`refresh_rows`), and the SCReLU output head over a ready
//! accumulator pair. All three are integer-exact, so every instruction-set
//! backend must produce bit-identical results — the parity and bench tests
//! depend on it.
//!
//! The backend is selected once per process from [`crate::platform::cpu`]
//! (`Isa`), with the ADR-0001 `KNIGHTBYTE_FORCE_ISA` override for tests and
//! CI. The x86 ladder maps `v3`, `v3-vnni` and `v4-vnni` to the 256-bit AVX2
//! kernels: the SCReLU head squares each element before the dot product,
//! which cannot use the VNNI dot-product instruction without changing the
//! per-element semantics, and Zen 4 executes 256-bit AVX2 natively. AArch64
//! (Apple Silicon) has exactly one compile-time backend: 128-bit NEON, which
//! is part of the AArch64 baseline and therefore safe on every Apple M chip.
//!
//! Selection is per process, not per call: the hot path pays one enum match
//! and then runs the chosen kernel without a feature test per node.

use std::sync::OnceLock;

use crate::eval::nnue::net::{
    EVAL_SCALE_MUL, EVAL_SCALE_SHIFT, OUTPUT_CAP, OUTPUT_SHIFT, QA, SCRELU_SHIFT,
};
use crate::types::color::Color;

#[cfg(target_arch = "x86_64")]
use crate::platform::cpu::{Isa, detect_isa};

/// The instruction-set backend chosen once per process.
///
/// Only the variants that exist on the compile target are defined, so the
/// dispatch matches stay exhaustive on every platform without a catch-all
/// arm that could silently downgrade a kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KernelKind {
    /// Pure Rust loops (the forced test fallback and non-SIMD builds).
    Scalar,
    /// 128-bit kernels over the x86-64 SSE4.1/SSE4.2 baseline.
    #[cfg(target_arch = "x86_64")]
    Sse41,
    /// 256-bit AVX2 kernels (the ladder's v3 and above).
    #[cfg(target_arch = "x86_64")]
    Avx2,
    /// 512-bit AVX-512 kernels (the ladder's v4-vnni rung).
    #[cfg(target_arch = "x86_64")]
    Avx512,
    /// 128-bit NEON kernels (the AArch64 baseline, Apple Silicon).
    #[cfg(target_arch = "aarch64")]
    Neon,
}

static KERNEL: OnceLock<KernelKind> = OnceLock::new();

/// Returns the selected backend, honouring `KNIGHTBYTE_FORCE_ISA`.
fn kernel() -> KernelKind {
    *KERNEL.get_or_init(|| {
        // ADR-0001: the override pins a backend so every variant can be
        // exercised on one machine in tests and CI. Names that do not exist
        // on this target are ignored, so the same CI script can set them.
        if let Ok(force) = std::env::var("KNIGHTBYTE_FORCE_ISA")
            && let Some(kernel) = forced_kernel(&force)
        {
            return kernel;
        }
        detected_kernel()
    })
}

/// Maps a `KNIGHTBYTE_FORCE_ISA` value to a backend on this target.
fn forced_kernel(name: &str) -> Option<KernelKind> {
    match name {
        "scalar" => Some(KernelKind::Scalar),
        #[cfg(target_arch = "x86_64")]
        "v2" | "sse41" => Some(KernelKind::Sse41),
        #[cfg(target_arch = "x86_64")]
        "v3" | "avx2" | "v3-vnni" => Some(KernelKind::Avx2),
        #[cfg(target_arch = "x86_64")]
        "v4-vnni" | "avx512" => Some(KernelKind::Avx512),
        #[cfg(target_arch = "aarch64")]
        "neon" | "aarch64" | "v8" => Some(KernelKind::Neon),
        _ => None,
    }
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

/// AArch64 hosts have exactly one backend: the AArch64 baseline includes
/// NEON, so every Apple M processor runs the same kernels.
#[cfg(target_arch = "aarch64")]
fn detected_kernel() -> KernelKind {
    KernelKind::Neon
}

/// Computes `dst = src - sum(subs) + sum(adds)` in one pass.
///
/// This is the incremental accumulator update: the parent perspective is
/// copied into the child while the changed weight rows are applied, so each
/// vector chunk is loaded once and stored once however many rows change.
/// `i16` wrapping addition is associative and commutative, so the result is
/// bit-identical to copying first and applying the rows one by one.
///
/// Every slice must be at least `dst.len()` long. The three row-count shapes
/// that moves produce (quiet move, capture or promotion, castling) have
/// specialised kernels; anything else takes the scalar path.
pub(crate) fn push_rows(dst: &mut [i16], src: &[i16], adds: &[&[i16]], subs: &[&[i16]]) {
    let n = dst.len();
    assert!(
        src.len() >= n && adds.iter().chain(subs).all(|row| row.len() >= n),
        "accumulator rows must cover the destination"
    );

    // SAFETY: the backend was selected from the detected (or forced and
    // verified) instruction set, and every slice covers `n`.
    macro_rules! by_shape {
        ($module:ident, $kernel:ident) => {
            match (adds.len(), subs.len()) {
                (1, 1) => unsafe {
                    $module::$kernel::<1, 1>(
                        dst,
                        src,
                        adds.try_into().unwrap(),
                        subs.try_into().unwrap(),
                    )
                },
                (1, 2) => unsafe {
                    $module::$kernel::<1, 2>(
                        dst,
                        src,
                        adds.try_into().unwrap(),
                        subs.try_into().unwrap(),
                    )
                },
                (2, 2) => unsafe {
                    $module::$kernel::<2, 2>(
                        dst,
                        src,
                        adds.try_into().unwrap(),
                        subs.try_into().unwrap(),
                    )
                },
                _ => scalar::push_rows(dst, src, adds, subs),
            }
        };
    }

    match kernel() {
        KernelKind::Scalar => scalar::push_rows(dst, src, adds, subs),
        #[cfg(target_arch = "x86_64")]
        KernelKind::Sse41 => by_shape!(wide, push_rows_sse41),
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx2 => by_shape!(wide, push_rows_avx2),
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx512 => by_shape!(wide, push_rows_avx512),
        #[cfg(target_arch = "aarch64")]
        KernelKind::Neon => by_shape!(neon, push_rows),
    }
}
/// Computes `dst = bias + sum(rows)` in one pass.
///
/// This is the full refresh of one perspective: every vector chunk starts from
/// the bias, collects the matching chunk of each active feature row in a
/// register and is stored once, instead of one load-modify-store pass over the
/// whole accumulator per row. Wrapping `i16` addition is associative and
/// commutative, so the result is bit-identical to adding the rows one by one.
///
/// `bias` and every row must be at least `dst.len()` long.
pub(crate) fn refresh_rows(dst: &mut [i16], bias: &[i16], rows: &[&[i16]]) {
    let n = dst.len();
    assert!(
        bias.len() >= n && rows.iter().all(|row| row.len() >= n),
        "accumulator rows must cover the destination"
    );
    match kernel() {
        KernelKind::Scalar => scalar::refresh_rows(dst, bias, rows),
        // SAFETY: the backend was selected from the detected (or forced and
        // verified) instruction set, and every slice covers `n`.
        #[cfg(target_arch = "x86_64")]
        KernelKind::Sse41 => unsafe { wide::refresh_rows_sse41(dst, bias, rows) },
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx2 => unsafe { wide::refresh_rows_avx2(dst, bias, rows) },
        #[cfg(target_arch = "x86_64")]
        KernelKind::Avx512 => unsafe { wide::refresh_rows_avx512(dst, bias, rows) },
        // SAFETY: NEON is part of the AArch64 baseline, so the selected
        // backend always runs on the host; every slice covers `n`.
        #[cfg(target_arch = "aarch64")]
        KernelKind::Neon => unsafe { neon::refresh_rows(dst, bias, rows) },
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
    assert!(
        pair[1].len() == pair[0].len() && output_weights.len() >= 2 * pair[0].len(),
        "the output weights must cover both accumulators"
    );
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
        // SAFETY: NEON is part of the AArch64 baseline, and the accumulator
        // pair and the output weights were validated by the caller.
        #[cfg(target_arch = "aarch64")]
        KernelKind::Neon => unsafe { neon::evaluate_head(stm, pair, output_weights, output_bias) },
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

    /// `dst = bias + sum(rows)` with wrapping `i16` arithmetic.
    pub(crate) fn refresh_rows(dst: &mut [i16], bias: &[i16], rows: &[&[i16]]) {
        for index in 0..dst.len() {
            let mut value = bias[index];
            for row in rows {
                value = value.wrapping_add(row[index]);
            }
            dst[index] = value;
        }
    }
    /// `dst = src - sum(subs) + sum(adds)` with wrapping `i16` arithmetic.
    pub(crate) fn push_rows(dst: &mut [i16], src: &[i16], adds: &[&[i16]], subs: &[&[i16]]) {
        for index in 0..dst.len() {
            let mut value = src[index];
            for row in subs {
                value = value.wrapping_sub(row[index]);
            }
            for row in adds {
                value = value.wrapping_add(row[index]);
            }
            dst[index] = value;
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
    //! The 128-bit, 256-bit and 512-bit x86 kernels.
    //!
    //! The head computes per element `v = (clamp(a)²) >> 8` exactly in 16-bit
    //! lanes (the clamped square of a `QA = 255` value fits an unsigned 16-bit
    //! lane), multiplies by the 16-bit output weight with `madd` and
    //! accumulates into `i32` lanes that fold into an `i64` total on a fixed
    //! schedule. The fold bounds keep every `i32` partial sum far below
    //! overflow, so the result is bit-identical to the scalar `i64`
    //! accumulation.

    use std::arch::x86_64::*;

    use super::{output, screlu};
    use crate::types::color::Color;

    /// Generates one output-head kernel for a vector width.
    ///
    /// The head needs `v = clamp(a, 0, QA)^2 >> 8` per element, multiplied by
    /// the output weight and summed. With `QA = 255` (the loader rejects any
    /// other network) the clamped square is at most 65,025 and fits an
    /// unsigned 16-bit lane, so `mullo_epi16` gives it exactly and a logical
    /// right shift by 8 leaves `v <= 254`, which fits a signed 16-bit lane.
    /// `madd_epi16` then multiplies `v` by the 16-bit weight and adds adjacent
    /// pairs into 32-bit lanes with no widening step. A lane gains at most
    /// `2 x 254 x 32768` per half per iteration, so folding into the `i64`
    /// total every 32 iterations keeps every partial sum below 1.1 G, far under
    /// `i32` overflow: the result is bit-identical to the scalar `i64`
    /// accumulation. A scalar loop finishes any tail narrower than one vector.
    macro_rules! head_kernel {
        ($name:ident, $feature:literal, $lanes:expr, $vector:ty, $load:ident,
         $setzero:ident, $set1:ident, $max:ident, $min:ident, $mullo:ident,
         $srli:ident, $madd:ident, $add32:ident, $fold:ident) => {
            /// # Safety
            ///
            /// The host CPU must support the instruction set named in the
            /// attribute, and `output_weights` must hold at least twice the
            /// accumulator width.
            #[target_feature(enable = $feature)]
            pub(super) unsafe fn $name(
                stm: Color,
                pair: &[Vec<i16>; 2],
                output_weights: &[i16],
                output_bias: i32,
            ) -> i64 {
                let stm_acc = &pair[stm as usize];
                let ntm_acc = &pair[(stm ^ 1) as usize];
                let l1 = stm_acc.len();
                let zero = $setzero();
                let qa = $set1(crate::eval::nnue::net::QA as i16);
                let mut total = i64::from(output_bias);
                let mut lanes = zero;
                let mut pending = 0_u32;
                let mut offset = 0;
                while offset + $lanes <= l1 {
                    // The stm rows and the ntm rows live in the two halves of
                    // the output weights; each chunk pairs its lanes with its
                    // own half.
                    for (accumulator, weight_base) in [(stm_acc, offset), (ntm_acc, l1 + offset)] {
                        unsafe {
                            let a = $load(accumulator.as_ptr().add(offset) as *const $vector);
                            let clamped = $min($max(a, zero), qa);
                            let v = $srli::<8>($mullo(clamped, clamped));
                            let w =
                                $load(output_weights.as_ptr().add(weight_base) as *const $vector);
                            lanes = $add32(lanes, $madd(v, w));
                        }
                    }
                    offset += $lanes;
                    pending += 1;
                    if pending == 32 {
                        total += $fold(lanes);
                        lanes = zero;
                        pending = 0;
                    }
                }
                total += $fold(lanes);
                for index in offset..l1 {
                    let a = stm_acc[index];
                    if a > 0 {
                        total += i64::from(output(screlu(a))) * i64::from(output_weights[index]);
                    }
                    let b = ntm_acc[index];
                    if b > 0 {
                        total +=
                            i64::from(output(screlu(b))) * i64::from(output_weights[l1 + index]);
                    }
                }
                total
            }
        };
    }

    head_kernel!(
        evaluate_head_sse41,
        "sse4.1",
        8,
        __m128i,
        _mm_loadu_si128,
        _mm_setzero_si128,
        _mm_set1_epi16,
        _mm_max_epi16,
        _mm_min_epi16,
        _mm_mullo_epi16,
        _mm_srli_epi16,
        _mm_madd_epi16,
        _mm_add_epi32,
        fold_lanes128
    );
    head_kernel!(
        evaluate_head_avx2,
        "avx2",
        16,
        __m256i,
        _mm256_loadu_si256,
        _mm256_setzero_si256,
        _mm256_set1_epi16,
        _mm256_max_epi16,
        _mm256_min_epi16,
        _mm256_mullo_epi16,
        _mm256_srli_epi16,
        _mm256_madd_epi16,
        _mm256_add_epi32,
        fold_lanes
    );
    head_kernel!(
        evaluate_head_avx512,
        "avx512f,avx512bw,avx512vl",
        32,
        __m512i,
        _mm512_loadu_si512,
        _mm512_setzero_si512,
        _mm512_set1_epi16,
        _mm512_max_epi16,
        _mm512_min_epi16,
        _mm512_mullo_epi16,
        _mm512_srli_epi16,
        _mm512_madd_epi16,
        _mm512_add_epi32,
        fold_lanes512
    );

    /// Generates one fused row-update kernel: `dst = src - sum(subs) + sum(adds)`.
    ///
    /// The shape (`A` rows added, `S` rows subtracted) is a const generic so
    /// the row loops unroll. Each vector chunk is loaded once, updated in a
    /// register and stored once; a scalar loop finishes any tail narrower than
    /// one vector.
    macro_rules! push_rows_kernel {
        ($name:ident, $feature:literal, $lanes:expr, $load:ident, $store:ident,
         $add:ident, $sub:ident, $vector:ty) => {
            /// # Safety
            ///
            /// The host CPU must support the instruction set named in the
            /// attribute, and `src` and every row must hold at least
            /// `dst.len()` elements.
            #[target_feature(enable = $feature)]
            pub(super) unsafe fn $name<const A: usize, const S: usize>(
                dst: &mut [i16],
                src: &[i16],
                adds: &[&[i16]; A],
                subs: &[&[i16]; S],
            ) {
                let n = dst.len();
                let mut offset = 0;
                while offset + $lanes <= n {
                    unsafe {
                        let mut value = $load(src.as_ptr().add(offset) as *const $vector);
                        for row in subs {
                            value = $sub(value, $load(row.as_ptr().add(offset) as *const $vector));
                        }
                        for row in adds {
                            value = $add(value, $load(row.as_ptr().add(offset) as *const $vector));
                        }
                        $store(dst.as_mut_ptr().add(offset) as *mut $vector, value);
                    }
                    offset += $lanes;
                }
                for index in offset..n {
                    let mut value = src[index];
                    for row in subs {
                        value = value.wrapping_sub(row[index]);
                    }
                    for row in adds {
                        value = value.wrapping_add(row[index]);
                    }
                    dst[index] = value;
                }
            }
        };
    }

    /// Generates one full-refresh kernel: `dst = bias + sum(rows)`.
    ///
    /// The chunk loop is outermost, so each vector chunk stays in a register
    /// while every row contributes to it; a scalar loop finishes any tail
    /// narrower than one vector.
    macro_rules! refresh_rows_kernel {
        ($name:ident, $feature:literal, $lanes:expr, $load:ident, $store:ident,
         $add:ident, $vector:ty) => {
            /// # Safety
            ///
            /// The host CPU must support the instruction set named in the
            /// attribute, and `bias` and every row must hold at least
            /// `dst.len()` elements.
            #[target_feature(enable = $feature)]
            pub(super) unsafe fn $name(dst: &mut [i16], bias: &[i16], rows: &[&[i16]]) {
                let n = dst.len();
                let mut offset = 0;
                while offset + $lanes <= n {
                    unsafe {
                        let mut value = $load(bias.as_ptr().add(offset) as *const $vector);
                        for row in rows {
                            value = $add(value, $load(row.as_ptr().add(offset) as *const $vector));
                        }
                        $store(dst.as_mut_ptr().add(offset) as *mut $vector, value);
                    }
                    offset += $lanes;
                }
                for index in offset..n {
                    let mut value = bias[index];
                    for row in rows {
                        value = value.wrapping_add(row[index]);
                    }
                    dst[index] = value;
                }
            }
        };
    }

    refresh_rows_kernel!(
        refresh_rows_sse41,
        "sse4.1",
        8,
        _mm_loadu_si128,
        _mm_storeu_si128,
        _mm_add_epi16,
        __m128i
    );
    refresh_rows_kernel!(
        refresh_rows_avx2,
        "avx2",
        16,
        _mm256_loadu_si256,
        _mm256_storeu_si256,
        _mm256_add_epi16,
        __m256i
    );
    refresh_rows_kernel!(
        refresh_rows_avx512,
        "avx512f,avx512bw,avx512vl",
        32,
        _mm512_loadu_si512,
        _mm512_storeu_si512,
        _mm512_add_epi16,
        __m512i
    );
    push_rows_kernel!(
        push_rows_sse41,
        "sse4.1",
        8,
        _mm_loadu_si128,
        _mm_storeu_si128,
        _mm_add_epi16,
        _mm_sub_epi16,
        __m128i
    );
    push_rows_kernel!(
        push_rows_avx2,
        "avx2",
        16,
        _mm256_loadu_si256,
        _mm256_storeu_si256,
        _mm256_add_epi16,
        _mm256_sub_epi16,
        __m256i
    );
    push_rows_kernel!(
        push_rows_avx512,
        "avx512f,avx512bw,avx512vl",
        32,
        _mm512_loadu_si512,
        _mm512_storeu_si512,
        _mm512_add_epi16,
        _mm512_sub_epi16,
        __m512i
    );
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

#[cfg(target_arch = "aarch64")]
pub(crate) mod neon {
    //! The 128-bit AArch64 kernels.
    //!
    //! NEON is part of the AArch64 baseline, so every Apple M processor runs
    //! these kernels and no runtime feature test is needed. The row kernels
    //! use the same fused loop shape as the x86 kernels: one load of the
    //! source chunk, the row additions/subtractions in registers, one store.
    //! Wrapping `i16` addition is associative and commutative, so the result
    //! is bit-identical to the scalar oracle.
    //!
    //! The head reproduces the scalar chain exactly: the clamped activation is
    //! squared in 16-bit lanes (its maximum 255^2 = 65,025 still fits an
    //! unsigned 16-bit lane), shifted right by eight bits (leaving at most
    //! 254), widened with `vmlal_s16` and accumulated in `i32` lanes. The
    //! lanes fold into the `i64` total every 16 vectors, which bounds every
    //! partial sum far below `i32` overflow, so the total equals the scalar
    //! `i64` sum exactly.

    use core::arch::aarch64::*;

    use super::{output, screlu};
    use crate::types::color::Color;

    /// Number of `i16` lanes in one 128-bit NEON vector.
    const LANES: usize = 8;

    /// Vectors accumulated in the `i32` lanes before folding into the `i64`
    /// total. Two perspectives contribute per iteration, so each lane sees at
    /// most `2 * 2 * FOLD_EVERY` products of `254 * 32768`, about `2^29`:
    /// far below `i32` overflow.
    const FOLD_EVERY: u32 = 16;

    /// `dst = src - sum(subs) + sum(adds)`, eight lanes at a time.
    ///
    /// # Safety
    ///
    /// `src` and every row must hold at least `dst.len()` elements.
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn push_rows<const A: usize, const S: usize>(
        dst: &mut [i16],
        src: &[i16],
        adds: &[&[i16]; A],
        subs: &[&[i16]; S],
    ) {
        let n = dst.len();
        let mut offset = 0;
        while offset + LANES <= n {
            unsafe {
                let mut value = vld1q_s16(src.as_ptr().add(offset));
                for row in subs {
                    value = vsubq_s16(value, vld1q_s16(row.as_ptr().add(offset)));
                }
                for row in adds {
                    value = vaddq_s16(value, vld1q_s16(row.as_ptr().add(offset)));
                }
                vst1q_s16(dst.as_mut_ptr().add(offset), value);
            }
            offset += LANES;
        }
        for index in offset..n {
            let mut value = src[index];
            for row in subs {
                value = value.wrapping_sub(row[index]);
            }
            for row in adds {
                value = value.wrapping_add(row[index]);
            }
            dst[index] = value;
        }
    }

    /// `dst = bias + sum(rows)`, eight lanes at a time.
    ///
    /// # Safety
    ///
    /// `bias` and every row must hold at least `dst.len()` elements.
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn refresh_rows(dst: &mut [i16], bias: &[i16], rows: &[&[i16]]) {
        let n = dst.len();
        let mut offset = 0;
        while offset + LANES <= n {
            unsafe {
                let mut value = vld1q_s16(bias.as_ptr().add(offset));
                for row in rows {
                    value = vaddq_s16(value, vld1q_s16(row.as_ptr().add(offset)));
                }
                vst1q_s16(dst.as_mut_ptr().add(offset), value);
            }
            offset += LANES;
        }
        for index in offset..n {
            let mut value = bias[index];
            for row in rows {
                value = value.wrapping_add(row[index]);
            }
            dst[index] = value;
        }
    }

    /// The SCReLU output head over both perspectives.
    ///
    /// # Safety
    ///
    /// `output_weights` must hold at least twice the accumulator width.
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn evaluate_head(
        stm: Color,
        pair: &[Vec<i16>; 2],
        output_weights: &[i16],
        output_bias: i32,
    ) -> i64 {
        let stm_acc = &pair[stm as usize];
        let ntm_acc = &pair[(stm ^ 1) as usize];
        let l1 = stm_acc.len();
        let zero = vdupq_n_s16(0);
        let qa = vdupq_n_s16(crate::eval::nnue::net::QA as i16);
        let mut total = i64::from(output_bias);
        let mut lanes = vdupq_n_s32(0);
        let mut pending = 0_u32;
        let mut offset = 0;
        while offset + LANES <= l1 {
            // The two perspectives read their own half of the output weights.
            for (accumulator, weight_base) in [(stm_acc, offset), (ntm_acc, l1 + offset)] {
                unsafe {
                    let a = vld1q_s16(accumulator.as_ptr().add(offset));
                    let clamped = vminq_s16(vmaxq_s16(a, zero), qa);
                    // The clamped square is at most 65,025, so the unsigned
                    // 16-bit product is exact and the shift leaves <= 254.
                    let squared = vreinterpretq_s16_u16(vshrq_n_u16::<8>(vreinterpretq_u16_s16(
                        vmulq_s16(clamped, clamped),
                    )));
                    let w = vld1q_s16(output_weights.as_ptr().add(weight_base));
                    lanes = vmlal_s16(lanes, vget_low_s16(squared), vget_low_s16(w));
                    lanes = vmlal_s16(lanes, vget_high_s16(squared), vget_high_s16(w));
                }
            }
            offset += LANES;
            pending += 1;
            if pending == FOLD_EVERY {
                total += fold_lanes(lanes);
                lanes = vdupq_n_s32(0);
                pending = 0;
            }
        }
        total += fold_lanes(lanes);
        for index in offset..l1 {
            let a = stm_acc[index];
            if a > 0 {
                total += i64::from(output(screlu(a))) * i64::from(output_weights[index]);
            }
            let b = ntm_acc[index];
            if b > 0 {
                total += i64::from(output(screlu(b))) * i64::from(output_weights[l1 + index]);
            }
        }
        total
    }

    /// Horizontal sum of the four `i32` lanes into the `i64` total.
    fn fold_lanes(lanes: int32x4_t) -> i64 {
        let mut out = [0_i32; 4];
        unsafe { vst1q_s32(out.as_mut_ptr(), lanes) };
        out.iter().map(|value| i64::from(*value)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_head, push_rows, refresh_rows, scalar};
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

    /// Builds `count` random rows of width `n`.
    fn random_rows(rng: &mut Lcg, count: usize, n: usize) -> Vec<Vec<i16>> {
        (0..count)
            .map(|_| (0..n).map(|_| rng.i16()).collect())
            .collect()
    }

    #[test]
    fn the_fused_row_update_matches_copy_then_apply() {
        let mut rng = Lcg(0xFACE);
        // Widths cover the production size, a multiple of 16 that is not a
        // multiple of 32, and an awkward tail.
        for n in [512, 48, 37] {
            for (add_count, sub_count) in [(1, 1), (1, 2), (2, 2), (0, 1), (3, 3)] {
                let src: Vec<i16> = (0..n).map(|_| rng.i16()).collect();
                let adds = random_rows(&mut rng, add_count, n);
                let subs = random_rows(&mut rng, sub_count, n);
                let add_refs: Vec<&[i16]> = adds.iter().map(Vec::as_slice).collect();
                let sub_refs: Vec<&[i16]> = subs.iter().map(Vec::as_slice).collect();

                // The obviously-correct oracle: copy, then one row at a time.
                let mut expected = src.clone();
                for row in &subs {
                    for (value, weight) in expected.iter_mut().zip(row) {
                        *value = value.wrapping_sub(*weight);
                    }
                }
                for row in &adds {
                    for (value, weight) in expected.iter_mut().zip(row) {
                        *value = value.wrapping_add(*weight);
                    }
                }

                let mut dispatched = vec![0_i16; n];
                push_rows(&mut dispatched, &src, &add_refs, &sub_refs);
                assert_eq!(
                    dispatched, expected,
                    "dispatched n={n} {add_count}/{sub_count}"
                );

                let mut reference = vec![0_i16; n];
                scalar::push_rows(&mut reference, &src, &add_refs, &sub_refs);
                assert_eq!(reference, expected, "scalar n={n} {add_count}/{sub_count}");
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn every_supported_fused_kernel_matches_the_scalar_reference() {
        use super::wide;
        let mut rng = Lcg(0xBEEF);
        for n in [512, 48, 37, 16] {
            let src: Vec<i16> = (0..n).map(|_| rng.i16()).collect();
            let rows = random_rows(&mut rng, 4, n);
            let (a1, a2) = (rows[0].as_slice(), rows[1].as_slice());
            let (s1, s2) = (rows[2].as_slice(), rows[3].as_slice());
            let mut expected = vec![0_i16; n];
            scalar::push_rows(&mut expected, &src, &[a1, a2], &[s1, s2]);

            // Each kernel runs only on a host that supports it.
            if is_x86_feature_detected!("sse4.1") {
                let mut out = vec![0_i16; n];
                unsafe { wide::push_rows_sse41::<2, 2>(&mut out, &src, &[a1, a2], &[s1, s2]) };
                assert_eq!(out, expected, "sse4.1 n={n}");
            }
            if is_x86_feature_detected!("avx2") {
                let mut out = vec![0_i16; n];
                unsafe { wide::push_rows_avx2::<2, 2>(&mut out, &src, &[a1, a2], &[s1, s2]) };
                assert_eq!(out, expected, "avx2 n={n}");
            }
            if is_x86_feature_detected!("avx512f")
                && is_x86_feature_detected!("avx512bw")
                && is_x86_feature_detected!("avx512vl")
            {
                let mut out = vec![0_i16; n];
                unsafe { wide::push_rows_avx512::<2, 2>(&mut out, &src, &[a1, a2], &[s1, s2]) };
                assert_eq!(out, expected, "avx512 n={n}");
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn every_supported_head_kernel_matches_the_scalar_reference() {
        use super::wide;
        type Head = unsafe fn(u8, &[Vec<i16>; 2], &[i16], i32) -> i64;
        let mut kernels: Vec<(&str, Head)> = Vec::new();
        if is_x86_feature_detected!("sse4.1") {
            kernels.push(("sse4.1", wide::evaluate_head_sse41));
        }
        if is_x86_feature_detected!("avx2") {
            kernels.push(("avx2", wide::evaluate_head_avx2));
        }
        if is_x86_feature_detected!("avx512f")
            && is_x86_feature_detected!("avx512bw")
            && is_x86_feature_detected!("avx512vl")
        {
            kernels.push(("avx512", wide::evaluate_head_avx512));
        }

        let mut rng = Lcg(0xD00D);
        // Random contents at several widths (including tails narrower than a
        // vector), then the worst case for the 32-bit lanes at the largest
        // width the loader accepts: every activation saturated and every
        // weight at an `i16` extreme, in both signs.
        struct Case {
            first: Vec<i16>,
            second: Vec<i16>,
            output_weights: Vec<i16>,
        }
        let mut cases: Vec<Case> = Vec::new();
        for l1 in [16, 48, 64, 512, 1040] {
            cases.push(Case {
                first: (0..l1).map(|_| rng.i16()).collect(),
                second: (0..l1).map(|_| rng.i16()).collect(),
                output_weights: (0..2 * l1).map(|_| rng.i16()).collect(),
            });
        }
        for weight in [i16::MAX, i16::MIN] {
            let l1 = 2048;
            cases.push(Case {
                first: vec![300; l1],
                second: vec![255; l1],
                output_weights: vec![weight; 2 * l1],
            });
        }

        for case in cases {
            let l1 = case.first.len();
            let output_weights = case.output_weights;
            let pair = [case.first, case.second];
            for stm in [WHITE, BLACK] {
                let expected = scalar::evaluate_head(stm, &pair, &output_weights, -777);
                for (name, kernel) in &kernels {
                    let got = unsafe { kernel(stm, &pair, &output_weights, -777) };
                    assert_eq!(got, expected, "{name} head diverges at l1 {l1}, side {stm}");
                }
            }
        }
    }

    #[test]
    fn the_fused_refresh_matches_adding_the_rows_one_by_one() {
        let mut rng = Lcg(0xC0DE);
        for n in [512, 48, 37] {
            for row_count in [0, 1, 2, 7, 30, 32] {
                let bias: Vec<i16> = (0..n).map(|_| rng.i16()).collect();
                let rows = random_rows(&mut rng, row_count, n);
                let refs: Vec<&[i16]> = rows.iter().map(Vec::as_slice).collect();

                let mut expected = bias.clone();
                for row in &rows {
                    for (value, weight) in expected.iter_mut().zip(row) {
                        *value = value.wrapping_add(*weight);
                    }
                }

                let mut dispatched = vec![0_i16; n];
                refresh_rows(&mut dispatched, &bias, &refs);
                assert_eq!(dispatched, expected, "dispatched n={n} rows={row_count}");

                let mut reference = vec![0_i16; n];
                scalar::refresh_rows(&mut reference, &bias, &refs);
                assert_eq!(reference, expected, "scalar n={n} rows={row_count}");

                #[cfg(target_arch = "x86_64")]
                {
                    use super::wide;
                    // Each kernel runs only on a host that supports it.
                    if is_x86_feature_detected!("sse4.1") {
                        let mut out = vec![0_i16; n];
                        unsafe { wide::refresh_rows_sse41(&mut out, &bias, &refs) };
                        assert_eq!(out, expected, "sse4.1 n={n} rows={row_count}");
                    }
                    if is_x86_feature_detected!("avx2") {
                        let mut out = vec![0_i16; n];
                        unsafe { wide::refresh_rows_avx2(&mut out, &bias, &refs) };
                        assert_eq!(out, expected, "avx2 n={n} rows={row_count}");
                    }
                    if is_x86_feature_detected!("avx512f")
                        && is_x86_feature_detected!("avx512bw")
                        && is_x86_feature_detected!("avx512vl")
                    {
                        let mut out = vec![0_i16; n];
                        unsafe { wide::refresh_rows_avx512(&mut out, &bias, &refs) };
                        assert_eq!(out, expected, "avx512 n={n} rows={row_count}");
                    }
                }
            }
        }
    }

    /// The NEON backend must match the scalar oracle at production widths,
    /// at widths with a tail narrower than one vector, and in the worst case
    /// for the 32-bit head lanes (saturated activations and extreme weights).
    #[cfg(target_arch = "aarch64")]
    #[test]
    fn the_neon_kernels_match_the_scalar_reference() {
        use super::neon;

        let mut rng = Lcg(0x0A11);
        for n in [512, 48, 37, 8] {
            let src: Vec<i16> = (0..n).map(|_| rng.i16()).collect();
            let rows = random_rows(&mut rng, 4, n);
            let (a1, a2) = (rows[0].as_slice(), rows[1].as_slice());
            let (s1, s2) = (rows[2].as_slice(), rows[3].as_slice());

            let mut expected = vec![0_i16; n];
            scalar::push_rows(&mut expected, &src, &[a1, a2], &[s1, s2]);
            let mut out = vec![0_i16; n];
            unsafe { neon::push_rows::<2, 2>(&mut out, &src, &[a1, a2], &[s1, s2]) };
            assert_eq!(out, expected, "neon push_rows n={n}");

            // The full refresh over a row count that crosses the vector width.
            let mut expected = vec![0_i16; n];
            scalar::refresh_rows(&mut expected, &src, &[a1, a2, s1, s2, s1]);
            let mut out = vec![0_i16; n];
            unsafe { neon::refresh_rows(&mut out, &src, &[a1, a2, s1, s2, s1]) };
            assert_eq!(out, expected, "neon refresh_rows n={n}");
        }

        // The head: random contents first, then the extreme cases.
        for l1 in [8, 16, 48, 512, 1040, 2048] {
            let pair = [
                (0..l1).map(|_| rng.i16()).collect::<Vec<i16>>(),
                (0..l1).map(|_| rng.i16()).collect::<Vec<i16>>(),
            ];
            let weights: Vec<i16> = (0..2 * l1).map(|_| rng.i16()).collect();
            for stm in [WHITE, BLACK] {
                let expected = scalar::evaluate_head(stm, &pair, &weights, -777);
                let got = unsafe { neon::evaluate_head(stm, &pair, &weights, -777) };
                assert_eq!(got, expected, "neon head l1={l1} side={stm}");
            }
        }
        for weight in [i16::MAX, i16::MIN] {
            let l1 = 2048;
            let pair = [vec![300_i16; l1], vec![255_i16; l1]];
            let weights = vec![weight; 2 * l1];
            for stm in [WHITE, BLACK] {
                let expected = scalar::evaluate_head(stm, &pair, &weights, -777);
                let got = unsafe { neon::evaluate_head(stm, &pair, &weights, -777) };
                assert_eq!(
                    got, expected,
                    "neon head worst case weight={weight} side={stm}"
                );
            }
        }
    }
}
