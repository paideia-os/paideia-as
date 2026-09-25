//! R226.M9 query-time type check fixture corpus (15 tests, tag
//! `r226m9-tc-NN`).
//!
//! Eight accept fixtures pin the type-clean paths (constants
//! matching the sig, `ValueType::Any` wildcard, var/bound-typed
//! slots, multi-goal rules, session-independent programs). Seven
//! reject fixtures pin the diagnostic contract (unknown predicate,
//! arity mismatch surfacing as unknown, type mismatch on each of
//! the three concrete slots, query-side atom rejection, multi-error
//! collection, exact position/expected/got on a mismatch).
//!
//! Each fixture's assert messages carry its fingerprint so the
//! R220.M10 `@fingerprint` correlator can attribute a regression to
//! a single fixture without re-parsing the test name.

mod common;

use common::{dlg_tokens, ident};
use paideia_as_shell_datalog::{
    parse_block, parse_query, Evaluator, EvalError, PredicateSignature, Program, Query,
    SchemaRegistry, TypeCheckError, Value, ValueType,
};

// --------------------------------------------------------------------
// Shared helpers — local to this file.
// --------------------------------------------------------------------

fn build_program(fp: &str, src: &str) -> Program {
    let tokens = dlg_tokens(src);
    parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse failed: {e:?}"))
}

fn build_query(fp: &str, src: &str) -> Query {
    let tokens = dlg_tokens(src);
    parse_query(&tokens).unwrap_or_else(|e| panic!("{fp}: query parse failed: {e:?}"))
}

fn sig(pred: &str, arg_types: Vec<ValueType>) -> PredicateSignature {
    PredicateSignature::new(pred, arg_types.len(), arg_types)
}

/// Assert `Ok(bindings)`; carry `fp` in the panic message.
fn expect_ok(
    fp: &str,
    r: Result<Vec<paideia_as_shell_datalog::Binding>, EvalError>,
) -> Vec<paideia_as_shell_datalog::Binding> {
    r.unwrap_or_else(|e| panic!("{fp}: expected Ok, got Err({e:?})"))
}

/// Assert `Err(EvalError::TypeCheckErrors(errors))`; return the
/// error vector.
fn expect_type_errors(
    fp: &str,
    r: Result<Vec<paideia_as_shell_datalog::Binding>, EvalError>,
) -> Vec<TypeCheckError> {
    match r {
        Err(EvalError::TypeCheckErrors(v)) => v,
        Err(other) => panic!("{fp}: expected TypeCheckErrors, got Err({other:?})"),
        Ok(_) => panic!("{fp}: expected TypeCheckErrors, got Ok"),
    }
}

// ====================================================================
// 01 — accept: registered predicate + matching-type facts
// ====================================================================

#[test]
fn r226m9_tc_01_accept_registered_facts() {
    let fp = "r226m9-tc-01";
    let program = build_program(fp, "parent(alice, bob).");
    let query = build_query(fp, "parent(alice, ?Y)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("parent", vec![ValueType::Ident, ValueType::Ident]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1, "{fp}: expected exactly one binding");
    assert_eq!(bindings[0].get("Y"), Some(&ident("bob")), "{fp}: Y binding");
}

// ====================================================================
// 02 — accept: rule with all Any-typed args
// ====================================================================

#[test]
fn r226m9_tc_02_accept_any_types_pass_every_constant() {
    let fp = "r226m9-tc-02";
    // Fact carries a Num and a Str — both accepted because sig says Any/Any.
    let program = build_program(fp, "wild(42, \"hi\").");
    let query = build_query(fp, "wild(?X, ?Y)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("wild", vec![ValueType::Any, ValueType::Any]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1, "{fp}: expected one binding");
    assert_eq!(bindings[0].get("X"), Some(&Value::Num(42)), "{fp}: X binding");
    assert_eq!(
        bindings[0].get("Y"),
        Some(&Value::Str("hi".into())),
        "{fp}: Y binding"
    );
}

// ====================================================================
// 03 — accept: mixed Any + specific types
// ====================================================================

#[test]
fn r226m9_tc_03_accept_mixed_any_and_concrete() {
    let fp = "r226m9-tc-03";
    // sig: mix(Ident, Any, Num) — position 1 wildcard, others exact.
    let program = build_program(fp, "mix(alice, \"whatever\", 7).");
    let query = build_query(fp, "mix(?X, ?Y, ?Z)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig(
        "mix",
        vec![ValueType::Ident, ValueType::Any, ValueType::Num],
    ));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1, "{fp}: expected one binding");
    assert_eq!(
        bindings[0].get("Y"),
        Some(&Value::Str("whatever".into())),
        "{fp}: Any slot bound to Str"
    );
}

// ====================================================================
// 04 — accept: rule with 2 positive body goals, both registered
// ====================================================================

