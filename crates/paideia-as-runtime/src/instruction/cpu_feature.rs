//! CPU-feature bits gating specific x86_64 mnemonics.
//!
//! Extracted from the monolithic `instruction.rs` (issue #1402, umbrella #1399).
//! Public path preserved: `paideia_as_runtime::instruction::CpuFeature`.

/// CPU feature bits (x86_64 CPUID subsets) that gate emission of
/// specific mnemonics. Absence of the corresponding root-module
/// `#![target_features = "..."]` declaration → U1612 diagnostic.
///
/// PA-r16-004-backtrack-a (#1033): compile-time CPU-feature declaration + gating mechanism.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CpuFeature {
    /// CPUID.01H:ECX.CMPXCHG16B[bit 13] — LockCmpxchg16b
    Cx16,
    /// CPUID.01H:ECX.POPCNT[bit 23] — Popcnt
    Popcnt,
    /// CPUID.07H:EBX[bit 3] — Tzcnt/Andn
    Bmi1,
    /// CPUID.07H:ECX[bit 23] — Endbr64/Endbr32 (Intel CET)
    Cet,
    /// Reserved for later mnemonics:
    Sse42,
    /// Reserved for later mnemonics:
    Avx,
    /// Reserved for later mnemonics:
    Avx512F,
    /// CPUID.01H:ECX.XSAVE[bit 26] — Xsave/Xrstor
    Xsave,
    /// CPUID.01H:ECX.XSAVEOPT[bit 27] — Xsaveopt (optimized save)
    Xsaveopt,
}

impl CpuFeature {
    /// Parse a CPU feature token string (e.g. "cx16", "popcnt", "bmi1").
    /// Returns `None` for unrecognized tokens.
    #[must_use]
    pub fn from_token(s: &str) -> Option<Self> {
        match s {
            "cx16" => Some(Self::Cx16),
            "popcnt" => Some(Self::Popcnt),
            "bmi1" => Some(Self::Bmi1),
            "cet" => Some(Self::Cet),
            "sse4.2" | "sse42" => Some(Self::Sse42),
            "avx" => Some(Self::Avx),
            "avx512f" => Some(Self::Avx512F),
            "xsave" => Some(Self::Xsave),
            "xsaveopt" => Some(Self::Xsaveopt),
            _ => None,
        }
    }

    /// Return the canonical string form of this feature.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cx16 => "cx16",
            Self::Popcnt => "popcnt",
            Self::Bmi1 => "bmi1",
            Self::Cet => "cet",
            Self::Sse42 => "sse4.2",
            Self::Avx => "avx",
            Self::Avx512F => "avx512f",
            Self::Xsave => "xsave",
            Self::Xsaveopt => "xsaveopt",
        }
    }
}
