//! R225.M2 Row-types fixture corpus.
//!
//! 20 tests, tagged `r225m2-row-NN`. Each exercises one surface
//! behaviour of the M2 row-typed extensions to
//! `paideia_as_shell_hm::infer` and the primitives it stands on
//! (`ty::RowType`, `subst::apply_row`, `unify::unify_with_fresh`).
//!
//! Tests must NOT break the 30 R225.M1 fixtures in
//! `tests/algorithm_w.rs`; test 20 below spot-checks that the M1
//! surface is untouched.

use std::collections::HashMap;

use paideia_as_shell_hm::{
    app, field, generalize, i, infer, lam, let_, record, s, unify, unify_with_fresh, v, Expr,
    FreshVarGen, InferError, MonoType, RowType, Substitution, TypeEnv, TypeScheme, TypeVar,
    UnifyError,
};

// ---------------------------------------------------------------------
// Helpers shared with the M1 corpus in spirit but not in file — kept
// local so this test crate compiles standalone.
// ---------------------------------------------------------------------

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

/// Extract a record's fields+tail from a `MonoType::Record` for
/// shape assertions in tests. Panics with a fingerprint tag if the
/// type is not a record.
fn expect_record(ty: &MonoType, tag: &str) -> (HashMap<String, MonoType>, Option<TypeVar>) {
    match ty {
        MonoType::Record(row) => row.to_map(),
        other => panic!("{tag}: expected a Record, got {other}"),
    }
}

// ---------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------

#[test]
fn r225m2_row_01_empty_record_literal() {
    // {} : {}
    let empty_fields: Vec<(&str, Expr)> = Vec::new();
    let e = record(empty_fields);
    let ty = infer_top(&e).expect("r225m2-row-01: infer must succeed");
    match &ty {
        MonoType::Record(RowType::Empty) => {}
        other => panic!("r225m2-row-01: expected Record(Empty), got {other}"),
    }
    // Display renders as `{}`.
    assert_eq!(format!("{ty}"), "{}", "r225m2-row-01: display");
}

#[test]
fn r225m2_row_02_single_field_record() {
    // {a = 42} : {a: Int}
    let e = record(vec![("a", i())]);
    let ty = infer_top(&e).expect("r225m2-row-02: infer must succeed");
    let (fields, tail) = expect_record(&ty, "r225m2-row-02");
    assert_eq!(fields.len(), 1, "r225m2-row-02: exactly one field");
    assert_eq!(fields.get("a"), Some(&int()), "r225m2-row-02: `a: Int`");
    assert!(tail.is_none(), "r225m2-row-02: closed row");
}

#[test]
fn r225m2_row_03_two_field_matching_types_unify() {
    // Direct unify: {a: Int, b: Str} ~ {a: Int, b: Str} -> Ok
    let left = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), str_())]),
        None,
    ));
    let right = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), str_())]),
        None,
    ));
    let sub = unify(&left, &right).expect("r225m2-row-03: unify must succeed");
    // Sub applied to either side leaves it unchanged.
    assert_eq!(sub.apply(&left), left, "r225m2-row-03: subst is trivial");
}

#[test]
fn r225m2_row_04_order_invariant_unification() {
    // {a: Int, b: Str} ~ {b: Str, a: Int} -> Ok
    // Rows are canonicalised by `from_map` (sorted), so the two
    // constructor orders here still produce structurally identical
    // rows — the point of the test is that unification works either
    // way, and does so with a trivial substitution.
    let left = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), str_())]),
        None,
    ));
    let right = MonoType::Record(RowType::from_map(
        HashMap::from([("b".to_owned(), str_()), ("a".to_owned(), int())]),
        None,
    ));
    let sub = unify(&left, &right).expect("r225m2-row-04: unify must succeed");
    assert_eq!(sub, Substitution::empty(), "r225m2-row-04: trivial subst");
}

#[test]
fn r225m2_row_05_missing_field_error() {
    // {a: Int} ~ {a: Int, b: Str} -> MissingField { field: "b", side: "left" }
    let left = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int())]),
        None,
    ));
    let right = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), str_())]),
        None,
    ));
    let err = unify(&left, &right).expect_err("r225m2-row-05: must fail");
    match err {
        UnifyError::MissingField { field, side } => {
            assert_eq!(field, "b", "r225m2-row-05: field name");
            assert_eq!(side, "left", "r225m2-row-05: missing from left side");
        }
        other => panic!("r225m2-row-05: expected MissingField, got {other:?}"),
    }
}

