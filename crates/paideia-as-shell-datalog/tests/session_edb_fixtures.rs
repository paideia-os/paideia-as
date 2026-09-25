//! R226.M8 session-local EDB fixture corpus (10 tests, tag
//! `r226m8-edb-NN`). Each fixture exercises a piece of the
//! `SessionEdb` contract or the `Evaluator::run_*_with_session`
//! surface: assert, retract, snapshot, cross-turn survival,
//! program-facts additivity, and per-session isolation.
//!
//! Every panic message carries its fingerprint so the R220.M10
//! `@fingerprint` correlator can attribute a regression to a single
//! fixture without re-parsing the test name.

mod common;

use common::{assert_values_eq, dlg_tokens, ident};
use paideia_as_shell_datalog::{
    parse_block, parse_query, AssertResult, Evaluator, Program, Query, RetractResult,
    SessionEdb, Value,
};

// --------------------------------------------------------------------
// Shared helpers — local to this file (unrelated corpora do not need
// them, so they stay out of `tests/common/mod.rs`).
// --------------------------------------------------------------------

fn build_program(fp: &str, src: &str) -> Program {
    let tokens = dlg_tokens(src);
    parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse failed: {e:?}"))
}

fn build_query(fp: &str, src: &str) -> Query {
    let tokens = dlg_tokens(src);
    parse_query(&tokens).unwrap_or_else(|e| panic!("{fp}: query parse failed: {e:?}"))
}

fn values_of(
    bindings: &[paideia_as_shell_datalog::Binding],
    var: &str,
) -> Vec<Value> {
    let mut vs: Vec<Value> = bindings
        .iter()
        .filter_map(|b| b.get(var).cloned())
        .collect();
    vs.sort_by(|a, b| format!("{a}").cmp(&format!("{b}")));
    vs.dedup();
    vs
}

// ====================================================================
// 01 — assert single fact, snapshot contains it
// ====================================================================

#[test]
fn r226m8_edb_01_assert_snapshot_contains() {
    let fp = "r226m8-edb-01";
    let mut session = SessionEdb::new();
    let res = session.assert("parent", vec![ident("alice"), ident("bob")]);
    assert_eq!(res, AssertResult::Added, "{fp}: first assert must be Added");

    let db = session.snapshot();
    let tuples = db
        .tuples("parent", 2)
        .unwrap_or_else(|| panic!("{fp}: snapshot missing parent/2 relation"));
    assert_eq!(tuples.len(), 1, "{fp}: snapshot must hold exactly one tuple");
    assert!(
        tuples.contains(&vec![ident("alice"), ident("bob")]),
        "{fp}: snapshot missing asserted tuple",
    );
}

// ====================================================================
// 02 — retract asserted fact, snapshot no longer contains it
// ====================================================================

#[test]
fn r226m8_edb_02_retract_removes_from_snapshot() {
    let fp = "r226m8-edb-02";
    let mut session = SessionEdb::new();
    session.assert("parent", vec![ident("alice"), ident("bob")]);

    let res = session.retract("parent", &[ident("alice"), ident("bob")]);
    assert_eq!(res, RetractResult::Removed, "{fp}: retract must succeed");

    let db = session.snapshot();
    let tuples = db.tuples("parent", 2);
    let empty = tuples.map(|t| t.is_empty()).unwrap_or(true);
    assert!(empty, "{fp}: snapshot must not carry the retracted tuple");
}

// ====================================================================
// 03 — assert-then-query — query returns asserted facts
// ====================================================================

#[test]
fn r226m8_edb_03_assert_then_query() {
    let fp = "r226m8-edb-03";
    // Empty program — every returned tuple comes exclusively from the
    // session overlay, so this pins the merge path on its own.
    let program = Program::empty();
    let query = build_query(fp, "color(?C)");

    let mut session = SessionEdb::new();
    session.assert("color", vec![ident("red")]);
    session.assert("color", vec![ident("blue")]);

    let ev = Evaluator::new();
    let bindings = ev
        .run_query_with_session(&program, &query, &session)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    let got = values_of(&bindings, "C");
    assert_values_eq(fp, got, &[ident("blue"), ident("red")]);
}

