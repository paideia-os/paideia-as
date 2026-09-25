//! R226.M1 parser fixture corpus (10 tests, tag `r226-m1-NN`).
//!
//! Each fixture drives the token stream through `parse_block` /
//! `parse_query` and asserts the shape of the resulting AST. Deep
//! evaluator tests live in `eval_fixtures.rs`.

mod common;

use common::dlg_tokens;
use paideia_as_shell_datalog::{
    parse_block, parse_query, Atom, ParseErrorKind, Program, Rule, Term, Value,
};

fn c_ident(s: &str) -> Term {
    Term::Const(Value::Ident(s.to_owned()))
}
fn c_num(n: i64) -> Term {
    Term::Const(Value::Num(n))
}
fn var(s: &str) -> Term {
    Term::Var(s.to_owned())
}

#[test]
fn r226_m1_01_empty_program_parses() {
    let program = parse_block(&dlg_tokens("")).expect("r226-m1-01: empty must parse");
    assert_eq!(program, Program::empty(), "r226-m1-01");
}

#[test]
fn r226_m1_02_single_ground_fact() {
    let program = parse_block(&dlg_tokens("parent(alice, bob).")).expect("r226-m1-02");
    assert_eq!(program.rules.len(), 0);
    assert_eq!(program.facts.len(), 1);
    assert_eq!(
        program.facts[0],
        Atom::new("parent", vec![c_ident("alice"), c_ident("bob")])
    );
}

#[test]
fn r226_m1_03_multiple_facts() {
    let program = parse_block(&dlg_tokens(
        "parent(alice, bob). parent(bob, carol). parent(carol, dave).",
    ))
    .expect("r226-m1-03");
    assert_eq!(program.rules.len(), 0);
    assert_eq!(program.facts.len(), 3);
}

#[test]
fn r226_m1_04_simple_rule() {
    let program = parse_block(&dlg_tokens(
        "ancestor(?X, ?Y) => parent(?X, ?Y).",
    ))
    .expect("r226-m1-04");
    assert_eq!(program.facts.len(), 0);
    assert_eq!(program.rules.len(), 1);
    let r: &Rule = &program.rules[0];
    assert_eq!(
        r.head,
        Atom::new("ancestor", vec![var("X"), var("Y")])
    );
    assert_eq!(r.body.len(), 1);
    assert_eq!(
        r.body[0],
        Atom::new("parent", vec![var("X"), var("Y")])
    );
}

#[test]
fn r226_m1_05_recursive_rule_with_multi_atom_body() {
    let program = parse_block(&dlg_tokens(
        "ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    ))
    .expect("r226-m1-05");
    let r = &program.rules[0];
    assert_eq!(r.body.len(), 2);
    assert_eq!(
        r.body[0],
        Atom::new("parent", vec![var("X"), var("Y")])
    );
    assert_eq!(
        r.body[1],
        Atom::new("ancestor", vec![var("Y"), var("Z")])
    );
}

#[test]
fn r226_m1_06_mixed_facts_and_rules() {
    let program = parse_block(&dlg_tokens(
        "parent(alice, bob).\n\
         parent(bob, carol).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    ))
    .expect("r226-m1-06");
    assert_eq!(program.facts.len(), 2);
    assert_eq!(program.rules.len(), 2);
}

#[test]
fn r226_m1_07_number_and_string_constants() {
    let program = parse_block(&dlg_tokens(
        "score(alice, 42). label(alice, \"admin\").",
    ))
    .expect("r226-m1-07");
    assert_eq!(program.facts.len(), 2);
    assert_eq!(program.facts[0].terms[1], c_num(42));
    assert!(matches!(
        program.facts[1].terms[1],
        Term::Const(Value::Str(ref s)) if s == "admin"
    ));
}

#[test]
fn r226_m1_08_non_ground_fact_rejected() {
    // A fact-shaped clause with a variable is a range-restriction
    // violation at M1: a fact must be ground.
    let err = parse_block(&dlg_tokens("parent(?X, bob).")).unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::NonGroundFact { .. }),
        "r226-m1-08: expected NonGroundFact, got {:?}",
        err.kind
    );
}

#[test]
fn r226_m1_09_zero_arity_atom_rejected() {
    let err = parse_block(&dlg_tokens("foo().")).unwrap_err();
    assert!(
        matches!(err.kind, ParseErrorKind::ZeroArityAtom { .. }),
        "r226-m1-09: expected ZeroArityAtom, got {:?}",
        err.kind
    );
}

#[test]
fn r226_m1_10_query_parses_with_var_and_const() {
    let q = parse_query(&dlg_tokens("ancestor(alice, ?Z)")).expect("r226-m1-10");
    assert_eq!(q.goals.len(), 1);
    assert_eq!(
        q.goals[0],
        Atom::new("ancestor", vec![c_ident("alice"), var("Z")])
    );
}