#[test]
fn r225m2_row_06_shared_field_type_mismatch() {
    // {a: Int} ~ {a: Str} -> Mismatch (on the shared field)
    let left = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int())]),
        None,
    ));
    let right = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), str_())]),
        None,
    ));
    let err = unify(&left, &right).expect_err("r225m2-row-06: must fail");
    assert!(
        matches!(err, UnifyError::Mismatch { .. }),
        "r225m2-row-06: expected Mismatch, got {err:?}"
    );
}

#[test]
fn r225m2_row_07_row_polymorphic_field_access() {
    // \r -> r.a  :  {a: t | rest} -> t
    // The inferred type is a function whose parameter is a
    // row-polymorphic record with at least field `a`, returning that
    // field's type.
    let e = lam("r", field(v("r"), "a"));
    let ty = infer_top(&e).expect("r225m2-row-07: infer must succeed");
    match &ty {
        MonoType::Arrow(param, ret) => {
            let (fields, tail) = expect_record(param, "r225m2-row-07 param");
            assert_eq!(fields.len(), 1, "r225m2-row-07: exactly one demanded field");
            let a_ty = fields.get("a").expect("r225m2-row-07: `a` demanded");
            assert!(
                tail.is_some(),
                "r225m2-row-07: row-polymorphic — tail row-var must be present"
            );
            // Return type equals the demanded field type.
            assert_eq!(
                a_ty, &**ret,
                "r225m2-row-07: return type matches field type"
            );
        }
        other => panic!("r225m2-row-07: expected Arrow, got {other}"),
    }
}

#[test]
fn r225m2_row_08_record_concat_via_extend() {
    // Direct row construction: build a two-field row by nested
    // Extend, then unify with a one-field-with-tail row that must
    // absorb the extra.
    let two_field = RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), str_())]),
        None,
    );
    let one_field_with_tail = RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::RowVar(TypeVar(99))),
    };
    let mut fresh = FreshVarGen::new();
    // Skip past id 99 so the fresh generator does not collide with
    // the hand-crafted row-var in `one_field_with_tail`.
    for _ in 0..200 {
        fresh.fresh();
    }
    let sub = unify_with_fresh(
        &MonoType::Record(two_field),
        &MonoType::Record(one_field_with_tail),
        &mut fresh,
    )
    .expect("r225m2-row-08: unify must succeed");
    // Row-var 99 must have been bound to a record carrying `b: Str`.
    let bound = sub
        .lookup(&TypeVar(99))
        .expect("r225m2-row-08: row-var 99 must be bound");
    match bound {
        MonoType::Record(row) => {
            let (fields, tail) = row.to_map();
            assert_eq!(fields.get("b"), Some(&str_()), "r225m2-row-08: b: Str");
            assert!(tail.is_none(), "r225m2-row-08: closed tail");
        }
        other => panic!("r225m2-row-08: expected Record binding, got {other}"),
    }
}

#[test]
fn r225m2_row_09_let_poly_row_access() {
    // let f = \r -> r.a in
    //   let x = f {a = 42} in
    //     let y = f {a = "s", b = 1} in y
    //
    // f is generalised to a row-polymorphic scheme, so both uses at
    // different record shapes typecheck. The final expression is `y`,
    // which is `Str`.
    let f_body = lam("r", field(v("r"), "a"));
    let x = record(vec![("a", i())]);
    let y = record(vec![("a", s()), ("b", i())]);
    let inner = let_(
        "x",
        app(v("f"), x),
        let_("y", app(v("f"), y), v("y")),
    );
    let e = let_("f", f_body, inner);
    let ty = infer_top(&e).expect("r225m2-row-09: infer must succeed");
    assert_eq!(ty, str_(), "r225m2-row-09: y : Str");
}

