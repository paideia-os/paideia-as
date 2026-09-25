//! R226.M4 magic-set rewriting fixture corpus (20 tests, tag
//! `r226m4-magic-NN`). Every fixture builds a program via the M1
//! parser, calls [`paideia_as_shell_datalog::magic_sets::rewrite`]
//! (directly or through the [`Evaluator::run_query_via_magic_sets`]
//! façade), and asserts one of two properties:
//!
//! * **Answer parity** — the magic-set path returns the same set of
//!   bindings the naïve seminaïve path returns. Correctness is
//!   Beeri-Ramakrishnan §4.3's headline claim; the corpus pins it on
//!   transitive closure, same-generation, cycles, deep recursion,
//!   and mixed-adornment queries.
//! * **Selectivity** — the magic-set fixpoint's DB is strictly
//!   smaller than the naïve fixpoint's DB. Two fixtures assert this
//!   numerically (`r226m4-magic-04` and `r226m4-magic-05`) so a
//!   regression that silently reverts to naïve evaluation still
//!   fails a test.
//!
//! Fingerprints live in the panic messages so the R220.M10
//! `@fingerprint` correlator can attribute a failure to a specific
//! fixture without re-parsing the test name.

mod common;

use common::{assert_values_eq, dlg_tokens, ident};
use paideia_as_shell_datalog::{
    magic_sets, parse_block, parse_query, query, Atom, Database, Evaluator, Program, Query,
    Term, Value,
};

// --------------------------------------------------------------------
// Shared helpers (kept local — they are magic-set specific and would
// be dead weight in tests/common/mod.rs).
// --------------------------------------------------------------------

/// Build a `Program` from source, panicking on parse failure. Fixtures
/// use this for both the naïve baseline and the magic-set rewrite —
/// keeping the parse step common ensures both paths see the same AST.
fn build_program(fingerprint: &str, src: &str) -> Program {
    let tokens = dlg_tokens(src);
    parse_block(&tokens).unwrap_or_else(|e| panic!("{fingerprint}: parse failed: {e:?}"))
}

/// Parse a query in the M1 grammar. Panics on parse failure.
fn build_query(fingerprint: &str, src: &str) -> Query {
    let tokens = dlg_tokens(src);
    parse_query(&tokens)
        .unwrap_or_else(|e| panic!("{fingerprint}: query parse failed: {e:?}"))
}

/// Extract the sorted list of values a single variable takes across
/// a binding vector. Sort is by `Display` string to give a stable
/// order regardless of `HashMap` iteration.
fn values_of(bindings: &[paideia_as_shell_datalog::Binding], var: &str) -> Vec<Value> {
    let mut vs: Vec<Value> = bindings.iter().filter_map(|b| b.get(var).cloned()).collect();
    vs.sort_by(|a, b| format!("{a}").cmp(&format!("{b}")));
    vs.dedup();
    vs
}

/// Count every tuple across every predicate in `db`. Used by the
/// selectivity fixtures as a coarse proxy for "intermediate work"
/// — a magic-set rewrite must produce a strictly smaller number
/// than the naïve baseline.
fn count_tuples(db: &Database) -> usize {
    db.total_tuple_count()
}

/// Compare bindings by their `?var` projection. Used when parity
/// fixtures need to check magic-set == naïve.
fn assert_bindings_match_var(
    fingerprint: &str,
    magic_bindings: &[paideia_as_shell_datalog::Binding],
    naive_bindings: &[paideia_as_shell_datalog::Binding],
    var: &str,
) {
    let m = values_of(magic_bindings, var);
    let n = values_of(naive_bindings, var);
    if m != n {
        panic!(
            "{fingerprint}: magic-set answer differs from naïve: \
             magic={m:?} naive={n:?}"
        );
    }
}

// --------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------

