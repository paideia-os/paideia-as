//! R225.M7: HM error reporting — `TypeError` with span attribution and
//! diagnostic context.
//!
//! The M1–M6 layers surface unification failures as `InferError` /
//! `UnifyError` — algebraic terms in the type-theory vocabulary, no
//! notion of where in the surface a failure came from and no room for
//! an authoring-side breadcrumb. R225.M7 wraps that layer without
//! reshaping it: [`TypeDiagnostic`] pins an optional source span and a
//! free-form context string onto whatever the inference driver emits,
//! and [`infer_with_diagnostic`] is a thin adapter over
//! [`crate::infer::infer`] that produces the wrapped result.
//!
//! # Lineage
//!
//! * Rustc's `Diagnostic` shape (a machine-readable primary code plus a
//!   human-readable label at a `Span`) is the direct model; the same
//!   split — algebraic error carried through, presentation layered on
//!   top — keeps this crate's public error type stable across releases
//!   while the surface diagnostic can grow (labels, hints, notes) in
//!   future milestones.
//! * Reynolds 1998, *Theories of Programming Languages*, §16 discusses
//!   the value of separating the term-level error algebra from its
//!   presentation for post-hoc rendering; that separation is what makes
//!   the M1 test corpus still valid unchanged at M7.
//!
//! # Non-goals at M7
//!
//! * No parser integration — the caller supplies spans; this crate
//!   never inspects source bytes. The shell frontend (R229) will wire
//!   its own byte-offset spans into these fields.
//! * No multi-note rendering — a diagnostic carries one span and one
//!   context string. Structured hints, related-spans and secondary
//!   labels are deferred to a later milestone once the surface UX has
//!   opinions on them.
//! * No colouring / ANSI — [`render_diagnostic`] emits plain UTF-8; the
//!   presentation layer (REPL, LSP, batch compiler) applies its own
//!   styling.

use crate::expr::Expr;
use crate::infer::{infer, FreshVarGen, InferError, TypeEnv};
use crate::subst::Substitution;
use crate::ty::MonoType;

/// A half-open byte span `[start, end)` into the source text that
/// produced the expression whose inference failed.
///
/// The span carries no reference back to the source buffer — it is
/// purely a pair of byte offsets. Interpretation (which file, which
/// slice) is the caller's business; this crate never opens a source
/// file. Zero-length spans (`start == end`) are legal — they mark a
/// point (e.g. a missing token) rather than a range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeSpan {
    /// Byte offset (inclusive) of the span's first character.
    pub start: usize,
    /// Byte offset (exclusive) one past the span's last character.
    pub end: usize,
}

impl TypeSpan {
    /// Build a span from its two byte offsets.
    ///
    /// No validation of `start <= end` is performed here — the span is
    /// a passive datum, and a mis-ordered pair is the caller's bug to
    /// notice. This mirrors `Range<usize>` in the standard library.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A type error paired with optional presentation context.
///
/// The `error` field carries the algebraic term unchanged from the
/// inference driver — pattern-matching on it works identically to
/// `InferError` at the M1–M6 layers. The `span` and `context` fields
/// are M7 additions that a caller composes onto the error at the point
/// it enters a presentation surface (REPL, LSP notification, batch
/// compiler diagnostic stream).
///
/// A missing span or empty context renders as if the field were not
/// present at all — see [`render_diagnostic`] for the exact grammar.
#[derive(Clone, Debug)]
pub struct TypeDiagnostic {
    /// The underlying inference failure, unmodified from the M1 driver.
    pub error: InferError,
    /// The source span the failure is attached to, if any.
    pub span: Option<TypeSpan>,
    /// A free-form breadcrumb — typically the enclosing form's name
    /// ("in body of `foo`", "at pipeline stage 2", …). Empty means no
    /// context is attached.
    pub context: String,
}

impl TypeDiagnostic {
    /// Construct a bare diagnostic carrying only the inference error.
    ///
    /// The span is left `None` and the context an empty string; both
    /// can be filled in fluently via [`with_span`](Self::with_span)
    /// and [`with_context`](Self::with_context).
    pub fn new(error: InferError) -> Self {
        Self {
            error,
            span: None,
            context: String::new(),
        }
    }

    /// Attach a span, returning the updated diagnostic.
    #[must_use]
    pub fn with_span(mut self, span: TypeSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a context string, returning the updated diagnostic. Any
    /// type that converts into `String` is accepted so both `&str` and
    /// owned strings compose without an explicit `.to_string()`.
    #[must_use]
    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context = ctx.into();
        self
    }
}

/// Render a diagnostic to a plain UTF-8 line.
///
/// The grammar is fixed so downstream diff-based tests (REPL golden
/// output, LSP notification snapshots) can rely on a stable shape:
///
/// | span      | context   | rendering                                 |
/// | --------- | --------- | ----------------------------------------- |
/// | `Some(_)` | non-empty | `"at {start}..{end}: {context}: {error}"` |
/// | `Some(_)` | empty     | `"at {start}..{end}: {error}"`            |
/// | `None`    | non-empty | `"{context}: {error}"`                    |
/// | `None`    | empty     | `"{error}"`                               |
///
/// The `{error}` slot delegates to `InferError`'s `Display` impl, which
/// in turn delegates to `UnifyError`'s — so every M1–M6 error variant
/// renders through its existing text, unmodified.
pub fn render_diagnostic(diag: &TypeDiagnostic) -> String {
    let has_context = !diag.context.is_empty();
    match (diag.span, has_context) {
        (Some(sp), true) => format!(
            "at {}..{}: {}: {}",
            sp.start, sp.end, diag.context, diag.error
        ),
        (Some(sp), false) => format!("at {}..{}: {}", sp.start, sp.end, diag.error),
        (None, true) => format!("{}: {}", diag.context, diag.error),
        (None, false) => format!("{}", diag.error),
    }
}

/// Run [`infer`] and wrap any failure in a [`TypeDiagnostic`] carrying
/// the caller-supplied span and context.
///
/// Success is passed through unmodified — the diagnostic surface is
/// only material on the error path. On failure, the underlying
/// `InferError` is preserved inside the returned diagnostic so a
/// consumer that wants to pattern-match on the algebraic error can do
/// so (`diag.error`) exactly as it would against the bare `infer`
/// result.
///
/// This adapter is deliberately tiny: it never inspects the error, and
/// it never mints its own spans. Both are the caller's business.
///
/// # Errors
///
/// Returns a [`TypeDiagnostic`] whose `error` field is whatever
/// [`infer`] surfaced (an `UnboundVar` or a wrapped `UnifyError`), and
/// whose `span` / `context` fields are as supplied by the caller.
pub fn infer_with_diagnostic(
    env: &TypeEnv,
    expr: &Expr,
    fresh: &mut FreshVarGen,
    span: Option<TypeSpan>,
    context: impl Into<String>,
) -> Result<(Substitution, MonoType), TypeDiagnostic> {
    let context = context.into();
    infer(env, expr, fresh).map_err(|err| TypeDiagnostic {
        error: err,
        span,
        context,
    })
}
