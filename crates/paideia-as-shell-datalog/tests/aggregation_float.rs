//! R226.M6-followup (issue #1460): float support for `sum`/`avg`.
//! Fingerprint tag `r226m6f-NN`.
//!
//! Ten fixtures exercising the [`Value::Float`] variant through the
//! aggregation surface:
//!
//! * 01..02 — ungrouped `sum` / `avg` over a homogeneous float column.
//! * 03     — `min` / `max` over a float column (order comparator).
//! * 04     — empty-input identity behaviour on the float path.
//! * 05     — mixed `Value::Num` + `Value::Float` in one group is
//!            rejected as [`AggregationError::NonHomogeneousNumeric`].
//! * 06     — NaN taints a sum (documented — `SumF(NaN)`).
//! * 07     — parser-driven float literal round-trip through
//!            `parse_block` / `parse_query`.
//! * 08     — grouped sum on floats, two groups.
//! * 09     — `sum` on an integer column still returns [`AggregateResult::Sum`]
//!            (not `SumF`) — variant discrimination pinned.
//! * 10     — programmatic `Value::Float` round-trip through
//!            [`SessionEdb`], to prove `HashSet<Vec<Value>>` handles
//!            floats correctly.

mod common;

use common::dlg_tokens;
use paideia_as_shell_datalog::{
    parse_aggregate_query, parse_block, AggregateResult, AggregationError,
    EvalError, Evaluator, Program, SessionEdb, Value,
};
use paideia_as_shell_datalog::ast::{Aggregate, AggregateQuery, Atom, BodyGoal, Term};
use std::collections::HashMap;

// --------------------------------------------------------------------
// Local helpers.
// --------------------------------------------------------------------

/// Assert two `f64` compare bitwise-equal — matches the `Value::Float`
/// contract (see the `ast` module doc). Preferred over an epsilon
/// compare here because every fixture uses exact-representable
/// operands, so any drift is a real bug, not float rounding.
fn assert_bits_eq(fp: &str, got: f64, want: f64) {
    assert_eq!(
        got.to_bits(),
        want.to_bits(),
        "{fp}: expected {want} (bits {:#x}), got {got} (bits {:#x})",
        want.to_bits(),
        got.to_bits()
    );
}

/// Extract the ungrouped-row result. Panics with the fixture tag
/// on the "no rows" or "more than one row" case.
fn ungrouped(
    fp: &str,
    got: &HashMap<Vec<Value>, AggregateResult>,
) -> AggregateResult {
    assert_eq!(
        got.len(),
        1,
        "{fp}: ungrouped query must emit exactly one row, got {got:?}"
    );
    got.get(&Vec::new())
        .cloned()
        .unwrap_or_else(|| panic!("{fp}: ungrouped key (empty vec) missing from {got:?}"))
}

/// Build a program with a single unary predicate `val(?x)` seeded
/// from `floats`. Uses `SessionEdb` because the parser scope-narrow
/// (per issue #1460's plan) does not yet drive float literals through
/// `parse_block` from every test — fixture 07 exercises the parser
/// path explicitly.
fn program_from_floats(pred: &str, floats: &[f64]) -> (Program, SessionEdb) {
    let mut session = SessionEdb::new();
    for x in floats {
        session.assert(pred, vec![Value::Float(*x)]);
    }
    (Program::empty(), session)
}

/// Build an ungrouped `agg(?n) where <pred>(?n)` query programmatically —
/// mirrors `parse_aggregate_query` for the shape the seeded programs
/// above expose. Kept as a helper so a fixture reads one line.
fn ungrouped_query(agg: Aggregate, pred: &str) -> AggregateQuery {
    AggregateQuery {
        agg,
        target_var: "n".to_owned(),
        goals: vec![BodyGoal::Positive(Atom::new(
            pred,
            vec![Term::Var("n".to_owned())],
        ))],
        group_by: vec![],
    }
}

fn run_session_agg(
    fp: &str,
    program: &Program,
    query: &AggregateQuery,
    session: &SessionEdb,
) -> HashMap<Vec<Value>, AggregateResult> {
    Evaluator::new()
        .run_aggregate_query_with_session(program, query, session)
        .unwrap_or_else(|e| panic!("{fp}: aggregate eval failed: {e:?}"))
}