#[test]
fn r226m4_magic_01_ancestor_bf_returns_correct_chain() {
    // Canonical BR91 example: two facts + two ancestor rules, one
    // half-bound query. Magic-set answer must equal the ground truth
    // {bob, carol}.
    let fp = "r226m4-magic-01";
    let program = build_program(
        fp,
        "parent(alice, bob).\n\
         parent(bob, carol).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let q = build_query(fp, "ancestor(alice, ?Z)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    assert_values_eq(fp, values_of(&magic, "Z"), &[ident("bob"), ident("carol")]);
}

#[test]
fn r226m4_magic_02_same_generation_bf() {
    // SG (same-generation) — the second staple magic-set benchmark
    // in the literature. All descendants two levels down from `alice`
    // are `alice`'s "same generation" via the parent tree.
    //
    // par(a, b) reads "a is a child of b" here. We assert the
    // reflexive base plus one recursion level.
    let fp = "r226m4-magic-02";
    let program = build_program(
        fp,
        "person(alice). person(bob). person(carol). person(dave).\n\
         person(x). person(y).\n\
         par(alice, x). par(bob, x). par(carol, y). par(dave, y).\n\
         sg(?X, ?X) => person(?X).\n\
         sg(?X, ?Y) => par(?X, ?X1), sg(?X1, ?Y1), par(?Y, ?Y1).",
    );
    let q = build_query(fp, "sg(alice, ?Y)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    // Parity assertion — SG's answer set is nontrivial to enumerate
    // by hand; delegate the ground truth to the naïve path.
    assert_bindings_match_var(fp, &magic, &naive, "Y");
    // Must contain alice (reflexive) and bob (sibling via par-x).
    let ys = values_of(&magic, "Y");
    assert!(ys.contains(&ident("alice")), "{fp}: expected alice in {ys:?}");
    assert!(ys.contains(&ident("bob")), "{fp}: expected bob in {ys:?}");
}

#[test]
fn r226m4_magic_03_ground_fact_query_no_rules() {
    // Pure EDB (no rules). Magic-set rewriting must be a no-op
    // beyond adding a harmless magic seed; the query must still
    // return the seeded fact.
    let fp = "r226m4-magic-03";
    let program = build_program(fp, "color(red). color(green). color(blue).");
    let q = build_query(fp, "color(?C)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    assert_values_eq(
        fp,
        values_of(&magic, "C"),
        &[ident("blue"), ident("green"), ident("red")],
    );
}

#[test]
fn r226m4_magic_04_transitive_closure_speedup() {
    // A short reachable chain n0..n5 (5 edges) plus a long unrelated
    // chain u0..u20 (20 edges). Naïve computes both closures (15
    // reachable + 210 unrelated = 225 ancestor tuples). Magic-set
    // only walks the n-chain (15 ancestors + 6 magic tuples).
    //
    // Left-linear `ancestor(X, Z) :- parent(X, Y), ancestor(Y, Z)`
    // means magic-set still derives the full closure *within* the
    // reachable slice — the payoff is skipping the unrelated slice
    // entirely, which is why the unrelated chain is much longer than
    // the reachable one.
    let fp = "r226m4-magic-04";
    let mut src = String::new();
    for i in 0..5 {
        src.push_str(&format!("parent(n{}, n{}). ", i, i + 1));
    }
    for i in 0..20 {
        src.push_str(&format!("parent(u{}, u{}). ", i, i + 1));
    }
    src.push_str("ancestor(?X, ?Y) => parent(?X, ?Y). ");
    src.push_str("ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).");
    let program = build_program(fp, &src);
    let q = build_query(fp, "ancestor(n0, ?Y)");
    let ev = Evaluator::new();

    // Answer parity first — a "speedup" that returns wrong answers
    // is worse than none.
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "Y");
    assert_eq!(magic.len(), 5, "{fp}: expected 5 chain ancestors from n0");

    // Now selectivity.
    let naive_db = Database::from_program(&program).expect(fp);
    let rewritten = magic_sets::rewrite(&program, &q);
    let magic_db = Database::from_program(&rewritten).expect(fp);
    let naive_count = count_tuples(&naive_db);
    let magic_count = count_tuples(&magic_db);
    assert!(
        magic_count < naive_count,
        "{fp}: magic DB should be strictly smaller: magic={magic_count} naive={naive_count}"
    );
}

#[test]
fn r226m4_magic_05_reach_speedup_disjoint_components() {
    // Two disjoint graph components. Component A is short and
    // reachable from the query root; component B is a long
    // unrelated chain that naïve evaluation walks in full but
    // magic-set completely skips.
    //
    // Component A: root → n1 → n2 → n3 → n4 (4 edges).
    // Component B: m0 → m1 → … → m8 (8 edges).
    let fp = "r226m4-magic-05";
    let mut src = String::new();
    src.push_str("edge(root, n1). edge(n1, n2). edge(n2, n3). edge(n3, n4). ");
    for i in 0..8 {
        src.push_str(&format!("edge(m{}, m{}). ", i, i + 1));
    }
    src.push_str("reach(?X, ?Y) => edge(?X, ?Y). ");
    src.push_str("reach(?X, ?Z) => edge(?X, ?Y), reach(?Y, ?Z).");
    let program = build_program(fp, &src);
    let q = build_query(fp, "reach(root, ?Y)");
    let ev = Evaluator::new();

    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "Y");
    // From root we reach {n1, n2, n3, n4} — 4 answers.
    assert_eq!(magic.len(), 4, "{fp}: expected 4 reachable nodes from root");

    let naive_db = Database::from_program(&program).expect(fp);
    let rewritten = magic_sets::rewrite(&program, &q);
    let magic_db = Database::from_program(&rewritten).expect(fp);
    let naive_count = count_tuples(&naive_db);
    let magic_count = count_tuples(&magic_db);
    assert!(
        magic_count < naive_count,
        "{fp}: magic DB must be smaller: magic={magic_count} naive={naive_count}"
    );
}