// ====================================================================
// 04 — retract-then-query — query no longer returns retracted facts
// ====================================================================

#[test]
fn r226m8_edb_04_retract_then_query() {
    let fp = "r226m8-edb-04";
    let program = Program::empty();
    let query = build_query(fp, "color(?C)");

    let mut session = SessionEdb::new();
    session.assert("color", vec![ident("red")]);
    session.assert("color", vec![ident("blue")]);
    session.assert("color", vec![ident("green")]);
    let res = session.retract("color", &[ident("blue")]);
    assert_eq!(res, RetractResult::Removed, "{fp}: retract must succeed");

    let ev = Evaluator::new();
    let bindings = ev
        .run_query_with_session(&program, &query, &session)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    let got = values_of(&bindings, "C");
    assert_values_eq(fp, got, &[ident("green"), ident("red")]);
}

// ====================================================================
// 05 — cross-turn survival — three sequential queries accumulate
// ====================================================================

#[test]
fn r226m8_edb_05_cross_turn_survival() {
    let fp = "r226m8-edb-05";
    let program = Program::empty();
    let query = build_query(fp, "note(?N)");
    let ev = Evaluator::new();
    let mut session = SessionEdb::new();

    // Turn 1: one fact.
    session.assert("note", vec![ident("a")]);
    let got1 = values_of(
        &ev.run_query_with_session(&program, &query, &session)
            .unwrap_or_else(|e| panic!("{fp}: turn1 failed: {e:?}")),
        "N",
    );
    assert_values_eq(fp, got1, &[ident("a")]);

    // Turn 2: assertion from turn 1 must still be visible; add one.
    session.assert("note", vec![ident("b")]);
    let got2 = values_of(
        &ev.run_query_with_session(&program, &query, &session)
            .unwrap_or_else(|e| panic!("{fp}: turn2 failed: {e:?}")),
        "N",
    );
    assert_values_eq(fp, got2, &[ident("a"), ident("b")]);

    // Turn 3: both prior assertions survive; add a third.
    session.assert("note", vec![ident("c")]);
    let got3 = values_of(
        &ev.run_query_with_session(&program, &query, &session)
            .unwrap_or_else(|e| panic!("{fp}: turn3 failed: {e:?}")),
        "N",
    );
    assert_values_eq(fp, got3, &[ident("a"), ident("b"), ident("c")]);
}

// ====================================================================
// 06 — clear removes all
// ====================================================================

#[test]
fn r226m8_edb_06_clear_removes_all() {
    let fp = "r226m8-edb-06";
    let mut session = SessionEdb::new();
    session.assert("a", vec![ident("x")]);
    session.assert("b", vec![ident("y"), ident("z")]);
    session.assert("c", vec![Value::Num(42)]);
    assert_eq!(session.total_tuple_count(), 3, "{fp}: pre-clear count");

    session.clear();
    assert_eq!(session.total_tuple_count(), 0, "{fp}: post-clear count");
    assert!(session.is_empty(), "{fp}: session must be empty after clear");

    // Snapshot must also be empty (no keys, no tuples).
    let db = session.snapshot();
    assert_eq!(
        db.total_tuple_count(),
        0,
        "{fp}: snapshot of cleared session must have zero tuples",
    );
    assert!(
        db.predicate_keys().is_empty(),
        "{fp}: snapshot of cleared session must have zero predicate keys",
    );
}

// ====================================================================
// 07 — assert-duplicate returns AlreadyPresent
// ====================================================================

#[test]
fn r226m8_edb_07_assert_duplicate_alreadypresent() {
    let fp = "r226m8-edb-07";
    let mut session = SessionEdb::new();

    let first = session.assert("parent", vec![ident("alice"), ident("bob")]);
    assert_eq!(first, AssertResult::Added, "{fp}: first assert must be Added");

    let second = session.assert("parent", vec![ident("alice"), ident("bob")]);
    assert_eq!(
        second,
        AssertResult::AlreadyPresent,
        "{fp}: duplicate assert must be AlreadyPresent",
    );

    // Deduplication must be exact — snapshot has one tuple, not two.
    let db = session.snapshot();
    assert_eq!(
        db.tuples("parent", 2).map(|s| s.len()).unwrap_or(0),
        1,
        "{fp}: duplicate assert must not double-insert",
    );
}

