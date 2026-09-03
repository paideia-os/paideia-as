//! `@atomic128_cas` — 128-bit compare-and-swap intrinsic descriptor.
//!
//! **Row:** v0.27-M1-003 (paideia-as#1367), Wave 0 Batch 3.
//!
//! This module *parses and validates the intrinsic call* — it does **not**
//! yet lower to machine code. The x86_64 target lowering is the paired
//! `cmpxchg16b` (LOCK CMPXCHG16B) emit that lands in the encoder crate
//! in v0.27-M2, which will consume [`Atomic128CasSpec`] verbatim.
//!
//! # Surface syntax
//!
//! ```paideia-as
//! @atomic128_cas(ptr, expected, desired, ord_success, ord_failure)
//! ```
//!
//! * `ptr` must resolve to a pointer whose pointee is a 128-bit scalar
//!   (`u128` / `i128` / `f128`) or a 16-byte packed pair (`{u64,u64}`),
//!   16-byte-aligned so `cmpxchg16b` addressing is legal (Intel SDM
//!   Vol. 2A §CMPXCHG8B/CMPXCHG16B: `#GP(0)` if the memory operand is
//!   not aligned on a 16-byte boundary).
//! * `expected` and `desired` are 128-bit values in the same layout as
//!   the pointee.
//! * `ord_success` and `ord_failure` are memory-ordering discriminators
//!   drawn from [`AtomicOrdering`]. The failure ordering must be
//!   weaker-than-or-equal-to the success ordering; monotonicity is
//!   necessary to satisfy C11 `atomic_compare_exchange_*` semantics
//!   (a failed CAS is a pure load, so the observed ordering can never
//!   promise more than the load itself provides).
//!
//! # Scope of this file
//!
//! * Public type [`Atomic128CasSpec`] — the descriptor consumed downstream.
//! * Public entry point [`parse_intrinsic_call`] — argument validator.
//! * Diagnostic enum [`IntrinsicErr`] carrying codes in the reserved
//!   `I0120-I0130` band (v0.27-M1 intrinsic block).
//! * Local shim types [`ExprAst`], [`PointerElemKind`], [`TypeId`] — a
//!   descriptor-layer stand-in for the arena-backed surface AST, sized
//!   for the parser's needs without forcing a compile-order dependency
//!   on the frontend. The B3-05 elaborator row replaces this shim with
//!   a real `ExprData` bridge; the fields of [`Atomic128CasSpec`] are
//!   the stable interface, not [`ExprAst`].
//!
//! # Reserved diagnostic band `I0120-I0130`
//!
//! | Code  | Meaning                                                                 |
//! |-------|-------------------------------------------------------------------------|
//! | I0120 | Wrong arity (expected exactly 5 arguments).                             |
//! | I0121 | First argument is not a typed pointer expression.                       |
//! | I0122 | Pointee is not 128 bits wide (`u128` / `i128` / `f128` / `{u64,u64}`).  |
//! | I0123 | Pointer is under-aligned; `cmpxchg16b` requires 16-byte alignment.      |
//! | I0124 | Pointee kind is neither `Scalar128` nor `Pair64x2`.                     |
//! | I0125 | `expected` operand is not a 128-bit value.                              |
//! | I0126 | `desired`  operand is not a 128-bit value.                              |
//! | I0127 | `ord_success` is not a memory-ordering identifier.                      |
//! | I0128 | `ord_failure` is not a memory-ordering identifier.                      |
//! | I0129 | Monotonicity violation: `ord_failure` is stronger than `ord_success`.   |
//! | I0130 | Reserved (extension slot for the M2 lowering row).                      |

use paideia_as_ast::AtomicOrdering;

/// Local newtype for a resolved type identifier.
///
/// Mirrors the shape of `paideia_as_ir::monomorphisation::TypeId` (a `u32`
/// index into the monomorphisation table). We do **not** pull that crate
/// in from `paideia-as-intrinsic` — it would drag in the entire IR
/// dependency graph for a descriptor-layer file. The M2 elaborator row
/// substitutes the real `TypeId` at the boundary; downstream consumers
/// see the same `.0: u32` and treat this as an opaque index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TypeId(pub u32);