#[test]
fn r226m9_tc_04_accept_rule_with_two_body_goals() {
    let fp = "r226m9-tc-04";
    // grandparent from parent — three atoms all need registered sigs.
    let program = build_program(
        fp,
        "parent(alice, bob).\n\
         parent(bob, carol).\n\
         grandparent(?X, ?Z) => parent(?X, ?Y), parent(?Y, ?Z).",
    );
    let query = build_query(fp, "grandparent(?X, ?Z)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("parent", vec![ValueType::Ident, ValueType::Ident]));
    reg.register(sig("grandparent", vec![ValueType::Ident, ValueType::Ident]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1, "{fp}: expected one grandparent");
    assert_eq!(bindings[0].get("X"), Some(&ident("alice")));
    assert_eq!(bindings[0].get("Z"), Some(&ident("carol")));
}

// ====================================================================
// 05 — accept: fact with Ident where Ident expected
// ====================================================================

#[test]
fn r226m9_tc_05_accept_ident_where_ident_expected() {
    let fp = "r226m9-tc-05";
    let program = build_program(fp, "colour(red).");
    let query = build_query(fp, "colour(?C)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("colour", vec![ValueType::Ident]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].get("C"), Some(&ident("red")), "{fp}: C binding");
}

// ====================================================================
// 06 — accept: fact with Str where Str expected
// ====================================================================

#[test]
fn r226m9_tc_06_accept_str_where_str_expected() {
    let fp = "r226m9-tc-06";
    let program = build_program(fp, "label(\"greeting\").");
    let query = build_query(fp, "label(?L)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("label", vec![ValueType::Str]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1);
    assert_eq!(
        bindings[0].get("L"),
        Some(&Value::Str("greeting".into())),
        "{fp}: L binding"
    );
}

// ====================================================================
// 07 — accept: fact with Num where Num expected
// ====================================================================

#[test]
fn r226m9_tc_07_accept_num_where_num_expected() {
    let fp = "r226m9-tc-07";
    let program = build_program(fp, "age(42).");
    let query = build_query(fp, "age(?N)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("age", vec![ValueType::Num]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].get("N"), Some(&Value::Num(42)), "{fp}: N binding");
}

// ====================================================================
// 08 — accept: signature registered after program construction still validates
// ====================================================================

#[test]
fn r226m9_tc_08_accept_late_registration_still_binds_at_check_time() {
    let fp = "r226m9-tc-08";
    // Build the program first; register the sig only afterwards. The
    // check reads the registry as of the call, so a late registration
    // is honoured — no compile-time freeze.
    let program = build_program(fp, "kind(species, dog).");
    let query = build_query(fp, "kind(?K, ?V)");
    let mut reg = SchemaRegistry::new();
    // Interleave: register some noise first, then the real sig.
    reg.register(sig("noise", vec![ValueType::Any]));
    reg.register(sig("kind", vec![ValueType::Ident, ValueType::Ident]));
    let ev = Evaluator::new();
    let bindings = expect_ok(fp, ev.run_query_typed(&program, &query, &reg));
    assert_eq!(bindings.len(), 1, "{fp}: expected one binding");
    assert_eq!(bindings[0].get("K"), Some(&ident("species")));
    assert_eq!(bindings[0].get("V"), Some(&ident("dog")));
}

// ====================================================================
// 09 — reject: unknown predicate
// ====================================================================

#[test]
fn r226m9_tc_09_reject_unknown_predicate() {
    let fp = "r226m9-tc-09";
    let program = build_program(fp, "orphan(alice).");
    let query = build_query(fp, "orphan(?X)");
    let reg = SchemaRegistry::new(); // empty on purpose
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    // Program-side + query-side both raise UnknownPredicate.
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::UnknownPredicate { predicate, arity } if predicate == "orphan" && *arity == 1)),
        "{fp}: expected UnknownPredicate for orphan/1, got {errors:?}",
    );
}

// ====================================================================
// 10 — reject: arity mismatch at atom level (registry keys on
//               (name, arity), so this surfaces as UnknownPredicate
//               on the mismatched-arity key)
// ====================================================================

#[test]
fn r226m9_tc_10_reject_arity_mismatch_surfaces_as_unknown_predicate() {
    let fp = "r226m9-tc-10";
    let program = build_program(fp, "p(a, b, c).");
    let query = build_query(fp, "p(?X, ?Y, ?Z)");
    let mut reg = SchemaRegistry::new();
    // Registered arity is 2; program uses arity 3.
    reg.register(sig("p", vec![ValueType::Ident, ValueType::Ident]));
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::UnknownPredicate { predicate, arity } if predicate == "p" && *arity == 3)),
        "{fp}: expected UnknownPredicate for p/3, got {errors:?}",
    );
}

// ====================================================================
// 11 — reject: type mismatch (Num where Ident expected)
// ====================================================================

#[test]
fn r226m9_tc_11_reject_num_where_ident_expected() {
    let fp = "r226m9-tc-11";
    let program = build_program(fp, "person(42).");
    let query = build_query(fp, "person(?P)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("person", vec![ValueType::Ident]));
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    let hits: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            TypeCheckError::TypeMismatch {
                predicate,
                position,
                expected,
                got,
            } if predicate == "person" => Some((*position, *expected, *got)),
            _ => None,
        })
        .collect();
    assert!(
        hits.contains(&(0usize, ValueType::Ident, ValueType::Num)),
        "{fp}: expected TypeMismatch(person, 0, Ident, Num), got {errors:?}",
    );
}

