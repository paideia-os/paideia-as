//! R229.M6 persistent let-binding corpus.
//!
//! Eight fixtures (`r229m6-let-01` .. `r229m6-let-08`) pin the shape
//! of [`paideia_as_shell_repl::eval_let_binding`] / [`execute_let`] and
//! the `SyntaxNode::Let` intercept in `turn::execute`. All eight go
//! through the [`eval_let_binding`] public helper because the R221.M5
//! pipeline parser does not (yet) emit a top-level `SyntaxNode::Let` —
//! bare `let x = 42` fails at the lexer (`=` is not a recognised
//! operator glyph). See the R229.M6 CHANGELOG entry for the Path B
//! rationale: adding `=` to the lexer plus a `let` keyword and a
//! top-level production is a separate parser-side change; M6 lands the
//! state-mutation substrate a follow-on parser milestone plugs into.
//!
//! Each test constructs the RHS as a hand-built `SyntaxNode` and hands
//! it to `eval_let_binding`. The helper bumps `turn_counter` and
//! renders through the same `render_value` path as the Lambda arm —
//! the tests pin that shape, plus cross-turn survival of the mutation.

use paideia_as_shell_ast::{Context, NodeSpan, SyntaxNode};
use paideia_as_shell_repl::{
    eval_let_binding, eval_turn, ReplState, TurnResult, Value,
};

/// Helper: synthesize a Lambda-context [`NodeSpan`] for
/// programmatically-built AST nodes.
fn lam_span() -> NodeSpan {
    NodeSpan::synthetic(Context::Lambda)
}

/// Helper: a `LitInt` RHS with a synthetic span.
fn lit_int(n: i64) -> SyntaxNode {
    SyntaxNode::LitInt { value: n, span: lam_span() }
}

/// Helper: a `Var` reference with a synthetic span.
fn var(name: &str) -> SyntaxNode {
    SyntaxNode::Var { name: name.to_owned(), span: lam_span() }
}

/// Helper: a `Lambda` node with the given params and body.
fn lambda(params: Vec<&str>, body: SyntaxNode) -> SyntaxNode {
    SyntaxNode::Lambda {
        params: params.into_iter().map(str::to_owned).collect(),
        body: Box::new(body),
        span: lam_span(),
    }
}

/// Helper: a `BinOp` with a synthetic span.
fn binop(op: &str, lhs: SyntaxNode, rhs: SyntaxNode) -> SyntaxNode {
    SyntaxNode::BinOp {
        op: op.to_owned(),
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span: lam_span(),
    }
}

