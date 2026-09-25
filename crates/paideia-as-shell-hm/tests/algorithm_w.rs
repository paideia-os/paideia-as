//! R225.M1 Algorithm W fixture corpus.
//!
//! 30 tests, tagged `r225m1-w-NN`. Each fixture exercises one surface
//! behaviour of `paideia_as_shell_hm::infer` or one of the algebraic
//! primitives it stands on (`ty`, `subst`, `unify`).

use paideia_as_shell_hm::{
    app, generalize, i, infer, instantiate, lam, let_, s, unify, v, Expr, FreshVarGen,
    InferError, MonoType, Substitution, TypeEnv, TypeScheme, TypeVar, UnifyError,
};

// ---------------------------------------------------------------------
// Shape assertions for principal types.
//
// The algorithm mints fresh variables monotonically, so the exact ids
// in a returned monotype depend on the order recursive calls execute.
// Tests below assert on *shape* — the tree structure — via a bespoke
// `same_shape` helper that treats every `Var` position as a wildcard
// but requires two positions that map to the same variable in `a` to
// map to the same variable in `b` (i.e. shape-equivalence up to
// consistent renaming). This is stronger than "count of arrows" but
// weaker than "identical monotype".
// ---------------------------------------------------------------------

fn same_shape(a: &MonoType, b: &MonoType) -> bool {
    use std::collections::HashMap;

    fn go(
        a: &MonoType,
        b: &MonoType,
        forward: &mut HashMap<TypeVar, TypeVar>,
        backward: &mut HashMap<TypeVar, TypeVar>,
    ) -> bool {
        match (a, b) {
            (MonoType::Var(x), MonoType::Var(y)) => {
                match (forward.get(x).copied(), backward.get(y).copied()) {
                    (Some(fy), Some(bx)) => fy == *y && bx == *x,
                    (Some(fy), None) => fy == *y,
                    (None, Some(bx)) => bx == *x,
                    (None, None) => {
                        forward.insert(*x, *y);
                        backward.insert(*y, *x);
                        true
                    }
                }
            }
            (MonoType::Con(x), MonoType::Con(y)) => x == y,
            (MonoType::Arrow(a1, a2), MonoType::Arrow(b1, b2)) => {
                go(a1, b1, forward, backward) && go(a2, b2, forward, backward)
            }
            _ => false,
        }
    }

    let mut forward = std::collections::HashMap::new();
    let mut backward = std::collections::HashMap::new();
    go(a, b, &mut forward, &mut backward)
}

fn a() -> MonoType {
    MonoType::Var(TypeVar(0))
}
fn b() -> MonoType {
    MonoType::Var(TypeVar(1))
}
fn c() -> MonoType {
    MonoType::Var(TypeVar(2))
}
fn arrow(l: MonoType, r: MonoType) -> MonoType {
    MonoType::Arrow(Box::new(l), Box::new(r))
}
fn int() -> MonoType {
    MonoType::Con("Int".to_owned())
}
fn str_() -> MonoType {
    MonoType::Con("Str".to_owned())
}

fn infer_top(expr: &Expr) -> Result<MonoType, InferError> {
    let env = TypeEnv::new();
    let mut fresh = FreshVarGen::new();
    let (subst, ty) = infer(&env, expr, &mut fresh)?;
    Ok(subst.apply(&ty))
}

// ---------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------

#[test]
fn r225m1_w_01_identity() {
    // \x -> x  :  a -> a
    let e = lam("x", v("x"));
    let ty = infer_top(&e).expect("r225m1-w-01: infer must succeed");
    let expected = arrow(a(), a());
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-01: identity, got {ty}"
    );
}

#[test]
fn r225m1_w_02_const_k() {
    // \x -> \y -> x  :  a -> b -> a
    let e = lam("x", lam("y", v("x")));
    let ty = infer_top(&e).expect("r225m1-w-02: infer must succeed");
    let expected = arrow(a(), arrow(b(), a()));
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-02: const K, got {ty}"
    );
}

#[test]
fn r225m1_w_03_simple_application_int() {
    // (\x -> x) 42  :  Int
    let e = app(lam("x", v("x")), i());
    let ty = infer_top(&e).expect("r225m1-w-03: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-03: apply id to Int");
}

#[test]
fn r225m1_w_04_let_poly_int() {
    // let id = \x -> x in id 5  :  Int
    let e = let_("id", lam("x", v("x")), app(v("id"), i()));
    let ty = infer_top(&e).expect("r225m1-w-04: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-04: let-poly instantiate to Int");
}

