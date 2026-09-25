//! R225.M5 TypedValue fixture corpus.
//!
//! 10 tests, tagged `r225m5-typed-01`..`r225m5-typed-10`. Each
//! exercises one surface behaviour of the M5 typed-value extensions:
//! [`paideia_as_shell_hm::TypedValue`],
//! [`paideia_as_shell_hm::unify_typed_values`], and the
//! `MonoType::Typed` variant's `apply` / `free_vars` /
//! cross-variant-Mismatch interplay with the earlier R225.M2 record
//! rows and R225.M3 effect rows.
//!
//! Tests must NOT perturb the M1 (`tests/algorithm_w.rs`), M2
//! (`tests/row_types.rs`), or M3 (`tests/effect_row.rs`) corpora — the
//! M5 module is a new algebraic variant, not a modification of the
//! existing ones.

use std::collections::{BTreeMap, HashMap};

use paideia_as_shell_hm::{
    unify_typed_values, unify_with_fresh, EffectRow, FreshVarGen, MonoType, RowType, Substitution,
    TypeVar, TypedValue, UnifyError,
};

// ---------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------

fn unit() -> MonoType {
    MonoType::Con("Unit".to_owned())
}

fn int() -> MonoType {
    MonoType::Con("Int".to_owned())
}

fn str_() -> MonoType {
    MonoType::Con("Str".to_owned())
}

/// Build an effect row from `(label, payload)` pairs and an optional
/// tail row-var. Mirrors the M3 corpus helper for consistency.
fn eff(fields: &[(&str, MonoType)], tail: Option<TypeVar>) -> EffectRow {
    let mut present = BTreeMap::new();
    for (name, ty) in fields {
        present.insert((*name).to_owned(), ty.clone());
    }
    EffectRow { present, tail }
}

/// Build a closed record row from `(field, type)` pairs.
fn row_of(fields: &[(&str, MonoType)]) -> RowType {
    let mut map = HashMap::new();
    for (name, ty) in fields {
        map.insert((*name).to_owned(), ty.clone());
    }
    RowType::from_map(map, None)
}

// ---------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------

#[test]
fn r225m5_typed_01_identical_typed_values_unify() {
    // {name: Str ! io} ~ {name: Str ! io} — trivial identity.
    let tv = TypedValue::with_effect(
        row_of(&[("name", str_())]),
        EffectRow::from_labels(&["io"]),
    );
    let sub = unify_typed_values(&tv, &tv)
        .expect("r225m5-typed-01: unify must succeed");
    assert_eq!(
        sub,
        Substitution::empty(),
        "r225m5-typed-01: trivial subst"
    );
}

#[test]
fn r225m5_typed_02_effect_side_mismatch_only() {
    // {name: Str ! io} ~ {name: Str ! fs}
    //   value side agrees, effect side disagrees.
    let a = TypedValue::with_effect(
        row_of(&[("name", str_())]),
        EffectRow::from_labels(&["io"]),
    );
    let b = TypedValue::with_effect(
        row_of(&[("name", str_())]),
        EffectRow::from_labels(&["fs"]),
    );
    let err = unify_typed_values(&a, &b)
        .expect_err("r225m5-typed-02: must fail");
    match err {
        UnifyError::TypedValueMismatch { value_err, effect_err } => {
            assert!(
                value_err.is_none(),
                "r225m5-typed-02: value side matched, value_err must be None"
            );
            let inner = effect_err.expect("r225m5-typed-02: effect_err populated");
            assert!(
                matches!(*inner, UnifyError::EffectRowMismatch { .. }),
                "r225m5-typed-02: effect_err is EffectRowMismatch, got {inner:?}"
            );
        }
        other => panic!("r225m5-typed-02: expected TypedValueMismatch, got {other:?}"),
    }
}

#[test]
fn r225m5_typed_03_value_side_mismatch_only() {
    // {a: Int ! io} ~ {b: Int ! io}
    //   value side disagrees (missing field), effect side agrees.
    let a = TypedValue::with_effect(
        row_of(&[("a", int())]),
        EffectRow::from_labels(&["io"]),
    );
    let b = TypedValue::with_effect(
        row_of(&[("b", int())]),
        EffectRow::from_labels(&["io"]),
    );
    let err = unify_typed_values(&a, &b)
        .expect_err("r225m5-typed-03: must fail");
    match err {
        UnifyError::TypedValueMismatch { value_err, effect_err } => {
            let inner = value_err.expect("r225m5-typed-03: value_err populated");
            assert!(
                matches!(*inner, UnifyError::MissingField { .. }),
                "r225m5-typed-03: value_err is MissingField, got {inner:?}"
            );
            assert!(
                effect_err.is_none(),
                "r225m5-typed-03: effect side matched, effect_err must be None"
            );
        }
        other => panic!("r225m5-typed-03: expected TypedValueMismatch, got {other:?}"),
    }
}