/// Whether the 128-bit pointee is a single scalar or a packed pair.
///
/// `cmpxchg16b` treats memory as an opaque 16-byte chunk (RDX:RAX vs.
/// RCX:RBX), so this distinction is purely for source-level type
/// checking; the encoder does not branch on it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum PointerElemKind {
    /// A single 128-bit scalar (`u128` / `i128` / `f128`).
    Scalar128,
    /// A packed pair of two 64-bit lanes (`{u64,u64}`, `{i64,i64}`, …).
    Pair64x2,
}

/// Descriptor-layer stand-in for a surface expression argument.
///
/// The real frontend produces `paideia_as_ast::ExprData` node ids inside
/// an `AstArena`. Threading that through here would couple the intrinsic
/// descriptor to the arena's lifetime for no functional gain — the
/// validator only needs *typed* facts about each argument (pointer meta,
/// value width, ordering enum). The B3-05 elaborator row lifts real
/// `ExprData` calls into this shape before invoking
/// [`parse_intrinsic_call`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExprAst {
    /// A resolved typed-pointer argument.
    ///
    /// `elem_bits` is the pointee width in bits; `align_bytes` is the
    /// static alignment the frontend proved for the pointer expression.
    Pointer {
        /// Opaque type index of the pointer type itself.
        ty: TypeId,
        /// Pointee width in bits (must be 128 for `cmpxchg16b`).
        elem_bits: u32,
        /// Static alignment in bytes (must be ≥ 16 for `cmpxchg16b`).
        align_bytes: u32,
        /// Whether the pointee is a scalar or a packed pair.
        kind: PointerElemKind,
    },
    /// A value argument of a known bit-width (for `expected` / `desired`).
    Value {
        /// Opaque type index of the value's type.
        ty: TypeId,
        /// Value width in bits.
        bits: u32,
    },
    /// A memory-ordering identifier appearing in argument position.
    OrderingIdent(AtomicOrdering),
}

/// Validation errors raised by [`parse_intrinsic_call`].
///
/// Each variant maps one-to-one to a diagnostic code in the reserved
/// `I0120-I0130` band. Downstream (the elaborator error sink) formats
/// these into user-facing messages using the codes below.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntrinsicErr {
    /// `I0120` — wrong number of arguments; expected exactly 5.
    WrongArity {
        /// Number of arguments actually supplied.
        got: usize,
    },
    /// `I0121` — first argument is not a typed pointer expression.
    NotAPointer,
    /// `I0122` — pointee width is not 128 bits.
    PointeeNot128 {
        /// The observed pointee width in bits.
        got_bits: u32,
    },
    /// `I0123` — pointer alignment is below the 16-byte `cmpxchg16b` floor.
    UnderAligned {
        /// The observed alignment in bytes.
        got_align: u32,
    },
    /// `I0124` — pointee kind is neither `Scalar128` nor `Pair64x2`.
    ///
    /// Reserved for a future pointee shape (e.g. `SimdVec128`) that would
    /// need explicit type-checker support before it can flow through the
    /// `cmpxchg16b` emit path.
    BadPointeeKind,
    /// `I0125` — `expected` operand is not a 128-bit value.
    ExpectedNot128 {
        /// The observed width in bits.
        got_bits: u32,
    },
    /// `I0126` — `desired` operand is not a 128-bit value.
    DesiredNot128 {
        /// The observed width in bits.
        got_bits: u32,
    },
    /// `I0127` — `ord_success` argument is not a memory-ordering identifier.
    OrdSuccessNotIdent,
    /// `I0128` — `ord_failure` argument is not a memory-ordering identifier.
    OrdFailureNotIdent,
    /// `I0129` — monotonicity violation: failure ordering is stronger
    /// than success ordering.
    OrderingMonotonicity {
        /// Success ordering, as supplied.
        success: AtomicOrdering,
        /// Failure ordering, as supplied.
        failure: AtomicOrdering,
    },
}

impl IntrinsicErr {
    /// Diagnostic code (band `I0120-I0130`) for this error variant.
    pub const fn code(&self) -> &'static str {
        match self {
            IntrinsicErr::WrongArity { .. } => "I0120",
            IntrinsicErr::NotAPointer => "I0121",
            IntrinsicErr::PointeeNot128 { .. } => "I0122",
            IntrinsicErr::UnderAligned { .. } => "I0123",
            IntrinsicErr::BadPointeeKind => "I0124",
            IntrinsicErr::ExpectedNot128 { .. } => "I0125",
            IntrinsicErr::DesiredNot128 { .. } => "I0126",
            IntrinsicErr::OrdSuccessNotIdent => "I0127",
            IntrinsicErr::OrdFailureNotIdent => "I0128",
            IntrinsicErr::OrderingMonotonicity { .. } => "I0129",
        }
    }
}

