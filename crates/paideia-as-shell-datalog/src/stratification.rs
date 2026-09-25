//! R226.M5 stratifier for programs with negation.
//!
//! # What stratification decides
//!
//! Given a program with `not p(...)` conjuncts in rule bodies, we
//! partition the predicate universe into a totally-ordered sequence
//! of *strata* such that:
//!
//! * If a rule with head predicate `H` uses `not q(...)` in its body,
//!   then `stratum(q) < stratum(H)` — the negated predicate must be
//!   *fully materialised* before `H`'s fixpoint runs.
//! * If a rule with head predicate `H` uses positive `p(...)` in its
//!   body, then `stratum(p) ≤ stratum(H)` — positive recursion is
//!   allowed within a stratum.
//!
//! Such a stratification exists iff the predicate dependency graph
//! has no cycle that crosses any negated edge (Apt-Blair-Walker 1988).
//! The evaluator then runs seminaïve fixpoint stratum by stratum, so
//! every negated goal is checked against a completed relation — the
//! standard closed-world semantics for stratified Datalog.
//!
//! # Algorithm (Tarjan SCC + topological stratum lift)
//!
//! 1. **Dependency graph.** Each rule `H :- b₁, …, bₙ` contributes,
//!    for each body conjunct `bᵢ`, one directed edge
//!    `predicate(bᵢ) -> predicate(H)` tagged with a boolean flag
//!    saying whether the conjunct is negated. A predicate that only
//!    appears in facts (EDB) still gets a node (no incoming edges).
//!
//! 2. **SCCs.** Tarjan's SCC discovers the strongly-connected
//!    components — each SCC is a maximal set of predicates that can
//!    reach each other in a cycle. Within an SCC, every internal
//!    edge is a recursive dependency; a *negated* internal edge is
//!    the "negation through recursion" that stratification forbids
//!    (return [`StratificationError::CycleThroughNegation`] naming
//!    the SCC's predicates).
//!
//! 3. **Stratum assignment.** The SCC DAG (nodes = SCCs, edges =
//!    inter-SCC dependencies) is topologically sorted. Each SCC's
//!    stratum is `max{ stratum(pred_source) + Δ : (source → target)
//!    edge lands in this SCC }`, where `Δ = 1` for a negated edge
//!    and `Δ = 0` for a positive edge. A predicate that participates
//!    in no dependency edge (an EDB-only predicate) gets stratum 0.
//!
//! # Complexity
//!
//! Tarjan is `O(V + E)` in predicate count and rule-body-atom count.
//! On the shell's corpus (`< 50` predicates per program) it is not
//! observable next to the fixpoint itself.
//!
//! # Fingerprints in this module
//!
//! Diagnostics returned here name the offending cycle predicates in
//! discovery order so the R220.M10 correlator can pin a specific
//! program's rejection to a specific SCC — the test corpus in
//! `tests/stratified_negation.rs` asserts non-emptiness only, keeping
//! the ordering free to change if the SCC walker's tie-breaking rules
//! evolve in a later milestone.

use crate::ast::{BodyGoal, Program};
use std::collections::{HashMap, HashSet};

/// Discriminated stratification-failure modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StratificationError {
    /// The predicate dependency graph contains a cycle whose edges
    /// include at least one negated body reference — negation
    /// through recursion, which stratified semantics cannot fixpoint.
    ///
    /// `cycle` names the predicates in the offending SCC; order
    /// follows Tarjan's discovery order for a given input, but no
    /// test-level guarantee is made about that order (only that the
    /// list is non-empty and every named predicate participates in
    /// the cycle).
    CycleThroughNegation {
        /// Predicates forming the cycle. Non-empty on construction.
        cycle: Vec<String>,
    },
}

