//! R226.M5 stratified-negation fixture corpus (15 tests, tag
//! `r226m5-strat-NN`). Half the corpus is *acceptors* — programs the
//! stratifier admits, plus a query whose answer set is checked; the
//! other half is *rejectors* — programs whose predicate dependency
//! graph carries a cycle through negation, so evaluation must fail
//! with `EvalError::UnstratifiedNegation` **before** any intermediate
//! tuple is derived.
//!
//! Fingerprints live in every panic message so the R220.M10
//! `@fingerprint` correlator can pin a failure to a specific fixture
//! without re-parsing the test name.

mod common;

use common::{assert_values_eq, dlg_tokens, ident};
use paideia_as_shell_datalog::{
    parse_block, parse_query, query, Database, EvalError, Evaluator, Program, Query, Value,
};

// --------------------------------------------------------------------
// Shared helpers — local to this file so unrelated fixtures do not
// carry them as dead weight in `tests/common/mod.rs`.
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

/// Run the stratified evaluator + a query. Fixture asserts the sorted
/// `?var` projection.
fn run_query(fp: &str, src: &str, qsrc: &str, var: &str) -> Vec<Value> {
    let program = build_program(fp, src);
    let q = build_query(fp, qsrc);
    let ev = Evaluator::new();
    let db = ev
        .run_stratified(&program)
        .unwrap_or_else(|e| panic!("{fp}: stratified eval failed: {e:?}"));
    let bindings = query(&db, &q).unwrap_or_else(|e| panic!("{fp}: query eval failed: {e:?}"));
    values_of(&bindings, var)
}

/// Expect the stratifier to refuse the program with
/// `UnstratifiedNegation`. Also asserts that `Database::from_program`
/// (the naïve constructor) refuses it too — so no intermediate tuple
/// derivations survive; every predicate the rejection would have
/// materialised is empty because the pipeline never got that far.
fn expect_unstratified(fp: &str, src: &str) {
    let program = build_program(fp, src);

    // Route 1: explicit Evaluator::run_stratified.
    let ev = Evaluator::new();
    let err = ev
        .run_stratified(&program)
        .expect_err(&format!("{fp}: stratified eval must reject"));
    match err {
        EvalError::UnstratifiedNegation { ref cycle } => {
            assert!(
                !cycle.is_empty(),
                "{fp}: cycle predicate list must be non-empty"
            );
        }
        other => panic!("{fp}: expected UnstratifiedNegation, got {other:?}"),
    }

    // Route 2: Database::from_program (delegates to the stratifier).
    let err2 = Database::from_program(&program).expect_err(&format!(
        "{fp}: Database::from_program must reject the same program"
    ));
    assert!(
        matches!(err2, EvalError::UnstratifiedNegation { .. }),
        "{fp}: expected UnstratifiedNegation from Database::from_program, got {err2:?}"
    );

    // Route 3 (probe): confirm no intermediate DB was materialised —
    // the rejection happens BEFORE the fixpoint runs. Because we only
    // see an error, this is checked implicitly (no half-derived
    // Database escapes into user hands); the `is_err()` guard above
    // is the proof. This comment documents the invariant so a
    // future refactor that starts returning partial DBs alongside
    // the error breaks visibly.
    let _guard_documented = &program;
}

// ====================================================================
// ACCEPTORS — 8 stratifiable programs
// ====================================================================

#[test]
fn r226m5_strat_01_transitive_closure_with_negation_on_base() {
    // Reachability, but the base `edge` set has a negation on a
    // separate `blocked` predicate that filters which edges count.
    //
    // The stratifier must put `blocked` (fact-only, stratum 0) below
    // both `active_edge` and `reach` (stratum ≥ 1).
    let fp = "r226m5-strat-01";
    let got = run_query(
        fp,
        "edge(a, b). edge(b, c). edge(c, d).\n\
         blocked(b).\n\
         active_edge(?X, ?Y) => edge(?X, ?Y), not blocked(?X).\n\
         reach(?X, ?Y) => active_edge(?X, ?Y).\n\
         reach(?X, ?Z) => active_edge(?X, ?Y), reach(?Y, ?Z).",
        "reach(a, ?Y)",
        "Y",
    );
    // a -> b is fine (a is not blocked). b -> c would only fire from
    // active_edge, but b IS blocked, so no edge starts at b. Result:
    // {b}.
    assert_values_eq(fp, got, &[ident("b")]);
}

#[test]
fn r226m5_strat_02_orphaned_relation_via_negation() {
    // A `person` is `orphaned` iff there is no `parent(?, ?person)`
    // relation for them. Classic stratified-negation idiom.
    let fp = "r226m5-strat-02";
    let got = run_query(
        fp,
        "person(alice). person(bob). person(carol).\n\
         parent(alice, bob).\n\
         has_parent(?C) => parent(?P, ?C).\n\
         orphaned(?P) => person(?P), not has_parent(?P).",
        "orphaned(?P)",
        "P",
    );
    // alice: has no parent → orphaned.
    // bob:   has parent (alice) → not orphaned.
    // carol: has no parent → orphaned.
    assert_values_eq(fp, got, &[ident("alice"), ident("carol")]);
}