#[test]
fn r225m2_row_10_nested_record_shape() {
    // {a = {b = 42}} : {a: {b: Int}}
    let inner = record(vec![("b", i())]);
    let e = record(vec![("a", inner)]);
    let ty = infer_top(&e).expect("r225m2-row-10: infer must succeed");
    let (outer_fields, outer_tail) = expect_record(&ty, "r225m2-row-10 outer");
    assert!(outer_tail.is_none(), "r225m2-row-10: outer closed");
    let inner_ty = outer_fields
        .get("a")
        .expect("r225m2-row-10: outer has `a`");
    let (inner_fields, inner_tail) = expect_record(inner_ty, "r225m2-row-10 inner");
    assert!(inner_tail.is_none(), "r225m2-row-10: inner closed");
    assert_eq!(
        inner_fields.get("b"),
        Some(&int()),
        "r225m2-row-10: inner b: Int"
    );
}

#[test]
fn r225m2_row_11_field_access_on_empty_record_errors() {
    // {}.a  should fail with MissingField (`a` missing from left).
    let empty_fields: Vec<(&str, Expr)> = Vec::new();
    let e = field(record(empty_fields), "a");
    let err = infer_top(&e).expect_err("r225m2-row-11: must fail");
    match err {
        InferError::UnifyError(UnifyError::MissingField { field, side }) => {
            assert_eq!(field, "a", "r225m2-row-11: field name");
            assert_eq!(side, "left", "r225m2-row-11: missing from empty side");
        }
        other => panic!("r225m2-row-11: expected MissingField, got {other:?}"),
    }
}

#[test]
fn r225m2_row_12_extend_adds_field_via_row_var() {
    // Direct: unify `{a: Int | r1}` with `{a: Int, b: Str}` — `r1`
    // must absorb `{b: Str}`.
    let left = MonoType::Record(RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::RowVar(TypeVar(50))),
    });
    let right = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), str_())]),
        None,
    ));
    let mut fresh = FreshVarGen::new();
    for _ in 0..100 {
        fresh.fresh();
    }
    let sub = unify_with_fresh(&left, &right, &mut fresh)
        .expect("r225m2-row-12: unify must succeed");
    let bound = sub
        .lookup(&TypeVar(50))
        .expect("r225m2-row-12: row-var 50 bound");
    match bound {
        MonoType::Record(row) => {
            let (fields, tail) = row.to_map();
            assert!(tail.is_none(), "r225m2-row-12: closed tail");
            assert_eq!(fields.len(), 1, "r225m2-row-12: exactly `b`");
            assert_eq!(
                fields.get("b"),
                Some(&str_()),
                "r225m2-row-12: b: Str"
            );
        }
        other => panic!("r225m2-row-12: expected Record binding, got {other}"),
    }
}

#[test]
fn r225m2_row_13_apply_row_walks_nested_extend() {
    // `apply` on a chain of Extend nodes must propagate the
    // substitution into every field's type and every rest position.
    let row = RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(MonoType::Var(TypeVar(0))),
        rest: Box::new(RowType::Extend {
            field: "b".to_owned(),
            ty: Box::new(MonoType::Var(TypeVar(1))),
            rest: Box::new(RowType::RowVar(TypeVar(2))),
        }),
    };
    let mut sub = Substitution::empty();
    sub.insert(TypeVar(0), int());
    sub.insert(TypeVar(1), str_());
    let applied = sub.apply(&MonoType::Record(row));
    let (fields, tail) = expect_record(&applied, "r225m2-row-13");
    assert_eq!(fields.get("a"), Some(&int()), "r225m2-row-13: a: Int");
    assert_eq!(fields.get("b"), Some(&str_()), "r225m2-row-13: b: Str");
    assert_eq!(
        tail,
        Some(TypeVar(2)),
        "r225m2-row-13: tail row-var preserved"
    );
}

#[test]
fn r225m2_row_14_row_var_substitution_splices_row() {
    // `apply` on a row-var that maps to a Record must splice its
    // fields in place.
    let row = RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::RowVar(TypeVar(5))),
    };
    let mut sub = Substitution::empty();
    sub.insert(
        TypeVar(5),
        MonoType::Record(RowType::from_map(
            HashMap::from([("b".to_owned(), str_())]),
            None,
        )),
    );
    let applied = sub.apply(&MonoType::Record(row));
    let (fields, tail) = expect_record(&applied, "r225m2-row-14");
    assert!(tail.is_none(), "r225m2-row-14: closed after splice");
    assert_eq!(fields.get("a"), Some(&int()), "r225m2-row-14: a: Int");
    assert_eq!(fields.get("b"), Some(&str_()), "r225m2-row-14: b: Str");
}