/// Compute a stratification of `program`. On success, the returned
/// map assigns each predicate mentioned in the program (facts, rule
/// heads, or rule bodies) a densely-packed stratum index starting
/// from 0.
///
/// The evaluator uses the map to group rules by their head
/// predicate's stratum and iterate strata in ascending order.
pub fn compute_strata(
    program: &Program,
) -> Result<HashMap<String, usize>, StratificationError> {
    // ------------------------------------------------------------
    // Step 1 — collect every predicate that appears anywhere, and
    // the (source → target, negated) edges induced by rule bodies.
    // ------------------------------------------------------------
    let mut preds: HashSet<String> = HashSet::new();
    for fact in &program.facts {
        preds.insert(fact.predicate.clone());
    }
    // (source_predicate, target_predicate, negated)
    let mut edges: Vec<(String, String, bool)> = Vec::new();
    for rule in &program.rules {
        preds.insert(rule.head.predicate.clone());
        for goal in &rule.body {
            let src = goal.atom().predicate.clone();
            preds.insert(src.clone());
            edges.push((src, rule.head.predicate.clone(), goal.is_negated()));
        }
    }

    // Deterministic ordering — sort predicates so SCC discovery
    // order is stable across runs (HashMap iteration is not).
    let mut pred_list: Vec<String> = preds.into_iter().collect();
    pred_list.sort();
    let pred_index: HashMap<String, usize> = pred_list
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i))
        .collect();
    let n = pred_list.len();

    // Adjacency by index — for each predicate, the list of predicates
    // it points to (i.e. that depend on it). Multiple edges between
    // the same pair are preserved so a "positive + negated" edge pair
    // is recognisable when scanning intra-SCC edges.
    let mut adj: Vec<Vec<(usize, bool)>> = vec![Vec::new(); n];
    for (src, tgt, neg) in &edges {
        let s = pred_index[src];
        let t = pred_index[tgt];
        adj[s].push((t, *neg));
    }

    // ------------------------------------------------------------
    // Step 2 — Tarjan SCCs.
    // ------------------------------------------------------------
    let sccs = tarjan_sccs(n, &adj);
    // Map each node → its SCC index.
    let mut scc_of: Vec<usize> = vec![0; n];
    for (i, scc) in sccs.iter().enumerate() {
        for &node in scc {
            scc_of[node] = i;
        }
    }

    // ------------------------------------------------------------
    // Step 3 — reject any SCC containing a negated internal edge.
    // A self-loop (single-node SCC with an edge to itself) also
    // counts as a cycle, so negated self-references (p :- not p) are
    // caught here without a size-2 special case.
    // ------------------------------------------------------------
    for scc in &sccs {
        let scc_set: HashSet<usize> = scc.iter().copied().collect();
        for &node in scc {
            for &(target, negated) in &adj[node] {
                if negated && scc_set.contains(&target) {
                    // If the SCC has size 1 and the ONLY reason it is
                    // a cycle is the negated edge (a p → p self-loop),
                    // still count it. Otherwise the SCC has ≥ 2 nodes
                    // and the negated edge is a recursive negation.
                    if scc.len() > 1 || (scc.len() == 1 && target == node) {
                        let mut cycle: Vec<String> =
                            scc.iter().map(|&i| pred_list[i].clone()).collect();
                        cycle.sort();
                        return Err(StratificationError::CycleThroughNegation { cycle });
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------
    // Step 4 — topological stratum lift over the SCC DAG.
    // For each SCC-level edge, target_stratum ≥ source_stratum + Δ.
    // Repeat until no update — the DAG guarantees convergence in
    // at most `sccs.len()` passes.
    // ------------------------------------------------------------
    let scc_count = sccs.len();
    let mut scc_stratum: Vec<usize> = vec![0; scc_count];
    // Deduplicate SCC-level edges but *maximize* Δ (a pair joined by
    // both positive and negated edges lifts by 1).
    let mut scc_edges: HashMap<(usize, usize), bool> = HashMap::new();
    for (u, adj_u) in adj.iter().enumerate() {
        let su = scc_of[u];
        for &(v, negated) in adj_u {
            let sv = scc_of[v];
            if su == sv {
                continue;
            }
            let entry = scc_edges.entry((su, sv)).or_insert(false);
            if negated {
                *entry = true;
            }
        }
    }
    // Bellman-Ford-style relaxation on a DAG — n iterations is a
    // safe over-approximation, and for the shell's corpus n is tiny.
    for _ in 0..scc_count {
        let mut changed = false;
        for (&(su, sv), &negated) in &scc_edges {
            let delta = if negated { 1 } else { 0 };
            let candidate = scc_stratum[su] + delta;
            if candidate > scc_stratum[sv] {
                scc_stratum[sv] = candidate;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // ------------------------------------------------------------
    // Step 5 — expand SCC strata back to per-predicate strata.
    // ------------------------------------------------------------
    let mut out: HashMap<String, usize> = HashMap::new();
    for (i, pred) in pred_list.iter().enumerate() {
        out.insert(pred.clone(), scc_stratum[scc_of[i]]);
    }
    Ok(out)
}

// --------------------------------------------------------------------
// Tarjan SCC — standard iterative variant (recursive would blow the
// stack on adversarial inputs; the shell's REPL can load arbitrary
// user programs so we do not assume any predicate-count bound).
// --------------------------------------------------------------------

fn tarjan_sccs(n: usize, adj: &[Vec<(usize, bool)>]) -> Vec<Vec<usize>> {
    let mut index_counter: usize = 0;
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut indices: Vec<Option<usize>> = vec![None; n];
    let mut lowlinks: Vec<usize> = vec![0; n];
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    // Iterative DFS frame: (node, child-iterator-position).
    // We hand-roll the recursion so a pathological input (10⁴+
    // predicates in a chain) does not overflow the OS stack.
    #[derive(Debug)]
    struct Frame {
        node: usize,
        next_child: usize,
    }

    for start in 0..n {
        if indices[start].is_some() {
            continue;
        }
        let mut frames: Vec<Frame> = Vec::new();
        indices[start] = Some(index_counter);
        lowlinks[start] = index_counter;
        index_counter += 1;
        stack.push(start);
        on_stack[start] = true;
        frames.push(Frame {
            node: start,
            next_child: 0,
        });

        while let Some(frame) = frames.last_mut() {
            let v = frame.node;
            if frame.next_child < adj[v].len() {
                let (w, _neg) = adj[v][frame.next_child];
                frame.next_child += 1;
                if indices[w].is_none() {
                    // Recurse into w.
                    indices[w] = Some(index_counter);
                    lowlinks[w] = index_counter;
                    index_counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    frames.push(Frame {
                        node: w,
                        next_child: 0,
                    });
                } else if on_stack[w] {
                    let w_index = indices[w].unwrap();
                    if w_index < lowlinks[v] {
                        lowlinks[v] = w_index;
                    }
                }
            } else {
                // All children of v processed — pop the frame and, if
                // v is an SCC root, collect the SCC off `stack`.
                let v_lowlink = lowlinks[v];
                let v_index = indices[v].unwrap();
                frames.pop();
                if let Some(parent) = frames.last_mut() {
                    if v_lowlink < lowlinks[parent.node] {
                        lowlinks[parent.node] = v_lowlink;
                    }
                }
                if v_lowlink == v_index {
                    let mut scc: Vec<usize> = Vec::new();
                    loop {
                        let w = stack.pop().expect("stack must contain the SCC nodes");
                        on_stack[w] = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(scc);
                }
            }
        }
    }
    sccs
}

// --------------------------------------------------------------------
// Compile-only sanity check: every public re-export path is exercised
// here so a downstream user of this module can rely on the same
// symbols the tests exercise, without depending on the test corpus.
// --------------------------------------------------------------------
#[allow(dead_code)]
fn _compile_check(program: &Program) {
    let _ = compute_strata(program);
    let _: BodyGoal = BodyGoal::Positive(crate::ast::Atom::new("p", Vec::new()));
}
