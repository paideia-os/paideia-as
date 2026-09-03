//! `@dma_buffer(size, alignment, coherency)` intrinsic descriptor
//! (v0.27-M1-001, issue #1365).
//!
//! Descriptor-only landing: this module lowers the argument list of the
//! intrinsic call into a typed [`DmaBufferSpec`] and enforces the
//! shape / range invariants documented on
//! `design/kernel/linearity-and-tags.md`. No code emission happens here
//! — the v0.27-M2 encoder rows consume the spec.
//!
//! # Argument grammar
//!
//! `@dma_buffer(<size:int>, <alignment:int>, <coherency:ident>)`
//!
//! - `size`      integer literal, `1..=u64::MAX`, and a multiple of `alignment`.
//! - `alignment` integer literal, power-of-two, `64..=4 MiB`.
//! - `coherency` bare identifier in `{Coherent, NonCoherent, WriteCombining}`.
//!
//! # Diagnostics (`I0100..I0110`)
//!
//! The `I` letter denotes intrinsic-time diagnostics — separate from the
//! `E/P/M/T/…` bands owned by `paideia-as-diagnostics::Category`, because
//! these fire before the parser has stitched the call site into the AST
//! and before the elaborator can attach a source span. See
//! [`IntrinsicErr::code`] for the wire form.
//!
//! # `ExprAst`
//!
//! A minimal argument-expression shape lives here rather than in the AST
//! crate so the descriptor is testable in isolation and does not force a
//! particular arena-lowering choice on the parser (that choice lands with
//! the M2 encoder wiring). The call-site adapter — v0.27-M2 — normalises
//! `paideia_as_ast::ExprData::Literal` / `ExprData::Path` (single-segment)
//! nodes into this shape before invoking [`parse_intrinsic_call`].

use core::fmt;

/// Minimum DMA-buffer alignment (bytes).
///
/// 64 B matches the cache-line size on every T14-G4-class x86_64 target
/// and the smallest useful stride for streaming DMA on Zen 4 / Raptor Lake.
pub const MIN_ALIGN: u64 = 64;

/// Maximum DMA-buffer alignment (bytes) — 4 MiB.
///
/// Ceilings at the 2 MiB huge-page × 2 boundary; anything larger is
/// almost certainly a user error (would require 1 GiB huge-page backing
/// which the DMA path does not yet support).
pub const MAX_ALIGN: u64 = 4 * 1024 * 1024;

/// Coherency policy requested for a DMA buffer.
///
/// The three variants correspond one-to-one to the DMA-API contract
/// documented in `design/kernel/linearity-and-tags.md` §KIND_DMA_BUFFER
/// and select the backing MTRR / PAT attribute at allocation time.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Coherency {
    /// Hardware-coherent DMA — CPU and device see the same memory image
    /// without explicit fences (typical on modern x86_64 with IOMMU
    /// enabled).
    Coherent,
    /// Non-coherent DMA — explicit
    /// `dma_sync_for_{cpu,device}` fences required around every access.
    NonCoherent,
    /// Write-combining (PAT `WC`) — bursts CPU stores into large
    /// transactions; readback is uncached. Used for framebuffer / GPU
    /// upload paths where the CPU only writes.
    WriteCombining,
}

impl Coherency {
    /// Parse an identifier (the third `@dma_buffer` argument) into a variant.
    ///
    /// Returns `None` for unknown identifiers; callers translate that into
    /// [`IntrinsicErr::UnknownCoherency`].
    #[must_use]
    pub fn from_ident(s: &str) -> Option<Self> {
        match s {
            "Coherent" => Some(Self::Coherent),
            "NonCoherent" => Some(Self::NonCoherent),
            "WriteCombining" => Some(Self::WriteCombining),
            _ => None,
        }
    }
}

/// Parsed `@dma_buffer` descriptor.
///
/// Field invariants (enforced by [`parse_intrinsic_call`]):
///
/// - `size >= 1`,
/// - `align.is_power_of_two()` and `MIN_ALIGN <= align <= MAX_ALIGN`,
/// - `size % align == 0`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DmaBufferSpec {
    /// Requested buffer size in bytes.
    pub size: u64,
    /// Requested alignment in bytes (power of two, 64..=4 MiB).
    pub align: u64,
    /// Coherency policy.
    pub coherency: Coherency,
}

/// Minimal argument-expression shape accepted by [`parse_intrinsic_call`].
///
/// Kept local to the intrinsic crate — the AST-level `ExprData` is much
/// larger, and forcing the descriptor to walk an arena would make it
/// awkward to test in isolation. The call-site adapter (v0.27-M2) is
/// responsible for translating `paideia_as_ast::ExprData::Literal` and
/// single-segment `ExprData::Path` nodes into this enum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExprAst {
    /// Integer literal (u128 preserves overflow detection past `u64::MAX`).
    Integer(u128),
    /// Bare single-segment identifier (e.g. `Coherent`).
    Ident(String),
}

