//! R225.M7 diagnostic-surface fixture corpus.
//!
//! Eight tests, tagged `r225m7-diag-01`..`r225m7-diag-08`. Each fixture
//! exercises one behaviour of the [`crate::diagnostic`] layer:
//!
//! * `01` — the success passthrough (a well-typed expression never
//!   materialises a `TypeDiagnostic`).
//! * `02..03` — real inference failures (`UnboundVar`, structural
//!   `Mismatch`) surface through [`infer_with_diagnostic`] with the
//!   caller-supplied span / context intact.
//! * `04..06` — the [`render_diagnostic`] grammar: all four
//!   presence-permutation cases of `(span, context)`.
//! * `07..08` — the exotic `UnifyError` variants (`RowMismatch`,
//!   `TypedValueMismatch`) render through the diagnostic surface. Both
//!   are constructed directly via [`TypeDiagnostic::new`] rather than
//!   through an `Expr` fixture — triggering them via `infer` requires
//!   the caller to construct rows-with-shared-tails or typed values
//!   that the M1 `Expr` grammar does not surface literals for, and the
//!   M7 layer's job is to render whatever error the layer beneath it
//!   emits, not to enumerate every path that emits each variant. That
//!   coverage lives in the R225.M2 / M3 / M5 corpora.

use paideia_as_shell_hm::{
    app, i, infer_with_diagnostic, lam, let_, render_diagnostic, s, v, EffectRow, FreshVarGen,
    InferError, RowType, TypeDiagnostic, TypeEnv, TypeSpan, TypeVar, TypedValue, UnifyError,
};

// ---------------------------------------------------------------------
// 01 — success passthrough: no diagnostic materialises for well-typed
// input, even when a span / context is offered.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_01_success_passthrough() {
    // (\x -> x) 42  :  Int   — well-typed, must succeed.
    let expr = app(lam("x", v("x")), i());
    let env = TypeEnv::new();
    let mut fresh = FreshVarGen::new();
    let result = infer_with_diagnostic(
        &env,
        &expr,
        &mut fresh,
        Some(TypeSpan::new(0, 6)),
        "top-level",
    );
    match result {
        Ok(_) => {}
        Err(diag) => panic!(
            "r225m7-diag-01: expected Ok, got diagnostic {}",
            render_diagnostic(&diag)
        ),
    }
}

