//! R225.M9 property-test harness fixture corpus.
//!
//! Nine tests, tagged `r225m9-prop-01`..`r225m9-prop-09`. The corpus
//! partitions into three halves:
//!
//! * `01..02, 07..08` — generator invariants: determinism, depth-0
//!   shape, and the depth-5 presence guarantee that gives every
//!   downstream unify/infer property a non-trivial subject.
//! * `03, 06` — [`check_infer_terminates`]: single-shot at a fixed
//!   seed, and a 100-iteration seed sweep.
//! * `04..05, 09` — the algebraic laws:
//!   [`check_unify_symmetric`] (uniform + open-row regression),
//!   [`check_generalize_instantiate_roundtrip`].
//!
//! Fixtures never route generated expressions through `infer` and then
//! assert on the inferred type — the algorithm mints fresh variables in
//! order-of-recursion, so a generator-driven assertion would depend on
//! internals not fixed by the property surface. Fixtures instead assert
//! on the *properties themselves* returning `true` — the algebraic
//! contract, not any specific witness.

use std::collections::HashMap;

use paideia_as_shell_hm::{
    check_generalize_instantiate_roundtrip, check_infer_terminates, check_unify_symmetric,
    random_expr, random_mono, Expr, MonoType, RowType, TypeEnv, TypeScheme, TypeVar,
};

// Walk an Expr and count App / Lam nodes together — used by fixture 08
// to check the "non-trivial tree" guarantee at depth 5. Also traverses
// Let and (harmlessly) leaves for a total tree walk.
fn count_app_lam(e: &Expr) -> usize {
    match e {
        Expr::Var(_) => 0,
        Expr::Lit(_) => 0,
        Expr::Lam(_, body) => 1 + count_app_lam(body),
        Expr::App(f, a) => 1 + count_app_lam(f) + count_app_lam(a),
        Expr::Let(_, val, body) => count_app_lam(val) + count_app_lam(body),
        Expr::RecordLit(fields) => fields.values().map(count_app_lam).sum(),
        Expr::Field(receiver, _) => count_app_lam(receiver),
    }
}

// ---------------------------------------------------------------------
// 01 — determinism: the same (depth, seed) pair produces the same tree.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_01_random_expr_deterministic() {
    let a = random_expr(3, 42);
    let b = random_expr(3, 42);
    assert_eq!(a, b, "r225m9-prop-01: random_expr is a pure function");
}

// ---------------------------------------------------------------------
// 02 — depth-0 termination across a handful of seeds. A depth-0 tree
// is a single leaf; infer never recurses, so every seed must produce a
// tree that infer completes on.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_02_depth_zero_always_terminates() {
    for seed in [0u64, 1, 7, 42, 1337, u64::MAX] {
        let e = random_expr(0, seed);
        assert!(
            check_infer_terminates(&e),
            "r225m9-prop-02: depth-0 tree at seed {seed} must terminate infer"
        );
    }
}

// ---------------------------------------------------------------------
// 03 — infer terminates on a depth-3 random expression at a specific
// seed. Exercises the generator at a non-trivial depth against the
// M1-M8 inference driver.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_03_infer_terminates_on_random_depth3() {
    let e = random_expr(3, 100);
    assert!(
        check_infer_terminates(&e),
        "r225m9-prop-03: infer must terminate on random_expr(3, 100)"
    );
}

// ---------------------------------------------------------------------
// 04 — unify symmetry on a random pair. Two independently seeded
// monotypes; unify(a,b) and unify(b,a) must agree on outcome and, on
// success, on alpha-equivalence of the refined principal type.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_04_unify_symmetric_on_random_pair() {
    let a = random_mono(2, 7);
    let b = random_mono(2, 13);
    assert!(
        check_unify_symmetric(&a, &b),
        "r225m9-prop-04: unify must be symmetric on (a, b) = ({a}, {b})"
    );
}