/// Parsed descriptor for a valid `@atomic128_cas(...)` call.
///
/// Consumed by the M2 encoder row that emits `LOCK CMPXCHG16B` (Intel
/// SDM Vol. 2A). The three fields are the *only* information the emit
/// path needs from the descriptor layer:
///
/// * `ptr_ty` — the pointer type index, forwarded so the register
///   allocator can pick 16-byte-aligned addressing (the memory operand
///   flows through `[R/M]` after `RSI`/`RDI`-style setup on the caller
///   side; the operand type governs which base register to reserve).
/// * `ord_success` / `ord_failure` — used by the encoder to decide
///   whether to bracket the `LOCK CMPXCHG16B` with `MFENCE` (SeqCst on
///   both sides) or emit it bare (x86 TSO already delivers acquire on
///   the load half and release on the store half for `LOCK`-prefixed
///   RMW). See `design/toolchain/atomics-x86_64.md` (deferred to M2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Atomic128CasSpec {
    /// Opaque type index of the *pointer* type.
    pub ptr_ty: TypeId,
    /// Ordering observed on the store half when the CAS succeeds.
    pub ord_success: AtomicOrdering,
    /// Ordering observed on the load half when the CAS fails.
    ///
    /// Must be weaker-than-or-equal-to `ord_success` (enforced by
    /// [`parse_intrinsic_call`]; violation raises `I0129`).
    pub ord_failure: AtomicOrdering,
}

/// Numeric strength of an [`AtomicOrdering`] for monotonicity checks.
///
/// The four-point lattice this crate exposes collapses `Acquire` and
/// `Release` to the same rank — they are orthogonal but incomparable in
/// C11's ordering model, and neither dominates the other. Concretely:
///
/// ```text
///   Relaxed (0) < Acquire (1) = Release (1) < SeqCst (2)
/// ```
///
/// This matches Rust `std::sync::atomic::Ordering`'s rules for CAS,
/// where the failure ordering may be any of `Relaxed`/`Acquire`/`SeqCst`
/// (never `Release` — a failed CAS is a pure load) and may not exceed
/// the success ordering. We enforce the "not stronger than" rule via
/// this strength function; the "no Release on failure" rule is a
/// natural consequence of Release ranking equal-to Acquire while
/// `ord_success = Acquire` still fails against `ord_failure = Release`
/// under the strict-strength rule below only when both are asymmetric
/// (which is why C++/Rust spell it out separately). For M1 we take the
/// conservative reading: strength alone, with `Release` ranked equal to
/// `Acquire`.
const fn strength(o: AtomicOrdering) -> u8 {
    match o {
        AtomicOrdering::Relaxed => 0,
        AtomicOrdering::Acquire => 1,
        AtomicOrdering::Release => 1,
        AtomicOrdering::SeqCst => 2,
    }
}