#[test]
fn r225m2_row_15_three_shared_fields_unification() {
    // {a: Int, b: Str, c: Int} ~ {a: Int, b: Str, c: Int} -> Ok
    // Exercises the shared-fields loop with more than two entries.
    let fields = HashMap::from([
        ("a".to_owned(), int()),
        ("b".to_owned(), str_()),
        ("c".to_owned(), int()),
    ]);
    let left = MonoType::Record(RowType::from_map(fields.clone(), None));
    let right = MonoType::Record(RowType::from_map(fields, None));
    let sub = unify(&left, &right).expect("r225m2-row-15: unify must succeed");
    assert_eq!(sub, Substitution::empty(), "r225m2-row-15: trivial subst");
}

#[test]
fn r225m2_row_16_partial_overlap_with_both_tails() {
    // {a: Int, b: Str | r1} ~ {a: Int, c: Int | r2}
    //   shared: {a: Int}
    //   left-only: {b: Str}, right-only: {c: Int}
    //   Rémy: fresh `r`, bind r1 -> {c: Int | r}, r2 -> {b: Str | r}.
    let left = MonoType::Record(RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::Extend {
            field: "b".to_owned(),
            ty: Box::new(str_()),
            rest: Box::new(RowType::RowVar(TypeVar(100))),
        }),
    });
    let right = MonoType::Record(RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::Extend {
            field: "c".to_owned(),
            ty: Box::new(int()),
            rest: Box::new(RowType::RowVar(TypeVar(101))),
        }),
    });
    let mut fresh = FreshVarGen::new();
    for _ in 0..200 {
        fresh.fresh();
    }
    let sub = unify_with_fresh(&left, &right, &mut fresh)
        .expect("r225m2-row-16: unify must succeed");
    // r1 must be bound to a record with `c: Int`, r2 to one with `b: Str`.
    let r1_bound = sub
        .lookup(&TypeVar(100))
        .expect("r225m2-row-16: r1 bound");
    let r2_bound = sub
        .lookup(&TypeVar(101))
        .expect("r225m2-row-16: r2 bound");
    let (r1_fields, r1_tail) = match r1_bound {
        MonoType::Record(row) => row.to_map(),
        other => panic!("r225m2-row-16: r1 -> {other}"),
    };
    let (r2_fields, r2_tail) = match r2_bound {
        MonoType::Record(row) => row.to_map(),
        other => panic!("r225m2-row-16: r2 -> {other}"),
    };
    assert_eq!(r1_fields.get("c"), Some(&int()), "r225m2-row-16: r1 has c");
    assert_eq!(r2_fields.get("b"), Some(&str_()), "r225m2-row-16: r2 has b");
    // Both tails should point to the *same* fresh row-var (the
    // Rémy witness) — this is what links the two rows structurally.
    assert!(
        r1_tail.is_some() && r1_tail == r2_tail,
        "r225m2-row-16: tails share the fresh row-var, r1={r1_tail:?} r2={r2_tail:?}"
    );
}

#[test]
fn r225m2_row_17_no_tail_vs_tail_asymmetry() {
    // {a: Int, b: Int} ~ {a: Int | r} — r must be bound to {b: Int}.
    let left = MonoType::Record(RowType::from_map(
        HashMap::from([("a".to_owned(), int()), ("b".to_owned(), int())]),
        None,
    ));
    let right = MonoType::Record(RowType::Extend {
        field: "a".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::RowVar(TypeVar(77))),
    });
    let mut fresh = FreshVarGen::new();
    for _ in 0..150 {
        fresh.fresh();
    }
    let sub = unify_with_fresh(&left, &right, &mut fresh)
        .expect("r225m2-row-17: unify must succeed");
    let bound = sub.lookup(&TypeVar(77)).expect("r225m2-row-17: r bound");
    match bound {
        MonoType::Record(row) => {
            let (fields, tail) = row.to_map();
            assert!(tail.is_none(), "r225m2-row-17: closed absorbed tail");
            assert_eq!(fields.get("b"), Some(&int()), "r225m2-row-17: b: Int");
            assert_eq!(fields.len(), 1, "r225m2-row-17: exactly one absorbed");
        }
        other => panic!("r225m2-row-17: expected Record binding, got {other}"),
    }

    // Mirror: {a: Int, b: Int} ~ {c: Int | r} — b has nowhere to go
    // because right has c but not b, and left has no tail.
    let right2 = MonoType::Record(RowType::Extend {
        field: "c".to_owned(),
        ty: Box::new(int()),
        rest: Box::new(RowType::RowVar(TypeVar(78))),
    });
    let err = unify(
        &MonoType::Record(RowType::from_map(
            HashMap::from([("a".to_owned(), int()), ("b".to_owned(), int())]),
            None,
        )),
        &right2,
    )
    .expect_err("r225m2-row-17: must fail");
    assert!(
        matches!(err, UnifyError::MissingField { .. }),
        "r225m2-row-17: expected MissingField, got {err:?}"
    );
}

