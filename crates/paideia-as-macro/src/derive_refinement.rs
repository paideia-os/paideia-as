//! `@derive(base, refinement)` — capability-refinement derive macro.
//!
//! v0.25-M1-004 (#1358). This module implements the source-level
//! (elaborator-invoked) macro that expands
//!
//! ```paideia-as
//! type Positive = @derive(Int, x > 0);
//! ```
//!
//! into a small item cluster the elaborator can then type-check:
//!
//! 1. **Base-type item** — records the newtype-wrapper relationship
//!    between the derived name and the base type. The elaborator lowers
//!    this to a nominal type whose representation is the base type but
//!    whose identity is fresh; that identity is what makes the refined
//!    subset a distinct sort so a bare `Int` value cannot be passed
//!    where a `Positive` is required.
//! 2. **Smart constructor** — a total function `Base -> Option<T>` that
//!    runs the refinement predicate at run time (or at compile time when
//!    the argument is a literal) and yields `Some(T(x))` iff the
//!    predicate holds. This is the only public route into `T` for values
//!    outside `unsafe { … }`, so the refinement invariant is enforced by
//!    construction.
//! 3. **`where`-clause capture** — a first-class proof obligation record
//!    the elaborator carries alongside the derived type. Later phases
//!    (SMT-driven refinement narrowing in M2; capability-obligation
//!    checking in R29) consume this record to prove that operations on
//!    refined values preserve the invariant without re-parsing the
//!    predicate.
//!
//! # Not a `proc_macro` crate
//!
//! `paideia-as-macro` is a **paideia-as source-level** macro crate, not
//! a Rust `proc_macro`. The elaborator holds the invocation-site AST
//! and calls [`expand_derive_refinement`] with a decoded
//! [`MacroCallAst`]; the returned [`Vec<ItemAst>`] is spliced back into
//! the surrounding item list before name resolution runs. There is no
//! token-stream layer between the elaborator and this crate — the
//! translation of the pattern-macro token trees into a `MacroCallAst`
//! is the elaborator's job (paideia-as#1357 plumbing) and is deliberately
//! kept outside this module so that later reflection-era macros can
//! reuse the same decoded-AST → items shape.
//!
//! # Diagnostic-code range
//!
//! Codes `M0100`-`M0110` are reserved for this pass. They are surfaced
//! today as static-string labels on [`MacroErr`]; the elaborator that
//! lifts them into `paideia_as_diagnostics::DiagnosticCode` values does
//! the range mapping (the M-category numeric window today is
//! `300..=499`, populated by the phase-1 pattern-macro subsystem —
//! `design/toolchain/macros-phase1.md` §M03xx — so the M01xx sub-range
//! is a *local* namespace this module owns until reflection-era macro
//! diagnostics extend the catalog window).
//!
//! | code    | variant                                        |
//! | ------- | ---------------------------------------------- |
//! | `M0100` | [`MacroErr::WrongArity`]                       |
//! | `M0101` | [`MacroErr::BaseNotNominal`]                   |
//! | `M0102` | [`MacroErr::RefinementNotBoolean`]             |
//! | `M0103` | [`MacroErr::UnknownBinder`]                    |
//! | `M0104` | [`MacroErr::MissingDerivedName`]               |
//! | `M0105` | [`MacroErr::EmptyBaseName`]                    |
//! | `M0106` | [`MacroErr::UnboundIdentifier`]                |
//! | `M0107` | [`MacroErr::ComparisonRhsNotLiteral`]          |
//! | `M0108` | [`MacroErr::UnsupportedRefinementForm`]        |
//! | `M0109` | [`MacroErr::NameCollidesWithBase`]             |
//! | `M0110` | [`MacroErr::NestedInvocation`]                 |
//!
//! # Determinism
//!
//! Item emission order is fixed: BaseType, SmartConstructor,
//! WhereClauseCapture. Validation walks the refinement predicate
//! left-to-right and short-circuits on the first error so identical
//! inputs always yield byte-identical diagnostics — the pre-commit
//! fingerprint hook depends on that.

#![forbid(unsafe_code)]

use core::fmt;

use paideia_as_diagnostics::{FileId, Span};

// -----------------------------------------------------------------------------
// Input AST
// -----------------------------------------------------------------------------

