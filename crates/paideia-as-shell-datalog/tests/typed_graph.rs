//! R226.M7 TypedGraph substrate fixture corpus (10 tests, tag
//! `r226m7-graph-NN`). Each fixture builds a `TypedGraph`, registers
//! it under a Datalog predicate name on an `Evaluator`, runs a query
//! via `run_query_with_graph`, and asserts on the projected result.
//!
//! Every panic message carries its fingerprint so the R220.M10
//! `@fingerprint` correlator can attribute a regression to a single
//! fixture without re-parsing the test name.
//!
//! # What this corpus exercises
//!
//! * Empty graph → empty projection (`01`).
//! * Bound-first walk (`02`) and multi-target enumeration (`03`).
//! * Cycle safety — a self-referential graph must terminate, not
//!   diverge (`04`).
//! * Label filtering — multiple label kinds coexist on the same
//!   source node (`05`).
//! * Graph tuples compose with rule-derived tuples (`06`).
//! * Node type-tag propagation through the graph accessors — the
//!   evaluator projects only the id, but the substrate carries type
//!   information a caller reads out through [`TypedGraph::node`] and
//!   [`TypedGraph::children_of`] (`07`).
//! * Unregistered predicates fall through to standard rule-based
//!   derivation (`08`).
//! * Two independently-registered graphs on the same evaluator
//!   contribute to disjoint predicates (`09`).
//! * Query with mixed bound + free vars against a graph predicate
//!   (`10`).

mod common;

use common::{assert_values_eq, dlg_tokens, ident};
use paideia_as_shell_datalog::{
    parse_block, parse_query, Binding, Evaluator, Program, Query, TypedGraph, Value,
};
use std::sync::Arc;

// --------------------------------------------------------------------
// Shared helpers — mirror the shape of `session_edb_fixtures.rs`.
// --------------------------------------------------------------------

fn build_program(fp: &str, src: &str) -> Program {
    let tokens = dlg_tokens(src);
    parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse failed: {e:?}"))
}

fn build_query(fp: &str, src: &str) -> Query {
    let tokens = dlg_tokens(src);
    parse_query(&tokens).unwrap_or_else(|e| panic!("{fp}: query parse failed: {e:?}"))
}

fn values_of(bindings: &[Binding], var: &str) -> Vec<Value> {
    let mut vs: Vec<Value> = bindings
        .iter()
        .filter_map(|b| b.get(var).cloned())
        .collect();
    vs.sort_by(|a, b| format!("{a}").cmp(&format!("{b}")));
    vs.dedup();
    vs
}

fn pairs_of(bindings: &[Binding], v1: &str, v2: &str) -> Vec<(Value, Value)> {
    let mut ps: Vec<(Value, Value)> = bindings
        .iter()
        .filter_map(|b| {
            let a = b.get(v1).cloned()?;
            let c = b.get(v2).cloned()?;
            Some((a, c))
        })
        .collect();
    ps.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    ps.dedup();
    ps
}

// ====================================================================
// 01 — empty graph → registered predicate returns no tuples
// ====================================================================

#[test]
fn r226m7_graph_01_empty_graph_no_tuples() {
    let fp = "r226m7-graph-01";
    let program = Program::empty();
    let query = build_query(fp, "edge(?X, ?Y)");

    let graph = Arc::new(TypedGraph::new());
    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    assert!(
        bindings.is_empty(),
        "{fp}: empty graph must yield no bindings, got {bindings:#?}",
    );
}

// ====================================================================
// 02 — 3-node linear a→b→c → edge(a, ?X) returns 1 result
// ====================================================================

#[test]
fn r226m7_graph_02_linear_bound_first_one_child() {
    let fp = "r226m7-graph-02";
    let program = Program::empty();
    let query = build_query(fp, "edge(a, ?X)");

    let mut g = TypedGraph::new();
    g.add_node("a", "dir");
    g.add_node("b", "dir");
    g.add_node("c", "file");
    g.add_edge("a", "b", "child");
    g.add_edge("b", "c", "child");
    let graph = Arc::new(g);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    let got = values_of(&bindings, "X");
    assert_values_eq(fp, got, &[ident("b")]);
}

