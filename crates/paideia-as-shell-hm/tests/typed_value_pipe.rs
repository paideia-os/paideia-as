//! R225.M6 typed-value pipeline-composition fixture corpus.
//!
//! 8 tests, tagged `r225m6-piped-01`..`r225m6-piped-08`. Each
//! exercises one surface behaviour of the M6 pipeline propagator:
//! [`paideia_as_shell_hm::union_effect_rows`],
//! [`paideia_as_shell_hm::compose_pipeline_effects`], and
//! [`paideia_as_shell_hm::typed_value_pipe`].
//!
//! The corpus is deliberately disjoint from the M5 corpus in
//! `tests/typed_value.rs`: M5 owns [`TypedValue`] construction and
//! unification; M6 owns the *union*-based pipeline fold that composes
//! typed values across stages without going through the unifier.
//!
//! Tests must NOT perturb the M1/M2/M3/M5 corpora — the M6 module is a
//! new algebraic operation on the same types, not a modification of
//! the existing algebra.

use std::collections::{BTreeMap, HashMap};

use paideia_as_shell_hm::{
    compose_pipeline_effects, typed_value_pipe, union_effect_rows, EffectRow, MonoType,
    RowType, TypeVar, TypedValue,
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
/// tail row-var. Mirrors the M5 corpus helper for consistency.
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
fn r225m6_piped_01_empty_pipeline_is_identity() {
    // typed_value_pipe(&[]) yields TypedValue::empty(): empty value row
    // and empty effect row. The monoidal-identity contract.
    let out = typed_value_pipe(&[]);
    assert_eq!(
        out.value_row,
        RowType::Empty,
        "r225m6-piped-01: empty pipeline yields empty value row"
    );
    assert_eq!(
        out.effect_row,
        EffectRow::empty(),
        "r225m6-piped-01: empty pipeline yields empty effect row"
    );
    assert_eq!(
        out,
        TypedValue::empty(),
        "r225m6-piped-01: composes to TypedValue::empty()"
    );
}

#[test]
fn r225m6_piped_02_single_stage_is_clone() {
    // typed_value_pipe(&[tv]) returns a clone of tv verbatim — no
    // rewriting of value or effect halves.
    let tv = TypedValue::with_effect(
        row_of(&[("name", str_())]),
        EffectRow::from_labels(&["io"]),
    );
    let out = typed_value_pipe(std::slice::from_ref(&tv));
    assert_eq!(
        out, tv,
        "r225m6-piped-02: single-stage pipeline is a clone"
    );
}

#[test]
fn r225m6_piped_03_two_stages_same_effect_idempotent() {
    // Two stages both {io}: the composed effect row still has exactly
    // one label `io` — union under LEFT-wins is idempotent on repeats.
    let a = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["io"]));
    let b = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["io"]));
    let out = typed_value_pipe(&[a, b]);
    assert_eq!(
        out.effect_row.present.len(),
        1,
        "r225m6-piped-03: idempotent union has one label"
    );
    assert!(
        out.effect_row.present.contains_key("io"),
        "r225m6-piped-03: `io` present"
    );
    assert!(
        out.effect_row.tail.is_none(),
        "r225m6-piped-03: closed row (no tail)"
    );
}

#[test]
fn r225m6_piped_04_two_stages_disjoint_effects_union() {
    // Stages {io} + {fs} → union carries both labels; BTreeMap
    // ordering makes {fs, io} the canonical form.
    let a = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["io"]));
    let b = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["fs"]));
    let out = typed_value_pipe(&[a, b]);
    assert_eq!(
        out.effect_row.present.len(),
        2,
        "r225m6-piped-04: two disjoint labels"
    );
    assert!(
        out.effect_row.present.contains_key("io"),
        "r225m6-piped-04: `io` from stage 1"
    );
    assert!(
        out.effect_row.present.contains_key("fs"),
        "r225m6-piped-04: `fs` from stage 2"
    );
    let keys: Vec<&String> = out.effect_row.present.keys().collect();
    assert_eq!(
        keys,
        vec![&"fs".to_owned(), &"io".to_owned()],
        "r225m6-piped-04: BTreeMap iteration is sorted"
    );
}