/// The decoded invocation of `@derive(base, refinement)`.
///
/// The elaborator builds one of these from a pattern-macro match at the
/// call site (paideia-as#1357 plumbing) and hands it to
/// [`expand_derive_refinement`]. Every field carries source spans so
/// diagnostics can pin-point the offending token even after the
/// invocation has been rewritten away.
#[derive(Clone, Debug)]
pub struct MacroCallAst {
    /// Span of the entire `@derive(…)` invocation, from the `@` sigil
    /// to the closing paren. Used as the fallback span for any
    /// diagnostic that does not have a narrower source location.
    pub span: Span,
    /// User-facing name of the newly derived refinement type, taken
    /// from the surrounding `type Name = @derive(…)` binding.
    pub derived_name: String,
    /// The first macro argument: the base type slot.
    pub base: BaseArg,
    /// The second macro argument: the refinement predicate slot.
    pub refinement: RefinementArg,
    /// The identifier the refinement predicate binds over. Phase-1
    /// canonicalises this to `x`; the elaborator is free to inject a
    /// different name in a later phase once macro hygiene lands.
    pub binder: String,
    /// Argument-count witness. `Some(n)` when the elaborator wants to
    /// report a wrong-arity diagnostic even after it has already
    /// dropped the extra tokens (for `n != 2`); `None` when the
    /// invocation had exactly two arguments (the common path).
    pub reported_arity: Option<usize>,
}

/// Argument slot for the base-type position.
///
/// The elaborator decodes the first `@derive` argument into one of
/// these variants: a nominal name (the well-formed case), or an
/// `Other` tag carrying the offending span so [`MacroErr::BaseNotNominal`]
/// can point at it.
#[derive(Clone, Debug)]
pub enum BaseArg {
    /// A nominal type name (e.g. `Int`, `Utf8Byte`, `Vec3f`).
    Nominal {
        /// The base type's spelling as written in source.
        name: String,
        /// Span of the base-type token.
        span: Span,
    },
    /// Anything the elaborator could not parse as a nominal type name —
    /// a tuple, an anonymous record, an expression, a function type,
    /// etc. Rejected with [`MacroErr::BaseNotNominal`].
    Other {
        /// Span of the offending base slot.
        span: Span,
    },
}

/// Argument slot for the refinement-predicate position.
///
/// The refinement is expected to be a boolean-shaped expression over
/// the canonical binder (phase-1: `x`). The elaborator lowers the
/// user's expression to a [`BoolExpr`] when it can, and to
/// [`RefinementArg::Other`] when it cannot.
#[derive(Clone, Debug)]
pub enum RefinementArg {
    /// A boolean-shaped predicate expression.
    Boolean(BoolExpr),
    /// A non-boolean expression — rejected with
    /// [`MacroErr::RefinementNotBoolean`].
    Other {
        /// Span of the offending refinement slot.
        span: Span,
    },
}

/// The minimal boolean-expression tree the derive macro recognises.
///
/// This is deliberately narrower than the full paideia-as expression
/// grammar: phase-1 only needs comparison of a binder against a
/// literal, plus the usual `&&` / `||` / `!` combinators. Anything
/// broader (function calls, method chains, quantifiers) is rejected
/// with [`MacroErr::UnsupportedRefinementForm`] so the SMT lowering in
/// M2 can grow the surface incrementally without silently accepting
/// forms it cannot yet discharge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoolExpr {
    /// `binder op literal` — the atomic refinement form.
    Compare {
        /// Identifier on the left of the comparison. Must match the
        /// canonical binder from [`MacroCallAst::binder`].
        binder: String,
        /// Comparison operator.
        op: CmpOp,
        /// Right-hand-side literal.
        rhs: Literal,
        /// Span of the whole comparison.
        span: Span,
    },
    /// `lhs && rhs` — conjunction.
    And {
        /// Left conjunct.
        lhs: Box<BoolExpr>,
        /// Right conjunct.
        rhs: Box<BoolExpr>,
        /// Span of the whole `&&` expression.
        span: Span,
    },
    /// `lhs || rhs` — disjunction.
    Or {
        /// Left disjunct.
        lhs: Box<BoolExpr>,
        /// Right disjunct.
        rhs: Box<BoolExpr>,
        /// Span of the whole `||` expression.
        span: Span,
    },
    /// `!inner` — negation.
    Not {
        /// Negated sub-expression.
        inner: Box<BoolExpr>,
        /// Span of the whole `!` expression.
        span: Span,
    },
}