// ====================================================================
// 03 — branching 1→{2,3} → edge(1, ?X) returns 2 results
// ====================================================================

#[test]
fn r226m7_graph_03_branching_two_children() {
    let fp = "r226m7-graph-03";
    let program = Program::empty();
    let query = build_query(fp, "edge(one, ?X)");

    let mut g = TypedGraph::new();
    g.add_node("one", "dir");
    g.add_node("two", "file");
    g.add_node("three", "file");
    g.add_edge("one", "two", "child");
    g.add_edge("one", "three", "child");
    let graph = Arc::new(g);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    let got = values_of(&bindings, "X");
    assert_values_eq(fp, got, &[ident("three"), ident("two")]);
}

// ====================================================================
// 04 — cyclic a→b→a — fixpoint must terminate without divergence
// ====================================================================

#[test]
fn r226m7_graph_04_cyclic_terminates() {
    let fp = "r226m7-graph-04";
    // Rule adds transitive closure over the graph predicate. On a
    // cyclic 2-node graph the closure is finite (4 pairs), but a naive
    // walker unaware of the fixpoint dedup would loop indefinitely.
    let program = build_program(
        fp,
        "reach(?X, ?Y) => edge(?X, ?Y).\n\
         reach(?X, ?Z) => edge(?X, ?Y), reach(?Y, ?Z).",
    );
    let query = build_query(fp, "reach(?X, ?Y)");

    let mut g = TypedGraph::new();
    g.add_node("a", "n");
    g.add_node("b", "n");
    g.add_edge("a", "b", "child");
    g.add_edge("b", "a", "child");
    let graph = Arc::new(g);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: cyclic query diverged or failed: {e:?}"));
    let got = pairs_of(&bindings, "X", "Y");
    // Closure over a↔b is {(a,a), (a,b), (b,a), (b,b)}.
    let want: Vec<(Value, Value)> = vec![
        (ident("a"), ident("a")),
        (ident("a"), ident("b")),
        (ident("b"), ident("a")),
        (ident("b"), ident("b")),
    ];
    let mut want_sorted = want;
    want_sorted.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(
        got, want_sorted,
        "{fp}: cyclic closure mismatch — got {got:#?}, want {want_sorted:#?}",
    );
}

// ====================================================================
// 05 — multi-label edges — filter by label
// ====================================================================

#[test]
fn r226m7_graph_05_multi_label_filter() {
    let fp = "r226m7-graph-05";
    let program = Program::empty();

    let mut g = TypedGraph::new();
    g.add_node("a", "dir");
    g.add_node("b", "file");
    g.add_node("c", "file");
    g.add_edge("a", "b", "child");
    g.add_edge("a", "c", "sibling");
    let graph = Arc::new(g);

    // Register the SAME graph under two predicate names, filtered by
    // two different labels. The projections must be disjoint.
    let mut ev = Evaluator::new();
    ev.register_graph_predicate("child_of", graph.clone(), "child");
    ev.register_graph_predicate("sibling_of", graph, "sibling");

    let q_child = build_query(fp, "child_of(a, ?X)");
    let child_bindings = ev
        .run_query_with_graph(&program, &q_child)
        .unwrap_or_else(|e| panic!("{fp}: child query failed: {e:?}"));
    assert_values_eq(fp, values_of(&child_bindings, "X"), &[ident("b")]);

    let q_sib = build_query(fp, "sibling_of(a, ?X)");
    let sib_bindings = ev
        .run_query_with_graph(&program, &q_sib)
        .unwrap_or_else(|e| panic!("{fp}: sibling query failed: {e:?}"));
    assert_values_eq(fp, values_of(&sib_bindings, "X"), &[ident("c")]);
}

// ====================================================================
// 06 — registered predicate combined with a rule-based predicate
// ====================================================================

