//! R226.M6 aggregation fixture corpus — 20 tests, fingerprint tag
//! `r226m6-agg-NN`. Each fixture builds a Datalog program, poses an
//! [`AggregateQuery`] (parsed through [`parse_aggregate_query`], to
//! exercise both the AST/parser surface and the aggregation
//! evaluator), and asserts the resulting `HashMap<Vec<Value>,
//! AggregateResult>`.
//!
//! Corpus split:
//!
//! * **Count** — 01..05  (5)
//! * **Sum**   — 06..09  (4)
//! * **Min**   — 10..13  (4)
//! * **Max**   — 14..17  (4)
//! * **Avg**   — 18..20  (3)
//!
//! Every panic carries its fixture tag so the R220.M10
//! `@fingerprint` correlator can pin a failure without re-parsing the
//! test name.

mod common;

use common::dlg_tokens;
use paideia_as_shell_datalog::{
    parse_aggregate_query, parse_block, AggregateQuery, AggregateResult, AggregationError,
    Evaluator, EvalError, Program, Value,
};
use std::collections::HashMap;

// --------------------------------------------------------------------
// Shared helpers — local to this file. The other fixture corpora do
// not need them, so they stay out of `tests/common/mod.rs`.
// --------------------------------------------------------------------

fn build_program(fp: &str, src: &str) -> Program {
    let tokens = dlg_tokens(src);
    parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse failed: {e:?}"))
}

fn build_agg_query(fp: &str, src: &str) -> AggregateQuery {
    let tokens = dlg_tokens(src);
    parse_aggregate_query(&tokens)
        .unwrap_or_else(|e| panic!("{fp}: aggregate query parse failed: {e:?}"))
}

fn run_agg(
    fp: &str,
    program_src: &str,
    query_src: &str,
) -> HashMap<Vec<Value>, AggregateResult> {
    let program = build_program(fp, program_src);
    let query = build_agg_query(fp, query_src);
    let ev = Evaluator::new();
    ev.run_aggregate_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: aggregate eval failed: {e:?}"))
}

fn run_agg_err(fp: &str, program_src: &str, query_src: &str) -> EvalError {
    let program = build_program(fp, program_src);
    let query = build_agg_query(fp, query_src);
    let ev = Evaluator::new();
    ev.run_aggregate_query(&program, &query)
        .expect_err(&format!("{fp}: aggregate eval must reject"))
}

/// Assert the ungrouped result is `expected`. Panics with the fixture
/// tag on any mismatch.
fn assert_ungrouped(
    fp: &str,
    got: &HashMap<Vec<Value>, AggregateResult>,
    expected: AggregateResult,
) {
    assert_eq!(
        got.len(),
        1,
        "{fp}: ungrouped query must emit exactly one row, got {got:?}"
    );
    let row = got
        .get(&Vec::new())
        .unwrap_or_else(|| panic!("{fp}: ungrouped key (empty vec) missing from {got:?}"));
    assert_eq!(row, &expected, "{fp}: ungrouped result mismatch");
}

/// Assert the grouped result exactly matches `expected` (same keys,
/// same values, both directions).
fn assert_grouped(
    fp: &str,
    got: &HashMap<Vec<Value>, AggregateResult>,
    expected: &[(Vec<Value>, AggregateResult)],
) {
    assert_eq!(
        got.len(),
        expected.len(),
        "{fp}: expected {} groups, got {} — full got={got:?}",
        expected.len(),
        got.len()
    );
    for (key, want) in expected {
        let got_val = got
            .get(key)
            .unwrap_or_else(|| panic!("{fp}: missing group key {key:?} — got={got:?}"));
        assert_eq!(got_val, want, "{fp}: group {key:?} value mismatch");
    }
}

fn ident(s: &str) -> Value {
    Value::Ident(s.to_owned())
}
fn num(n: i64) -> Value {
    Value::Num(n)
}

// ====================================================================
// COUNT — r226m6-agg-01 .. r226m6-agg-05
// ====================================================================