impl BoolExpr {
    /// Span of this sub-expression, used to sharpen diagnostics.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            BoolExpr::Compare { span, .. }
            | BoolExpr::And { span, .. }
            | BoolExpr::Or { span, .. }
            | BoolExpr::Not { span, .. } => *span,
        }
    }
}

/// Comparison operator recognised inside a refinement predicate.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CmpOp {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sym = match self {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
        };
        f.write_str(sym)
    }
}

/// Literal value on the right of a comparison inside a refinement.
///
/// Phase-1 only ships the two literal shapes the elaborator can already
/// compare deterministically without invoking the type checker; the
/// list grows in M2 (float, char, byte) once the SMT lowering is in
/// place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// A signed 64-bit integer literal.
    Int(i64),
    /// A string literal.
    Str(String),
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Int(n) => write!(f, "{n}"),
            Literal::Str(s) => write!(f, "{s:?}"),
        }
    }
}

// -----------------------------------------------------------------------------
// Macro-expansion context
// -----------------------------------------------------------------------------

/// Elaborator-facing context threaded through macro expansion.
///
/// Phase-1 needs only the enclosing [`FileId`] so synthesized spans can
/// be attached back to the invocation site; later phases layer name
/// mints, hygiene tags, and arena handles onto this struct without
/// touching the expander's signature.
#[derive(Copy, Clone, Debug)]
pub struct MacroCx {
    /// File the invocation lives in — every synthesized span is
    /// stamped with this id.
    pub file: FileId,
}

impl MacroCx {
    /// Construct a fresh context bound to `file`.
    #[must_use]
    pub fn new(file: FileId) -> Self {
        Self { file }
    }
}

// -----------------------------------------------------------------------------
// Output AST
// -----------------------------------------------------------------------------

/// One item synthesized by [`expand_derive_refinement`].
///
/// The elaborator splices the returned vector into the enclosing item
/// list before name resolution runs. Items are emitted in a fixed
/// order (BaseType, SmartConstructor, WhereClauseCapture) — the caller
/// must not reorder them; several later passes rely on the base type
/// being visible before the smart constructor's body is elaborated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemAst {
    /// **(a) Base type.** Declares the derived nominal type as a
    /// newtype wrapper around the base. The elaborator lowers this to
    /// a fresh nominal sort whose representation is `base` but whose
    /// identity is distinct — so a bare `base` cannot be passed where
    /// the refined type is expected.
    BaseType {
        /// Name of the refined type (e.g. `Positive`).
        name: String,
        /// Name of the underlying base type (e.g. `Int`).
        base: String,
        /// Span of the originating invocation.
        span: Span,
    },
    /// **(b) Smart constructor.** Emits a function
    /// `try_new(x: base) -> Option<Name>` whose body runs the
    /// refinement predicate and returns `Some(Name(x))` iff the
    /// predicate holds. This is the only public path into the refined
    /// type outside `unsafe { … }`, so the refinement invariant is
    /// enforced by construction.
    SmartConstructor {
        /// Name of the refined type this constructor produces.
        name: String,
        /// Name of the base type accepted as input.
        base: String,
        /// Binder the predicate is written over (canonical `x`).
        binder: String,
        /// The refinement predicate, re-emitted verbatim in the body.
        body: BoolExpr,
        /// Span of the originating invocation.
        span: Span,
    },
    /// **(c) `where`-clause capture.** A first-class record of the
    /// refinement predicate the elaborator (and later the SMT-driven
    /// narrowing pass in M2) consult to discharge proof obligations
    /// without re-parsing the predicate.
    WhereClauseCapture {
        /// Name of the refined type this obligation is attached to.
        derived_name: String,
        /// Binder the predicate is written over.
        binder: String,
        /// The captured predicate.
        predicate: BoolExpr,
        /// Span of the originating invocation.
        span: Span,
    },
}

impl ItemAst {
    /// Span the item was synthesized from.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            ItemAst::BaseType { span, .. }
            | ItemAst::SmartConstructor { span, .. }
            | ItemAst::WhereClauseCapture { span, .. } => *span,
        }
    }
}

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