/// `r229m6-let-01`: a single `let x = 42` renders `"x = 42"` and
/// installs `x → Int(42)` in `value_env`.
#[test]
fn r229m6_let_01_single_binding_renders_and_installs() {
    const FP: &str = "r229m6-let-01";
    let mut state = ReplState::new();
    let turn = eval_let_binding(&mut state, "x", lit_int(42));
    match turn {
        TurnResult::Value(v) => assert_eq!(
            v, "x = 42",
            "{FP}: single binding must render `x = 42`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
    match state.value_env.get("x") {
        Some(Value::Int(42)) => {}
        other => panic!("{FP}: value_env[\"x\"] must be Int(42), got: {other:?}"),
    }
    assert_eq!(
        state.turn_counter, 1,
        "{FP}: eval_let_binding must bump turn_counter (got {})",
        state.turn_counter
    );
}

/// `r229m6-let-02`: after `let x = 42`, a lambda thunk `{ x }` in a
/// subsequent turn resolves `x` from the persisted `value_env` and
/// renders `"42"` (the M5 render rule for `Int`).
#[test]
fn r229m6_let_02_binding_visible_next_turn() {
    const FP: &str = "r229m6-let-02";
    let mut state = ReplState::new();
    let _t1 = eval_let_binding(&mut state, "x", lit_int(42));
    // Drive the second turn through the real parser via `eval_turn`
    // on a source string that becomes a `Lambda { params: [], body:
    // Var("x") }` — the empty-params auto-force in `eval_lambda` then
    // evaluates the body under `state.value_env` and sees `x`.
    let t2 = eval_turn(&mut state, "{ x }".to_owned());
    match t2.result {
        TurnResult::Value(v) => assert_eq!(
            v, "42",
            "{FP}: `{{ x }}` after `let x = 42` must render `42`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
    assert_eq!(
        state.turn_counter, 2,
        "{FP}: two turns must reach turn_counter=2 (got {})",
        state.turn_counter
    );
}

/// `r229m6-let-03`: sequential `let x = 1`, `let y = 2`, then a thunk
/// `{ x + y }` reduces to `3` — proves the environment accumulates
/// across turns, not just single-shadow.
#[test]
fn r229m6_let_03_multi_let_cross_turn() {
    const FP: &str = "r229m6-let-03";
    let mut state = ReplState::new();
    let _t1 = eval_let_binding(&mut state, "x", lit_int(1));
    let _t2 = eval_let_binding(&mut state, "y", lit_int(2));
    // Synthesize the thunk `{ x + y }` and drive through `eval_turn`'s
    // Lambda arm — but the source `{ x + y }` parses cleanly since the
    // shell-lex layer handles `+` and idents inside a lambda body.
    let t3 = eval_turn(&mut state, "{ x + y }".to_owned());
    match t3.result {
        TurnResult::Value(v) => assert_eq!(
            v, "3",
            "{FP}: `{{ x + y }}` after two lets must render `3`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
    assert!(
        state.value_env.contains_key("x") && state.value_env.contains_key("y"),
        "{FP}: both x and y must remain in value_env"
    );
}

/// `r229m6-let-04`: shadowing — `let x = 1` then `let x = 99` leaves
/// `x → 99`; a subsequent thunk `{ x }` renders `99`. `HashMap::insert`
/// is the shadowing primitive; the fixture pins the last-writer-wins
/// semantic.
#[test]
fn r229m6_let_04_shadowing_last_writer_wins() {
    const FP: &str = "r229m6-let-04";
    let mut state = ReplState::new();
    let _t1 = eval_let_binding(&mut state, "x", lit_int(1));
    let _t2 = eval_let_binding(&mut state, "x", lit_int(99));
    match state.value_env.get("x") {
        Some(Value::Int(99)) => {}
        other => panic!("{FP}: after shadowing, value_env[\"x\"] must be Int(99), got: {other:?}"),
    }
    let t3 = eval_turn(&mut state, "{ x }".to_owned());
    match t3.result {
        TurnResult::Value(v) => assert_eq!(
            v, "99",
            "{FP}: `{{ x }}` after shadowing must render `99`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m6-let-05`: a Lambda RHS `let inc = { |x| x + 1 }` installs a
/// `Value::Fn` in `value_env`; a subsequent turn `{ inc }` renders
/// `<closure>` (the M5 render rule for `Fn`). Proves the walker
/// captures the current env at close-over time and that a closure
/// value survives the HashMap round-trip.
#[test]
fn r229m6_let_05_lambda_rhs_stores_closure() {
    const FP: &str = "r229m6-let-05";
    let mut state = ReplState::new();
    let inc_lambda = lambda(vec!["x"], binop("+", var("x"), lit_int(1)));
    let _t1 = eval_let_binding(&mut state, "inc", inc_lambda);
    match state.value_env.get("inc") {
        Some(Value::Fn(_)) => {}
        other => panic!("{FP}: value_env[\"inc\"] must be Value::Fn, got: {other:?}"),
    }
    let t2 = eval_turn(&mut state, "{ inc }".to_owned());
    match t2.result {
        TurnResult::Value(v) => assert_eq!(
            v, "<closure>",
            "{FP}: `{{ inc }}` must render `<closure>`, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m6-let-06`: a failing RHS (`let x = undef` where `undef` is
/// unbound) surfaces as `TurnResult::Error("lambda: unbound variable
/// \`undef\`")` and leaves `value_env` untouched — a failed binding
/// must not leak a partial mutation.
#[test]
fn r229m6_let_06_error_rhs_leaves_env_untouched() {
    const FP: &str = "r229m6-let-06";
    let mut state = ReplState::new();
    // Pre-seed with a marker binding so we can prove the failed let
    // did not touch the map beyond that.
    state.value_env.insert("keep".to_owned(), Value::Int(7));
    let bad = var("undef");
    let turn = eval_let_binding(&mut state, "x", bad);
    match turn {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("lambda:"),
                "{FP}: error must be prefixed `lambda:`, got: {e:?}"
            );
            assert!(
                e.contains("undef"),
                "{FP}: error must name the unbound variable, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!("{FP}: expected Error, got Value: {v:?}"),
    }
    assert!(
        !state.value_env.contains_key("x"),
        "{FP}: failed let must not install the target name in value_env"
    );
    assert!(
        state.type_env.lookup("x").is_none(),
        "{FP}: failed let must not install the target name in type_env either"
    );
    match state.value_env.get("keep") {
        Some(Value::Int(7)) => {}
        other => panic!("{FP}: pre-seeded binding must survive, got: {other:?}"),
    }
    assert_eq!(
        state.turn_counter, 1,
        "{FP}: failed let must still bump turn_counter (got {})",
        state.turn_counter
    );
}

/// `r229m6-let-07`: bindings installed by `let` are visible inside a
/// lambda body via capture. After `let x = 5`, evaluating a synthetic
/// application `(|y| x + y)(10)` under `state.value_env` produces
/// `Int(15)` — the closure captures the current env at close-over
/// time, and that env sees the persisted `x`.
#[test]
fn r229m6_let_07_bindings_captured_by_subsequent_lambda() {
    const FP: &str = "r229m6-let-07";
    let mut state = ReplState::new();
    let _t1 = eval_let_binding(&mut state, "x", lit_int(5));
    // Build `(|y| x + y) 10` as an `App` node and drive it through
    // `eval_lambda` directly under `state.value_env`. The parser does
    // not fold `(...) arg` at the top level, so the synthetic App is
    // the only way to prove close-over-of-persisted-env end-to-end.
    let closure = lambda(vec!["y"], binop("+", var("x"), var("y")));
    let app = SyntaxNode::App {
        func: Box::new(SyntaxNode::Group {
            inner: Box::new(closure),
            span: lam_span(),
        }),
        args: vec![lit_int(10)],
        span: lam_span(),
    };
    let result =
        paideia_as_shell_repl::eval_lambda(&app, &state.value_env);
    match result {
        Ok(Value::Int(n)) => assert_eq!(
            n, 15,
            "{FP}: (|y| x + y)(10) with x=5 must be Int(15), got: Int({n})"
        ),
        Ok(other) => panic!("{FP}: expected Int(15), got: {other:?}"),
        Err(e) => panic!("{FP}: unexpected error: {e}"),
    }
}

/// `r229m6-let-08`: a 10-turn mix of let / cmd / datalog / lambda
/// reaches `turn_counter == 10`. Pins the counter contract across the
/// two entry points (`eval_turn` and `eval_let_binding`) — a driver
/// that mixes both sees a single monotone counter.
#[test]
fn r229m6_let_08_mixed_ten_turn_counter() {
    const FP: &str = "r229m6-let-08";
    let mut state = ReplState::new();

    // Turn 1: let x = 1
    let _ = eval_let_binding(&mut state, "x", lit_int(1));
    // Turn 2: let y = 2
    let _ = eval_let_binding(&mut state, "y", lit_int(2));
    // Turn 3: a bare lambda thunk
    let _ = eval_turn(&mut state, "{ 42 }".to_owned());
    // Turn 4: an unregistered command (drives execute_cmd_node; error
    // is fine — counter still bumps). Name is a bare ident (hyphens
    // would lex as `-` operator tokens and fail at parse).
    let _ = eval_turn(&mut state, "nosuchcmd".to_owned());
    // Turn 5: a datalog block (empty; lowers to zero-fact program).
    let _ = eval_turn(&mut state, "datalog { }".to_owned());
    // Turn 6: another let
    let _ = eval_let_binding(&mut state, "z", lit_int(3));
    // Turn 7: reference the persisted binding through a thunk.
    let _ = eval_turn(&mut state, "{ x + y }".to_owned());
    // Turn 8: shadow y
    let _ = eval_let_binding(&mut state, "y", lit_int(20));
    // Turn 9: another thunk
    let _ = eval_turn(&mut state, "{ x + y }".to_owned());
    // Turn 10: install a lambda binding
    let _ = eval_let_binding(
        &mut state,
        "inc",
        lambda(vec!["n"], binop("+", var("n"), lit_int(1))),
    );

    assert_eq!(
        state.turn_counter, 10,
        "{FP}: ten mixed turns must reach turn_counter=10 (got {})",
        state.turn_counter
    );
    // Sanity: the persisted map holds every binding a let installed.
    assert!(
        state.value_env.contains_key("x")
            && state.value_env.contains_key("y")
            && state.value_env.contains_key("z")
            && state.value_env.contains_key("inc"),
        "{FP}: value_env must retain every let binding across the 10-turn mix"
    );
    match state.value_env.get("y") {
        Some(Value::Int(20)) => {}
        other => panic!("{FP}: shadowed y must be Int(20), got: {other:?}"),
    }
    match state.value_env.get("inc") {
        Some(Value::Fn(_)) => {}
        other => panic!("{FP}: inc must be Value::Fn, got: {other:?}"),
    }
}