#[test]
fn r225m5_typed_04_both_sides_mismatch() {
    // {a: Int ! io} ~ {b: Int ! fs}
    //   both sides disagree — both value_err and effect_err populated.
    let a = TypedValue::with_effect(
        row_of(&[("a", int())]),
        EffectRow::from_labels(&["io"]),
    );
    let b = TypedValue::with_effect(
        row_of(&[("b", int())]),
        EffectRow::from_labels(&["fs"]),
    );
    let err = unify_typed_values(&a, &b)
        .expect_err("r225m5-typed-04: must fail");
    match err {
        UnifyError::TypedValueMismatch { value_err, effect_err } => {
            assert!(
                value_err.is_some(),
                "r225m5-typed-04: value_err populated"
            );
            assert!(
                effect_err.is_some(),
                "r225m5-typed-04: effect_err populated"
            );
        }
        other => panic!("r225m5-typed-04: expected TypedValueMismatch, got {other:?}"),
    }
}

#[test]
fn r225m5_typed_05_open_tail_propagates_through_apply() {
    // {a: Int | r_val ! io | r_eff} ~ {a: Int, b: Str ! io, fs}
    //   The value tail `r_val` absorbs {b: Str}; the effect tail
    //   `r_eff` absorbs {fs}. Applying the resulting substitution to
    //   the open TypedValue must produce the closed one.
    let r_val = TypeVar(200);
    let r_eff = TypeVar(201);
    let open = TypedValue::with_effect(
        RowType::Extend {
            field: "a".to_owned(),
            ty: Box::new(int()),
            rest: Box::new(RowType::RowVar(r_val)),
        },
        eff(&[("io", unit())], Some(r_eff)),
    );
    let closed = TypedValue::with_effect(
        row_of(&[("a", int()), ("b", str_())]),
        EffectRow::from_labels(&["io", "fs"]),
    );
    let sub = unify_typed_values(&open, &closed)
        .expect("r225m5-typed-05: unify must succeed");

    // Apply the substitution to the open value's MonoType wrapper.
    // The result must equal the closed side structurally.
    let applied = sub.apply(&MonoType::Typed(Box::new(open.clone())));
    match applied {
        MonoType::Typed(tv) => {
            let (v_fields, v_tail) = tv.value_row.to_map();
            assert!(
                v_tail.is_none(),
                "r225m5-typed-05: value tail absorbed"
            );
            assert_eq!(
                v_fields.get("a"),
                Some(&int()),
                "r225m5-typed-05: a: Int survives"
            );
            assert_eq!(
                v_fields.get("b"),
                Some(&str_()),
                "r225m5-typed-05: b: Str spliced through r_val"
            );

            assert!(
                tv.effect_row.tail.is_none(),
                "r225m5-typed-05: effect tail absorbed"
            );
            assert_eq!(
                tv.effect_row.present.get("io"),
                Some(&unit()),
                "r225m5-typed-05: io survives"
            );
            assert_eq!(
                tv.effect_row.present.get("fs"),
                Some(&unit()),
                "r225m5-typed-05: fs spliced through r_eff"
            );
        }
        other => panic!("r225m5-typed-05: expected Typed, got {other:?}"),
    }
}

#[test]
fn r225m5_typed_06_cross_variant_row_vs_typed_is_mismatch() {
    // A bare record vs a typed value must NOT unify — they occupy
    // disjoint kinds. Failure surfaces as a plain UnifyError::Mismatch,
    // not TypedValueMismatch / RowMismatch / MissingField.
    let record = MonoType::Record(row_of(&[("a", int())]));
    let typed = MonoType::Typed(Box::new(TypedValue::with_effect(
        row_of(&[("a", int())]),
        EffectRow::from_labels(&["io"]),
    )));
    let mut fresh = FreshVarGen::new();
    let err = unify_with_fresh(&record, &typed, &mut fresh)
        .expect_err("r225m5-typed-06: must fail");
    assert!(
        matches!(err, UnifyError::Mismatch { .. }),
        "r225m5-typed-06: expected Mismatch, got {err:?}"
    );

    // And the mirror image — effect row vs typed value.
    let effect = MonoType::EffectRow(EffectRow::from_labels(&["io"]));
    let err2 = unify_with_fresh(&effect, &typed, &mut fresh)
        .expect_err("r225m5-typed-06: mirror must fail");
    assert!(
        matches!(err2, UnifyError::Mismatch { .. }),
        "r225m5-typed-06: mirror expected Mismatch, got {err2:?}"
    );
}