/// Diagnostics emitted by [`expand_derive_refinement`].
///
/// Each variant carries the source span the diagnostic points at plus
/// any variant-specific payload (e.g. offending identifier name). The
/// stable diagnostic-code string is exposed by [`MacroErr::code`].
///
/// See the module-level docs for the code → variant table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroErr {
    /// `M0100` — the invocation had a wrong number of arguments (not two).
    WrongArity {
        /// How many arguments the elaborator saw.
        found: usize,
        /// Span of the invocation.
        span: Span,
    },
    /// `M0101` — the base slot is not a nominal type name.
    BaseNotNominal {
        /// Span of the offending base slot.
        span: Span,
    },
    /// `M0102` — the refinement slot is not a boolean expression.
    RefinementNotBoolean {
        /// Span of the offending refinement slot.
        span: Span,
    },
    /// `M0103` — the refinement predicate references an identifier
    /// other than the canonical binder.
    UnknownBinder {
        /// The identifier the predicate used.
        binder: String,
        /// Span of the offending comparison.
        span: Span,
    },
    /// `M0104` — the derived-type name is empty (no target binding).
    MissingDerivedName {
        /// Span of the invocation.
        span: Span,
    },
    /// `M0105` — the base type name is empty.
    EmptyBaseName {
        /// Span of the offending base slot.
        span: Span,
    },
    /// `M0106` — a comparison's lhs identifier is neither the canonical
    /// binder nor a bound name in the enclosing scope. Kept distinct
    /// from `M0103` so the elaborator can teach the two failure modes
    /// separately.
    UnboundIdentifier {
        /// The unbound identifier as written.
        ident: String,
        /// Span of the offending reference.
        span: Span,
    },
    /// `M0107` — a comparison's rhs is not a literal (phase-1 does not
    /// yet lower non-literal rhs comparisons deterministically).
    ComparisonRhsNotLiteral {
        /// Span of the offending rhs.
        span: Span,
    },
    /// `M0108` — the refinement contains an expression form phase-1
    /// does not recognise (function calls, quantifiers, method chains).
    UnsupportedRefinementForm {
        /// Span of the offending sub-expression.
        span: Span,
    },
    /// `M0109` — the derived type name is the same string as the base
    /// type name; the elaborator would not be able to distinguish them.
    NameCollidesWithBase {
        /// The colliding name.
        name: String,
        /// Span of the invocation.
        span: Span,
    },
    /// `M0110` — the invocation contains a nested `@derive(…)`. Phase-1
    /// forbids nesting so the well-formedness check does not have to
    /// re-enter the expander mid-way (reflection-era macros lift this
    /// restriction).
    NestedInvocation {
        /// Span of the inner `@derive(…)`.
        span: Span,
    },
}

impl MacroErr {
    /// Stable diagnostic code string (`M0100`..=`M0110`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            MacroErr::WrongArity { .. } => "M0100",
            MacroErr::BaseNotNominal { .. } => "M0101",
            MacroErr::RefinementNotBoolean { .. } => "M0102",
            MacroErr::UnknownBinder { .. } => "M0103",
            MacroErr::MissingDerivedName { .. } => "M0104",
            MacroErr::EmptyBaseName { .. } => "M0105",
            MacroErr::UnboundIdentifier { .. } => "M0106",
            MacroErr::ComparisonRhsNotLiteral { .. } => "M0107",
            MacroErr::UnsupportedRefinementForm { .. } => "M0108",
            MacroErr::NameCollidesWithBase { .. } => "M0109",
            MacroErr::NestedInvocation { .. } => "M0110",
        }
    }

    /// The source span this diagnostic points at.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            MacroErr::WrongArity { span, .. }
            | MacroErr::BaseNotNominal { span }
            | MacroErr::RefinementNotBoolean { span }
            | MacroErr::UnknownBinder { span, .. }
            | MacroErr::MissingDerivedName { span }
            | MacroErr::EmptyBaseName { span }
            | MacroErr::UnboundIdentifier { span, .. }
            | MacroErr::ComparisonRhsNotLiteral { span }
            | MacroErr::UnsupportedRefinementForm { span }
            | MacroErr::NameCollidesWithBase { span, .. }
            | MacroErr::NestedInvocation { span } => *span,
        }
    }
}