fn run_session_agg_err(
    fp: &str,
    program: &Program,
    query: &AggregateQuery,
    session: &SessionEdb,
) -> EvalError {
    Evaluator::new()
        .run_aggregate_query_with_session(program, query, session)
        .expect_err(&format!("{fp}: aggregate eval must reject"))
}

// ====================================================================
// 01 — ungrouped sum over floats → SumF.
// ====================================================================

#[test]
fn r226m6f_01_sum_ungrouped_floats() {
    let fp = "r226m6f-01";
    let (program, session) = program_from_floats("val", &[1.5, 2.25, 0.25]);
    let query = ungrouped_query(Aggregate::Sum, "val");
    let got = run_session_agg(fp, &program, &query, &session);
    match ungrouped(fp, &got) {
        AggregateResult::SumF(x) => assert_bits_eq(fp, x, 4.0),
        other => panic!("{fp}: expected SumF, got {other:?}"),
    }
}

// ====================================================================
// 02 — ungrouped avg over floats → AvgF.
// ====================================================================

#[test]
fn r226m6f_02_avg_ungrouped_floats() {
    let fp = "r226m6f-02";
    let (program, session) = program_from_floats("val", &[1.0, 2.0, 3.0, 4.0]);
    let query = ungrouped_query(Aggregate::Avg, "val");
    let got = run_session_agg(fp, &program, &query, &session);
    match ungrouped(fp, &got) {
        AggregateResult::AvgF(x) => assert_bits_eq(fp, x, 2.5),
        other => panic!("{fp}: expected AvgF, got {other:?}"),
    }
}

// ====================================================================
// 03 — min/max over floats via existing Min/Max path.
// ====================================================================

#[test]
fn r226m6f_03_min_max_floats() {
    let fp = "r226m6f-03";
    let (program, session) = program_from_floats("val", &[3.5, 1.25, 2.75, 0.5, 4.0]);

    let min_query = ungrouped_query(Aggregate::Min, "val");
    let min_got = run_session_agg(fp, &program, &min_query, &session);
    match ungrouped(fp, &min_got) {
        AggregateResult::Min(Value::Float(x)) => assert_bits_eq(fp, x, 0.5),
        other => panic!("{fp}: expected Min(Float), got {other:?}"),
    }

    let max_query = ungrouped_query(Aggregate::Max, "val");
    let max_got = run_session_agg(fp, &program, &max_query, &session);
    match ungrouped(fp, &max_got) {
        AggregateResult::Max(Value::Float(x)) => assert_bits_eq(fp, x, 4.0),
        other => panic!("{fp}: expected Max(Float), got {other:?}"),
    }
}

// ====================================================================
// 04 — empty float sequence: sum → Sum(0) (integer identity), avg
// → Empty (documented — no numeric-kind pinning is possible without
// any observations, so the reducer falls back to the integer identity
// for the module-doc-defined ungrouped-empty behaviour).
// ====================================================================

#[test]
fn r226m6f_04_empty_float_sequence_identity() {
    let fp = "r226m6f-04";
    // Empty session — no `val` facts land in the DB, so ungrouped
    // sum/avg hit the identity path in `aggregation::evaluate`.
    let (program, session) = program_from_floats("val", &[]);

    let sum_query = ungrouped_query(Aggregate::Sum, "val");
    let sum_got = run_session_agg(fp, &program, &sum_query, &session);
    assert_eq!(
        ungrouped(fp, &sum_got),
        AggregateResult::Sum(0),
        "{fp}: empty ungrouped sum must be integer identity Sum(0)"
    );

    let avg_query = ungrouped_query(Aggregate::Avg, "val");
    let avg_got = run_session_agg(fp, &program, &avg_query, &session);
    assert_eq!(
        ungrouped(fp, &avg_got),
        AggregateResult::Empty,
        "{fp}: empty ungrouped avg must be Empty sentinel"
    );
}

// ====================================================================
// 05 — mixing Value::Num and Value::Float in the same group is
// rejected as NonHomogeneousNumeric.
// ====================================================================