#[test]
fn r225m5_typed_07_free_vars_include_both_sides() {
    // free_vars of MonoType::Typed({a: t0 | r_val ! io | r_eff}) =
    //   {t0, r_val, r_eff}. Both row halves contribute; a shared
    //   substitution domain means the union is the answer.
    let t0 = TypeVar(10);
    let r_val = TypeVar(11);
    let r_eff = TypeVar(12);
    let tv = TypedValue::with_effect(
        RowType::Extend {
            field: "a".to_owned(),
            ty: Box::new(MonoType::Var(t0)),
            rest: Box::new(RowType::RowVar(r_val)),
        },
        eff(&[("io", unit())], Some(r_eff)),
    );
    let mono = MonoType::Typed(Box::new(tv));
    let fvs = mono.free_vars();
    assert_eq!(fvs.len(), 3, "r225m5-typed-07: exactly three free vars");
    assert!(fvs.contains(&t0), "r225m5-typed-07: t0 free (payload)");
    assert!(fvs.contains(&r_val), "r225m5-typed-07: r_val free (value tail)");
    assert!(fvs.contains(&r_eff), "r225m5-typed-07: r_eff free (effect tail)");
}

#[test]
fn r225m5_typed_08_apply_walks_both_rows() {
    // apply on MonoType::Typed({a: t0 ! err: t1}) with
    //   {t0 ↦ Int, t1 ↦ Str} yields {a: Int ! err: Str}.
    let t0 = TypeVar(20);
    let t1 = TypeVar(21);
    let tv = TypedValue::with_effect(
        RowType::Extend {
            field: "a".to_owned(),
            ty: Box::new(MonoType::Var(t0)),
            rest: Box::new(RowType::Empty),
        },
        eff(&[("err", MonoType::Var(t1))], None),
    );
    let mono = MonoType::Typed(Box::new(tv));

    let mut sub = Substitution::empty();
    sub.insert(t0, int());
    sub.insert(t1, str_());
    let applied = sub.apply(&mono);

    match applied {
        MonoType::Typed(tv2) => {
            let (v_fields, v_tail) = tv2.value_row.to_map();
            assert!(v_tail.is_none(), "r225m5-typed-08: closed value tail preserved");
            assert_eq!(
                v_fields.get("a"),
                Some(&int()),
                "r225m5-typed-08: value payload substituted"
            );

            assert!(
                tv2.effect_row.tail.is_none(),
                "r225m5-typed-08: closed effect tail preserved"
            );
            assert_eq!(
                tv2.effect_row.present.get("err"),
                Some(&str_()),
                "r225m5-typed-08: effect payload substituted"
            );
        }
        other => panic!("r225m5-typed-08: expected Typed, got {other:?}"),
    }
}

#[test]
fn r225m5_typed_09_of_row_produces_record_only() {
    // TypedValue::of_row(row) yields value_row = row, effect_row =
    //   EffectRow::empty() — no labels, no tail.
    let row = row_of(&[("a", int()), ("b", str_())]);
    let tv = TypedValue::of_row(row.clone());
    assert_eq!(tv.value_row, row, "r225m5-typed-09: value_row preserved");
    assert!(
        tv.effect_row.present.is_empty(),
        "r225m5-typed-09: no effect labels"
    );
    assert!(
        tv.effect_row.tail.is_none(),
        "r225m5-typed-09: no effect tail"
    );
    assert_eq!(
        tv.effect_row,
        EffectRow::empty(),
        "r225m5-typed-09: effect_row is the empty row"
    );
}

#[test]
fn r225m5_typed_10_with_effect_composes_both_halves() {
    // TypedValue::with_effect(row, eff) preserves both halves verbatim.
    let row = row_of(&[("name", str_())]);
    let effect = EffectRow::from_labels(&["io", "fs"]);
    let tv = TypedValue::with_effect(row.clone(), effect.clone());
    assert_eq!(tv.value_row, row, "r225m5-typed-10: value_row preserved");
    assert_eq!(
        tv.effect_row, effect,
        "r225m5-typed-10: effect_row preserved"
    );

    // And the round-trip through MonoType::Typed unifies with itself
    // trivially — sanity that the constructor's output is well-formed.
    let mono = MonoType::Typed(Box::new(tv.clone()));
    let mut fresh = FreshVarGen::new();
    let sub = unify_with_fresh(&mono, &mono, &mut fresh)
        .expect("r225m5-typed-10: self-unify must succeed");
    assert_eq!(
        sub,
        Substitution::empty(),
        "r225m5-typed-10: self-unify subst trivial"
    );
}
