//! `@mulu64` and `@divu64` — wide-integer intrinsic descriptors.
//!
//! **Row:** v0.26-M1-002 (paideia-as#1361), Wave 0 Batch 4.
//!
//! This module *describes and validates the intrinsic calls* — it does
//! **not** yet lower to machine code. The x86_64 target lowering pairs
//! (`mul r64` / `div r64` in Intel SDM Vol. 2A, both single-operand forms
//! taking `RAX` and returning `RDX:RAX`) lands in the encoder crate in
//! v0.26-M2 and will consume [`Mulu64Spec`] / [`Divu64Spec`] verbatim.
//!
//! # Surface syntax
//!
//! ```paideia-as
//! (lo, hi) = @mulu64(a, b)               // a: u64, b: u64
//! (q, r)   = @divu64(hi, lo, div)        // all u64; div != 0
//! ```
//!
//! Both intrinsics operate on unsigned 64-bit lanes. `@mulu64` widens the
//! product into a 128-bit result split across two 64-bit registers
//! (matching `mul r64`, which writes `RDX:RAX`). `@divu64` mirrors the
//! `div r64` instruction: it divides the 128-bit dividend `hi:lo` by the
//! 64-bit `div`, yielding a 64-bit quotient and a 64-bit remainder, and
//! traps on both `div == 0` and quotient overflow (the CPU raises `#DE`
//! in either case — the elaborator lowers a *literal* divisor of zero to
//! a hard error at parse time; the overflow case is only detectable at
//! run time and is handled by the M2 trap emit).
//!
//! # Scope of this file
//!
//! * Public [`IntrinsicDescriptor`] — static metadata (name, arity,
//!   argument widths, return widths) surfaced by [`Intrinsic::describe`].
//! * Public trait [`Intrinsic`] with `describe()` + `validate_call()`.
//! * Marker types [`Mulu64`] and [`Divu64`] implementing [`Intrinsic`].
//! * Public specs [`Mulu64Spec`] / [`Divu64Spec`] consumed by v0.26-M2.
//! * Diagnostic enum [`IntrinsicErr`] carrying codes in the reserved
//!   `I0100-I0102` band (local to this module — each intrinsic module in
//!   the crate carries its own local band; the elaborator disambiguates
//!   at the call-site).
//! * Local shim [`ExprAst`] — a descriptor-layer stand-in for a typed
//!   argument expression. The B4-05 elaborator row replaces this shim
//!   with a real `ExprData` bridge; the spec fields are the stable
//!   interface, not [`ExprAst`].
//!
//! # Reserved diagnostic band `I0100-I0102`
//!
//! | Code  | Meaning                                                              |
//! |-------|----------------------------------------------------------------------|
//! | I0100 | Wrong arity (`@mulu64` expects 2; `@divu64` expects 3).              |
//! | I0101 | Argument at index `n` is not a 64-bit unsigned value.                |
//! | I0102 | `@divu64` divisor is a literal zero (compile-time trap detection).   |

use core::fmt;

// ---------------------------------------------------------------------------
// Descriptor metadata
// ---------------------------------------------------------------------------

/// Bit-width of a scalar operand accepted by the wide-int intrinsics.
///
/// Only [`ScalarType::U64`] is legal today. Extending the descriptor to
/// signed 64-bit lanes (`@mul64`, `@div64`, `@rem64` — SDM `IMUL` /
/// `IDIV`) is a mechanical addition tracked under the v0.26-M2 emit row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ScalarType {
    /// Unsigned 64-bit integer.
    U64,
}

impl ScalarType {
    /// Width in bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::U64 => 64,
        }
    }

    /// Whether the type is unsigned.
    #[must_use]
    pub const fn is_unsigned(self) -> bool {
        matches!(self, Self::U64)
    }
}

impl fmt::Display for ScalarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::U64 => f.write_str("u64"),
        }
    }
}

/// Static metadata for one intrinsic — the shape [`Intrinsic::describe`]
/// returns.
///
/// This is a *descriptor*, not a spec: it says what the intrinsic *is*,
/// not what any particular call to it looks like. The elaborator uses
/// it to build the `@`-intrinsic dispatch table and to render usage
/// messages when [`Intrinsic::validate_call`] fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntrinsicDescriptor {
    /// Surface name — as it appears after the `@` sigil.
    pub name: &'static str,
    /// Expected argument count.
    pub arity: usize,
    /// Expected scalar type of each positional argument.
    ///
    /// `arg_types.len() == arity` is invariant on every descriptor
    /// exposed by this module.
    pub arg_types: &'static [ScalarType],
    /// Scalar types of the tuple-returned components, in order.
    ///
    /// `@mulu64` returns `(lo: u64, hi: u64)`; `@divu64` returns
    /// `(q: u64, r: u64)`. Both are two-tuples of `u64`.
    pub ret_types: &'static [ScalarType],
}

