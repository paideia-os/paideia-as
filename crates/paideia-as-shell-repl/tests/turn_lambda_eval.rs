//! R229.M5 lambda-executor corpus.
//!
//! Eight fixtures (`r229m5-lam-01` .. `r229m5-lam-08`) pinning the
//! shape of [`paideia_as_shell_repl::lambda_eval::eval_lambda`] and the
//! rendering the `SyntaxNode::Lambda` arm of `turn::execute` produces
//! on the [`TurnResult`] surface.
//!
//! The end-to-end tests (`r229m5-lam-01..04`) drive `eval_turn`
//! against real source strings; the synthetic-AST tests
//! (`r229m5-lam-05..07`) construct nodes by hand so the walker can be
//! exercised on shapes the R221.M5 lambda parser does not yet emit
//! (function application at the top level, arity mismatches, and
//! deliberate unbound-variable references). Fixture 08 pins the
//! `value_env` field's shape and cross-turn survival — the M5 executor
//! does not persist top-level bindings yet, but the field must exist
//! and stay stable across turns for the follow-on milestone to grow
//! `let` at the REPL surface.

use std::collections::HashMap;

use paideia_as_shell_ast::{Context, NodeSpan, SyntaxNode};
use paideia_as_shell_repl::{
    eval_lambda, eval_turn, LambdaError, ReplState, TurnResult, Value,
};

/// Helper: synthesize a Lambda-context [`NodeSpan`] for
/// programmatically-built AST nodes. Real spans come from the parser;
/// tests that build nodes by hand use [`NodeSpan::synthetic`].
fn lam_span() -> NodeSpan {
    NodeSpan::synthetic(Context::Lambda)
}

/// `r229m5-lam-01`: the identity lambda `"{ |x| x }"` evaluates to a
/// closure value. The `Value::Fn(_)` arm of `turn::execute` renders it
/// as the literal `"<closure>"` — this replaces the R229.M1..M4 stub
/// `"lambda: <not yet implemented>"`.
#[test]
fn r229m5_lam_01_identity_lambda_renders_closure() {
    const FP: &str = "r229m5-lam-01";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ |x| x }".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "<closure>",
            "{FP}: identity lambda must render `<closure>`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m5-lam-02`: a zero-parameter thunk `"{ 42 }"` parses as
/// `Lambda { params: [], body: LitInt(42) }`; the M5 walker forces
/// the empty-parameter form (thunk auto-evaluation) so the turn
/// renders the underlying value `"42"`, not `"<closure>"`.
#[test]
fn r229m5_lam_02_zero_param_thunk_forces_body() {
    const FP: &str = "r229m5-lam-02";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ 42 }".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "42",
            "{FP}: zero-param thunk must force to its body value, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m5-lam-03`: a zero-param thunk over a binop `"{ 1 + 2 }"`
/// forces to `Int(3)` and renders as `"3"`.
#[test]
fn r229m5_lam_03_binop_arithmetic_in_thunk() {
    const FP: &str = "r229m5-lam-03";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ 1 + 2 }".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "3",
            "{FP}: `1 + 2` in thunk must render `3`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m5-lam-04`: a lambda with a body binop `"{ |x| x + 1 }"`
/// remains a closure at the top level (no application), so it renders
/// as `"<closure>"`.
#[test]
fn r229m5_lam_04_lambda_with_binop_body_stays_closure() {
    const FP: &str = "r229m5-lam-04";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ |x| x + 1 }".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "<closure>",
            "{FP}: parameter-bearing lambda must render `<closure>`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m5-lam-05`: synthetic application. Build a `Cmd` whose head is
