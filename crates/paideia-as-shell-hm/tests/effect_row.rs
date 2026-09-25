//! R225.M3 effect-row fixture corpus.
//!
//! 10 tests, tagged `r225m3-eff-01`..`r225m3-eff-10`. Each exercises
//! one surface behaviour of the M3 effect-row extensions:
//! [`paideia_as_shell_hm::EffectRow`],
//! [`paideia_as_shell_hm::unify_effect_rows`], and the
//! `MonoType::EffectRow` variant's `apply` / `free_vars` interplay
//! with [`paideia_as_shell_hm::Substitution`].
//!
//! Tests must NOT perturb the M1 (`tests/algorithm_w.rs`) or M2
//! (`tests/row_types.rs`) corpora — the M3 module is a new algebraic
//! variant, not a modification of the existing ones.

use std::collections::BTreeMap;

use paideia_as_shell_hm::{
    unify_effect_rows, unify_with_fresh, EffectRow, FreshVarGen, MonoType, RowType, Substitution,
    TypeVar, UnifyError,
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

/// Convenience: build an EffectRow from `(label, payload)` pairs and an
/// optional tail.
fn eff(fields: &[(&str, MonoType)], tail: Option<TypeVar>) -> EffectRow {
    let mut present = BTreeMap::new();
    for (name, ty) in fields {
        present.insert((*name).to_owned(), ty.clone());
    }
    EffectRow { present, tail }
}

// ---------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------

#[test]
fn r225m3_eff_01_closed_singleton_matches_itself() {
    // !{io} ~ !{io} : Ok, empty subst.
    let a = EffectRow::from_labels(&["io"]);
    let b = EffectRow::from_labels(&["io"]);
    let mut fresh = FreshVarGen::new();
    let sub = unify_effect_rows(&a, &b, &mut fresh).expect("r225m3-eff-01: unify must succeed");
    assert_eq!(sub, Substitution::empty(), "r225m3-eff-01: trivial subst");
}

#[test]
fn r225m3_eff_02_disjoint_closed_singletons_mismatch() {
    // !{io} ~ !{fs} : Err EffectRowMismatch { missing:["fs"], extra:["io"] }.
    let a = EffectRow::from_labels(&["io"]);
    let b = EffectRow::from_labels(&["fs"]);
    let mut fresh = FreshVarGen::new();
    let err = unify_effect_rows(&a, &b, &mut fresh).expect_err("r225m3-eff-02: must fail");
    match err {
        UnifyError::EffectRowMismatch { missing, extra } => {
            assert_eq!(missing, vec!["fs".to_owned()], "r225m3-eff-02: missing = [fs]");
            assert_eq!(extra, vec!["io".to_owned()], "r225m3-eff-02: extra = [io]");
        }
        other => panic!("r225m3-eff-02: expected EffectRowMismatch, got {other:?}"),
    }
}

#[test]
fn r225m3_eff_03_smaller_closed_row_missing_label() {
    // !{io} ~ !{io, fs} : both tails None, right has an extra `fs` that
    // left cannot absorb — Err EffectRowMismatch { missing:["fs"], extra:[] }.
    let a = EffectRow::from_labels(&["io"]);
    let b = EffectRow::from_labels(&["io", "fs"]);
    let mut fresh = FreshVarGen::new();
    let err = unify_effect_rows(&a, &b, &mut fresh).expect_err("r225m3-eff-03: must fail");
    match err {
        UnifyError::EffectRowMismatch { missing, extra } => {
            assert_eq!(missing, vec!["fs".to_owned()], "r225m3-eff-03: missing = [fs]");
            assert!(extra.is_empty(), "r225m3-eff-03: no extras on left");
        }
        other => panic!("r225m3-eff-03: expected EffectRowMismatch, got {other:?}"),
    }
}

#[test]
fn r225m3_eff_04_label_order_agnostic() {
    // !{io, fs} ~ !{fs, io} : BTreeMap canonicalises key order, so both
    // sides are structurally identical — unify with empty subst.
    let a = EffectRow::from_labels(&["io", "fs"]);
    let b = EffectRow::from_labels(&["fs", "io"]);
    assert_eq!(
        a, b,
        "r225m3-eff-04: BTreeMap normalisation makes the two constructors equal"
    );
    let mut fresh = FreshVarGen::new();
    let sub = unify_effect_rows(&a, &b, &mut fresh).expect("r225m3-eff-04: unify must succeed");
    assert_eq!(sub, Substitution::empty(), "r225m3-eff-04: trivial subst");
}

#[test]
fn r225m3_eff_05_open_row_absorbs_extras() {
    // !{io | rho} ~ !{io, fs} : rho ↦ !{fs}.
    let rho = TypeVar(50);
    let a = eff(&[("io", unit())], Some(rho));
    let b = EffectRow::from_labels(&["io", "fs"]);
    let mut fresh = FreshVarGen::new();
    // Skip past rho so a Rémy witness would not collide.
    for _ in 0..100 {
        fresh.fresh();
    }
    let sub = unify_effect_rows(&a, &b, &mut fresh).expect("r225m3-eff-05: unify must succeed");
    let bound = sub.lookup(&rho).expect("r225m3-eff-05: rho must be bound");
    match bound {
        MonoType::EffectRow(row) => {
            assert!(row.tail.is_none(), "r225m3-eff-05: closed absorbed tail");
            assert_eq!(row.present.len(), 1, "r225m3-eff-05: exactly `fs` absorbed");
            assert_eq!(
                row.present.get("fs"),
                Some(&unit()),
                "r225m3-eff-05: fs: Unit"
            );
        }
        other => panic!("r225m3-eff-05: expected EffectRow binding, got {other:?}"),
    }
}

#[test]
fn r225m3_eff_06_two_open_rows_share_fresh_tail() {
    // !{io | rho} ~ !{io | rho'} : both extras empty, Rémy witness
    // links the two tails through a fresh row-var `r`.
    let rho = TypeVar(60);
    let rho_prime = TypeVar(61);
    let a = eff(&[("io", unit())], Some(rho));
    let b = eff(&[("io", unit())], Some(rho_prime));
    let mut fresh = FreshVarGen::new();
    for _ in 0..100 {
        fresh.fresh();
    }
    let sub = unify_effect_rows(&a, &b, &mut fresh).expect("r225m3-eff-06: unify must succeed");
    let rho_bound = sub.lookup(&rho).expect("r225m3-eff-06: rho bound");
    let rho_prime_bound = sub
        .lookup(&rho_prime)
        .expect("r225m3-eff-06: rho' bound");
    let a_row = match rho_bound {
        MonoType::EffectRow(r) => r,
        other => panic!("r225m3-eff-06: rho -> {other:?}"),
    };
    let b_row = match rho_prime_bound {
        MonoType::EffectRow(r) => r,
        other => panic!("r225m3-eff-06: rho' -> {other:?}"),
    };
    assert!(
        a_row.present.is_empty(),
        "r225m3-eff-06: rho binding has no present labels (extras were empty)"
    );
    assert!(
        b_row.present.is_empty(),
        "r225m3-eff-06: rho' binding has no present labels"
    );
    assert!(
        a_row.tail.is_some() && a_row.tail == b_row.tail,
        "r225m3-eff-06: both tails share the fresh Rémy witness, a={:?} b={:?}",
        a_row.tail,
        b_row.tail
    );
}

#[test]
fn r225m3_eff_07_record_vs_effect_cross_variant_mismatch() {
    // A record row and an effect row must NOT unify — they occupy
    // disjoint namespaces. The failure surfaces as a plain
    // UnifyError::Mismatch, NOT an EffectRowMismatch / RowMismatch /
    // MissingField (which would falsely suggest the two are the same
    // shape modulo labels).
    let mut fresh = FreshVarGen::new();
    let record = MonoType::Record(RowType::from_map(
        std::iter::once(("io".to_owned(), unit())).collect(),
        None,
    ));
    let effect = MonoType::EffectRow(EffectRow::from_labels(&["io"]));
    let err = unify_with_fresh(&record, &effect, &mut fresh)
        .expect_err("r225m3-eff-07: must fail");
    assert!(
        matches!(err, UnifyError::Mismatch { .. }),
        "r225m3-eff-07: expected Mismatch (not a row-family error), got {err:?}"
    );
}

#[test]
fn r225m3_eff_08_free_vars_include_tail() {
    // free_vars of MonoType::EffectRow(!{io | rho}) = {rho}. `io`'s
    // Unit payload has no free vars, so only the tail contributes.
    let rho = TypeVar(7);
    let row = MonoType::EffectRow(eff(&[("io", unit())], Some(rho)));
    let fvs = row.free_vars();
    assert_eq!(fvs.len(), 1, "r225m3-eff-08: exactly one free var");
    assert!(fvs.contains(&rho), "r225m3-eff-08: rho is free");
}

#[test]
fn r225m3_eff_09_apply_walks_payload_types() {
    // apply on !{io: t0, fs: Int} with { t0 ↦ Int } yields
    // !{io: Int, fs: Int} — the walker must descend into every payload.
    let t0 = TypeVar(3);
    let row = MonoType::EffectRow(eff(
        &[("io", MonoType::Var(t0)), ("fs", int())],
        None,
    ));
    let mut sub = Substitution::empty();
    sub.insert(t0, int());
    let applied = sub.apply(&row);
    match applied {
        MonoType::EffectRow(r) => {
            assert!(r.tail.is_none(), "r225m3-eff-09: closed tail preserved");
            assert_eq!(r.present.get("io"), Some(&int()), "r225m3-eff-09: io: Int");
            assert_eq!(r.present.get("fs"), Some(&int()), "r225m3-eff-09: fs: Int");
        }
        other => panic!("r225m3-eff-09: expected EffectRow, got {other:?}"),
    }
}

#[test]
fn r225m3_eff_10_transitive_binding_splices_through_third_row() {
    // Step 1: unify !{io, fs} ~ !{io | rho} ⇒ rho ↦ !{fs}.
    // Step 2: apply that substitution to a third row !{ | rho}. The
    // tail must be spliced with `fs` and closed.
    let rho = TypeVar(80);
    let a = EffectRow::from_labels(&["io", "fs"]);
    let b = eff(&[("io", unit())], Some(rho));
    let mut fresh = FreshVarGen::new();
    for _ in 0..150 {
        fresh.fresh();
    }
    let sub = unify_effect_rows(&a, &b, &mut fresh).expect("r225m3-eff-10: step-1 unify");

    // Sanity: rho got bound to a closed row carrying `fs`.
    let bound = sub.lookup(&rho).expect("r225m3-eff-10: rho bound");
    match bound {
        MonoType::EffectRow(r) => {
            assert!(r.tail.is_none(), "r225m3-eff-10: rho binding is closed");
            assert_eq!(r.present.len(), 1, "r225m3-eff-10: rho carries exactly fs");
            assert_eq!(
                r.present.get("fs"),
                Some(&unit()),
                "r225m3-eff-10: rho: {{fs}}"
            );
        }
        other => panic!("r225m3-eff-10: rho -> {other:?}"),
    }

    // Step 2: apply to a third row !{ | rho}.
    let third = MonoType::EffectRow(eff(&[], Some(rho)));
    let applied = sub.apply(&third);
    match applied {
        MonoType::EffectRow(r) => {
            assert!(r.tail.is_none(), "r225m3-eff-10: spliced third row is closed");
            assert_eq!(r.present.len(), 1, "r225m3-eff-10: third row spliced to {{fs}}");
            assert_eq!(
                r.present.get("fs"),
                Some(&unit()),
                "r225m3-eff-10: spliced fs: Unit"
            );
        }
        other => panic!("r225m3-eff-10: expected spliced EffectRow, got {other:?}"),
    }
}