// ---------------------------------------------------------------------------
// Argument shim + errors
// ---------------------------------------------------------------------------

/// Descriptor-layer stand-in for an argument expression.
///
/// The frontend produces `paideia_as_ast::ExprData` node ids inside an
/// `AstArena`. Threading that through here would couple the intrinsic
/// descriptor to the arena's lifetime for no functional gain — the
/// validator only needs *typed* facts about each argument (bit width,
/// and, for divisor operands, whether the value is a compile-time
/// literal zero). The B4-05 elaborator row lifts real `ExprData` calls
/// into this shape before invoking [`Intrinsic::validate_call`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExprAst {
    /// A typed value of known width in bits.
    Value {
        /// Value width in bits.
        bits: u32,
        /// Whether the value's type is unsigned.
        ///
        /// The wide-int intrinsics reject signed operands (Intel `MUL` /
        /// `DIV` are the *unsigned* forms; `IMUL` / `IDIV` are separate
        /// instructions with distinct overflow semantics).
        unsigned: bool,
    },
    /// A compile-time integer literal, with its inferred width in bits.
    ///
    /// Distinguished from [`ExprAst::Value`] so that
    /// [`Divu64::validate_call`] can detect a literal-zero divisor and
    /// surface `I0102` before the M2 encoder needs to emit a
    /// static-trap sequence.
    IntLiteral {
        /// The literal value.
        value: u64,
        /// Inferred type width in bits.
        bits: u32,
        /// Whether the inferred type is unsigned.
        unsigned: bool,
    },
}

impl ExprAst {
    /// Width in bits, whether the argument is a value or a literal.
    #[must_use]
    pub const fn bits(&self) -> u32 {
        match self {
            Self::Value { bits, .. } => *bits,
            Self::IntLiteral { bits, .. } => *bits,
        }
    }

    /// Whether the argument's inferred type is unsigned.
    #[must_use]
    pub const fn is_unsigned(&self) -> bool {
        match self {
            Self::Value { unsigned, .. } => *unsigned,
            Self::IntLiteral { unsigned, .. } => *unsigned,
        }
    }

    /// Whether the argument matches a required [`ScalarType`].
    ///
    /// A shape mismatch (wrong width *or* wrong signedness) is the
    /// single `I0101` trigger; keeping the check in one predicate keeps
    /// the validator's error site colocated with the type comparison.
    #[must_use]
    pub const fn matches(&self, ty: ScalarType) -> bool {
        self.bits() == ty.bits() && self.is_unsigned() == ty.is_unsigned()
    }
}

/// Validation errors raised by [`Intrinsic::validate_call`] for both
/// wide-int intrinsics.
///
/// Each variant maps one-to-one to a diagnostic code in the reserved
/// `I0100-I0102` band via [`Self::code`]. Downstream (the elaborator
/// error sink) formats these into user-facing messages using the codes
/// below, then stitches the intrinsic name into the message from the
/// [`IntrinsicDescriptor`] the dispatch table stored for the call site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntrinsicErr {
    /// `I0100` — wrong argument count.
    WrongArity {
        /// Expected argument count for this intrinsic.
        expected: usize,
        /// Actual argument count observed at the call site.
        got: usize,
    },
    /// `I0101` — argument at `index` is not the expected 64-bit unsigned
    /// value.
    WrongArgType {
        /// Zero-based positional index of the offending argument.
        index: usize,
        /// Expected scalar type at that position.
        expected: ScalarType,
        /// Observed width in bits.
        got_bits: u32,
        /// Observed signedness (`true` == unsigned).
        got_unsigned: bool,
    },
    /// `I0102` — `@divu64` divisor is a literal zero.
    ///
    /// A literal-zero divisor would deterministically trap at run time
    /// (Intel `DIV` raises `#DE` on a zero divisor). Rejecting it at the
    /// descriptor layer prevents the encoder from having to emit a
    /// static-trap sequence for a call that has no correct behaviour.
    DivByZeroLiteral,
}

