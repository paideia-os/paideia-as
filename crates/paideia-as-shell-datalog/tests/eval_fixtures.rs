//! R226.M2 seminaïve evaluator fixture corpus (10 tests, tag
//! `r226-m2-NN`). Every fixture builds a `Database` by fixpoint and
//! then poses a query, asserting the free-variable projection.

mod common;

use common::{assert_values_eq, build_db, ident, query_var};
use paideia_as_shell_datalog::{
    parse_block, parse_query, query, Atom, Database, EvalError, Program, Query, Term, Value,
};

#[test]
fn r226_m2_01_edb_only_returns_seeded_facts() {
    let db = build_db(
        "r226-m2-01",
        "parent(alice, bob). parent(bob, carol).",
    );
    let got = query_var("r226-m2-01", &db, "parent(alice, ?Y)", "Y");
    assert_values_eq("r226-m2-01", got, &[ident("bob")]);
}

#[test]
fn r226_m2_02_one_hop_rule_derives_ancestor() {
    let db = build_db(
        "r226-m2-02",
        "parent(alice, bob).\n\
         parent(bob, carol).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).",
    );
    let got = query_var("r226-m2-02", &db, "ancestor(alice, ?Y)", "Y");
    assert_values_eq("r226-m2-02", got, &[ident("bob")]);
}

#[test]
fn r226_m2_03_recursive_rule_full_ancestor_chain() {
    // Canonical fixture from the task prompt: two facts, two rules,
    // one query. `ancestor(alice, ?Z)` must return {bob, carol}.
    let db = build_db(
        "r226-m2-03",
        "parent(alice, bob).\n\
         parent(bob, carol).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let got = query_var("r226-m2-03", &db, "ancestor(alice, ?Z)", "Z");
    assert_values_eq("r226-m2-03", got, &[ident("bob"), ident("carol")]);
}

#[test]
fn r226_m2_04_deep_recursion_10_levels() {
    // Chain of 10 parent facts: n0 → n1 → n2 → … → n10.
    // ancestor(n0, ?X) must return n1 through n10 (10 rows).
    let mut src = String::new();
    for i in 0..10 {
        src.push_str(&format!("parent(n{}, n{}). ", i, i + 1));
    }
    src.push_str("ancestor(?X, ?Y) => parent(?X, ?Y). ");
    src.push_str("ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).");
    let db = build_db("r226-m2-04", &src);
    let got = query_var("r226-m2-04", &db, "ancestor(n0, ?X)", "X");
    let want: Vec<Value> = (1..=10).map(|i| ident(&format!("n{}", i))).collect();
    assert_values_eq("r226-m2-04", got, &want);
}

#[test]
fn r226_m2_05_empty_program_query_empty() {
    let program = Program::empty();
    let db = Database::from_program(&program).expect("r226-m2-05");
    let goal = Atom::new("anything", vec![Term::Var("X".to_owned())]);
    let q = Query { goals: vec![goal] };
    let results = query(&db, &q).expect("r226-m2-05");
    assert!(results.is_empty(), "r226-m2-05: got {results:#?}");
}

#[test]
fn r226_m2_06_conjunctive_query_two_atoms() {
    let db = build_db(
        "r226-m2-06",
        "man(socrates). mortal(socrates). mortal(plato). man(plato). man(cat).",
    );
    // ?X is a man AND is mortal — socrates and plato, not cat.
    let got = query_var("r226-m2-06", &db, "man(?X), mortal(?X)", "X");
    assert_values_eq("r226-m2-06", got, &[ident("plato"), ident("socrates")]);
}

#[test]
fn r226_m2_07_fixpoint_terminates_on_cycles() {
    // Cyclic parent-of: A→B, B→A. Full ancestor closure must include
    // (A,B), (B,A), (A,A), (B,B) — and terminate.
    let db = build_db(
        "r226-m2-07",
        "parent(a, b). parent(b, a).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let got = query_var("r226-m2-07", &db, "ancestor(a, ?Y)", "Y");
    assert_values_eq("r226-m2-07", got, &[ident("a"), ident("b")]);
}

#[test]
fn r226_m2_08_query_on_program_with_zero_rules_returns_edb() {
    // A program consisting entirely of ground facts must still be
    // queryable (no rules means fixpoint = EDB itself).
    let db = build_db("r226-m2-08", "color(red). color(green). color(blue).");
    let got = query_var("r226-m2-08", &db, "color(?C)", "C");
    assert_values_eq(
        "r226-m2-08",
        got,
        &[ident("blue"), ident("green"), ident("red")],
    );
}

#[test]
fn r226_m2_09_query_with_all_constant_terms_membership() {
    // Ground query: does the tuple exist? Result is one empty binding
    // if yes, zero bindings if no.
    let db = build_db("r226-m2-09", "likes(alice, bob).");
    let toks_yes = common::dlg_tokens("likes(alice, bob)");
    let toks_no = common::dlg_tokens("likes(alice, carol)");
    let yes = query(&db, &parse_query(&toks_yes).unwrap()).unwrap();
    let no = query(&db, &parse_query(&toks_no).unwrap()).unwrap();
    assert_eq!(yes.len(), 1, "r226-m2-09: yes must be 1 empty binding");
    assert!(yes[0].is_empty(), "r226-m2-09: binding must be empty");
    assert!(no.is_empty(), "r226-m2-09: no must be 0 bindings");
}

#[test]
fn r226_m2_10_bound_term_rejected_at_eval() {
    // `$sess` = pipeline interpolation, not landed yet — evaluator
    // must refuse rather than silently no-match.
    let src = "user(alice). user(bob). owner(?X) => user(?X), likes(?X, $sess).";
    let program = parse_block(&common::dlg_tokens(src)).expect("r226-m2-10: parse");
    let err = Database::from_program(&program).unwrap_err();
    assert!(
        matches!(err, EvalError::UnresolvedPipelineValue { .. }),
        "r226-m2-10: expected UnresolvedPipelineValue, got {err:?}"
    );
}