// ---------------------------------------------------------------------
// 05 — generalize/instantiate roundtrip on a chosen (env, mono). The
// env binds `y` to a monotype scheme mentioning TypeVar(0), so `mono`'s
// Var(0) is env-free (not quantifiable) and Var(1) is genuinely
// generic. The roundtrip must preserve both distinctions.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_05_generalize_instantiate_roundtrip() {
    let env = TypeEnv::new().extend(
        "y".to_owned(),
        TypeScheme {
            quantified: Vec::new(),
            body: MonoType::Var(TypeVar(0)),
        },
    );
    let mono = MonoType::Arrow(
        Box::new(MonoType::Var(TypeVar(0))),
        Box::new(MonoType::Var(TypeVar(1))),
    );
    assert!(
        check_generalize_instantiate_roundtrip(&env, &mono),
        "r225m9-prop-05: generalize . instantiate must roundtrip up to alpha"
    );
}

// ---------------------------------------------------------------------
// 06 — 100-iteration seed sweep of infer termination. If any seed in
// [0, 100) produced a tree infer panicked on, the assertion below fires
// and names the offending seed.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_06_infer_terminates_across_100_seeds() {
    for seed in 0u64..100 {
        let e = random_expr(3, seed);
        assert!(
            check_infer_terminates(&e),
            "r225m9-prop-06: infer panicked on random_expr(3, {seed})"
        );
    }
}

// ---------------------------------------------------------------------
// 07 — depth-0 shape guarantee: every leaf is Lit (Int or Str). No
// Var / Lam / App / Let / RecordLit / Field at the root of a depth-0
// generation.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_07_depth_zero_is_literal() {
    for seed in [0u64, 1, 2, 3, 100, 12345] {
        let e = random_expr(0, seed);
        match e {
            Expr::Lit(_) => {}
            other => panic!(
                "r225m9-prop-07: depth-0 tree at seed {seed} must be Lit, got {other:?}"
            ),
        }
    }
}

// ---------------------------------------------------------------------
// 08 — depth-5 non-triviality: the generated tree contains at least one
// App or Lam node. The algorithm at depth > 0 picks the root from
// {Lam, App, Let, Var} by `seed % 4`; seed = 1 yields App at the root,
// so `count_app_lam >= 1` is guaranteed.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_08_depth_five_contains_app_or_lam() {
    let e = random_expr(5, 1);
    let n = count_app_lam(&e);
    assert!(
        n >= 1,
        "r225m9-prop-08: depth-5 tree at seed 1 must contain at least one App or Lam (got {n})"
    );
}

// ---------------------------------------------------------------------
// 09 — check_unify_symmetric on two OPEN-tailed records with hand-
// authored low-id type variables. Regression pin for the row-tail
// aliasing hazard identified by the v0.36.40 adversarial-verify pass:
// the pre-fix helper called the bare `unify()` wrapper, which allocated
// `FreshVarGen::new()` (id 0) on every call. The Rémy `(open, open)`
// branch would then mint a row-tail witness aliased to any hand-
// authored `TypeVar(0)` in the input, corrupting downstream applies.
// Fixed by advancing a shared `FreshVarGen` past every free var in a/b
// before threading it through `unify_with_fresh` in both directions.
// This fixture uses `TypeVar(0)` as the record's `x` field type and
// `TypeVar(1)` / `TypeVar(3)` as the open tails, exactly the aliasing
// shape the debugger repro flagged.
// ---------------------------------------------------------------------

#[test]
fn r225m9_prop_09_unify_symmetric_open_row_no_aliasing() {
    let mut a_fields = HashMap::new();
    a_fields.insert("x".to_string(), MonoType::Var(TypeVar(0)));
    let a = MonoType::Record(RowType::from_map(a_fields, Some(TypeVar(1))));

    let mut b_fields = HashMap::new();
    b_fields.insert("y".to_string(), MonoType::Var(TypeVar(2)));
    let b = MonoType::Record(RowType::from_map(b_fields, Some(TypeVar(3))));

    assert!(
        check_unify_symmetric(&a, &b),
        "r225m9-prop-09: open-tail record pair must be symmetric under unify"
    );
}