#[test]
fn r226m4_magic_06_mixed_bf_adornment_first_position_bound() {
    // Classic bf: bound left position, free right position. Directly
    // tests the SIP that Beeri-Ramakrishnan §4.3 uses as the
    // canonical example. Parity with naïve is the correctness pin.
    let fp = "r226m4-magic-06";
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c). edge(c, d). edge(a, x). edge(x, y).\n\
         path(?U, ?V) => edge(?U, ?V).\n\
         path(?U, ?W) => edge(?U, ?V), path(?V, ?W).",
    );
    let q = build_query(fp, "path(a, ?W)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "W");
    // From a, we reach {b, c, d, x, y} — 5 answers.
    assert_values_eq(
        fp,
        values_of(&magic, "W"),
        &[ident("b"), ident("c"), ident("d"), ident("x"), ident("y")],
    );
}

#[test]
fn r226m4_magic_07_mixed_fb_adornment_second_position_bound() {
    // The mirror-image adornment: right position bound, left free.
    // Requires the left-to-right SIP to notice that the rule
    // `path(U, W) :- edge(U, V), path(V, W)` binds W from the
    // recursive call *at the tail*, not from the head.
    let fp = "r226m4-magic-07";
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c). edge(c, d). edge(a, x). edge(x, y).\n\
         path(?U, ?V) => edge(?U, ?V).\n\
         path(?U, ?W) => edge(?U, ?V), path(?V, ?W).",
    );
    let q = build_query(fp, "path(?U, d)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    // Parity — this is the third of the ≥3 parity fixtures.
    assert_bindings_match_var(fp, &magic, &naive, "U");
    // Everything that reaches d: {a, b, c}.
    assert_values_eq(
        fp,
        values_of(&magic, "U"),
        &[ident("a"), ident("b"), ident("c")],
    );
}