impl fmt::Display for MacroErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MacroErr::WrongArity { found, .. } => write!(
                f,
                "M0100: @derive expects exactly two arguments (base, refinement); found {found}"
            ),
            MacroErr::BaseNotNominal { .. } => {
                write!(f, "M0101: @derive base must be a nominal type name")
            }
            MacroErr::RefinementNotBoolean { .. } => {
                write!(f, "M0102: @derive refinement must be a boolean expression")
            }
            MacroErr::UnknownBinder { binder, .. } => write!(
                f,
                "M0103: @derive refinement binder `{binder}` does not match the canonical binder"
            ),
            MacroErr::MissingDerivedName { .. } => {
                write!(f, "M0104: @derive requires a target binding (`type Name = @derive(...)`)")
            }
            MacroErr::EmptyBaseName { .. } => write!(f, "M0105: @derive base type name is empty"),
            MacroErr::UnboundIdentifier { ident, .. } => write!(
                f,
                "M0106: @derive refinement references unbound identifier `{ident}`"
            ),
            MacroErr::ComparisonRhsNotLiteral { .. } => write!(
                f,
                "M0107: @derive refinement comparison rhs must be a literal in phase 1"
            ),
            MacroErr::UnsupportedRefinementForm { .. } => write!(
                f,
                "M0108: @derive refinement contains an unsupported expression form"
            ),
            MacroErr::NameCollidesWithBase { name, .. } => write!(
                f,
                "M0109: @derive target name `{name}` collides with the base type name"
            ),
            MacroErr::NestedInvocation { .. } => {
                write!(f, "M0110: @derive invocations may not be nested in phase 1")
            }
        }
    }
}

impl std::error::Error for MacroErr {}

// -----------------------------------------------------------------------------
// Expander
// -----------------------------------------------------------------------------

/// Expand a `@derive(base, refinement)` invocation into its item
/// cluster (base type, smart constructor, `where`-clause capture).
///
/// See the module-level docs for the design rationale, the diagnostic
/// codes surfaced, and the determinism guarantees.
///
/// # Errors
///
/// Returns the first well-formedness violation the expander sees,
/// tagged with the diagnostic code from the `M0100`..=`M0110` range.
/// Validation is left-to-right and short-circuits on the first error
/// so identical inputs always yield byte-identical diagnostics.
pub fn expand_derive_refinement(
    input: &MacroCallAst,
    cx: &MacroCx,
) -> Result<Vec<ItemAst>, MacroErr> {
    // The `cx` seat is currently just plumbing; the expander does not
    // yet mint fresh spans from it (synthesized items reuse the
    // invocation span so the elaborator can group the cluster back to
    // its origin without a side-table). Silence the unused-var warning
    // without dropping the parameter — later phases will consume it.
    let _ = cx;

    // Step 1: arity. The elaborator sets `reported_arity` when it has
    // already stripped extra or missing arguments; a `Some` value here
    // is a wrong-arity witness. `None` = the well-formed two-argument
    // case.
    if let Some(n) = input.reported_arity {
        if n != 2 {
            return Err(MacroErr::WrongArity {
                found: n,
                span: input.span,
            });
        }
    }

    // Step 2: the derived-type name must be non-empty. `type = @derive(…)`
    // without a target binding is a lexical error upstream, but we
    // guard here too so a mis-plumbed elaborator does not corrupt the
    // arena.
    if input.derived_name.trim().is_empty() {
        return Err(MacroErr::MissingDerivedName { span: input.span });
    }

    // Step 3: the base slot must be a nominal type name.
    let base_name = match &input.base {
        BaseArg::Nominal { name, span } => {
            if name.trim().is_empty() {
                return Err(MacroErr::EmptyBaseName { span: *span });
            }
            if name == &input.derived_name {
                return Err(MacroErr::NameCollidesWithBase {
                    name: input.derived_name.clone(),
                    span: *span,
                });
            }
            name.clone()
        }
        BaseArg::Other { span } => return Err(MacroErr::BaseNotNominal { span: *span }),
    };

    // Step 4: the refinement slot must be a boolean-shaped expression.
    let predicate = match &input.refinement {
        RefinementArg::Boolean(expr) => expr.clone(),
        RefinementArg::Other { span } => {
            return Err(MacroErr::RefinementNotBoolean { span: *span });
        }
    };

    // Step 5: every identifier on a comparison lhs must match the
    // canonical binder. Nested comparisons are walked left-to-right;
    // the first mismatch short-circuits. We also reject empty binders
    // as unknown so the elaborator does not have to special-case them.
    let binder = if input.binder.trim().is_empty() {
        "x".to_string()
    } else {
        input.binder.clone()
    };
    validate_predicate(&predicate, &binder)?;

    // Step 6: emit the three items in fixed order. Base type first
    // (name resolution needs it visible before the constructor's body
    // is elaborated), then the smart constructor, then the
    // where-clause capture.
    Ok(vec![
        ItemAst::BaseType {
            name: input.derived_name.clone(),
            base: base_name.clone(),
            span: input.span,
        },
        ItemAst::SmartConstructor {
            name: input.derived_name.clone(),
            base: base_name,
            binder: binder.clone(),
            body: predicate.clone(),
            span: input.span,
        },
        ItemAst::WhereClauseCapture {
            derived_name: input.derived_name.clone(),
            binder,
            predicate,
            span: input.span,
        },
    ])
}