// ====================================================================
// 12 — reject: type mismatch (Str where Num expected)
// ====================================================================

#[test]
fn r226m9_tc_12_reject_str_where_num_expected() {
    let fp = "r226m9-tc-12";
    let program = build_program(fp, "count(\"twelve\").");
    let query = build_query(fp, "count(?N)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("count", vec![ValueType::Num]));
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    let hits: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            TypeCheckError::TypeMismatch {
                predicate,
                position,
                expected,
                got,
            } if predicate == "count" => Some((*position, *expected, *got)),
            _ => None,
        })
        .collect();
    assert!(
        hits.contains(&(0usize, ValueType::Num, ValueType::Str)),
        "{fp}: expected TypeMismatch(count, 0, Num, Str), got {errors:?}",
    );
}

// ====================================================================
// 13 — reject: query goal with unknown predicate → error before fixpoint
// ====================================================================

#[test]
fn r226m9_tc_13_reject_query_side_unknown_predicate() {
    let fp = "r226m9-tc-13";
    // Program is type-clean. The query mentions an unregistered
    // predicate — the check must catch it *before* the fixpoint runs.
    let program = build_program(fp, "known(alice).");
    let query = build_query(fp, "unknown(?X)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("known", vec![ValueType::Ident]));
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::UnknownPredicate { predicate, arity } if predicate == "unknown" && *arity == 1)),
        "{fp}: expected query-side UnknownPredicate for unknown/1, got {errors:?}",
    );
}

// ====================================================================
// 14 — reject: multi-error collection (ill-typed program surfaces
//               multiple diagnostics in one batch)
// ====================================================================

#[test]
fn r226m9_tc_14_reject_multi_error_collection() {
    let fp = "r226m9-tc-14";
    // Three distinct mismatches all in one program+query.
    let program = build_program(
        fp,
        "p(42).\n\
         q(\"hello\").\n\
         r(a).",
    );
    let query = build_query(fp, "s(?X)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig("p", vec![ValueType::Ident])); // 42 vs Ident → TypeMismatch
    reg.register(sig("q", vec![ValueType::Num]));    // "hello" vs Num → TypeMismatch
    // r/1 unregistered → UnknownPredicate.
    // s/1 unregistered → UnknownPredicate (query side).
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    assert!(
        errors.len() >= 4,
        "{fp}: expected at least 4 collected errors (2 TypeMismatch + 2 UnknownPredicate), got {}: {errors:?}",
        errors.len(),
    );
    // Spot-check each expected shape is present.
    assert!(
        errors.iter().any(|e| matches!(e, TypeCheckError::TypeMismatch { predicate, .. } if predicate == "p")),
        "{fp}: missing TypeMismatch on p",
    );
    assert!(
        errors.iter().any(|e| matches!(e, TypeCheckError::TypeMismatch { predicate, .. } if predicate == "q")),
        "{fp}: missing TypeMismatch on q",
    );
    assert!(
        errors.iter().any(|e| matches!(e, TypeCheckError::UnknownPredicate { predicate, .. } if predicate == "r")),
        "{fp}: missing UnknownPredicate for r",
    );
    assert!(
        errors.iter().any(|e| matches!(e, TypeCheckError::UnknownPredicate { predicate, .. } if predicate == "s")),
        "{fp}: missing query-side UnknownPredicate for s",
    );
}

// ====================================================================
// 15 — reject: TypeMismatch position is accurate
//               (p(a, 42, c) with sig arg_types=[Ident, Ident, Ident]
//                → error position=1, expected=Ident, got=Num)
// ====================================================================

#[test]
fn r226m9_tc_15_reject_position_and_types_exact() {
    let fp = "r226m9-tc-15";
    let program = build_program(fp, "p(a, 42, c).");
    let query = build_query(fp, "p(?X, ?Y, ?Z)");
    let mut reg = SchemaRegistry::new();
    reg.register(sig(
        "p",
        vec![ValueType::Ident, ValueType::Ident, ValueType::Ident],
    ));
    let ev = Evaluator::new();
    let errors = expect_type_errors(fp, ev.run_query_typed(&program, &query, &reg));
    // Exactly one TypeMismatch, on position 1, expecting Ident, got Num.
    let mismatches: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            TypeCheckError::TypeMismatch {
                predicate,
                position,
                expected,
                got,
            } => Some((predicate.clone(), *position, *expected, *got)),
            _ => None,
        })
        .collect();
    assert_eq!(
        mismatches.len(),
        1,
        "{fp}: expected exactly one TypeMismatch, got {mismatches:?}",
    );
    let (predicate, position, expected, got) = &mismatches[0];
    assert_eq!(predicate, "p", "{fp}: predicate name");
    assert_eq!(*position, 1usize, "{fp}: mismatch position");
    assert_eq!(*expected, ValueType::Ident, "{fp}: expected type");
    assert_eq!(*got, ValueType::Num, "{fp}: got type");
}