/// a `Lambda { params: [x], body: x + 1 }` and whose arg is `LitInt(41)`;
/// the walker's Cmd-as-App arm evaluates the head to a closure, extends
/// the captured env with `x → 41`, and reduces the body to `Int(42)`.
///
/// The R221.M5 lambda parser does not (yet) emit this shape at the
/// top level — `"{|x| x + 1} 41"` does not parse into a Cmd whose head
/// is a Lambda — so the fixture constructs the AST by hand.
#[test]
fn r229m5_lam_05_application_via_synthetic_cmd() {
    const FP: &str = "r229m5-lam-05";
    let span = lam_span();
    let lambda = SyntaxNode::Lambda {
        params: vec!["x".to_owned()],
        body: Box::new(SyntaxNode::BinOp {
            op: "+".to_owned(),
            lhs: Box::new(SyntaxNode::Var { name: "x".to_owned(), span }),
            rhs: Box::new(SyntaxNode::LitInt { value: 1, span }),
            span,
        }),
        span,
    };
    let call = SyntaxNode::Cmd {
        name: Box::new(lambda),
        args: vec![SyntaxNode::LitInt { value: 41, span }],
        span,
    };
    let env: HashMap<String, Value> = HashMap::new();
    match eval_lambda(&call, &env) {
        Ok(Value::Int(n)) => assert_eq!(
            n, 42,
            "{FP}: (|x| x + 1)(41) must reduce to Int(42), got: Int({n})"
        ),
        Ok(other) => panic!("{FP}: expected Int(42), got: {other:?}"),
        Err(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m5-lam-06`: a bare `Var("undef")` under an empty env surfaces
/// as [`LambdaError::UnboundVar`] with the name intact. The synthetic
/// path lets us exercise this without wrapping in a lambda that would
/// bind the name.
#[test]
fn r229m5_lam_06_unbound_var_errors() {
    const FP: &str = "r229m5-lam-06";
    let node = SyntaxNode::Var {
        name: "undef".to_owned(),
        span: lam_span(),
    };
    let env: HashMap<String, Value> = HashMap::new();
    match eval_lambda(&node, &env) {
        Err(LambdaError::UnboundVar(name)) => assert_eq!(
            name, "undef",
            "{FP}: unbound-var error must carry the name intact, got: {name:?}"
        ),
        Err(other) => panic!("{FP}: expected UnboundVar, got: {other:?}"),
        Ok(v) => panic!("{FP}: expected UnboundVar error, got Value: {v:?}"),
    }
}

/// `r229m5-lam-07`: an application whose closure declares two params
/// but the caller supplies one surfaces as [`LambdaError::ArityMismatch`]
/// with `expected: 2, actual: 1`. Synthetic because the R221.M5 lambda
/// parser does not emit App-shaped top-level applications; the Cmd-as-App
/// path is exercised the same way.
#[test]
fn r229m5_lam_07_arity_mismatch_errors() {
    const FP: &str = "r229m5-lam-07";
    let span = lam_span();
    let lambda = SyntaxNode::Lambda {
        params: vec!["x".to_owned(), "y".to_owned()],
        body: Box::new(SyntaxNode::BinOp {
            op: "+".to_owned(),
            lhs: Box::new(SyntaxNode::Var { name: "x".to_owned(), span }),
            rhs: Box::new(SyntaxNode::Var { name: "y".to_owned(), span }),
            span,
        }),
        span,
    };
    let call = SyntaxNode::Cmd {
        name: Box::new(lambda),
        args: vec![SyntaxNode::LitInt { value: 1, span }],
        span,
    };
    let env: HashMap<String, Value> = HashMap::new();
    match eval_lambda(&call, &env) {
        Err(LambdaError::ArityMismatch { expected, actual }) => {
            assert_eq!(
                (expected, actual),
                (2, 1),
                "{FP}: 2-param closure applied to 1 arg must report expected=2, actual=1, got: expected={expected}, actual={actual}"
            );
        }
        Err(other) => panic!("{FP}: expected ArityMismatch, got: {other:?}"),
        Ok(v) => panic!("{FP}: expected ArityMismatch error, got Value: {v:?}"),
    }
}

/// `r229m5-lam-08`: [`ReplState::value_env`] exists as a field and
/// survives across turns. R229.M5 does not persist top-level bindings
/// (no source syntax exposes it yet), so the field stays empty across
/// two turns; the fixture pins the shape so the follow-on milestone
/// that wires `let` at the REPL surface has a stable field to grow
/// into.
#[test]
fn r229m5_lam_08_value_env_field_survives_across_turns() {
    const FP: &str = "r229m5-lam-08";
    let mut state = ReplState::new();
    assert!(
        state.value_env.is_empty(),
        "{FP}: fresh ReplState must start with an empty value_env"
    );

    let _t1 = eval_turn(&mut state, "{ |x| x }".to_owned());
    assert!(
        state.value_env.is_empty(),
        "{FP}: value_env must remain empty after a lambda turn (M5 does not persist top-level bindings)"
    );

    let _t2 = eval_turn(&mut state, "{ 1 + 2 }".to_owned());
    assert!(
        state.value_env.is_empty(),
        "{FP}: value_env must remain empty after a thunk-force turn"
    );
    assert_eq!(
        state.turn_counter, 2,
        "{FP}: two turns must advance the counter to 2 (survival sanity)"
    );
}