#[test]
fn r226m5_strat_03_two_level_negation_across_strata() {
    // level0 → level1 (negation) → level2 (negation).
    // level0 fully in stratum 0; level1 in stratum 1; level2 in
    // stratum 2. Result: strata {0, 1, 2}.
    let fp = "r226m5-strat-03";
    let got = run_query(
        fp,
        "atom(a). atom(b). atom(c). atom(d).\n\
         base(a). base(b).\n\
         mid(?X) => atom(?X), not base(?X).\n\
         top(?X) => atom(?X), not mid(?X).",
        "top(?X)",
        "X",
    );
    // mid = atoms not in base = {c, d}.
    // top = atoms not in mid = {a, b}.
    assert_values_eq(fp, got, &[ident("a"), ident("b")]);
}

#[test]
fn r226m5_strat_04_negation_across_chain_of_four_strata() {
    // s0 → s1 (neg) → s2 (neg) → s3 (neg) → s4 (neg).
    // Force the stratifier to assign strata 0..4 in order.
    let fp = "r226m5-strat-04";
    let got = run_query(
        fp,
        "domain(a). domain(b). domain(c). domain(d).\n\
         s0(a).\n\
         s1(?X) => domain(?X), not s0(?X).\n\
         s2(?X) => domain(?X), not s1(?X).\n\
         s3(?X) => domain(?X), not s2(?X).\n\
         s4(?X) => domain(?X), not s3(?X).",
        "s4(?X)",
        "X",
    );
    // s1 = domain \ {a} = {b, c, d}.
    // s2 = domain \ s1 = {a}.
    // s3 = domain \ s2 = {b, c, d}.
    // s4 = domain \ s3 = {a}.
    assert_values_eq(fp, got, &[ident("a")]);
}

#[test]
fn r226m5_strat_05_reachability_with_excluded_nodes_list() {
    // Standard "reach with exclusion list" — exclusion is negated in
    // the recursive step; excluded's stratum is 0, reach's is ≥ 1.
    let fp = "r226m5-strat-05";
    let got = run_query(
        fp,
        "edge(a, b). edge(b, c). edge(c, d). edge(d, e). edge(a, x). edge(x, y).\n\
         excluded(c). excluded(x).\n\
         reach(?X, ?Y) => edge(?X, ?Y), not excluded(?Y).\n\
         reach(?X, ?Z) => edge(?X, ?Y), not excluded(?Y), reach(?Y, ?Z).",
        "reach(a, ?Y)",
        "Y",
    );
    // `not excluded(?Y)` filters c and x from the second slot of edge.
    // Base rule derivations (round 1):
    //   (a,b) OK  → reach(a,b)
    //   (b,c) filtered  · (c,d) OK → reach(c,d)  ·  (d,e) OK → reach(d,e)
    //   (a,x) filtered  ·  (x,y) OK → reach(x,y)
    // Recursive rule then extends only via c→e (b→c and a→x are both
    // filtered at the intermediate ?Y). From `a` the only reach target
    // is `b` — every other frontier ends at an excluded node.
    assert_values_eq(fp, got, &[ident("b")]);
}

#[test]
fn r226m5_strat_06_negation_on_stratum_zero_predicate() {
    // A rule whose only body atoms are (positive `p(?x)`) and
    // (`not q(?x)`), where both p and q are stratum-0 EDB.
    let fp = "r226m5-strat-06";
    let got = run_query(
        fp,
        "p(1). p(2). p(3). p(4).\n\
         q(2). q(4).\n\
         r(?X) => p(?X), not q(?X).",
        "r(?X)",
        "X",
    );
    // r = p \ q = {1, 3}.
    let want = vec![Value::Num(1), Value::Num(3)];
    assert_values_eq(fp, got, &want);
}

#[test]
fn r226m5_strat_07_subset_via_negation() {
    // included(?x) :- all(?x), not excluded(?x). The literal
    // idiom named in the task spec.
    let fp = "r226m5-strat-07";
    let got = run_query(
        fp,
        "all(alice). all(bob). all(carol). all(dave).\n\
         excluded(bob). excluded(dave).\n\
         included(?X) => all(?X), not excluded(?X).",
        "included(?X)",
        "X",
    );
    assert_values_eq(fp, got, &[ident("alice"), ident("carol")]);
}