/// Intrinsic-time error codes for `@dma_buffer` (`I0100..I0110`).
///
/// Each variant maps to exactly one stable wire code via [`Self::code`].
/// The `I` band is intrinsic-owned and does not overlap the
/// `paideia-as-diagnostics::Category` letters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntrinsicErr {
    /// `I0100` — wrong argument count (expected 3).
    ArityMismatch {
        /// Expected argument count (always 3 for `@dma_buffer`).
        expected: usize,
        /// Actual argument count observed at the call site.
        got: usize,
    },
    /// `I0101` — `size` argument is not an integer literal.
    NonLiteralSize,
    /// `I0102` — `size` exceeds `u64::MAX`.
    SizeOverflow,
    /// `I0103` — `alignment` argument is not an integer literal.
    NonLiteralAlign,
    /// `I0104` — `alignment` is not a power of two.
    AlignNotPowerOfTwo(u64),
    /// `I0105` — `alignment` is below the 64-byte floor.
    AlignBelowMin {
        /// Value observed at the call site.
        got: u64,
        /// The floor ([`MIN_ALIGN`]).
        min: u64,
    },
    /// `I0106` — `alignment` is above the 4 MiB ceiling.
    AlignAboveMax {
        /// Value observed at the call site.
        got: u64,
        /// The ceiling ([`MAX_ALIGN`]).
        max: u64,
    },
    /// `I0107` — `coherency` argument is not a bare identifier.
    NonIdentCoherency,
    /// `I0108` — coherency identifier is not one of
    /// `{Coherent, NonCoherent, WriteCombining}`.
    UnknownCoherency(String),
    /// `I0109` — `size` is zero (rejected: a zero-sized DMA buffer is
    /// almost always a user mistake and would collide with tag `null` in
    /// the linearity table).
    SizeZero,
    /// `I0110` — `size` is not a whole multiple of `alignment`.
    SizeNotAlignedMultiple {
        /// `size` observed at the call site.
        size: u64,
        /// `alignment` observed at the call site.
        align: u64,
    },
}

impl IntrinsicErr {
    /// Stable wire-form code (`"I0100"`..`"I0110"`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ArityMismatch { .. } => "I0100",
            Self::NonLiteralSize => "I0101",
            Self::SizeOverflow => "I0102",
            Self::NonLiteralAlign => "I0103",
            Self::AlignNotPowerOfTwo(_) => "I0104",
            Self::AlignBelowMin { .. } => "I0105",
            Self::AlignAboveMax { .. } => "I0106",
            Self::NonIdentCoherency => "I0107",
            Self::UnknownCoherency(_) => "I0108",
            Self::SizeZero => "I0109",
            Self::SizeNotAlignedMultiple { .. } => "I0110",
        }
    }
}

impl fmt::Display for IntrinsicErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArityMismatch { expected, got } => {
                write!(
                    f,
                    "{}: @dma_buffer expects {} arguments, got {}",
                    self.code(),
                    expected,
                    got
                )
            }
            Self::NonLiteralSize => {
                write!(f, "{}: `size` must be an integer literal", self.code())
            }
            Self::SizeOverflow => {
                write!(f, "{}: `size` exceeds u64::MAX", self.code())
            }
            Self::NonLiteralAlign => write!(
                f,
                "{}: `alignment` must be an integer literal",
                self.code()
            ),
            Self::AlignNotPowerOfTwo(n) => write!(
                f,
                "{}: `alignment` {} is not a power of two",
                self.code(),
                n
            ),
            Self::AlignBelowMin { got, min } => write!(
                f,
                "{}: `alignment` {} below floor {}",
                self.code(),
                got,
                min
            ),
            Self::AlignAboveMax { got, max } => write!(
                f,
                "{}: `alignment` {} above ceiling {}",
                self.code(),
                got,
                max
            ),
            Self::NonIdentCoherency => write!(
                f,
                "{}: `coherency` must be a bare identifier",
                self.code()
            ),
            Self::UnknownCoherency(s) => write!(
                f,
                "{}: unknown coherency variant `{}` (expected Coherent, NonCoherent, or WriteCombining)",
                self.code(),
                s
            ),
            Self::SizeZero => write!(f, "{}: `size` must be non-zero", self.code()),
            Self::SizeNotAlignedMultiple { size, align } => write!(
                f,
                "{}: `size` {} is not a multiple of `alignment` {}",
                self.code(),
                size,
                align
            ),
        }
    }
}