#[test]
fn r225m1_w_05_let_poly_str() {
    // let id = \x -> x in id "s"  :  Str
    let e = let_("id", lam("x", v("x")), app(v("id"), s()));
    let ty = infer_top(&e).expect("r225m1-w-05: infer must succeed");
    assert_eq!(ty, str_(), "r225m1-w-05: let-poly instantiate to Str");
}

#[test]
fn r225m1_w_06_occurs_check_self_application() {
    // \x -> x x  should fail the occurs check.
    let e = lam("x", app(v("x"), v("x")));
    let err = infer_top(&e).expect_err("r225m1-w-06: must fail");
    assert!(
        matches!(err, InferError::UnifyError(UnifyError::OccursCheck { .. })),
        "r225m1-w-06: expected OccursCheck, got {err:?}"
    );
}

#[test]
fn r225m1_w_07_curry_second_arg() {
    // \x -> \y -> y  :  a -> b -> b
    let e = lam("x", lam("y", v("y")));
    let ty = infer_top(&e).expect("r225m1-w-07: infer must succeed");
    let expected = arrow(a(), arrow(b(), b()));
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-07: curry, got {ty}"
    );
}

#[test]
fn r225m1_w_08_compose_combinator() {
    // let compose = \f -> \g -> \x -> f (g x) in compose
    //   :  (b -> c) -> (a -> b) -> a -> c
    let compose = lam(
        "f",
        lam(
            "g",
            lam("x", app(v("f"), app(v("g"), v("x")))),
        ),
    );
    let e = let_("compose", compose, v("compose"));
    let ty = infer_top(&e).expect("r225m1-w-08: infer must succeed");
    let expected = arrow(arrow(b(), c()), arrow(arrow(a(), b()), arrow(a(), c())));
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-08: compose, got {ty}"
    );
}

#[test]
fn r225m1_w_09_s_combinator() {
    // \f -> \g -> \x -> f x (g x)
    //   :  (a -> b -> c) -> (a -> b) -> a -> c
    let s_comb = lam(
        "f",
        lam(
            "g",
            lam("x", app(app(v("f"), v("x")), app(v("g"), v("x")))),
        ),
    );
    let ty = infer_top(&s_comb).expect("r225m1-w-09: infer must succeed");
    let expected = arrow(
        arrow(a(), arrow(b(), c())),
        arrow(arrow(a(), b()), arrow(a(), c())),
    );
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-09: S combinator, got {ty}"
    );
}

#[test]
fn r225m1_w_10_k_combinator_applied() {
    // let k = \x -> \y -> x in k 5 "hello"  :  Int
    let k = lam("x", lam("y", v("x")));
    let e = let_("k", k, app(app(v("k"), i()), s()));
    let ty = infer_top(&e).expect("r225m1-w-10: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-10: k Int Str = Int");
}

#[test]
fn r225m1_w_11_i_combinator_via_let() {
    // let i = \x -> x in i  :  a -> a
    let e = let_("i", lam("x", v("x")), v("i"));
    let ty = infer_top(&e).expect("r225m1-w-11: infer must succeed");
    let expected = arrow(a(), a());
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-11: I combinator via let, got {ty}"
    );
}

#[test]
fn r225m1_w_12_b_combinator_via_let() {
    // let b = \f -> \g -> \x -> f (g x) in b
    //   :  (b -> c) -> (a -> b) -> a -> c
    let bcomb = lam(
        "f",
        lam(
            "g",
            lam("x", app(v("f"), app(v("g"), v("x")))),
        ),
    );
    let e = let_("b", bcomb, v("b"));
    let ty = infer_top(&e).expect("r225m1-w-12: infer must succeed");
    let expected = arrow(arrow(b(), c()), arrow(arrow(a(), b()), arrow(a(), c())));
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-12: B combinator via let, got {ty}"
    );
}

#[test]
fn r225m1_w_13_unbound_variable() {
    // Free reference to `nope` under an empty environment.
    let e = v("nope");
    let err = infer_top(&e).expect_err("r225m1-w-13: must fail");
    match err {
        InferError::UnboundVar(name) => assert_eq!(name, "nope", "r225m1-w-13"),
        other => panic!("r225m1-w-13: expected UnboundVar, got {other:?}"),
    }
}

#[test]
fn r225m1_w_14_nested_let_poly() {
    // let id  = \x -> x in
    // let id2 = \x -> id x in
    // id2 5
    let inner = let_(
        "id2",
        lam("x", app(v("id"), v("x"))),
        app(v("id2"), i()),
    );
    let e = let_("id", lam("x", v("x")), inner);
    let ty = infer_top(&e).expect("r225m1-w-14: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-14: nested let-poly resolves to Int");
}