#[test]
fn r226m7_graph_06_graph_plus_rule() {
    let fp = "r226m7-graph-06";
    // Program declares its own `interesting/1` relation via facts, and
    // a rule that JOINS the graph predicate with `interesting`.
    // A node `?Y` is `notable` iff it is a child of `a` in the graph
    // AND is interesting per the rule-side EDB.
    let program = build_program(
        fp,
        "interesting(b). interesting(d).\n\
         notable(?Y) => edge(a, ?Y), interesting(?Y).",
    );
    let query = build_query(fp, "notable(?Y)");

    let mut g = TypedGraph::new();
    g.add_node("a", "dir");
    g.add_node("b", "file");
    g.add_node("c", "file");
    g.add_node("d", "file");
    g.add_edge("a", "b", "child");
    g.add_edge("a", "c", "child");
    // Note: d is interesting but NOT a child of a in the graph.
    let graph = Arc::new(g);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    // Only `b` satisfies both — c is a child but not interesting; d is
    // interesting but not a child.
    assert_values_eq(fp, values_of(&bindings, "Y"), &[ident("b")]);
}

// ====================================================================
// 07 — type-filter via GraphNode.type_name
// ====================================================================

#[test]
fn r226m7_graph_07_children_carry_expected_type() {
    let fp = "r226m7-graph-07";
    // The evaluator projects only ids; the substrate still carries
    // node type tags a caller reads directly. This fixture pins that
    // the projection AND the accessor agree — a child returned by the
    // query is the same node the substrate's `node()` accessor reports
    // with the expected `type_name`.
    let program = Program::empty();
    let query = build_query(fp, "edge(root, ?X)");

    let mut g = TypedGraph::new();
    g.add_node("root", "dir");
    g.add_node("a", "file");
    g.add_node("b", "file");
    g.add_edge("root", "a", "child");
    g.add_edge("root", "b", "child");
    let graph = Arc::new(g);

    // Substrate-side accessor: every child must carry type "file".
    for child in graph.children_of("root", "child") {
        assert_eq!(
            child.type_name, "file",
            "{fp}: substrate child {} has unexpected type {}",
            child.id, child.type_name,
        );
    }

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph.clone(), "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    let got = values_of(&bindings, "X");
    assert_values_eq(fp, got.clone(), &[ident("a"), ident("b")]);

    // Cross-check: every projected id resolves to a "file"-typed node
    // via the substrate accessor.
    for v in &got {
        let Value::Ident(id) = v else {
            panic!("{fp}: expected Ident, got {v:?}");
        };
        let node = graph
            .node(id)
            .unwrap_or_else(|| panic!("{fp}: projected id {id} missing from substrate"));
        assert_eq!(
            node.type_name, "file",
            "{fp}: projected node {id} has type {}, expected file",
            node.type_name,
        );
    }
}

// ====================================================================
// 08 — unregistered predicate falls back to standard rule
// ====================================================================