#[test]
fn r226m6f_05_mixed_int_float_rejected() {
    let fp = "r226m6f-05";
    // Seed one integer and one float under the same predicate. The
    // aggregation walks the substitution set produced by the DB's
    // `HashSet<Vec<Value>>` iteration, which is unordered — so we
    // cannot pin *which* kind wins the "first" slot; we only pin
    // that (a) the reducer surfaces `NonHomogeneousNumeric` rather
    // than silently coercing, and (b) the two reported kinds are
    // `Num` and `Float` in some order.
    let mut session = SessionEdb::new();
    session.assert("val", vec![Value::Num(42)]);
    session.assert("val", vec![Value::Float(3.14)]);

    let query = ungrouped_query(Aggregate::Sum, "val");
    let err = run_session_agg_err(fp, &Program::empty(), &query, &session);
    match err {
        EvalError::AggregationError(AggregationError::NonHomogeneousNumeric {
            first_type,
            got,
        }) => {
            assert!(
                (first_type == "Num" && got == "Float")
                    || (first_type == "Float" && got == "Num"),
                "{fp}: unexpected kind pair first_type={first_type} got={got}"
            );
            assert_ne!(
                first_type, got,
                "{fp}: homogeneity error must report two DIFFERENT kinds"
            );
        }
        other => panic!("{fp}: expected NonHomogeneousNumeric, got {other:?}"),
    }

    // Same check on avg — the pinning path is shared but the
    // dispatch site is separate, so a copy-paste regression on one
    // reducer does not silently pass through the other.
    let avg_query = ungrouped_query(Aggregate::Avg, "val");
    let avg_err = run_session_agg_err(fp, &Program::empty(), &avg_query, &session);
    match avg_err {
        EvalError::AggregationError(AggregationError::NonHomogeneousNumeric { .. }) => {}
        other => panic!(
            "{fp}: avg over mixed Num/Float must also error; got {other:?}"
        ),
    }
}

// ====================================================================
// 06 — NaN taints a float sum. Documented behaviour: SumF(NaN).
// ====================================================================

#[test]
fn r226m6f_06_nan_taints_sum() {
    let fp = "r226m6f-06";
    let (program, session) = program_from_floats("val", &[1.0, f64::NAN, 2.0]);
    let query = ungrouped_query(Aggregate::Sum, "val");
    let got = run_session_agg(fp, &program, &query, &session);
    match ungrouped(fp, &got) {
        AggregateResult::SumF(x) => {
            assert!(
                x.is_nan(),
                "{fp}: NaN in a float sum must taint the total; got {x}"
            );
        }
        other => panic!("{fp}: expected SumF(NaN), got {other:?}"),
    }
}

// ====================================================================
// 07 — parser-driven float literal round-trip. Exercises the
// `Number(raw)` → `Value::Float` branch added to the parser as part
// of this issue.
// ====================================================================

#[test]
fn r226m6f_07_parser_float_literal_roundtrip() {
    let fp = "r226m6f-07";
    let program_src = "val(a, 1.5). val(b, 2.5). val(c, 4.0).";
    let query_src = "sum(?n) where val(?x, ?n)";

    // Parse the program and confirm the ground facts carry Value::Float.
    let program_tokens = dlg_tokens(program_src);
    let program = parse_block(&program_tokens)
        .unwrap_or_else(|e| panic!("{fp}: parse_block failed: {e:?}"));
    assert_eq!(
        program.facts.len(),
        3,
        "{fp}: expected 3 facts, got {}",
        program.facts.len()
    );
    for fact in &program.facts {
        match &fact.terms[1] {
            Term::Const(Value::Float(_)) => {}
            other => panic!(
                "{fp}: expected float in slot 1 of {fact}, got {other:?}"
            ),
        }
    }

    // Parse the query, run it, and confirm SumF surfaces.
    let query_tokens = dlg_tokens(query_src);
    let query = parse_aggregate_query(&query_tokens)
        .unwrap_or_else(|e| panic!("{fp}: parse_aggregate_query failed: {e:?}"));
    let got = Evaluator::new()
        .run_aggregate_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: aggregate eval failed: {e:?}"));
    match ungrouped(fp, &got) {
        AggregateResult::SumF(x) => assert_bits_eq(fp, x, 8.0),
        other => panic!("{fp}: expected SumF(8.0), got {other:?}"),
    }
}

// ====================================================================
// 08 — grouped sum on floats: two groups, distinct SumF results.
// ====================================================================

