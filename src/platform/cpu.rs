//! Runtime instruction-set detection and one-time backend selection.
//!
//! The Windows release is a single executable that contains several
//! instruction-set variants. Detection happens once, before any worker thread
//! starts; the hot paths then use the selected variant without a per-node
//! feature test. The selector is deliberately kept in one small module so the
//! dispatch decisions can be unit-tested on every host.

/// Instruction-set family selected at startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Isa {
    /// Baseline x86-64 with SSE4.2 and POPCNT.
    X86V2,
    /// x86-64-v3: AVX2, BMI2 and FMA.
    X86V3,
    /// x86-64-v3 plus the 256-bit VNNI extension.
    X86V3Vnni,
    /// x86-64-v4 plus the 512-bit VNNI extension.
    X86V4Vnni,
    /// AArch64 with the NEON baseline.
    Aarch64,
}

/// x86 feature flags used by the selector.
///
/// A struct instead of direct macro calls keeps the selection ladder testable
/// on a host that is not x86.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct X86Features {
    /// SSE4.2 support.
    pub sse42: bool,
    /// POPCNT support.
    pub popcnt: bool,
    /// AVX2 support.
    pub avx2: bool,
    /// BMI1/BMI2 support.
    pub bmi2: bool,
    /// AVX-512 foundation.
    pub avx512f: bool,
    /// AVX-512 conflict detection.
    pub avx512cd: bool,
    /// AVX-512 doubleword/quadword instructions.
    pub avx512dq: bool,
    /// AVX-512 byte/word instructions.
    pub avx512bw: bool,
    /// AVX-512 vector-length extensions.
    pub avx512vl: bool,
    /// AVX-512 VNNI.
    pub avx512vnni: bool,
    /// 256-bit AVX-VNNI.
    pub avxvnni: bool,
    /// BMI2 is microcoded on this CPU, so PEXT/PDEP should be avoided.
    pub slow_bmi2: bool,
}

/// Returns the best safe x86 variant for the given feature set.
///
/// The ladder is ordered from most to least capable. A missing feature always
/// falls back to the next lower variant; the lowest variant is the universal
/// safety net.
pub fn select_x86(features: X86Features) -> Isa {
    if !features.sse42 || !features.popcnt {
        return Isa::X86V2;
    }

    if features.avx2
        && features.bmi2
        && features.avx512f
        && features.avx512cd
        && features.avx512dq
        && features.avx512bw
        && features.avx512vl
        && features.avx512vnni
    {
        return Isa::X86V4Vnni;
    }

    if features.avx2 && features.bmi2 && features.avxvnni {
        return Isa::X86V3Vnni;
    }

    if features.avx2 && features.bmi2 {
        return Isa::X86V3;
    }

    Isa::X86V2
}

/// Returns whether PEXT/PDEP should be used for sliding attacks.
///
/// PEXT is fast on Intel Haswell+ and AMD Zen 3+, but microcoded on AMD
/// Excavator and Zen 1/2. The selection is independent of the code-generation
/// variant so a v3 build can still use magic bitboards on those CPUs.
pub fn use_pext(features: X86Features) -> bool {
    features.avx2 && features.bmi2 && !features.slow_bmi2
}

/// Detects the best instruction-set variant for the current process.
///
/// On AArch64 the answer is always [`Isa::Aarch64`]; the Apple Silicon build
/// has a single compile-time feature set and does not need multi-versioning.
pub fn detect_isa() -> Isa {
    #[cfg(target_arch = "x86_64")]
    {
        select_x86(detect_x86())
    }

    #[cfg(target_arch = "aarch64")]
    {
        Isa::Aarch64
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        compile_error!("KnightByte supports x86_64 and aarch64 only");
    }
}

/// Queries the host CPU for the x86 features used by [`select_x86`].
#[cfg(target_arch = "x86_64")]
fn detect_x86() -> X86Features {
    X86Features {
        sse42: std::is_x86_feature_detected!("sse4.2"),
        popcnt: std::is_x86_feature_detected!("popcnt"),
        avx2: std::is_x86_feature_detected!("avx2"),
        bmi2: std::is_x86_feature_detected!("bmi2"),
        avx512f: std::is_x86_feature_detected!("avx512f"),
        avx512cd: std::is_x86_feature_detected!("avx512cd"),
        avx512dq: std::is_x86_feature_detected!("avx512dq"),
        avx512bw: std::is_x86_feature_detected!("avx512bw"),
        avx512vl: std::is_x86_feature_detected!("avx512vl"),
        avx512vnni: std::is_x86_feature_detected!("avx512vnni"),
        avxvnni: std::is_x86_feature_detected!("avxvnni"),
        slow_bmi2: has_slow_bmi2(),
    }
}