#[test]
fn r225m1_w_15_let_shadowing_alpha_safety() {
    // let x = 5 in (\x -> x) "s"
    // The outer `x` binding is shadowed by the lambda parameter; the
    // body infers to Str.
    let e = let_("x", i(), app(lam("x", v("x")), s()));
    let ty = infer_top(&e).expect("r225m1-w-15: infer must succeed");
    assert_eq!(ty, str_(), "r225m1-w-15: lambda shadows let binding");
}

#[test]
fn r225m1_w_16_deep_application_chain() {
    // let id = \x -> x in id (id (id (id (id (id (id (id (id (id 5)))))))))
    let id = lam("x", v("x"));
    let mut body = i();
    for _ in 0..10 {
        body = app(v("id"), body);
    }
    let e = let_("id", id, body);
    let ty = infer_top(&e).expect("r225m1-w-16: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-16: ten-deep identity chain resolves to Int");
}

#[test]
fn r225m1_w_17_higher_order_no_false_occurs() {
    // \x -> \y -> x (y x)
    //   The classic false-alarm shape that a naive occurs check
    //   would reject; the correct algorithm accepts and returns
    //   ((r1 -> r2) -> ((r1 -> r2) -> r1) -> r2) up to renaming.
    let e = lam(
        "x",
        lam("y", app(v("x"), app(v("y"), v("x")))),
    );
    let ty = infer_top(&e).expect("r225m1-w-17: infer must succeed");
    let expected = arrow(
        arrow(a(), b()),
        arrow(arrow(arrow(a(), b()), a()), b()),
    );
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-17: higher-order shape, got {ty}"
    );
}

#[test]
fn r225m1_w_18_let_poly_id_id_applied() {
    // let id = \x -> x in id id 5  :  Int
    let e = let_(
        "id",
        lam("x", v("x")),
        app(app(v("id"), v("id")), i()),
    );
    let ty = infer_top(&e).expect("r225m1-w-18: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-18: id id 5 = Int");
}

#[test]
fn r225m1_w_19_lambda_self_application_ok() {
    // (\x -> x) (\y -> y)  :  a -> a
    let e = app(lam("x", v("x")), lam("y", v("y")));
    let ty = infer_top(&e).expect("r225m1-w-19: infer must succeed");
    let expected = arrow(a(), a());
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-19: id applied to id, got {ty}"
    );
}

#[test]
fn r225m1_w_20_let_binds_literal() {
    // let x = 5 in x  :  Int
    let e = let_("x", i(), v("x"));
    let ty = infer_top(&e).expect("r225m1-w-20: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-20: let binds Int literal");
}

#[test]
fn r225m1_w_21_let_binds_lambda_and_applies() {
    // let f = \x -> \y -> x in f 5 "s"  :  Int
    let e = let_(
        "f",
        lam("x", lam("y", v("x"))),
        app(app(v("f"), i()), s()),
    );
    let ty = infer_top(&e).expect("r225m1-w-21: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-21: let-bound k applied");
}

#[test]
fn r225m1_w_22_sequential_lets() {
    // let a = 1 in let b = "s" in a  :  Int
    let inner = let_("b", s(), v("a"));
    let e = let_("a", i(), inner);
    let ty = infer_top(&e).expect("r225m1-w-22: infer must succeed");
    assert_eq!(ty, int(), "r225m1-w-22: sequential let bindings");
}

#[test]
fn r225m1_w_23_mismatch_int_where_function_expected() {
    // let f = \g -> g 5 in f 3
    //   Argument `3 : Int` must unify with `Int -> a` — a mismatch.
    let e = let_(
        "f",
        lam("g", app(v("g"), i())),
        app(v("f"), i()),
    );
    let err = infer_top(&e).expect_err("r225m1-w-23: must fail");
    assert!(
        matches!(err, InferError::UnifyError(UnifyError::Mismatch { .. })),
        "r225m1-w-23: expected Mismatch, got {err:?}"
    );
}

#[test]
fn r225m1_w_24_type_scheme_free_vars() {
    // ∀a. a -> b   (b free, a bound) — free_vars should return {b}.
    let scheme = TypeScheme {
        quantified: vec![TypeVar(0)],
        body: arrow(a(), b()),
    };
    let fv = scheme.free_vars();
    assert_eq!(fv.len(), 1, "r225m1-w-24: exactly one free var");
    assert!(
        fv.contains(&TypeVar(1)),
        "r225m1-w-24: `b` is the free var"
    );
}