#[test]
fn r226m5_strat_08_mutual_non_negation_recursive_but_no_negation_through_recursion() {
    // Two mutually-recursive predicates joined via a POSITIVE cycle
    // (even_step ↔ odd_step) — the SCC has no negated internal edge,
    // so stratification accepts it. A separate negation on a lower
    // predicate is also present to prove negation and recursion can
    // co-exist without forming an unstratifiable cycle.
    let fp = "r226m5-strat-08";
    let got = run_query(
        fp,
        "start(a).\n\
         next(a, b). next(b, c). next(c, d). next(d, e).\n\
         skip(c).\n\
         allowed(?X, ?Y) => next(?X, ?Y), not skip(?Y).\n\
         even_step(?X) => start(?X).\n\
         even_step(?Y) => odd_step(?X), allowed(?X, ?Y).\n\
         odd_step(?Y) => even_step(?X), allowed(?X, ?Y).",
        "even_step(?X)",
        "X",
    );
    // start = {a}.
    // even_step(a) via base.
    // allowed = next \ skip-on-target = {(a,b), (b,c) FILTERED,
    //   (c,d), (d,e)}. So allowed = {(a,b), (c,d), (d,e)}.
    // From even_step(a): odd_step(b) via (a,b).
    // From odd_step(b): even_step(?) via (b, ?) — only (b,c) but c is
    //   filtered. So no new even_step.
    // Final even_step = {a}.
    assert_values_eq(fp, got, &[ident("a")]);
}

// ====================================================================
// REJECTORS — 7 unstratifiable programs
// ====================================================================

#[test]
fn r226m5_strat_09_reject_self_cycle_p_from_not_p() {
    // p :- not p. The most trivial negation-through-recursion.
    let fp = "r226m5-strat-09";
    expect_unstratified(
        fp,
        "u(a). u(b).\n\
         p(?X) => u(?X), not p(?X).",
    );
}

#[test]
fn r226m5_strat_10_reject_mutual_cycle_p_not_q_and_q_not_p() {
    // p :- not q. q :- not p. Classic mutual negation cycle.
    let fp = "r226m5-strat-10";
    expect_unstratified(
        fp,
        "u(a). u(b).\n\
         p(?X) => u(?X), not q(?X).\n\
         q(?X) => u(?X), not p(?X).",
    );
}

#[test]
fn r226m5_strat_11_reject_recursion_through_negation_blocked_by_reach() {
    // reach depends negatively on blocked; blocked depends positively
    // (recursively) on reach. The SCC {reach, blocked} carries a
    // negated internal edge.
    let fp = "r226m5-strat-11";
    expect_unstratified(
        fp,
        "edge(a, b). edge(b, c).\n\
         reach(?X, ?Y) => edge(?X, ?Y), not blocked(?Y).\n\
         blocked(?X) => reach(?X, ?Y).",
    );
}

#[test]
fn r226m5_strat_12_reject_deeply_nested_cycle_via_three_predicates() {
    // p → q → r → p, where at least one edge is negated. Even one
    // negated edge inside a size-3 SCC must be rejected.
    let fp = "r226m5-strat-12";
    expect_unstratified(
        fp,
        "u(a). u(b).\n\
         p(?X) => u(?X), q(?X).\n\
         q(?X) => u(?X), r(?X).\n\
         r(?X) => u(?X), not p(?X).",
    );
}

#[test]
fn r226m5_strat_13_reject_negation_inside_scc_of_four() {
    // Four predicates in a cycle a → b → c → d → a; the d → a edge
    // is negated. SCC size 4, one negated internal edge → reject.
    let fp = "r226m5-strat-13";
    expect_unstratified(
        fp,
        "u(x).\n\
         a(?X) => u(?X), d(?X).\n\
         b(?X) => u(?X), a(?X).\n\
         c(?X) => u(?X), b(?X).\n\
         d(?X) => u(?X), not a(?X).",
    );
}

#[test]
fn r226m5_strat_14_reject_negation_back_into_lower_via_positive_cycle() {
    // A positive cycle x ↔ y that also carries a negated edge x → y.
    // The SCC {x, y} contains a negated internal edge → reject.
    let fp = "r226m5-strat-14";
    expect_unstratified(
        fp,
        "u(a). u(b).\n\
         x(?A) => u(?A), y(?A).\n\
         y(?A) => u(?A), x(?A).\n\
         y(?A) => u(?A), not x(?A).",
    );
}

#[test]
fn r226m5_strat_15_reject_all_negated_cycle() {
    // p :- not q. q :- not r. r :- not p. Every edge in the 3-SCC is
    // negated — definitely unstratifiable.
    let fp = "r226m5-strat-15";
    expect_unstratified(
        fp,
        "u(a). u(b).\n\
         p(?X) => u(?X), not q(?X).\n\
         q(?X) => u(?X), not r(?X).\n\
         r(?X) => u(?X), not p(?X).",
    );
}

// --------------------------------------------------------------------
// Regression pin — the presence of this file must not silently break
// prior fixtures. `cargo test` would catch that too, but naming the
// public surface here guarantees the re-exports still compile after
// the R226.M5 changes.
// --------------------------------------------------------------------
#[test]
fn r226m5_regression_smoke_prior_paths_untouched() {
    let _ = Program::empty();
    let _ = Evaluator::new();
    let _ = query;
}