// ====================================================================
// 08 — retract-nonexistent returns NotFound
// ====================================================================

#[test]
fn r226m8_edb_08_retract_nonexistent_notfound() {
    let fp = "r226m8-edb-08";
    let mut session = SessionEdb::new();

    // Retract from a completely unseen predicate.
    let miss = session.retract("nothing", &[ident("x")]);
    assert_eq!(
        miss,
        RetractResult::NotFound,
        "{fp}: retract on unknown predicate must be NotFound",
    );

    // Retract a tuple from a known predicate that does not carry it.
    session.assert("parent", vec![ident("alice"), ident("bob")]);
    let miss2 = session.retract("parent", &[ident("carol"), ident("dave")]);
    assert_eq!(
        miss2,
        RetractResult::NotFound,
        "{fp}: retract of non-present tuple must be NotFound",
    );

    // The prior assert must be untouched.
    let db = session.snapshot();
    assert_eq!(
        db.tuples("parent", 2).map(|s| s.len()).unwrap_or(0),
        1,
        "{fp}: NotFound retract must not perturb existing tuples",
    );
}

// ====================================================================
// 09 — EDB additive with program facts
// ====================================================================

#[test]
fn r226m8_edb_09_additive_with_program_facts() {
    let fp = "r226m8-edb-09";
    // Program contributes parent(a, b); session contributes parent(c, d).
    // Query over parent/2 must return BOTH tuples.
    let program = build_program(fp, "parent(a, b).");
    let query = build_query(fp, "parent(?X, ?Y)");

    let mut session = SessionEdb::new();
    session.assert("parent", vec![ident("c"), ident("d")]);

    let ev = Evaluator::new();
    let bindings = ev
        .run_query_with_session(&program, &query, &session)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));

    // Project both columns into `(X, Y)` pairs and sort for a stable
    // comparison — HashMap iteration order is non-deterministic.
    let mut pairs: Vec<(Value, Value)> = bindings
        .iter()
        .map(|b| {
            (
                b.get("X").cloned().expect("X binding"),
                b.get("Y").cloned().expect("Y binding"),
            )
        })
        .collect();
    pairs.sort_by(|a, b| format!("{}|{}", a.0, a.1).cmp(&format!("{}|{}", b.0, b.1)));

    let want: Vec<(Value, Value)> = vec![
        (ident("a"), ident("b")),
        (ident("c"), ident("d")),
    ];
    assert_eq!(
        pairs, want,
        "{fp}: expected program+session tuples both present",
    );
}

// ====================================================================
// 10 — EDB isolation — two sessions do not share state
// ====================================================================

#[test]
fn r226m8_edb_10_two_sessions_isolated() {
    let fp = "r226m8-edb-10";
    let mut session_a = SessionEdb::new();
    let session_b = SessionEdb::new();

    session_a.assert("only_in_a", vec![ident("x")]);

    // Session B must still be empty — no shared static, no globals.
    assert!(
        session_b.is_empty(),
        "{fp}: sibling session must remain empty after assert into other",
    );
    assert_eq!(
        session_b.total_tuple_count(),
        0,
        "{fp}: sibling session tuple count must stay 0",
    );

    // A query against session B over the same program must see no
    // session tuples for `only_in_a`.
    let program = Program::empty();
    let query = build_query(fp, "only_in_a(?V)");
    let ev = Evaluator::new();
    let bindings_b = ev
        .run_query_with_session(&program, &query, &session_b)
        .unwrap_or_else(|e| panic!("{fp}: query on B failed: {e:?}"));
    assert!(
        bindings_b.is_empty(),
        "{fp}: sibling session must not observe A's assertions",
    );

    // Sanity check: session A does see its own assertion.
    let bindings_a = ev
        .run_query_with_session(&program, &query, &session_a)
        .unwrap_or_else(|e| panic!("{fp}: query on A failed: {e:?}"));
    let got_a = values_of(&bindings_a, "V");
    assert_values_eq(fp, got_a, &[ident("x")]);
}