#[test]
fn r226m6_agg_01_count_all_facts_of_one_predicate() {
    // Three person facts; count(?x) where person(?x) → 3.
    let fp = "r226m6-agg-01";
    let got = run_agg(
        fp,
        "person(alice). person(bob). person(carol).",
        "count(?x) where person(?x)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Count(3));
}

#[test]
fn r226m6_agg_02_count_with_filter() {
    // person + is_admin; count(?x) where person(?x), is_admin(?x) → 1.
    let fp = "r226m6-agg-02";
    let got = run_agg(
        fp,
        "person(alice). person(bob). person(carol).\n\
         is_admin(bob).",
        "count(?x) where person(?x), is_admin(?x)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Count(1));
}

#[test]
fn r226m6_agg_03_count_group_by_one_var() {
    // child(parent, kid). Count kids per parent.
    // alice → 2 (bob, carol); mary → 1 (dan).
    let fp = "r226m6-agg-03";
    let got = run_agg(
        fp,
        "child(alice, bob). child(alice, carol). child(mary, dan).",
        "count(?c) group by ?p where child(?p, ?c)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("alice")], AggregateResult::Count(2)),
            (vec![ident("mary")], AggregateResult::Count(1)),
        ],
    );
}

#[test]
fn r226m6_agg_04_count_group_by_two_vars() {
    // sale(dept, quarter, item) — count items per (dept, quarter).
    // (d1, q1) → 2 (alice, bob); (d1, q2) → 1 (carol); (d2, q1) → 1 (dan).
    let fp = "r226m6-agg-04";
    let got = run_agg(
        fp,
        "sale(d1, q1, alice). sale(d1, q1, bob).\n\
         sale(d1, q2, carol).\n\
         sale(d2, q1, dan).",
        "count(?x) group by ?d, ?q where sale(?d, ?q, ?x)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("d1"), ident("q1")], AggregateResult::Count(2)),
            (vec![ident("d1"), ident("q2")], AggregateResult::Count(1)),
            (vec![ident("d2"), ident("q1")], AggregateResult::Count(1)),
        ],
    );
}