#[test]
fn r225m2_row_18_nested_record_two_inner_fields() {
    // {a = {b = 42, c = "s"}} : {a: {b: Int, c: Str}}
    let inner = record(vec![("b", i()), ("c", s())]);
    let e = record(vec![("a", inner)]);
    let ty = infer_top(&e).expect("r225m2-row-18: infer must succeed");
    let (outer_fields, _) = expect_record(&ty, "r225m2-row-18 outer");
    let inner_ty = outer_fields
        .get("a")
        .expect("r225m2-row-18: outer has `a`");
    let (inner_fields, inner_tail) = expect_record(inner_ty, "r225m2-row-18 inner");
    assert!(inner_tail.is_none(), "r225m2-row-18: inner closed");
    assert_eq!(inner_fields.len(), 2, "r225m2-row-18: two inner fields");
    assert_eq!(
        inner_fields.get("b"),
        Some(&int()),
        "r225m2-row-18: b: Int"
    );
    assert_eq!(
        inner_fields.get("c"),
        Some(&str_()),
        "r225m2-row-18: c: Str"
    );
}

#[test]
fn r225m2_row_19_generalize_row_polymorphic_body() {
    // generalize a row-polymorphic monotype: `{a: Int | r} -> Int`
    // becomes `∀r. {a: Int | r} -> Int`. The generalizer must
    // recognise `r` (a row-var) as a quantifiable free variable.
    let env = TypeEnv::new();
    let mono = MonoType::Arrow(
        Box::new(MonoType::Record(RowType::Extend {
            field: "a".to_owned(),
            ty: Box::new(int()),
            rest: Box::new(RowType::RowVar(TypeVar(0))),
        })),
        Box::new(int()),
    );
    let scheme = generalize(&env, &mono);
    assert_eq!(
        scheme.quantified,
        vec![TypeVar(0)],
        "r225m2-row-19: `r` quantified"
    );
    // The scheme's free_vars must be empty.
    assert!(
        scheme.free_vars().is_empty(),
        "r225m2-row-19: no free vars after generalisation"
    );
}

#[test]
fn r225m2_row_20_sanity_m1_surface_unchanged() {
    // Sanity: the M1 identity infers to a -> a exactly as before.
    // The 30 R225.M1 fixtures live in `tests/algorithm_w.rs`; this
    // test is a fingerprint that the M1 surface is still walkable
    // from the M2 crate root.
    let e = lam("x", v("x"));
    let ty = infer_top(&e).expect("r225m2-row-20: infer must succeed");
    match &ty {
        MonoType::Arrow(a, b) => {
            assert_eq!(a, b, "r225m2-row-20: identity — both arrow arms match");
        }
        other => panic!("r225m2-row-20: expected Arrow, got {other}"),
    }

    // And the M1 `unify(a, b)` signature still works on non-record
    // input with the same result set as before.
    let sub = unify(&int(), &int()).expect("r225m2-row-20: unify Int ~ Int");
    assert_eq!(sub, Substitution::empty(), "r225m2-row-20: trivial subst");
    let err = unify(&int(), &str_()).expect_err("r225m2-row-20: Int ~ Str fails");
    assert!(
        matches!(err, UnifyError::Mismatch { .. }),
        "r225m2-row-20: mismatch preserved"
    );

    // Type schemes still round-trip cleanly.
    let scheme = TypeScheme {
        quantified: vec![TypeVar(0)],
        body: MonoType::Arrow(
            Box::new(MonoType::Var(TypeVar(0))),
            Box::new(MonoType::Var(TypeVar(0))),
        ),
    };
    assert!(
        scheme.free_vars().is_empty(),
        "r225m2-row-20: closed scheme"
    );
}