/// Parse the argument list of an `@dma_buffer(size, alignment, coherency)`
/// intrinsic call into a [`DmaBufferSpec`].
///
/// Validation order — deterministic so downstream tests and rustdoc
/// snapshots stay stable across refactors:
///
/// 1. arity (`I0100`)
/// 2. `size` shape (`I0101`) → overflow (`I0102`) → non-zero (`I0109`)
/// 3. `align` shape (`I0103`) → power-of-two (`I0104`) → floor (`I0105`)
///    → ceiling (`I0106`)
/// 4. `size % align == 0` (`I0110`)
/// 5. `coherency` shape (`I0107`) → variant lookup (`I0108`)
///
/// Placing `size % align` before the coherency lookup keeps the failure
/// message pointing at the numeric block the caller most likely mistyped
/// — coherency errors are usually independent typos.
///
/// # Errors
///
/// Returns [`IntrinsicErr`] on the first invariant that fails. Only one
/// error is returned per call — the caller (v0.27-M2 elaborator wiring)
/// forwards it to the diagnostic sink.
pub fn parse_intrinsic_call(args: &[ExprAst]) -> Result<DmaBufferSpec, IntrinsicErr> {
    if args.len() != 3 {
        return Err(IntrinsicErr::ArityMismatch {
            expected: 3,
            got: args.len(),
        });
    }

    // (2) size.
    let size = match &args[0] {
        ExprAst::Integer(n) => {
            if *n > u128::from(u64::MAX) {
                return Err(IntrinsicErr::SizeOverflow);
            }
            let n = *n as u64;
            if n == 0 {
                return Err(IntrinsicErr::SizeZero);
            }
            n
        }
        _ => return Err(IntrinsicErr::NonLiteralSize),
    };

    // (3) alignment.
    let align = match &args[1] {
        ExprAst::Integer(n) => {
            if *n > u128::from(u64::MAX) {
                return Err(IntrinsicErr::AlignAboveMax {
                    got: u64::MAX,
                    max: MAX_ALIGN,
                });
            }
            let n = *n as u64;
            if !n.is_power_of_two() {
                return Err(IntrinsicErr::AlignNotPowerOfTwo(n));
            }
            if n < MIN_ALIGN {
                return Err(IntrinsicErr::AlignBelowMin {
                    got: n,
                    min: MIN_ALIGN,
                });
            }
            if n > MAX_ALIGN {
                return Err(IntrinsicErr::AlignAboveMax {
                    got: n,
                    max: MAX_ALIGN,
                });
            }
            n
        }
        _ => return Err(IntrinsicErr::NonLiteralAlign),
    };

    // (4) size must be a whole multiple of alignment.
    if size % align != 0 {
        return Err(IntrinsicErr::SizeNotAlignedMultiple { size, align });
    }

    // (5) coherency.
    let coherency = match &args[2] {
        ExprAst::Ident(s) => Coherency::from_ident(s)
            .ok_or_else(|| IntrinsicErr::UnknownCoherency(s.clone()))?,
        _ => return Err(IntrinsicErr::NonIdentCoherency),
    };

    Ok(DmaBufferSpec {
        size,
        align,
        coherency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(size: u128, align: u128, coh: &str) -> Result<DmaBufferSpec, IntrinsicErr> {
        parse_intrinsic_call(&[
            ExprAst::Integer(size),
            ExprAst::Integer(align),
            ExprAst::Ident(coh.into()),
        ])
    }

    // --- Happy path -------------------------------------------------------

    #[test]
    fn valid_spec_parses() {
        let spec = call(4096, 64, "Coherent").expect("valid spec must parse");
        assert_eq!(
            spec,
            DmaBufferSpec {
                size: 4096,
                align: 64,
                coherency: Coherency::Coherent,
            }
        );
    }

    #[test]
    fn valid_spec_all_coherency_variants() {
        assert_eq!(
            call(4096, 64, "Coherent").unwrap().coherency,
            Coherency::Coherent
        );
        assert_eq!(
            call(4096, 64, "NonCoherent").unwrap().coherency,
            Coherency::NonCoherent
        );
        assert_eq!(
            call(4096, 64, "WriteCombining").unwrap().coherency,
            Coherency::WriteCombining
        );
    }

    #[test]
    fn max_alignment_accepted() {
        let spec = call(MAX_ALIGN as u128, MAX_ALIGN as u128, "WriteCombining").unwrap();
        assert_eq!(spec.align, MAX_ALIGN);
    }

    // --- I0100 arity ------------------------------------------------------

    #[test]
    fn arity_mismatch_rejected() {
        let err = parse_intrinsic_call(&[ExprAst::Integer(4096), ExprAst::Integer(64)])
            .expect_err("2 args must be rejected");
        assert_eq!(err.code(), "I0100");
        assert!(matches!(
            err,
            IntrinsicErr::ArityMismatch {
                expected: 3,
                got: 2
            }
        ));
    }

    // --- I0101/I0103/I0107 shape -----------------------------------------

    #[test]
    fn non_literal_size_rejected() {
        let err = parse_intrinsic_call(&[
            ExprAst::Ident("foo".into()),
            ExprAst::Integer(64),
            ExprAst::Ident("Coherent".into()),
        ])
        .unwrap_err();
        assert_eq!(err.code(), "I0101");
    }

    #[test]
    fn non_literal_align_rejected() {
        let err = parse_intrinsic_call(&[
            ExprAst::Integer(4096),
            ExprAst::Ident("foo".into()),
            ExprAst::Ident("Coherent".into()),
        ])
        .unwrap_err();
        assert_eq!(err.code(), "I0103");
    }

    #[test]
    fn non_ident_coherency_rejected() {
        let err = parse_intrinsic_call(&[
            ExprAst::Integer(4096),
            ExprAst::Integer(64),
            ExprAst::Integer(1),
        ])
        .unwrap_err();
        assert_eq!(err.code(), "I0107");
    }

    // --- I0102/I0109 size numeric ----------------------------------------

    #[test]
    fn size_overflow_rejected() {
        let err = call(u128::from(u64::MAX) + 1, 64, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0102");
        assert!(matches!(err, IntrinsicErr::SizeOverflow));
    }

    #[test]
    fn size_zero_rejected() {
        let err = call(0, 64, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0109");
    }

    // --- I0104/I0105/I0106 alignment numeric -----------------------------

    #[test]
    fn non_power_of_two_align_rejected() {
        let err = call(4096, 96, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0104");
        assert!(matches!(err, IntrinsicErr::AlignNotPowerOfTwo(96)));
    }

    #[test]
    fn align_below_min_rejected() {
        // 32 is a power of two but below MIN_ALIGN=64.
        let err = call(4096, 32, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0105");
        assert!(matches!(
            err,
            IntrinsicErr::AlignBelowMin { got: 32, min: 64 }
        ));
    }

    #[test]
    fn align_above_max_rejected() {
        // 8 MiB — power of two but above the 4 MiB ceiling.
        let err = call(8 * 1024 * 1024, 8 * 1024 * 1024, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0106");
    }

    #[test]
    fn align_overflow_treated_as_above_max() {
        let err = call(4096, u128::from(u64::MAX) + 1, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0106");
    }

    // --- I0108 coherency variant -----------------------------------------

    #[test]
    fn unknown_coherency_variant_rejected() {
        let err = call(4096, 64, "Cached").unwrap_err();
        assert_eq!(err.code(), "I0108");
        assert!(matches!(err, IntrinsicErr::UnknownCoherency(ref s) if s == "Cached"));
    }

    // --- I0110 size % align ----------------------------------------------

    #[test]
    fn size_not_align_multiple_rejected() {
        // 4097 is not a multiple of 64.
        let err = call(4097, 64, "Coherent").unwrap_err();
        assert_eq!(err.code(), "I0110");
        assert!(matches!(
            err,
            IntrinsicErr::SizeNotAlignedMultiple {
                size: 4097,
                align: 64
            }
        ));
    }

    // --- code() coverage -------------------------------------------------

    #[test]
    fn every_variant_has_distinct_code() {
        let codes = [
            IntrinsicErr::ArityMismatch {
                expected: 3,
                got: 0,
            }
            .code(),
            IntrinsicErr::NonLiteralSize.code(),
            IntrinsicErr::SizeOverflow.code(),
            IntrinsicErr::NonLiteralAlign.code(),
            IntrinsicErr::AlignNotPowerOfTwo(3).code(),
            IntrinsicErr::AlignBelowMin { got: 1, min: 64 }.code(),
            IntrinsicErr::AlignAboveMax {
                got: 1 << 30,
                max: MAX_ALIGN,
            }
            .code(),
            IntrinsicErr::NonIdentCoherency.code(),
            IntrinsicErr::UnknownCoherency("x".into()).code(),
            IntrinsicErr::SizeZero.code(),
            IntrinsicErr::SizeNotAlignedMultiple {
                size: 1,
                align: 64,
            }
            .code(),
        ];
        let mut sorted: Vec<&&str> = codes.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "codes must be distinct");
        // Bounds check: every code must live in the reserved I0100..=I0110 range.
        for c in &codes {
            assert!(c.starts_with('I'));
            let n: u16 = c[1..].parse().unwrap();
            assert!((100..=110).contains(&n), "code {c} out of I0100..I0110 range");
        }
    }
}