impl IntrinsicErr {
    /// Stable wire-form diagnostic code (`"I0100"`..`"I0102"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::WrongArity { .. } => "I0100",
            Self::WrongArgType { .. } => "I0101",
            Self::DivByZeroLiteral => "I0102",
        }
    }
}

impl fmt::Display for IntrinsicErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongArity { expected, got } => write!(
                f,
                "{}: expected {} argument(s), got {}",
                self.code(),
                expected,
                got
            ),
            Self::WrongArgType {
                index,
                expected,
                got_bits,
                got_unsigned,
            } => write!(
                f,
                "{}: argument #{} must be `{}` (got {}-bit {})",
                self.code(),
                index,
                expected,
                got_bits,
                if *got_unsigned { "unsigned" } else { "signed" }
            ),
            Self::DivByZeroLiteral => write!(
                f,
                "{}: divisor is a literal zero; the instruction would trap (`#DE`) at run time",
                self.code()
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Intrinsic trait
// ---------------------------------------------------------------------------

/// Uniform surface for every descriptor-layer intrinsic in this module.
///
/// A `describe()` + `validate_call()` pair is the whole descriptor
/// contract: `describe()` returns the static shape (arity, arg types,
/// return types) that the elaborator's dispatch table stores, and
/// `validate_call()` lowers a concrete argument slice into a typed
/// `Spec` the encoder consumes.
///
/// The trait is deliberately object-unsafe (associated `Spec` type) —
/// the dispatch table keys off the intrinsic *name* and calls into the
/// concrete `validate_call` for the matched entry, so no `dyn Intrinsic`
/// ever needs to exist.
pub trait Intrinsic {
    /// Parsed descriptor returned on success.
    type Spec;

    /// Static metadata for this intrinsic.
    ///
    /// The returned [`IntrinsicDescriptor`] is a `const` in every
    /// implementor and carries no per-call state.
    fn describe() -> IntrinsicDescriptor;

    /// Validate the argument slice against [`Self::describe`] and
    /// lower it into [`Self::Spec`].
    ///
    /// # Errors
    ///
    /// See [`IntrinsicErr`]. All errors carry a stable diagnostic code
    /// in the `I0100-I0102` band.
    fn validate_call(args: &[ExprAst]) -> Result<Self::Spec, IntrinsicErr>;
}

// ---------------------------------------------------------------------------
// @mulu64 — unsigned wide multiply
// ---------------------------------------------------------------------------

/// Parsed descriptor for a valid `@mulu64(a, b)` call.
///
/// The encoder consumes this to place `a` in `RAX` and issue `MUL r64`
/// against `b` (single-operand form: implicit `RAX`, result in
/// `RDX:RAX`). The spec is a marker type today — every well-formed
/// `@mulu64` call is fungible at the encoder because both operands
/// resolve to plain `u64` values with no per-call configuration knobs.
/// Adding an operand-source tag later (register / memory / immediate)
/// would extend this struct without disturbing the trait surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mulu64Spec;

/// Marker type — the descriptor for the `@mulu64` intrinsic.
///
/// Zero-sized; exists purely to hang the [`Intrinsic`] impl and
/// [`Mulu64::describe`] off a nameable type the elaborator dispatch
/// table can key against.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mulu64;

impl Mulu64 {
    const ARG_TYPES: &'static [ScalarType] = &[ScalarType::U64, ScalarType::U64];
    const RET_TYPES: &'static [ScalarType] = &[ScalarType::U64, ScalarType::U64];
}

impl Intrinsic for Mulu64 {
    type Spec = Mulu64Spec;

    fn describe() -> IntrinsicDescriptor {
        IntrinsicDescriptor {
            name: "mulu64",
            arity: 2,
            arg_types: Self::ARG_TYPES,
            ret_types: Self::RET_TYPES,
        }
    }

    fn validate_call(args: &[ExprAst]) -> Result<Self::Spec, IntrinsicErr> {
        let desc = Self::describe();
        check_arity(&desc, args)?;
        check_arg_types(&desc, args)?;
        Ok(Mulu64Spec)
    }
}

// ---------------------------------------------------------------------------
// @divu64 — unsigned wide divide
// ---------------------------------------------------------------------------

/// Parsed descriptor for a valid `@divu64(hi, lo, div)` call.
///
/// # Fields
///
/// * `divisor_is_literal` — recorded so the encoder can constant-fold
///   the `hi:lo / literal` case when profitable (a `divisor` known at
///   compile time may lower to a reciprocal-multiply sequence rather
///   than the `DIV` instruction, sidestepping the CPU's slow-path
///   divider). The optimisation itself lands in v0.26-M2; the flag is
///   captured here so the descriptor already carries the information
///   the encoder needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Divu64Spec {
    /// `true` when the `div` argument was an [`ExprAst::IntLiteral`].
    pub divisor_is_literal: bool,
}