#[test]
fn r226m6_agg_05_count_empty_result_is_zero() {
    // No `nonexistent` facts → ungrouped count → identity Count(0).
    let fp = "r226m6-agg-05";
    let got = run_agg(
        fp,
        "person(alice).",
        "count(?x) where nonexistent(?x)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Count(0));
}

// ====================================================================
// SUM — r226m6-agg-06 .. r226m6-agg-09
// ====================================================================

#[test]
fn r226m6_agg_06_sum_ungrouped() {
    // val(?item, ?n): 10 + 20 + 30 + 40 = 100.
    let fp = "r226m6-agg-06";
    let got = run_agg(
        fp,
        "val(a, 10). val(b, 20). val(c, 30). val(d, 40).",
        "sum(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Sum(100));
}

#[test]
fn r226m6_agg_07_sum_group_by() {
    // sale(dept, amt) — sum(?amt) group by ?dept.
    // sales: 100 + 200 = 300; eng: 500 + 700 = 1200.
    let fp = "r226m6-agg-07";
    let got = run_agg(
        fp,
        "sale(sales, 100). sale(sales, 200).\n\
         sale(eng, 500). sale(eng, 700).",
        "sum(?amt) group by ?dept where sale(?dept, ?amt)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("sales")], AggregateResult::Sum(300)),
            (vec![ident("eng")], AggregateResult::Sum(1200)),
        ],
    );
}

#[test]
fn r226m6_agg_08_sum_empty_is_zero() {
    // No `val` facts → ungrouped sum → additive identity Sum(0).
    let fp = "r226m6-agg-08";
    let got = run_agg(
        fp,
        "person(alice).",
        "sum(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Sum(0));
}

#[test]
fn r226m6_agg_09_sum_non_numeric_target_errors() {
    // val(a, "hello") — the string cannot be coerced; must error, not
    // panic.
    let fp = "r226m6-agg-09";
    let err = run_agg_err(
        fp,
        "val(a, 10). val(b, \"hello\").",
        "sum(?n) where val(?x, ?n)",
    );
    match err {
        EvalError::AggregationError(AggregationError::NonNumericTarget { got }) => {
            assert_eq!(got, Value::Str("hello".to_owned()), "{fp}: wrong offending value");
        }
        other => panic!("{fp}: expected NonNumericTarget, got {other:?}"),
    }
}

// ====================================================================
// MIN — r226m6-agg-10 .. r226m6-agg-13
// ====================================================================

#[test]
fn r226m6_agg_10_min_ungrouped() {
    // val values: 40, 10, 30, 20 → min = 10.
    let fp = "r226m6-agg-10";
    let got = run_agg(
        fp,
        "val(a, 40). val(b, 10). val(c, 30). val(d, 20).",
        "min(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Min(num(10)));
}

#[test]
fn r226m6_agg_11_min_group_by() {
    // sale(dept, amt) — min per dept.
    // sales: min(300, 100) = 100. eng: min(700, 500) = 500.
    let fp = "r226m6-agg-11";
    let got = run_agg(
        fp,
        "sale(sales, 300). sale(sales, 100).\n\
         sale(eng, 700). sale(eng, 500).",
        "min(?amt) group by ?dept where sale(?dept, ?amt)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("sales")], AggregateResult::Min(num(100))),
            (vec![ident("eng")], AggregateResult::Min(num(500))),
        ],
    );
}

#[test]
fn r226m6_agg_12_min_empty_is_empty_sentinel() {
    // No `val` facts → ungrouped min → Empty (no meaningful minimum).
    let fp = "r226m6-agg-12";
    let got = run_agg(
        fp,
        "person(alice).",
        "min(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Empty);
}

#[test]
fn r226m6_agg_13_min_mixed_with_min_in_group() {
    // Group by ?g; each group has distinct minima. A duplicate
    // low-value in one group must not confuse the reducer.
    // g1: 5, 5, 12 → 5. g2: 7, 3 → 3. g3: 9 → 9.
    let fp = "r226m6-agg-13";
    let got = run_agg(
        fp,
        "obs(g1, 5). obs(g1, 5). obs(g1, 12).\n\
         obs(g2, 7). obs(g2, 3).\n\
         obs(g3, 9).",
        "min(?v) group by ?g where obs(?g, ?v)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("g1")], AggregateResult::Min(num(5))),
            (vec![ident("g2")], AggregateResult::Min(num(3))),
            (vec![ident("g3")], AggregateResult::Min(num(9))),
        ],
    );
}

// ====================================================================
// MAX — r226m6-agg-14 .. r226m6-agg-17
// ====================================================================

#[test]
fn r226m6_agg_14_max_ungrouped() {
    // val values: 40, 10, 30, 20 → max = 40.
    let fp = "r226m6-agg-14";
    let got = run_agg(
        fp,
        "val(a, 40). val(b, 10). val(c, 30). val(d, 20).",
        "max(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Max(num(40)));
}

#[test]
fn r226m6_agg_15_max_group_by() {
    // Same corpus as agg-11 with max instead of min.
    // sales: max(300, 100) = 300. eng: max(700, 500) = 700.
    let fp = "r226m6-agg-15";
    let got = run_agg(
        fp,
        "sale(sales, 300). sale(sales, 100).\n\
         sale(eng, 700). sale(eng, 500).",
        "max(?amt) group by ?dept where sale(?dept, ?amt)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("sales")], AggregateResult::Max(num(300))),
            (vec![ident("eng")], AggregateResult::Max(num(700))),
        ],
    );
}

#[test]
fn r226m6_agg_16_max_empty_is_empty_sentinel() {
    let fp = "r226m6-agg-16";
    let got = run_agg(
        fp,
        "person(alice).",
        "max(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Empty);
}

#[test]
fn r226m6_agg_17_max_on_tied_values() {
    // Two rows share the max value; max must still be that value.
    // val: 42, 42, 17 → max = 42.
    let fp = "r226m6-agg-17";
    let got = run_agg(
        fp,
        "val(a, 42). val(b, 42). val(c, 17).",
        "max(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Max(num(42)));
}

// ====================================================================
// AVG — r226m6-agg-18 .. r226m6-agg-20
// ====================================================================

#[test]
fn r226m6_agg_18_avg_ungrouped() {
    // val values: 10, 20, 30, 40 → avg = 25.0.
    let fp = "r226m6-agg-18";
    let got = run_agg(
        fp,
        "val(a, 10). val(b, 20). val(c, 30). val(d, 40).",
        "avg(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Avg(25.0));
}

#[test]
fn r226m6_agg_19_avg_group_by() {
    // sale(dept, amt): sales = (100 + 300) / 2 = 200.0.
    //                   eng  = (500 + 700 + 600) / 3 = 600.0.
    let fp = "r226m6-agg-19";
    let got = run_agg(
        fp,
        "sale(sales, 100). sale(sales, 300).\n\
         sale(eng, 500). sale(eng, 700). sale(eng, 600).",
        "avg(?amt) group by ?dept where sale(?dept, ?amt)",
    );
    assert_grouped(
        fp,
        &got,
        &[
            (vec![ident("sales")], AggregateResult::Avg(200.0)),
            (vec![ident("eng")], AggregateResult::Avg(600.0)),
        ],
    );
}

#[test]
fn r226m6_agg_20_avg_empty_is_empty_sentinel() {
    // No `val` facts → ungrouped avg → Empty (0/0 has no value).
    let fp = "r226m6-agg-20";
    let got = run_agg(
        fp,
        "person(alice).",
        "avg(?n) where val(?x, ?n)",
    );
    assert_ungrouped(fp, &got, AggregateResult::Empty);
}