#[test]
fn r226m4_magic_08_all_bound_membership_query() {
    // A ground query — every argument is a constant. Magic-set
    // adornment is "bbbb…" and the answer is a boolean (one empty
    // binding for yes, zero bindings for no).
    let fp = "r226m4-magic-08";
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c).\n\
         path(?U, ?V) => edge(?U, ?V).\n\
         path(?U, ?W) => edge(?U, ?V), path(?V, ?W).",
    );
    let ev = Evaluator::new();

    let yes = build_query(fp, "path(a, c)");
    let no = build_query(fp, "path(c, a)");
    let m_yes = ev.run_query_via_magic_sets(&program, &yes).expect(fp);
    let m_no = ev.run_query_via_magic_sets(&program, &no).expect(fp);
    assert_eq!(m_yes.len(), 1, "{fp}: yes expected one empty binding");
    assert!(m_yes[0].is_empty(), "{fp}: binding must be empty");
    assert!(m_no.is_empty(), "{fp}: no expected zero bindings");
}

#[test]
fn r226m4_magic_09_all_free_query_returns_full_relation() {
    // An all-free query hands magic sets no restriction — the
    // rewrite is documented as a no-op. Result must match the naïve
    // path exactly.
    let fp = "r226m4-magic-09";
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c). edge(c, d).\n\
         path(?U, ?V) => edge(?U, ?V).\n\
         path(?U, ?W) => edge(?U, ?V), path(?V, ?W).",
    );
    let q = build_query(fp, "path(?U, ?V)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_eq!(
        magic.len(),
        naive.len(),
        "{fp}: all-free query must return the full relation"
    );

    // Explicitly assert the rewriter is a no-op on all-free.
    let rewritten = magic_sets::rewrite(&program, &q);
    assert_eq!(
        rewritten, program,
        "{fp}: rewrite of all-free query should equal input"
    );
}

#[test]
fn r226m4_magic_10_cyclic_graph_terminates_with_correct_answer() {
    // Two-node cycle a↔b. The naïve fixpoint terminates because the
    // Herbrand base is finite; the magic-set fixpoint must too, and
    // must return the same answer.
    let fp = "r226m4-magic-10";
    let program = build_program(
        fp,
        "parent(a, b). parent(b, a).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let q = build_query(fp, "ancestor(a, ?Y)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "Y");
    assert_values_eq(fp, values_of(&magic, "Y"), &[ident("a"), ident("b")]);
}