#[test]
fn r225m6_piped_05_three_stages_all_labels_present() {
    // Stages {io} + {fs} + {net} → union has all three labels; the
    // fold is left-associative but symmetric in this disjoint case.
    let a = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["io"]));
    let b = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["fs"]));
    let c = TypedValue::with_effect(RowType::Empty, EffectRow::from_labels(&["net"]));
    let out = typed_value_pipe(&[a, b, c]);
    assert_eq!(
        out.effect_row.present.len(),
        3,
        "r225m6-piped-05: three disjoint labels"
    );
    for label in ["io", "fs", "net"] {
        assert!(
            out.effect_row.present.contains_key(label),
            "r225m6-piped-05: `{label}` present"
        );
    }

    // And compose_pipeline_effects agrees with the effect half.
    let rows = [
        EffectRow::from_labels(&["io"]),
        EffectRow::from_labels(&["fs"]),
        EffectRow::from_labels(&["net"]),
    ];
    let composed = compose_pipeline_effects(&rows);
    assert_eq!(
        composed, out.effect_row,
        "r225m6-piped-05: pipe.effect matches compose_pipeline_effects"
    );
}

#[test]
fn r225m6_piped_06_value_row_is_last_stage() {
    // typed_value_pipe(&[tv1, tv2, tv3]).value_row == tv3.value_row —
    // the pipeline's output shape is the tail stage's output shape.
    let tv1 = TypedValue::of_row(row_of(&[("a", int())]));
    let tv2 = TypedValue::of_row(row_of(&[("b", str_())]));
    let tv3 = TypedValue::of_row(row_of(&[("c", int()), ("d", str_())]));
    let out = typed_value_pipe(&[tv1.clone(), tv2.clone(), tv3.clone()]);
    assert_eq!(
        out.value_row, tv3.value_row,
        "r225m6-piped-06: value_row is the last stage's"
    );
    // Sanity: it is NOT the first stage's.
    assert_ne!(
        out.value_row, tv1.value_row,
        "r225m6-piped-06: value_row is not the first stage's"
    );
}

#[test]
fn r225m6_piped_07_left_tail_wins_over_empty_right() {
    // union_effect_rows({r_tail | io}, {}) preserves the LEFT tail —
    // the RIGHT side is closed-empty, so LEFT's openness carries.
    let r_tail = TypeVar(700);
    let left = eff(&[("io", unit())], Some(r_tail));
    let right = EffectRow::empty();
    let out = union_effect_rows(&left, &right);
    assert_eq!(
        out.tail,
        Some(r_tail),
        "r225m6-piped-07: LEFT's tail preserved"
    );
    assert!(
        out.present.contains_key("io"),
        "r225m6-piped-07: `io` from LEFT preserved"
    );
    assert_eq!(
        out.present.len(),
        1,
        "r225m6-piped-07: only one label"
    );

    // Mirror: right-tail-only should also survive via the fallback.
    let left2 = EffectRow::empty();
    let right2 = eff(&[("fs", unit())], Some(r_tail));
    let out2 = union_effect_rows(&left2, &right2);
    assert_eq!(
        out2.tail,
        Some(r_tail),
        "r225m6-piped-07: RIGHT's tail used when LEFT has none"
    );
    assert!(
        out2.present.contains_key("fs"),
        "r225m6-piped-07: `fs` from RIGHT preserved"
    );
}

#[test]
fn r225m6_piped_08_duplicate_effect_left_wins() {
    // Stages {io} + {io, fs}: the composed row has only two distinct
    // labels — `io` (from stage 1, LEFT-wins on collision) and `fs`
    // (only in stage 2). Payloads distinguish the winner: give the two
    // `io` entries different payload types and confirm the LEFT one
    // survives.
    let left_io = int();
    let right_io = str_();
    let a = TypedValue::with_effect(
        RowType::Empty,
        eff(&[("io", left_io.clone())], None),
    );
    let b = TypedValue::with_effect(
        RowType::Empty,
        eff(&[("io", right_io.clone()), ("fs", unit())], None),
    );
    let out = typed_value_pipe(&[a, b]);
    assert_eq!(
        out.effect_row.present.len(),
        2,
        "r225m6-piped-08: two distinct labels (io deduplicated)"
    );
    assert_eq!(
        out.effect_row.present.get("io"),
        Some(&left_io),
        "r225m6-piped-08: LEFT wins on `io` collision (Int, not Str)"
    );
    assert_eq!(
        out.effect_row.present.get("fs"),
        Some(&unit()),
        "r225m6-piped-08: `fs` from stage 2 preserved"
    );
    assert!(
        out.effect_row.tail.is_none(),
        "r225m6-piped-08: both stages closed → closed union"
    );
}