/// Marker type — the descriptor for the `@divu64` intrinsic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Divu64;

impl Divu64 {
    const ARG_TYPES: &'static [ScalarType] =
        &[ScalarType::U64, ScalarType::U64, ScalarType::U64];
    const RET_TYPES: &'static [ScalarType] = &[ScalarType::U64, ScalarType::U64];
    /// Zero-based index of the divisor argument (`hi`, `lo`, **`div`**).
    const DIVISOR_INDEX: usize = 2;
}

impl Intrinsic for Divu64 {
    type Spec = Divu64Spec;

    fn describe() -> IntrinsicDescriptor {
        IntrinsicDescriptor {
            name: "divu64",
            arity: 3,
            arg_types: Self::ARG_TYPES,
            ret_types: Self::RET_TYPES,
        }
    }

    fn validate_call(args: &[ExprAst]) -> Result<Self::Spec, IntrinsicErr> {
        let desc = Self::describe();
        check_arity(&desc, args)?;
        check_arg_types(&desc, args)?;

        // Arity and shape have passed → the divisor slot exists and is
        // known to be a well-typed `u64` argument. Inspect it for the
        // literal-zero case.
        let divisor_is_literal = match &args[Self::DIVISOR_INDEX] {
            ExprAst::IntLiteral { value, .. } => {
                if *value == 0 {
                    return Err(IntrinsicErr::DivByZeroLiteral);
                }
                true
            }
            ExprAst::Value { .. } => false,
        };

        Ok(Divu64Spec {
            divisor_is_literal,
        })
    }
}

// ---------------------------------------------------------------------------
// Shared validation helpers
// ---------------------------------------------------------------------------

/// Arity check against a descriptor.
///
/// Extracted so both intrinsics surface `I0100` with identical payloads
/// and so a fresh intrinsic added in v0.26-M2 (e.g. `@mul64` /
/// `@divi64`) can reuse the exact predicate.
fn check_arity(desc: &IntrinsicDescriptor, args: &[ExprAst]) -> Result<(), IntrinsicErr> {
    if args.len() != desc.arity {
        return Err(IntrinsicErr::WrongArity {
            expected: desc.arity,
            got: args.len(),
        });
    }
    Ok(())
}