#[test]
fn r226m6f_08_grouped_sum_floats() {
    let fp = "r226m6f-08";
    // `sale(?dept, ?amt)` with dept ∈ {sales, eng} and float amounts.
    let mut session = SessionEdb::new();
    for (dept, amt) in [
        ("sales", 1.5f64),
        ("sales", 0.5),
        ("eng", 2.0),
        ("eng", 3.0),
        ("eng", 0.5),
    ] {
        session.assert(
            "sale",
            vec![Value::Ident(dept.to_owned()), Value::Float(amt)],
        );
    }

    let query = AggregateQuery {
        agg: Aggregate::Sum,
        target_var: "amt".to_owned(),
        goals: vec![BodyGoal::Positive(Atom::new(
            "sale",
            vec![Term::Var("dept".to_owned()), Term::Var("amt".to_owned())],
        ))],
        group_by: vec!["dept".to_owned()],
    };
    let got = run_session_agg(fp, &Program::empty(), &query, &session);

    assert_eq!(
        got.len(),
        2,
        "{fp}: expected 2 groups, got {}: {got:?}",
        got.len()
    );

    let sales_key = vec![Value::Ident("sales".to_owned())];
    let eng_key = vec![Value::Ident("eng".to_owned())];

    match got.get(&sales_key) {
        Some(AggregateResult::SumF(x)) => assert_bits_eq(fp, *x, 2.0),
        other => panic!("{fp}: sales group: expected SumF(2.0), got {other:?}"),
    }
    match got.get(&eng_key) {
        Some(AggregateResult::SumF(x)) => assert_bits_eq(fp, *x, 5.5),
        other => panic!("{fp}: eng group: expected SumF(5.5), got {other:?}"),
    }
}

// ====================================================================
// 09 — sum over Value::Num still returns AggregateResult::Sum
// (integer variant), not SumF. Pins the variant discrimination so
// a caller that pattern-matches on Sum never silently starts seeing
// SumF once floats coexist elsewhere in the codebase.
// ====================================================================

#[test]
fn r226m6f_09_sum_int_column_stays_int_variant() {
    let fp = "r226m6f-09";
    // Integer-only session.
    let mut session = SessionEdb::new();
    for n in [10i64, 20, 30] {
        session.assert("val", vec![Value::Num(n)]);
    }
    let query = ungrouped_query(Aggregate::Sum, "val");
    let got = run_session_agg(fp, &Program::empty(), &query, &session);
    match ungrouped(fp, &got) {
        AggregateResult::Sum(x) => assert_eq!(x, 60, "{fp}: integer sum mismatch"),
        other => panic!(
            "{fp}: integer column must stay Sum(_), got {other:?} (SumF regression?)"
        ),
    }
}

// ====================================================================
// 10 — programmatic Value::Float round-trip through SessionEdb.
// Proves `HashSet<Vec<Value>>` correctly stores and retrieves float
// values (the whole point of the custom PartialEq/Hash impls in
// `ast::Value`).
// ====================================================================

#[test]
fn r226m6f_10_value_float_hashset_roundtrip() {
    let fp = "r226m6f-10";
    let mut session = SessionEdb::new();

    // Distinct floats — each assertion should land in the set.
    for x in [1.5f64, 2.5, 3.5, 4.5] {
        let res = session.assert("val", vec![Value::Float(x)]);
        assert!(
            matches!(res, paideia_as_shell_datalog::AssertResult::Added),
            "{fp}: expected Added for {x}, got {res:?}"
        );
    }

    // Duplicate assertion of an equal-bit-pattern float must
    // deduplicate through the HashSet — pins the `Hash` + `PartialEq`
    // contract from the ast module doc.
    let dup = session.assert("val", vec![Value::Float(1.5)]);
    assert!(
        matches!(dup, paideia_as_shell_datalog::AssertResult::AlreadyPresent),
        "{fp}: duplicate float must dedupe; got {dup:?}"
    );

    // Round-trip: sum via aggregation, comparing bits (values were
    // chosen to make 12.0 exact).
    let query = ungrouped_query(Aggregate::Sum, "val");
    let got = Evaluator::new()
        .run_aggregate_query_with_session(&Program::empty(), &query, &session)
        .unwrap_or_else(|e| panic!("{fp}: aggregate eval failed: {e:?}"));
    match ungrouped(fp, &got) {
        AggregateResult::SumF(x) => assert_bits_eq(fp, x, 12.0),
        other => panic!("{fp}: expected SumF(12.0), got {other:?}"),
    }
}