#[test]
fn r226m7_graph_08_unregistered_falls_back() {
    let fp = "r226m7-graph-08";
    // Register a graph under `edge`, but the query hits an unrelated
    // predicate `role/2` that the program supplies via ordinary
    // facts + one rule. The graph must contribute nothing to that
    // relation.
    let program = build_program(
        fp,
        "role(alice, admin). role(bob, user).\n\
         is_admin(?X) => role(?X, admin).",
    );
    let query = build_query(fp, "is_admin(?X)");

    let mut g = TypedGraph::new();
    g.add_node("alice", "n");
    g.add_node("bob", "n");
    g.add_edge("alice", "bob", "child");
    let graph = Arc::new(g);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    let bindings = ev
        .run_query_with_graph(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: query failed: {e:?}"));
    // Only alice is an admin per the program's role facts. The graph
    // predicate `edge` is not consulted by any rule here.
    assert_values_eq(fp, values_of(&bindings, "X"), &[ident("alice")]);
}

// ====================================================================
// 09 — two graphs, disjoint predicate names, evaluated independently
// ====================================================================

#[test]
fn r226m7_graph_09_two_graphs_independent() {
    let fp = "r226m7-graph-09";
    let program = Program::empty();

    let mut g1 = TypedGraph::new();
    g1.add_node("a", "n");
    g1.add_node("b", "n");
    g1.add_edge("a", "b", "child");
    let graph1 = Arc::new(g1);

    let mut g2 = TypedGraph::new();
    g2.add_node("x", "n");
    g2.add_node("y", "n");
    g2.add_node("z", "n");
    g2.add_edge("x", "y", "child");
    g2.add_edge("x", "z", "child");
    let graph2 = Arc::new(g2);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("fs", graph1, "child");
    ev.register_graph_predicate("net", graph2, "child");

    // Query against `fs` sees graph1's edges only.
    let q_fs = build_query(fp, "fs(a, ?X)");
    let fs_bindings = ev
        .run_query_with_graph(&program, &q_fs)
        .unwrap_or_else(|e| panic!("{fp}: fs query failed: {e:?}"));
    assert_values_eq(fp, values_of(&fs_bindings, "X"), &[ident("b")]);

    // Query against `net` sees graph2's edges only.
    let q_net = build_query(fp, "net(x, ?X)");
    let net_bindings = ev
        .run_query_with_graph(&program, &q_net)
        .unwrap_or_else(|e| panic!("{fp}: net query failed: {e:?}"));
    assert_values_eq(fp, values_of(&net_bindings, "X"), &[ident("y"), ident("z")]);

    // Cross-check: `fs(x, ?X)` must be empty — x is in graph2, not
    // graph1, and the `fs` predicate only projects graph1's tuples.
    let q_cross = build_query(fp, "fs(x, ?X)");
    let cross_bindings = ev
        .run_query_with_graph(&program, &q_cross)
        .unwrap_or_else(|e| panic!("{fp}: cross query failed: {e:?}"));
    assert!(
        cross_bindings.is_empty(),
        "{fp}: fs(x, ?X) must be empty (x belongs to graph2), got {cross_bindings:#?}",
    );
}

// ====================================================================
// 10 — query with mixed bound + free vars against a graph predicate
// ====================================================================

#[test]
fn r226m7_graph_10_mixed_bound_free_query() {
    let fp = "r226m7-graph-10";
    let program = Program::empty();

    let mut g = TypedGraph::new();
    g.add_node("root", "dir");
    g.add_node("a", "file");
    g.add_node("b", "file");
    g.add_node("c", "file");
    g.add_edge("root", "a", "child");
    g.add_edge("root", "b", "child");
    g.add_edge("root", "c", "child");
    g.add_edge("a", "b", "child");
    let graph = Arc::new(g);

    let mut ev = Evaluator::new();
    ev.register_graph_predicate("edge", graph, "child");

    // Bound-first: source pinned, target free — 3 children of root.
    let q_bf = build_query(fp, "edge(root, ?X)");
    let bf_bindings = ev
        .run_query_with_graph(&program, &q_bf)
        .unwrap_or_else(|e| panic!("{fp}: bound-first query failed: {e:?}"));
    assert_values_eq(
        fp,
        values_of(&bf_bindings, "X"),
        &[ident("a"), ident("b"), ident("c")],
    );

    // Free-first: source free, target pinned — only `root` and `a`
    // reach `b` under the "child" label.
    let q_fb = build_query(fp, "edge(?X, b)");
    let fb_bindings = ev
        .run_query_with_graph(&program, &q_fb)
        .unwrap_or_else(|e| panic!("{fp}: free-first query failed: {e:?}"));
    assert_values_eq(
        fp,
        values_of(&fb_bindings, "X"),
        &[ident("a"), ident("root")],
    );

    // Both bound (member check): edge(root, a) exists → one binding;
    // edge(a, root) does NOT exist → zero bindings.
    let q_yes = build_query(fp, "edge(root, a)");
    let yes_bindings = ev
        .run_query_with_graph(&program, &q_yes)
        .unwrap_or_else(|e| panic!("{fp}: yes query failed: {e:?}"));
    assert_eq!(
        yes_bindings.len(),
        1,
        "{fp}: edge(root, a) must return one binding, got {yes_bindings:#?}",
    );

    let q_no = build_query(fp, "edge(a, root)");
    let no_bindings = ev
        .run_query_with_graph(&program, &q_no)
        .unwrap_or_else(|e| panic!("{fp}: no query failed: {e:?}"));
    assert!(
        no_bindings.is_empty(),
        "{fp}: edge(a, root) must be empty, got {no_bindings:#?}",
    );
}