/// Positional type check.
///
/// Iterates the descriptor's `arg_types` in lockstep with `args` and
/// raises `I0101` at the *first* offending index — deterministic order
/// keeps error snapshots stable across refactors.
///
/// Precondition: `args.len() == desc.arity` (i.e. call after
/// [`check_arity`]).
fn check_arg_types(desc: &IntrinsicDescriptor, args: &[ExprAst]) -> Result<(), IntrinsicErr> {
    debug_assert_eq!(
        args.len(),
        desc.arity,
        "check_arg_types called before arity was validated"
    );
    for (index, (arg, expected)) in args.iter().zip(desc.arg_types.iter()).enumerate() {
        if !arg.matches(*expected) {
            return Err(IntrinsicErr::WrongArgType {
                index,
                expected: *expected,
                got_bits: arg.bits(),
                got_unsigned: arg.is_unsigned(),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A well-typed 64-bit unsigned value argument.
    fn u64_val() -> ExprAst {
        ExprAst::Value {
            bits: 64,
            unsigned: true,
        }
    }

    /// A well-typed 64-bit unsigned integer literal.
    fn u64_lit(value: u64) -> ExprAst {
        ExprAst::IntLiteral {
            value,
            bits: 64,
            unsigned: true,
        }
    }

    // --- describe() ------------------------------------------------------

    #[test]
    fn mulu64_descriptor_shape() {
        let d = Mulu64::describe();
        assert_eq!(d.name, "mulu64");
        assert_eq!(d.arity, 2);
        assert_eq!(d.arg_types, &[ScalarType::U64, ScalarType::U64]);
        assert_eq!(d.ret_types, &[ScalarType::U64, ScalarType::U64]);
        assert_eq!(d.arg_types.len(), d.arity, "arg_types.len() must equal arity");
    }

    #[test]
    fn divu64_descriptor_shape() {
        let d = Divu64::describe();
        assert_eq!(d.name, "divu64");
        assert_eq!(d.arity, 3);
        assert_eq!(
            d.arg_types,
            &[ScalarType::U64, ScalarType::U64, ScalarType::U64]
        );
        assert_eq!(d.ret_types, &[ScalarType::U64, ScalarType::U64]);
        assert_eq!(d.arg_types.len(), d.arity, "arg_types.len() must equal arity");
    }

    // --- happy path ------------------------------------------------------

    #[test]
    fn wide_int_valid_mulu64_value_args() {
        let spec = Mulu64::validate_call(&[u64_val(), u64_val()])
            .expect("well-typed @mulu64 must parse");
        assert_eq!(spec, Mulu64Spec);
    }

    #[test]
    fn wide_int_valid_mulu64_literal_args() {
        // Literals in both slots are still legal (u64 * u64 folds fine).
        let spec = Mulu64::validate_call(&[u64_lit(3), u64_lit(5)])
            .expect("literal @mulu64 must parse");
        assert_eq!(spec, Mulu64Spec);
    }

    #[test]
    fn wide_int_valid_divu64_value_divisor() {
        let spec = Divu64::validate_call(&[u64_val(), u64_val(), u64_val()])
            .expect("well-typed @divu64 with value divisor must parse");
        assert_eq!(
            spec,
            Divu64Spec {
                divisor_is_literal: false
            }
        );
    }

    #[test]
    fn wide_int_valid_divu64_nonzero_literal_divisor_records_flag() {
        let spec = Divu64::validate_call(&[u64_val(), u64_val(), u64_lit(7)])
            .expect("@divu64 with nonzero literal divisor must parse");
        assert_eq!(
            spec,
            Divu64Spec {
                divisor_is_literal: true
            },
            "literal divisor must be recorded so the encoder can constant-fold"
        );
    }

    // --- I0100 wrong arity -----------------------------------------------

    #[test]
    fn wide_int_mulu64_wrong_arity_rejected() {
        let err = Mulu64::validate_call(&[u64_val()]).expect_err("1 arg must be rejected");
        assert_eq!(err.code(), "I0100");
        assert_eq!(
            err,
            IntrinsicErr::WrongArity {
                expected: 2,
                got: 1
            }
        );
    }

    #[test]
    fn wide_int_divu64_wrong_arity_rejected() {
        let err = Divu64::validate_call(&[u64_val(), u64_val()])
            .expect_err("2 args must be rejected for @divu64");
        assert_eq!(err.code(), "I0100");
        assert_eq!(
            err,
            IntrinsicErr::WrongArity {
                expected: 3,
                got: 2
            }
        );
    }

    #[test]
    fn wide_int_divu64_too_many_args_rejected() {
        let err = Divu64::validate_call(&[u64_val(), u64_val(), u64_val(), u64_val()])
            .expect_err("4 args must be rejected for @divu64");
        assert_eq!(err.code(), "I0100");
        assert_eq!(
            err,
            IntrinsicErr::WrongArity {
                expected: 3,
                got: 4
            }
        );
    }

    // --- I0101 wrong arg types -------------------------------------------

    #[test]
    fn wide_int_mulu64_wrong_arg_width_rejected() {
        // Second slot has a 32-bit value.
        let args = [
            u64_val(),
            ExprAst::Value {
                bits: 32,
                unsigned: true,
            },
        ];
        let err = Mulu64::validate_call(&args).unwrap_err();
        assert_eq!(err.code(), "I0101");
        assert_eq!(
            err,
            IntrinsicErr::WrongArgType {
                index: 1,
                expected: ScalarType::U64,
                got_bits: 32,
                got_unsigned: true,
            }
        );
    }

    #[test]
    fn wide_int_mulu64_signed_arg_rejected() {
        // A 64-bit *signed* value is not `u64` — MUL and IMUL are
        // distinct instructions with distinct semantics.
        let args = [
            u64_val(),
            ExprAst::Value {
                bits: 64,
                unsigned: false,
            },
        ];
        let err = Mulu64::validate_call(&args).unwrap_err();
        assert_eq!(err.code(), "I0101");
        assert!(matches!(
            err,
            IntrinsicErr::WrongArgType {
                index: 1,
                expected: ScalarType::U64,
                got_bits: 64,
                got_unsigned: false,
            }
        ));
    }

    #[test]
    fn wide_int_divu64_wrong_arg_width_rejected() {
        // First slot has a 128-bit value — legal-looking width but wrong type.
        let args = [
            ExprAst::Value {
                bits: 128,
                unsigned: true,
            },
            u64_val(),
            u64_val(),
        ];
        let err = Divu64::validate_call(&args).unwrap_err();
        assert_eq!(err.code(), "I0101");
        assert_eq!(
            err,
            IntrinsicErr::WrongArgType {
                index: 0,
                expected: ScalarType::U64,
                got_bits: 128,
                got_unsigned: true,
            }
        );
    }

    #[test]
    fn wide_int_divu64_literal_wrong_width_rejected() {
        // A 32-bit literal in the divisor slot fails type checking
        // *before* the div-by-zero check runs — arity/type checks
        // gate the literal-zero inspection, so a 32-bit `0` surfaces
        // I0101, not I0102.
        let args = [
            u64_val(),
            u64_val(),
            ExprAst::IntLiteral {
                value: 0,
                bits: 32,
                unsigned: true,
            },
        ];
        let err = Divu64::validate_call(&args).unwrap_err();
        assert_eq!(err.code(), "I0101");
        assert!(matches!(
            err,
            IntrinsicErr::WrongArgType {
                index: 2,
                expected: ScalarType::U64,
                got_bits: 32,
                got_unsigned: true,
            }
        ));
    }

    // --- I0102 div-by-zero literal ---------------------------------------

    #[test]
    fn wide_int_divu64_literal_zero_divisor_rejected() {
        let args = [u64_val(), u64_val(), u64_lit(0)];
        let err = Divu64::validate_call(&args).unwrap_err();
        assert_eq!(err.code(), "I0102");
        assert_eq!(err, IntrinsicErr::DivByZeroLiteral);
    }

    #[test]
    fn wide_int_divu64_nonliteral_zero_divisor_accepted() {
        // A runtime `Value` that *happens* to be zero cannot be detected
        // at descriptor time — only literal zeros surface I0102. The
        // encoder emits a runtime trap check in the M2 row.
        let spec = Divu64::validate_call(&[u64_val(), u64_val(), u64_val()])
            .expect("runtime zero is invisible at descriptor time");
        assert_eq!(spec.divisor_is_literal, false);
    }

    // --- code() coverage -------------------------------------------------

    #[test]
    fn wide_int_diagnostic_codes_in_band() {
        // Lock in the I0100-I0102 band mapping. If a downstream renderer
        // regresses the code, this test surfaces it before the module
        // docs table drifts.
        let cases: &[(IntrinsicErr, &str)] = &[
            (
                IntrinsicErr::WrongArity {
                    expected: 2,
                    got: 0,
                },
                "I0100",
            ),
            (
                IntrinsicErr::WrongArgType {
                    index: 0,
                    expected: ScalarType::U64,
                    got_bits: 32,
                    got_unsigned: true,
                },
                "I0101",
            ),
            (IntrinsicErr::DivByZeroLiteral, "I0102"),
        ];
        for (err, code) in cases {
            assert_eq!(err.code(), *code, "code drift for {:?}", err);
        }
    }

    #[test]
    fn wide_int_diagnostic_codes_distinct_and_in_range() {
        let codes = [
            IntrinsicErr::WrongArity {
                expected: 2,
                got: 0,
            }
            .code(),
            IntrinsicErr::WrongArgType {
                index: 0,
                expected: ScalarType::U64,
                got_bits: 0,
                got_unsigned: true,
            }
            .code(),
            IntrinsicErr::DivByZeroLiteral.code(),
        ];
        let mut sorted: Vec<&&str> = codes.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "codes must be distinct");
        for c in &codes {
            assert!(c.starts_with('I'));
            let n: u16 = c[1..].parse().unwrap();
            assert!(
                (100..=102).contains(&n),
                "code {c} out of I0100..I0102 range"
            );
        }
    }

    // --- Display renders the code ---------------------------------------

    #[test]
    fn wide_int_display_prefixes_code() {
        let s = format!("{}", IntrinsicErr::DivByZeroLiteral);
        assert!(s.starts_with("I0102:"), "Display must lead with the code, got {s:?}");
    }
}