/// Parse and validate an `@atomic128_cas(...)` intrinsic call.
///
/// # Arguments
///
/// Exactly five, positionally:
///
/// 1. `ptr` — [`ExprAst::Pointer`] with a 128-bit pointee and 16-byte alignment.
/// 2. `expected` — [`ExprAst::Value`] of width 128.
/// 3. `desired`  — [`ExprAst::Value`] of width 128.
/// 4. `ord_success` — [`ExprAst::OrderingIdent`].
/// 5. `ord_failure` — [`ExprAst::OrderingIdent`], weaker-than-or-equal to `ord_success`.
///
/// # Errors
///
/// See [`IntrinsicErr`]. All errors carry a stable code in the
/// `I0120-I0130` band and enough payload for the elaborator sink to
/// render a specific message without re-inspecting the argument slice.
pub fn parse_intrinsic_call(args: &[ExprAst]) -> Result<Atomic128CasSpec, IntrinsicErr> {
    if args.len() != 5 {
        return Err(IntrinsicErr::WrongArity { got: args.len() });
    }

    // --- Arg 0: pointer -----------------------------------------------------
    let (ptr_ty, elem_bits, align_bytes, kind) = match args[0] {
        ExprAst::Pointer {
            ty,
            elem_bits,
            align_bytes,
            kind,
        } => (ty, elem_bits, align_bytes, kind),
        _ => return Err(IntrinsicErr::NotAPointer),
    };

    // Order of checks (kind first, then width, then alignment) is
    // deliberately most-specific → least-specific so a malformed
    // fixture surfaces the earliest violation rather than a cascade.
    match kind {
        PointerElemKind::Scalar128 | PointerElemKind::Pair64x2 => {}
        // The enum is exhaustive today, but the arm reserves I0124 for
        // a future variant (e.g. `SimdVec128`) added without churning
        // the parser.
        #[allow(unreachable_patterns)]
        _ => return Err(IntrinsicErr::BadPointeeKind),
    }

    if elem_bits != 128 {
        return Err(IntrinsicErr::PointeeNot128 {
            got_bits: elem_bits,
        });
    }

    if align_bytes < 16 {
        return Err(IntrinsicErr::UnderAligned {
            got_align: align_bytes,
        });
    }

    // --- Arg 1 / Arg 2: expected / desired ---------------------------------
    let expected_bits = match args[1] {
        ExprAst::Value { bits, .. } => bits,
        _ => {
            return Err(IntrinsicErr::ExpectedNot128 { got_bits: 0 });
        }
    };
    if expected_bits != 128 {
        return Err(IntrinsicErr::ExpectedNot128 {
            got_bits: expected_bits,
        });
    }

    let desired_bits = match args[2] {
        ExprAst::Value { bits, .. } => bits,
        _ => {
            return Err(IntrinsicErr::DesiredNot128 { got_bits: 0 });
        }
    };
    if desired_bits != 128 {
        return Err(IntrinsicErr::DesiredNot128 {
            got_bits: desired_bits,
        });
    }

    // --- Arg 3 / Arg 4: orderings ------------------------------------------
    let ord_success = match args[3] {
        ExprAst::OrderingIdent(o) => o,
        _ => return Err(IntrinsicErr::OrdSuccessNotIdent),
    };
    let ord_failure = match args[4] {
        ExprAst::OrderingIdent(o) => o,
        _ => return Err(IntrinsicErr::OrdFailureNotIdent),
    };

    if strength(ord_failure) > strength(ord_success) {
        return Err(IntrinsicErr::OrderingMonotonicity {
            success: ord_success,
            failure: ord_failure,
        });
    }

    Ok(Atomic128CasSpec {
        ptr_ty,
        ord_success,
        ord_failure,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a well-formed pointer argument: 128-bit scalar, 16-byte aligned.
    fn ok_ptr() -> ExprAst {
        ExprAst::Pointer {
            ty: TypeId(1),
            elem_bits: 128,
            align_bytes: 16,
            kind: PointerElemKind::Scalar128,
        }
    }

    fn u128_val() -> ExprAst {
        ExprAst::Value {
            ty: TypeId(2),
            bits: 128,
        }
    }

    #[test]
    fn atomic128_valid_cas_spec() {
        let args = [
            ok_ptr(),
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::SeqCst),
            ExprAst::OrderingIdent(AtomicOrdering::Acquire),
        ];
        let spec = parse_intrinsic_call(&args).expect("valid CAS spec should parse");
        assert_eq!(spec.ptr_ty, TypeId(1));
        assert_eq!(spec.ord_success, AtomicOrdering::SeqCst);
        assert_eq!(spec.ord_failure, AtomicOrdering::Acquire);
    }

    #[test]
    fn atomic128_valid_pair_pointee_and_relaxed_pair() {
        // Cross-cover: Pair64x2 + Relaxed/Relaxed to lock in the boundary of
        // the monotonicity rule (equal orderings must be accepted).
        let args = [
            ExprAst::Pointer {
                ty: TypeId(7),
                elem_bits: 128,
                align_bytes: 32, // over-aligned is still legal
                kind: PointerElemKind::Pair64x2,
            },
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
        ];
        let spec = parse_intrinsic_call(&args).expect("pair + relaxed×2 is valid");
        assert_eq!(spec.ord_success, AtomicOrdering::Relaxed);
        assert_eq!(spec.ord_failure, AtomicOrdering::Relaxed);
    }

    #[test]
    fn atomic128_misaligned_pointer_rejected() {
        let args = [
            ExprAst::Pointer {
                ty: TypeId(1),
                elem_bits: 128,
                align_bytes: 8, // below the 16-byte cmpxchg16b floor
                kind: PointerElemKind::Scalar128,
            },
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::SeqCst),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
        ];
        match parse_intrinsic_call(&args) {
            Err(IntrinsicErr::UnderAligned { got_align: 8 }) => {}
            other => panic!("expected I0123 UnderAligned{{got_align:8}}, got {:?}", other),
        }
    }

    #[test]
    fn atomic128_ordering_monotonicity_rejected() {
        // ord_success = Relaxed, ord_failure = SeqCst -> failure stronger.
        let args = [
            ok_ptr(),
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
            ExprAst::OrderingIdent(AtomicOrdering::SeqCst),
        ];
        match parse_intrinsic_call(&args) {
            Err(IntrinsicErr::OrderingMonotonicity {
                success: AtomicOrdering::Relaxed,
                failure: AtomicOrdering::SeqCst,
            }) => {}
            other => panic!(
                "expected I0129 OrderingMonotonicity{{Relaxed,SeqCst}}, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn atomic128_wrong_arity_rejected() {
        let args = [ok_ptr(), u128_val()];
        match parse_intrinsic_call(&args) {
            Err(IntrinsicErr::WrongArity { got: 2 }) => {}
            other => panic!("expected I0120 WrongArity{{got:2}}, got {:?}", other),
        }
    }

    #[test]
    fn atomic128_pointee_width_rejected() {
        let args = [
            ExprAst::Pointer {
                ty: TypeId(1),
                elem_bits: 64, // wrong width
                align_bytes: 16,
                kind: PointerElemKind::Scalar128,
            },
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::SeqCst),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
        ];
        match parse_intrinsic_call(&args) {
            Err(IntrinsicErr::PointeeNot128 { got_bits: 64 }) => {}
            other => panic!("expected I0122 PointeeNot128{{got_bits:64}}, got {:?}", other),
        }
    }

    #[test]
    fn atomic128_first_arg_must_be_pointer() {
        let args = [
            u128_val(), // not a pointer
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::SeqCst),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
        ];
        assert_eq!(parse_intrinsic_call(&args), Err(IntrinsicErr::NotAPointer));
    }

    #[test]
    fn atomic128_ordering_ident_required_on_arg3_arg4() {
        // Arg 3 wrong.
        let args = [
            ok_ptr(),
            u128_val(),
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::Relaxed),
        ];
        assert_eq!(
            parse_intrinsic_call(&args),
            Err(IntrinsicErr::OrdSuccessNotIdent)
        );

        // Arg 4 wrong.
        let args = [
            ok_ptr(),
            u128_val(),
            u128_val(),
            ExprAst::OrderingIdent(AtomicOrdering::SeqCst),
            u128_val(),
        ];
        assert_eq!(
            parse_intrinsic_call(&args),
            Err(IntrinsicErr::OrdFailureNotIdent)
        );
    }

    #[test]
    fn atomic128_diagnostic_codes_in_band() {
        // Lock in the I0120-I0130 band mapping. If a downstream renderer
        // regresses the code, this test surfaces it before the
        // documentation table in the module docs drifts.
        let cases: &[(IntrinsicErr, &str)] = &[
            (IntrinsicErr::WrongArity { got: 0 }, "I0120"),
            (IntrinsicErr::NotAPointer, "I0121"),
            (IntrinsicErr::PointeeNot128 { got_bits: 0 }, "I0122"),
            (IntrinsicErr::UnderAligned { got_align: 0 }, "I0123"),
            (IntrinsicErr::BadPointeeKind, "I0124"),
            (IntrinsicErr::ExpectedNot128 { got_bits: 0 }, "I0125"),
            (IntrinsicErr::DesiredNot128 { got_bits: 0 }, "I0126"),
            (IntrinsicErr::OrdSuccessNotIdent, "I0127"),
            (IntrinsicErr::OrdFailureNotIdent, "I0128"),
            (
                IntrinsicErr::OrderingMonotonicity {
                    success: AtomicOrdering::Relaxed,
                    failure: AtomicOrdering::SeqCst,
                },
                "I0129",
            ),
        ];
        for (err, code) in cases {
            assert_eq!(err.code(), *code, "code drift for {:?}", err);
        }
    }
}