// ---------------------------------------------------------------------
// 02 — an unbound variable surfaces as InferError::UnboundVar carried
// inside a TypeDiagnostic that preserves the caller's span.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_02_unbound_var_span_propagates() {
    let expr = v("nope");
    let env = TypeEnv::new();
    let mut fresh = FreshVarGen::new();
    let span = TypeSpan::new(4, 8);
    let diag = infer_with_diagnostic(&env, &expr, &mut fresh, Some(span), "lookup site")
        .expect_err("r225m7-diag-02: must fail on unbound var");
    match &diag.error {
        InferError::UnboundVar(name) => {
            assert_eq!(name, "nope", "r225m7-diag-02: name preserved");
        }
        other => panic!("r225m7-diag-02: expected UnboundVar, got {other:?}"),
    }
    assert_eq!(
        diag.span,
        Some(span),
        "r225m7-diag-02: caller span survives the wrap"
    );
    assert_eq!(
        diag.context, "lookup site",
        "r225m7-diag-02: context survives the wrap"
    );
    let rendered = render_diagnostic(&diag);
    assert!(
        rendered.contains("at 4..8:"),
        "r225m7-diag-02: rendered span prefix, got {rendered}"
    );
    assert!(
        rendered.contains("lookup site"),
        "r225m7-diag-02: rendered context, got {rendered}"
    );
    assert!(
        rendered.contains("nope"),
        "r225m7-diag-02: rendered underlying error text, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// 03 — a real UnifyError (structural mismatch) surfaces through the
// diagnostic surface unmodified.
//
// `app(i(), s())` — apply an integer to a string. The function
// position forces the M1 driver to unify `Int` with `Str -> r`, which
// fails as Mismatch(Con("Int"), Arrow(_, _)).
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_03_unify_mismatch_surfaces() {
    let expr = app(i(), s());
    let env = TypeEnv::new();
    let mut fresh = FreshVarGen::new();
    let diag = infer_with_diagnostic(&env, &expr, &mut fresh, None, "")
        .expect_err("r225m7-diag-03: applying Int to Str must fail");
    match &diag.error {
        InferError::UnifyError(UnifyError::Mismatch { .. }) => {}
        other => panic!("r225m7-diag-03: expected UnifyError::Mismatch, got {other:?}"),
    }
    let rendered = render_diagnostic(&diag);
    assert!(
        rendered.contains("cannot unify"),
        "r225m7-diag-03: rendered mismatch text, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// 04 — full rendering: span AND context both present. The grammar is
// `"at S..E: CTX: ERR"`; every fragment must appear in order.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_04_render_span_and_context() {
    let diag = TypeDiagnostic::new(InferError::UnboundVar("foo".to_owned()))
        .with_span(TypeSpan::new(1, 5))
        .with_context("in body of foo");
    let rendered = render_diagnostic(&diag);
    assert!(
        rendered.starts_with("at 1..5: in body of foo:"),
        "r225m7-diag-04: prefix, got {rendered}"
    );
    assert!(
        rendered.contains("unbound variable"),
        "r225m7-diag-04: underlying error text, got {rendered}"
    );
    assert!(
        rendered.contains("foo"),
        "r225m7-diag-04: variable name, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// 05 — span present, context empty: prefix appears, no trailing
// "context:" segment.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_05_render_span_only() {
    let diag = TypeDiagnostic::new(InferError::UnboundVar("bar".to_owned()))
        .with_span(TypeSpan::new(10, 13));
    let rendered = render_diagnostic(&diag);
    assert_eq!(
        rendered, "at 10..13: unbound variable `bar`",
        "r225m7-diag-05: exact rendering"
    );
}

// ---------------------------------------------------------------------
// 06 — context present, span absent: "CTX: ERR", no "at ...:" prefix.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_06_render_context_only() {
    let diag = TypeDiagnostic::new(InferError::UnboundVar("baz".to_owned()))
        .with_context("at REPL prompt");
    let rendered = render_diagnostic(&diag);
    assert_eq!(
        rendered, "at REPL prompt: unbound variable `baz`",
        "r225m7-diag-06: exact rendering"
    );
    assert!(
        !rendered.starts_with("at 0.."),
        "r225m7-diag-06: no numeric span prefix, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// Also cover the neither-present case here to keep the grammar table
// exhaustively exercised; a fourth fixture would be a marginal
// duplicate of the primary Display path already exercised by the
// M1–M6 tests.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_06b_render_neither() {
    let diag = TypeDiagnostic::new(InferError::UnboundVar("qux".to_owned()));
    let rendered = render_diagnostic(&diag);
    assert_eq!(
        rendered, "unbound variable `qux`",
        "r225m7-diag-06b: neither prefix nor context, just the error"
    );
}

// ---------------------------------------------------------------------
// 07 — RowMismatch trip through the diagnostic surface. Constructed
// directly per the fixture-block escape hatch: reaching this variant
// from an `Expr` requires a receiver whose inferred type carries the
// same row-var on both sides with extras — an artefact the M1 surface
// does not literalise. The M7 layer's contract is that it renders
// whatever `UnifyError` variant the layer beneath emits.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_07_row_mismatch_renders() {
    let diag = TypeDiagnostic::new(InferError::UnifyError(UnifyError::RowMismatch {
        reason: "row variable `?r0` appears on both sides with incompatible extras".to_owned(),
    }))
    .with_span(TypeSpan::new(20, 30))
    .with_context("record projection");
    // Sanity: the row itself is representable and unifies with itself
    // trivially — we build one only to exercise the crate re-exports.
    let _row_marker = RowType::RowVar(TypeVar(0));
    let rendered = render_diagnostic(&diag);
    assert!(
        rendered.starts_with("at 20..30: record projection:"),
        "r225m7-diag-07: prefix, got {rendered}"
    );
    assert!(
        rendered.contains("row unification failed"),
        "r225m7-diag-07: rendered RowMismatch text, got {rendered}"
    );
    assert!(
        rendered.contains("incompatible extras"),
        "r225m7-diag-07: reason preserved, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// 08 — TypedValueMismatch through the diagnostic surface. Same
// escape-hatch reasoning as fixture 07: the algebraic error is
// constructed directly, and we assert the M7 render produces a
// legible one-line diagnostic that names both sides.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_08_typed_value_mismatch_renders() {
    let inner = UnifyError::EffectRowMismatch {
        missing: vec!["fs".to_owned()],
        extra: vec!["io".to_owned()],
    };
    let diag = TypeDiagnostic::new(InferError::UnifyError(UnifyError::TypedValueMismatch {
        value_err: None,
        effect_err: Some(Box::new(inner)),
    }))
    .with_context("pipeline stage");
    // Sanity: the TypedValue / EffectRow re-exports resolve — the
    // marker builds the same shape the diagnostic is describing.
    let _tv_marker = TypedValue::with_effect(RowType::Empty, EffectRow::empty());
    let rendered = render_diagnostic(&diag);
    assert!(
        rendered.starts_with("pipeline stage:"),
        "r225m7-diag-08: prefix, got {rendered}"
    );
    assert!(
        rendered.contains("typed value mismatch"),
        "r225m7-diag-08: rendered TypedValueMismatch text, got {rendered}"
    );
    assert!(
        rendered.contains("effect:"),
        "r225m7-diag-08: names the effect side, got {rendered}"
    );
    assert!(
        !rendered.contains("value:"),
        "r225m7-diag-08: value side was None, must not appear, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// Coverage note (aside): let-binding paths route their inner failure
// through the same `infer` entry point, so we sanity-check that a
// failure inside a `let` body still surfaces the caller's context.
// This is not a numbered fixture — it exists to catch a regression in
// which the adapter shadowed the caller's context.
// ---------------------------------------------------------------------

#[test]
fn r225m7_diag_let_body_context_survives() {
    // let x = 1 in y     — `y` is unbound in the body.
    let expr = let_("x", i(), v("y"));
    let env = TypeEnv::new();
    let mut fresh = FreshVarGen::new();
    let diag = infer_with_diagnostic(&env, &expr, &mut fresh, None, "top-level let")
        .expect_err("let-body unbound reference must fail");
    assert!(
        matches!(diag.error, InferError::UnboundVar(ref n) if n == "y"),
        "expected UnboundVar(y), got {:?}",
        diag.error
    );
    assert_eq!(diag.context, "top-level let");
}