#[test]
fn r225m1_w_25_substitution_compose_matches_apply() {
    // For any monotype `t`, (s1 ∘ s2).apply(t) == s1.apply(s2.apply(t)).
    let s2 = Substitution::singleton(TypeVar(0), arrow(b(), c()));
    let s1 = Substitution::singleton(TypeVar(1), int());
    let composed = s1.compose(&s2);
    let t = a();
    let via_compose = composed.apply(&t);
    let via_manual = s1.apply(&s2.apply(&t));
    assert_eq!(
        via_compose, via_manual,
        "r225m1-w-25: compose ≠ manual apply for `a`"
    );
    let t2 = arrow(a(), b());
    assert_eq!(
        composed.apply(&t2),
        s1.apply(&s2.apply(&t2)),
        "r225m1-w-25: compose ≠ manual apply for `a -> b`"
    );
}

#[test]
fn r225m1_w_26_fresh_var_gen_monotonic() {
    let mut fresh = FreshVarGen::new();
    let v0 = fresh.fresh();
    let v1 = fresh.fresh();
    let v2 = fresh.fresh();
    assert_eq!(v0.0, 0, "r225m1-w-26: first var is id 0");
    assert_eq!(v1.0, 1, "r225m1-w-26: second var is id 1");
    assert_eq!(v2.0, 2, "r225m1-w-26: third var is id 2");
    assert_ne!(v0, v1, "r225m1-w-26: fresh vars are distinct");
    assert_ne!(v1, v2, "r225m1-w-26: fresh vars are distinct");
}

#[test]
fn r225m1_w_27_unify_arrows_shape() {
    // unify(a -> Int, Str -> b) = {a := Str, b := Int}
    let left = arrow(a(), int());
    let right = arrow(str_(), b());
    let sub = unify(&left, &right).expect("r225m1-w-27: unify must succeed");
    assert_eq!(
        sub.apply(&a()),
        str_(),
        "r225m1-w-27: `a` unified with `Str`"
    );
    assert_eq!(
        sub.apply(&b()),
        int(),
        "r225m1-w-27: `b` unified with `Int`"
    );
}

#[test]
fn r225m1_w_28_let_poly_id_id_shape() {
    // let id = \x -> x in id id  :  a -> a
    let e = let_(
        "id",
        lam("x", v("x")),
        app(v("id"), v("id")),
    );
    let ty = infer_top(&e).expect("r225m1-w-28: infer must succeed");
    let expected = arrow(a(), a());
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-28: id id has shape a -> a, got {ty}"
    );
}

#[test]
fn r225m1_w_29_generalize_then_instantiate_roundtrip() {
    // generalize(∅, a -> a) yields ∀a. a -> a; instantiate mints a
    // fresh var and the resulting monotype should have the same
    // shape (a fresh a' -> a') even though the id differs.
    let env = TypeEnv::new();
    let mono = arrow(a(), a());
    let scheme = generalize(&env, &mono);
    assert_eq!(
        scheme.quantified.len(),
        1,
        "r225m1-w-29: exactly one quantifier"
    );
    let mut fresh = FreshVarGen::new();
    // Skip past ids that would collide with `a` (id 0) — the
    // shape-check below tolerates renaming but consistency requires
    // the two arrow arms map to the same fresh var, which they will
    // because `instantiate` uses one fresh var per quantifier.
    let inst = instantiate(&scheme, &mut fresh);
    match &inst {
        MonoType::Arrow(l, r) => {
            assert_eq!(
                l, r,
                "r225m1-w-29: both arms of the instantiated arrow share one fresh var"
            );
        }
        other => panic!("r225m1-w-29: expected an arrow, got {other:?}"),
    }
}

#[test]
fn r225m1_w_30_lambda_body_uses_polymorphic_let() {
    // \z -> let id = \x -> x in id z
    //   : a -> a
    //   The lambda parameter's type is *not* generalisable (it is
    //   free in the surrounding env inside the lambda body), so the
    //   `id z` call locks id's parameter to `a` at that use site
    //   only; the outer identity behaviour of the whole lambda is
    //   preserved.
    let inner = let_("id", lam("x", v("x")), app(v("id"), v("z")));
    let e = lam("z", inner);
    let ty = infer_top(&e).expect("r225m1-w-30: infer must succeed");
    let expected = arrow(a(), a());
    assert!(
        same_shape(&ty, &expected),
        "r225m1-w-30: shape a -> a, got {ty}"
    );
}