/// Returns whether BMI2 is microcoded on this CPU.
///
/// Only AMD Excavator (family 15h) and Zen 1/2 (family 17h) are affected.
/// Every other CPU, including all AArch64 hosts, returns `false`.
pub fn has_slow_bmi2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        use core::arch::x86_64::__cpuid;

        // CPUID leaf 0 is available on every x86_64 CPU and does not touch
        // memory; it only reports the vendor string.
        let vendor = __cpuid(0);
        let is_amd =
            vendor.ebx == 0x6874_7541 && vendor.edx == 0x6974_6E65 && vendor.ecx == 0x444D_4163;
        if !is_amd {
            return false;
        }

        // Leaf 1 is available on every CPU that can execute this code.
        let signature = __cpuid(1);
        let base_family = (signature.eax >> 8) & 0xF;
        let extended_family = (signature.eax >> 20) & 0xFF;
        let family = if base_family == 0xF {
            base_family + extended_family
        } else {
            base_family
        };
        family == 0x15 || family == 0x17
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{Isa, X86Features, detect_isa, has_slow_bmi2, select_x86, use_pext};

    /// A fully capable x86 feature set used as the test baseline.
    fn full_features() -> X86Features {
        X86Features {
            sse42: true,
            popcnt: true,
            avx2: true,
            bmi2: true,
            avx512f: true,
            avx512cd: true,
            avx512dq: true,
            avx512bw: true,
            avx512vl: true,
            avx512vnni: true,
            avxvnni: true,
            slow_bmi2: false,
        }
    }

    /// Removes every AVX-512 flag so the selector falls to the 256-bit rungs.
    fn without_avx512(features: &mut X86Features) {
        features.avx512f = false;
        features.avx512cd = false;
        features.avx512dq = false;
        features.avx512bw = false;
        features.avx512vl = false;
        features.avx512vnni = false;
    }

    #[test]
    fn selects_the_highest_supported_x86_variant() {
        assert_eq!(select_x86(full_features()), Isa::X86V4Vnni);

        let mut features = full_features();
        features.avx512vnni = false;
        assert_eq!(select_x86(features), Isa::X86V3Vnni);

        let mut features = full_features();
        without_avx512(&mut features);
        assert_eq!(select_x86(features), Isa::X86V3Vnni);

        let mut features = full_features();
        without_avx512(&mut features);
        features.avxvnni = false;
        assert_eq!(select_x86(features), Isa::X86V3);
    }

    #[test]
    fn falls_back_to_the_baseline_when_a_required_feature_is_missing() {
        let mut features = full_features();
        features.avx2 = false;
        assert_eq!(select_x86(features), Isa::X86V2);

        let mut features = full_features();
        features.bmi2 = false;
        assert_eq!(select_x86(features), Isa::X86V2);

        let mut features = full_features();
        features.popcnt = false;
        assert_eq!(select_x86(features), Isa::X86V2);
    }

    #[test]
    fn slow_bmi2_disables_pext_without_changing_the_variant() {
        let mut features = full_features();
        features.slow_bmi2 = true;
        assert_eq!(select_x86(features), Isa::X86V4Vnni);
        assert!(!use_pext(features));
        assert!(use_pext(full_features()));
    }

    #[test]
    fn detection_returns_a_supported_variant() {
        let isa = detect_isa();
        #[cfg(target_arch = "aarch64")]
        assert_eq!(isa, Isa::Aarch64);
        #[cfg(target_arch = "x86_64")]
        assert!(matches!(
            isa,
            Isa::X86V2 | Isa::X86V3 | Isa::X86V3Vnni | Isa::X86V4Vnni
        ));
    }

    #[test]
    fn slow_bmi2_detection_does_not_panic() {
        let _ = has_slow_bmi2();
    }
}