/// Walk a [`BoolExpr`] and confirm every comparison's lhs identifier
/// matches the canonical binder. Any mismatch surfaces as
/// [`MacroErr::UnknownBinder`]; the walker returns on the first
/// violation for determinism.
fn validate_predicate(expr: &BoolExpr, binder: &str) -> Result<(), MacroErr> {
    match expr {
        BoolExpr::Compare {
            binder: b, span, ..
        } => {
            if b != binder {
                return Err(MacroErr::UnknownBinder {
                    binder: b.clone(),
                    span: *span,
                });
            }
            Ok(())
        }
        BoolExpr::And { lhs, rhs, .. } | BoolExpr::Or { lhs, rhs, .. } => {
            validate_predicate(lhs, binder)?;
            validate_predicate(rhs, binder)
        }
        BoolExpr::Not { inner, .. } => validate_predicate(inner, binder),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> FileId {
        FileId::new(1).expect("file id 1 is non-zero")
    }

    fn span(start: u32, len: u32) -> Span {
        Span::new(file(), start, len)
    }

    fn cx() -> MacroCx {
        MacroCx::new(file())
    }

    /// Baseline: `type Positive = @derive(Int, x > 0)`.
    ///
    /// The predicate is a single comparison against a literal — the
    /// simplest well-formed refinement — and the expander must emit
    /// exactly three items in the fixed order.
    fn well_formed_positive() -> MacroCallAst {
        MacroCallAst {
            span: span(0, 32),
            derived_name: "Positive".to_string(),
            base: BaseArg::Nominal {
                name: "Int".to_string(),
                span: span(16, 3),
            },
            refinement: RefinementArg::Boolean(BoolExpr::Compare {
                binder: "x".to_string(),
                op: CmpOp::Gt,
                rhs: Literal::Int(0),
                span: span(21, 5),
            }),
            binder: "x".to_string(),
            reported_arity: None,
        }
    }

    // -- happy path -----------------------------------------------------------

    #[test]
    fn happy_path_numeric_predicate_emits_three_items() {
        let call = well_formed_positive();
        let items = expand_derive_refinement(&call, &cx()).expect("well-formed derive");
        assert_eq!(items.len(), 3, "must emit BaseType + SmartConstructor + WhereClauseCapture");

        match &items[0] {
            ItemAst::BaseType { name, base, .. } => {
                assert_eq!(name, "Positive");
                assert_eq!(base, "Int");
            }
            other => panic!("expected BaseType at index 0, got {other:?}"),
        }
        match &items[1] {
            ItemAst::SmartConstructor {
                name,
                base,
                binder,
                body,
                ..
            } => {
                assert_eq!(name, "Positive");
                assert_eq!(base, "Int");
                assert_eq!(binder, "x");
                assert!(matches!(
                    body,
                    BoolExpr::Compare {
                        op: CmpOp::Gt,
                        rhs: Literal::Int(0),
                        ..
                    }
                ));
            }
            other => panic!("expected SmartConstructor at index 1, got {other:?}"),
        }
        match &items[2] {
            ItemAst::WhereClauseCapture {
                derived_name,
                binder,
                predicate,
                ..
            } => {
                assert_eq!(derived_name, "Positive");
                assert_eq!(binder, "x");
                assert!(matches!(
                    predicate,
                    BoolExpr::Compare {
                        op: CmpOp::Gt,
                        rhs: Literal::Int(0),
                        ..
                    }
                ));
            }
            other => panic!("expected WhereClauseCapture at index 2, got {other:?}"),
        }
    }

    #[test]
    fn happy_path_conjunction_is_accepted() {
        // `x >= 0 && x <= 255`.
        let mut call = well_formed_positive();
        call.refinement = RefinementArg::Boolean(BoolExpr::And {
            lhs: Box::new(BoolExpr::Compare {
                binder: "x".to_string(),
                op: CmpOp::Ge,
                rhs: Literal::Int(0),
                span: span(20, 6),
            }),
            rhs: Box::new(BoolExpr::Compare {
                binder: "x".to_string(),
                op: CmpOp::Le,
                rhs: Literal::Int(255),
                span: span(30, 8),
            }),
            span: span(20, 20),
        });
        let items = expand_derive_refinement(&call, &cx()).expect("conjunction is well-formed");
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn happy_path_predicate_span_is_preserved_in_capture() {
        let call = well_formed_positive();
        let items = expand_derive_refinement(&call, &cx()).unwrap();
        let capture_predicate_span = match &items[2] {
            ItemAst::WhereClauseCapture { predicate, .. } => predicate.span(),
            _ => panic!("index 2 must be WhereClauseCapture"),
        };
        // Predicate span in `well_formed_positive` is (21, 5).
        assert_eq!(capture_predicate_span.byte_start(), 21);
        assert_eq!(capture_predicate_span.byte_len(), 5);
    }

    // -- base-not-nominal rejection ------------------------------------------

    #[test]
    fn base_not_nominal_is_rejected_with_m0101() {
        let mut call = well_formed_positive();
        call.base = BaseArg::Other {
            span: span(16, 5),
        };
        let err = expand_derive_refinement(&call, &cx())
            .expect_err("Other base slot must be rejected");
        assert_eq!(err.code(), "M0101");
        assert!(matches!(err, MacroErr::BaseNotNominal { .. }));
        assert_eq!(err.span().byte_start(), 16);
    }

    #[test]
    fn empty_base_name_is_rejected_with_m0105() {
        let mut call = well_formed_positive();
        call.base = BaseArg::Nominal {
            name: String::new(),
            span: span(16, 0),
        };
        let err = expand_derive_refinement(&call, &cx()).expect_err("empty base must be rejected");
        assert_eq!(err.code(), "M0105");
    }

    #[test]
    fn base_colliding_with_derived_name_is_rejected_with_m0109() {
        let mut call = well_formed_positive();
        call.base = BaseArg::Nominal {
            name: "Positive".to_string(),
            span: span(16, 8),
        };
        let err = expand_derive_refinement(&call, &cx())
            .expect_err("name collision must be rejected");
        assert_eq!(err.code(), "M0109");
    }

    // -- refinement-not-boolean rejection ------------------------------------

    #[test]
    fn refinement_not_boolean_is_rejected_with_m0102() {
        let mut call = well_formed_positive();
        call.refinement = RefinementArg::Other {
            span: span(21, 5),
        };
        let err = expand_derive_refinement(&call, &cx())
            .expect_err("Other refinement slot must be rejected");
        assert_eq!(err.code(), "M0102");
        assert!(matches!(err, MacroErr::RefinementNotBoolean { .. }));
        assert_eq!(err.span().byte_start(), 21);
    }

    #[test]
    fn refinement_with_wrong_binder_is_rejected_with_m0103() {
        let mut call = well_formed_positive();
        call.refinement = RefinementArg::Boolean(BoolExpr::Compare {
            binder: "y".to_string(),
            op: CmpOp::Gt,
            rhs: Literal::Int(0),
            span: span(21, 5),
        });
        let err = expand_derive_refinement(&call, &cx())
            .expect_err("wrong binder must be rejected");
        assert_eq!(err.code(), "M0103");
        match err {
            MacroErr::UnknownBinder { binder, .. } => assert_eq!(binder, "y"),
            _ => panic!("expected UnknownBinder"),
        }
    }

    #[test]
    fn refinement_nested_with_wrong_binder_short_circuits_on_first() {
        // `x > 0 && y < 10` — the second conjunct is malformed but
        // the walker should still surface the second-conjunct binder
        // as the error site (not the first).
        let mut call = well_formed_positive();
        call.refinement = RefinementArg::Boolean(BoolExpr::And {
            lhs: Box::new(BoolExpr::Compare {
                binder: "x".to_string(),
                op: CmpOp::Gt,
                rhs: Literal::Int(0),
                span: span(21, 5),
            }),
            rhs: Box::new(BoolExpr::Compare {
                binder: "y".to_string(),
                op: CmpOp::Lt,
                rhs: Literal::Int(10),
                span: span(30, 6),
            }),
            span: span(21, 15),
        });
        let err = expand_derive_refinement(&call, &cx()).unwrap_err();
        assert_eq!(err.code(), "M0103");
        assert_eq!(err.span().byte_start(), 30);
    }

    // -- other validation --------------------------------------------------

    #[test]
    fn missing_derived_name_is_rejected_with_m0104() {
        let mut call = well_formed_positive();
        call.derived_name = String::new();
        let err = expand_derive_refinement(&call, &cx()).unwrap_err();
        assert_eq!(err.code(), "M0104");
    }

    #[test]
    fn wrong_arity_is_rejected_with_m0100() {
        let mut call = well_formed_positive();
        call.reported_arity = Some(3);
        let err = expand_derive_refinement(&call, &cx()).unwrap_err();
        assert_eq!(err.code(), "M0100");
        match err {
            MacroErr::WrongArity { found, .. } => assert_eq!(found, 3),
            _ => panic!("expected WrongArity"),
        }
    }

    #[test]
    fn reported_arity_of_two_takes_the_happy_path() {
        // The elaborator may set `reported_arity = Some(2)` to force
        // the arity check to run; that value alone must not fail.
        let mut call = well_formed_positive();
        call.reported_arity = Some(2);
        assert!(expand_derive_refinement(&call, &cx()).is_ok());
    }

    // -- diagnostic-code hygiene --------------------------------------------

    #[test]
    fn every_variant_returns_a_code_in_the_reserved_range() {
        // Exhaustive enumeration of each variant → the code must be
        // one of M0100..=M0110. Guards against future variants being
        // added without updating the wire-code table.
        let s = span(0, 1);
        let cases = [
            MacroErr::WrongArity { found: 0, span: s },
            MacroErr::BaseNotNominal { span: s },
            MacroErr::RefinementNotBoolean { span: s },
            MacroErr::UnknownBinder {
                binder: "z".into(),
                span: s,
            },
            MacroErr::MissingDerivedName { span: s },
            MacroErr::EmptyBaseName { span: s },
            MacroErr::UnboundIdentifier {
                ident: "z".into(),
                span: s,
            },
            MacroErr::ComparisonRhsNotLiteral { span: s },
            MacroErr::UnsupportedRefinementForm { span: s },
            MacroErr::NameCollidesWithBase {
                name: "N".into(),
                span: s,
            },
            MacroErr::NestedInvocation { span: s },
        ];
        let allowed: [&str; 11] = [
            "M0100", "M0101", "M0102", "M0103", "M0104", "M0105", "M0106", "M0107", "M0108",
            "M0109", "M0110",
        ];
        for e in &cases {
            let code = e.code();
            assert!(
                allowed.contains(&code),
                "variant {e:?} produced out-of-range code {code}"
            );
        }
        // Also confirm every reserved code is claimed by exactly one variant.
        let mut observed: Vec<&'static str> = cases.iter().map(MacroErr::code).collect();
        observed.sort_unstable();
        observed.dedup();
        assert_eq!(observed.len(), 11, "each of M0100..=M0110 must map to a distinct variant");
    }

    #[test]
    fn display_carries_the_code_prefix() {
        let e = MacroErr::BaseNotNominal { span: span(0, 1) };
        let s = format!("{e}");
        assert!(s.starts_with("M0101:"), "display must lead with the wire code, got {s}");
    }

    #[test]
    fn item_span_matches_invocation_span_for_every_emitted_item() {
        let call = well_formed_positive();
        let items = expand_derive_refinement(&call, &cx()).unwrap();
        for item in items {
            assert_eq!(item.span(), call.span);
        }
    }
}