#[test]
fn r226m4_magic_11_deep_recursion_20_levels() {
    // 20-level chain — enough for a naïve implementation to look
    // wildly different from a magic-set one, but the answer must
    // still be identical.
    let fp = "r226m4-magic-11";
    let mut src = String::new();
    for i in 0..20 {
        src.push_str(&format!("parent(n{}, n{}). ", i, i + 1));
    }
    src.push_str("ancestor(?X, ?Y) => parent(?X, ?Y). ");
    src.push_str("ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).");
    let program = build_program(fp, &src);
    let q = build_query(fp, "ancestor(n0, ?Y)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let want: Vec<Value> = (1..=20).map(|i| ident(&format!("n{}", i))).collect();
    assert_values_eq(fp, values_of(&magic, "Y"), &want);
}

#[test]
fn r226m4_magic_12_empty_answer_when_query_has_no_witness() {
    // Query that no derivation supports must return zero bindings,
    // even when the underlying relation is well-populated.
    let fp = "r226m4-magic-12";
    let program = build_program(
        fp,
        "parent(alice, bob). parent(bob, carol).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    // `zeb` is not in the DB at all.
    let q = build_query(fp, "ancestor(zeb, ?Y)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    assert!(magic.is_empty(), "{fp}: expected zero answers, got {magic:?}");
}

#[test]
fn r226m4_magic_13_parity_with_naive_on_branching_tree() {
    // Branching parent structure — every node under `alice` is a
    // descendant. Parity fixture #4 (of the "≥3" the task requires),
    // covering a non-linear graph shape.
    let fp = "r226m4-magic-13";
    let program = build_program(
        fp,
        "parent(alice, bob).\n\
         parent(alice, carol).\n\
         parent(bob, dave).\n\
         parent(bob, eve).\n\
         parent(carol, frank).\n\
         parent(dave, greg).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let q = build_query(fp, "ancestor(alice, ?Y)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "Y");
    assert_values_eq(
        fp,
        values_of(&magic, "Y"),
        &[
            ident("bob"),
            ident("carol"),
            ident("dave"),
            ident("eve"),
            ident("frank"),
            ident("greg"),
        ],
    );
}

#[test]
fn r226m4_magic_14_multiple_recursive_body_atoms() {
    // A rule with the recursive predicate appearing *twice* in the
    // body (`sg`-like shape). Exercises the SIP propagation into
    // both recursive calls.
    let fp = "r226m4-magic-14";
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c). edge(c, d).\n\
         edge(d, e). edge(e, f).\n\
         path(?X, ?Y) => edge(?X, ?Y).\n\
         path(?X, ?Z) => path(?X, ?Y), path(?Y, ?Z).",
    );
    let q = build_query(fp, "path(a, ?Z)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "Z");
    // From a we reach {b, c, d, e, f}.
    assert_values_eq(
        fp,
        values_of(&magic, "Z"),
        &[ident("b"), ident("c"), ident("d"), ident("e"), ident("f")],
    );
}

#[test]
fn r226m4_magic_15_edb_and_idb_mixed_in_body() {
    // Body mixes an EDB `edge` and an IDB `path`. The rewriter must
    // emit supplementary magic for the IDB atom only, leaving the
    // EDB atom untouched.
    let fp = "r226m4-magic-15";
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c). edge(c, d).\n\
         path(?X, ?Y) => edge(?X, ?Y).\n\
         path(?X, ?Z) => edge(?X, ?Y), path(?Y, ?Z).",
    );
    let q = build_query(fp, "path(a, ?Z)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "Z");

    // Verify the rewritten program actually contains a magic
    // supplementary rule for `path` and NOT for `edge`.
    let rewritten = magic_sets::rewrite(&program, &q);
    let has_magic_path = rewritten
        .rules
        .iter()
        .any(|r| r.head.predicate == "magic_path_bf");
    let has_magic_edge = rewritten
        .rules
        .iter()
        .any(|r| r.head.predicate.starts_with("magic_edge"));
    assert!(has_magic_path, "{fp}: expected magic_path_bf rule");
    assert!(
        !has_magic_edge,
        "{fp}: EDB predicate `edge` must not get a magic rule"
    );
}

#[test]
fn r226m4_magic_16_adornment_bf_string_present_in_rewrite() {
    // Structural check: the rewritten program must contain a
    // predicate literally named `magic_ancestor_bf` (the seed) with
    // the query constant as its single argument.
    let fp = "r226m4-magic-16";
    let program = build_program(
        fp,
        "parent(alice, bob).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let q = build_query(fp, "ancestor(alice, ?Y)");
    let rewritten = magic_sets::rewrite(&program, &q);
    let seed = rewritten
        .facts
        .iter()
        .find(|a| a.predicate == "magic_ancestor_bf")
        .unwrap_or_else(|| panic!("{fp}: seed fact magic_ancestor_bf missing"));
    assert_eq!(seed.terms.len(), 1, "{fp}: seed arity 1");
    assert_eq!(
        seed.terms[0],
        Term::Const(Value::Ident("alice".to_owned())),
        "{fp}: seed argument should be `alice`"
    );
}

#[test]
fn r226m4_magic_17_edb_only_program_survives_rewrite() {
    // A program with zero rules — only facts. The rewriter must
    // preserve every fact (returning them unmodified plus the seed).
    let fp = "r226m4-magic-17";
    let program = build_program(fp, "color(red). color(green). color(blue).");
    let q = build_query(fp, "color(red)");
    let rewritten = magic_sets::rewrite(&program, &q);
    // Every original fact must survive.
    for fact in &program.facts {
        assert!(
            rewritten.facts.contains(fact),
            "{fp}: fact {fact:?} must survive rewrite"
        );
    }
    // The magic seed for a ground query is redundant but must not
    // crash: it is added as-is.
    assert!(
        rewritten.facts.iter().any(|a| a.predicate == "magic_color_b"),
        "{fp}: expected magic_color_b seed"
    );
}

#[test]
fn r226m4_magic_18_query_predicate_only_edb_no_rules_but_others_have() {
    // A program with rules for `ancestor` but a query against a
    // rule-less `color` predicate. The rewriter should leave the
    // color relation intact and the query should return the seeded
    // colors — the `ancestor` rules are dead code for this query.
    let fp = "r226m4-magic-18";
    let program = build_program(
        fp,
        "color(red). color(blue).\n\
         parent(a, b).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).",
    );
    let q = build_query(fp, "color(?C)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    assert_values_eq(fp, values_of(&magic, "C"), &[ident("blue"), ident("red")]);
}

#[test]
fn r226m4_magic_19_answer_preserving_over_shuffled_rule_order() {
    // Rule order should not affect the magic-set answer set. Two
    // programs with the same rules in different orders must produce
    // the same bindings.
    let fp = "r226m4-magic-19";
    let program_a = build_program(
        fp,
        "parent(alice, bob). parent(bob, carol).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    let program_b = build_program(
        fp,
        "parent(alice, bob). parent(bob, carol).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).",
    );
    let q = build_query(fp, "ancestor(alice, ?Z)");
    let ev = Evaluator::new();
    let a = ev.run_query_via_magic_sets(&program_a, &q).expect(fp);
    let b = ev.run_query_via_magic_sets(&program_b, &q).expect(fp);
    assert_bindings_match_var(fp, &a, &b, "Z");
}

#[test]
fn r226m4_magic_20_two_hop_number_query_ground_second_arg() {
    // Ground-second-arg query on a 2-hop relation. Same shape as the
    // FS-graph queries R226.M4 targets in production (find every
    // ancestor of a given node). Assert answer parity with naïve.
    let fp = "r226m4-magic-20";
    let program = build_program(
        fp,
        "parent(alice, bob).\n\
         parent(bob, carol).\n\
         parent(carol, dave).\n\
         parent(dave, eve).\n\
         ancestor(?X, ?Y) => parent(?X, ?Y).\n\
         ancestor(?X, ?Z) => parent(?X, ?Y), ancestor(?Y, ?Z).",
    );
    // Every ancestor of dave.
    let q = build_query(fp, "ancestor(?X, dave)");
    let ev = Evaluator::new();
    let magic = ev.run_query_via_magic_sets(&program, &q).expect(fp);
    let naive = ev.run_query(&program, &q).expect(fp);
    assert_bindings_match_var(fp, &magic, &naive, "X");
    assert_values_eq(
        fp,
        values_of(&magic, "X"),
        &[ident("alice"), ident("bob"), ident("carol")],
    );
}

// --------------------------------------------------------------------
// Regression pin — every prior fixture (parser + eval) should still
// pass; this file adds tests but does not touch the shared modules.
// --------------------------------------------------------------------
#[test]
fn r226m4_regression_smoke_prior_paths_untouched() {
    // Deliberate zero-assertion smoke: touching this file must not
    // silently break the prior seminaïve path. If M1/M2 fixtures
    // break because a shared crate change bled through, `cargo test`
    // catches it before this one — but naming a symbol from every
    // public surface here ensures the crate's re-exports still
    // compile after the M4 changes.
    let _ = Program::empty();
    let _ = Query { goals: vec![Atom::new("p", vec![Term::Var("X".into())])] };
    let _ = query;
    let _ = Evaluator::new();
}
