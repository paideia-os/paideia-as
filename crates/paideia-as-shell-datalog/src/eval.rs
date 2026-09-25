//! R226.M2 seminaïve bottom-up fixpoint evaluator + query engine.
//!
//! # Algorithm
//!
//! Standard seminaïve (Abiteboul-Hull-Vianu §13). Notation: `T` is the
//! full database, `Δ` is the round's new-tuples set, `Δ_prev` is the
//! previous round's `Δ`.
//!
//! ```text
//!   T ← EDB facts
//!   Δ_prev ← T
//!   loop:
//!       Δ ← ∅
//!       for each rule  H :- B_1, B_2, …, B_n :
//!           for each pivot position  i ∈ [0, n):
//!               // seminaïve pivot: at least one body match comes from
//!               // the freshly-added Δ_prev; the rest from T (which
//!               // includes Δ_prev already).
//!               for each substitution σ satisfying
//!                       σ(B_i) ∈ Δ_prev  and
//!                       ∀ j ≠ i: σ(B_j) ∈ T :
//!                   candidate ← σ(H)
//!                   if candidate ∉ T:  Δ.insert(candidate)
//!       if Δ.is_empty(): break
//!       T ← T ∪ Δ
//!       Δ_prev ← Δ
//! ```
//!
//! The pivot loop is what makes this seminaïve rather than naive: a
//! candidate derivation from round `k` must consume at least one round-
//! `k-1` tuple, so re-derivations that were already produced in round
//! `k-1` are not attempted again in round `k`. A rule with `n` body
//! atoms produces `n` pivot variants per round; on the R226.M4 magic-
//! set corpus this is dominated by the graph-walk restriction the
//! rewrite injects. At M1/M2 corpus scale (≤10³ tuples) the pivot loop
//! is the whole engine.
//!
//! # Index shape
//!
//! `HashMap<(PredicateName, arity), HashSet<Vec<Value>>>`. The tuple
//! set doubles as the "membership" test the fixpoint needs (`candidate
//! ∉ T`). R226.M6/M7 will layer typed indices per FS §7.2 on top; the
//! naive shape here is fine for the small M1/M2 test corpus and
//! interpretable enough that a debugger reads it directly.
//!
//! # Termination
//!
//! Datalog without function symbols has a finite Herbrand base (the
//! product of constant × arity), so the fixpoint always terminates.
//! We do not track a step budget at M1/M2; R226.M10 (progress
//! emission) will add a cooperative-cancel token so a runaway magic-
//! set rewrite cannot hang the REPL.

use crate::ast::{Atom, BodyGoal, Program, Query, Rule, Term, Value};
use crate::magic_sets;
use crate::stratification::{self, StratificationError};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Concrete per-predicate table. `predicate` name + `arity` are the
/// composite key so `p/2` and `p/3` are distinct relations (the R229
/// schema-registry contract, which R226.M9 will re-check at query
/// time).
type Key = (String, usize);
type TupleSet = HashSet<Vec<Value>>;

/// The seminaïve fixpoint database — a per-predicate index of ground
/// tuples plus a snapshot of the delta from the last round.
///
/// Public API is deliberately small: `from_program` builds one from
/// an AST, `tuples` reads back the relation for a predicate, and
/// `query` (module-level free function) runs a conjunctive query
/// against a materialized DB. R226.M8 (assert/retract) will add
/// mutation methods; M1/M2 keeps it immutable-after-build so a REPL
/// can cache the fixpoint for repeated queries.
#[derive(Clone, Debug, Default)]
pub struct Database {
    tables: HashMap<Key, TupleSet>,
}

impl Database {
    /// Materialize the seminaïve fixpoint of `program`.
    ///
    /// Returns `Err(EvalError)` if the program mentions a `Term::Bound`
    /// (pipeline interpolation), which is not resolvable without a
    /// running shell — see the `ast::Term` module doc.
    ///
    /// R226.M5: if the program mentions `not p(...)` in any rule body,
    /// it is evaluated under stratified negation
    /// (Apt-Blair-Walker 1988); an unstratifiable program (a cycle in
    /// the predicate dependency graph carrying a negated edge) is
    /// rejected with `EvalError::UnstratifiedNegation` **before** the
    /// fixpoint runs — so no half-derived tuples leak out. A program
    /// with no negation collapses to a single stratum and evaluates
    /// exactly as the R226.M2 seminaïve loop did.
    pub fn from_program(program: &Program) -> Result<Self, EvalError> {
        evaluate_stratified(program)
    }

    /// Read the ground tuples of a `predicate/arity` relation, if any.
    /// Returns `None` when the predicate never appeared in the EDB or
    /// as any rule's head.
    pub fn tuples(&self, predicate: &str, arity: usize) -> Option<&HashSet<Vec<Value>>> {
        self.tables.get(&(predicate.to_owned(), arity))
    }

    /// Sum of tuple counts across every `(predicate, arity)` relation
    /// materialized in this database. Introduced for R226.M4 magic-set
    /// selectivity fixtures — they compare the total DB size of the
    /// naïve fixpoint against the total DB size of the magic-set
    /// fixpoint, and require an accessor that does not depend on the
    /// caller enumerating every predicate name it might have created
    /// (magic-set rewriting invents fresh predicate names the caller
    /// cannot easily enumerate).
    pub fn total_tuple_count(&self) -> usize {
        self.tables.values().map(|s| s.len()).sum()
    }

    /// Enumerate every `(predicate, arity)` key currently populated.
    /// Primarily for tests that need a full-DB walk without
    /// re-discovering predicate names. Order is unspecified — callers
    /// that need a stable order sort the result themselves.
    pub fn predicate_keys(&self) -> Vec<(String, usize)> {
        self.tables.keys().cloned().collect()
    }

    /// Insert one tuple into the relation for `predicate` (used by
    /// `from_program` while seeding the EDB; kept `pub(crate)` because
    /// mutation from outside is R226.M8's territory).
    pub(crate) fn insert(&mut self, predicate: String, tuple: Vec<Value>) {
        let key = (predicate, tuple.len());
        self.tables.entry(key).or_default().insert(tuple);
    }
}

/// One substitution — a partial mapping from logic-variable name to a
/// ground value. Small maps (a query typically has ≤10 free variables)
/// are the common case; a plain `HashMap` beats fancier alternatives
/// at this scale.
pub type Binding = HashMap<String, Value>;

/// Run a conjunctive query against a materialized database.
///
/// Returns every substitution σ such that σ(goal) is in the database
/// for each goal in the query. Free-variable ordering is preserved
/// per first appearance; `Term::Const` slots enforce equality; a
/// `Term::Bound` in the query is rejected the same way it is in a
/// program (pipeline resolution is R229's).
pub fn query(db: &Database, q: &Query) -> Result<Vec<Binding>, EvalError> {
    for atom in &q.goals {
        reject_bound(atom)?;
    }
    if q.goals.is_empty() {
        // Convention: an empty query has a single empty binding — every
        // trivially-conjunctive query is satisfied. The R226.M9
        // rejection layer will make this an error, but at M1/M2 we
        // preserve algebraic consistency.
        return Ok(vec![Binding::new()]);
    }
    let mut results = vec![Binding::new()];
    for goal in &q.goals {
        let mut next = Vec::new();
        let table = db.tables.get(&(goal.predicate.clone(), goal.arity()));
        let empty: TupleSet = HashSet::new();
        let candidates: &TupleSet = table.unwrap_or(&empty);
        for binding in &results {
            for tuple in candidates {
                if let Some(extended) = unify_atom(goal, tuple, binding) {
                    next.push(extended);
                }
            }
        }
        results = next;
        if results.is_empty() {
            break;
        }
    }
    Ok(results)
}

/// Zero-state façade over the module's free functions plus the
/// R226.M4 magic-set path. Kept as a unit struct rather than a
/// stateful engine: the underlying evaluator does not carry
/// configuration yet — R226.M10 (progress emission) and R226.M11
/// (fingerprint stream) will be the first to grow real fields, at
/// which point `Evaluator` becomes the natural place to hold them.
///
/// Present today so downstream crates can name a stable entry point
/// for the magic-set path (`Evaluator::run_query_via_magic_sets`)
/// rather than importing a free function that would migrate in a
/// later milestone.
#[derive(Clone, Copy, Debug, Default)]
pub struct Evaluator;

impl Evaluator {
    /// Convenience constructor. Identical to `Evaluator::default()`;
    /// preserved because `Evaluator::new()` reads more naturally in
    /// caller code where "new evaluator, then use it" is the mental
    /// model.
    pub fn new() -> Self {
        Self
    }

    /// Materialize `program` under the standard seminaïve fixpoint
    /// and return every substitution that satisfies `query`. Wraps
    /// `Database::from_program` + `query` in one call — kept for
    /// symmetry with `run_query_via_magic_sets`, so a caller
    /// benchmarking one path against the other reads left-to-right
    /// without swapping call shapes.
    pub fn run_query(
        &self,
        program: &Program,
        query: &Query,
    ) -> Result<Vec<Binding>, EvalError> {
        let db = Database::from_program(program)?;
        crate::eval::query(&db, query)
    }

    /// Materialize `program` under stratified negation (R226.M5) and
    /// return the resulting database — no query. The caller then runs
    /// [`query`] against the DB directly.
    ///
    /// This is the entry point for programs that mention `not p(...)`
    /// in any rule body: the evaluator groups rules by their head
    /// predicate's stratum (see [`crate::stratification`]) and runs
    /// seminaïve fixpoint stratum by stratum, so every negated goal is
    /// resolved against a completed relation from a strictly lower
    /// stratum. An unstratifiable program (a cycle in the predicate
    /// dependency graph that carries any negated edge) is rejected
    /// with [`EvalError::UnstratifiedNegation`] **before** the fixpoint
    /// runs, so no partial derivations survive.
    ///
    /// Positive-only programs collapse to a single stratum containing
    /// every rule and evaluate exactly as
    /// [`Database::from_program`] does — no observable difference.
    pub fn run_stratified(&self, program: &Program) -> Result<Database, EvalError> {
        evaluate_stratified(program)
    }

    /// Rewrite `program` via magic-set rewriting for `query`, then
    /// materialize the rewritten program's seminaïve fixpoint and
    /// answer `query` against the resulting DB.
    ///
    /// # Semantics
    ///
    /// The answer set is identical to
    /// [`run_query`](Self::run_query) — magic-set rewriting is
    /// answer-preserving (Beeri & Ramakrishnan 1991, §4.3). What
    /// differs is the intermediate DB size: fewer IDB tuples are
    /// materialized when the query has bound arguments, so the
    /// fixpoint terminates sooner and uses less memory.
    ///
    /// # Rewritten-DB inspection
    ///
    /// Callers that want to introspect the magic-set DB directly
    /// (e.g. count intermediate tuples for a speedup assertion) can
    /// call `magic_sets::rewrite` + `Database::from_program`
    /// themselves — this helper does not expose the intermediate
    /// `Database` because most consumers only care about the
    /// bindings.
    pub fn run_query_via_magic_sets(
        &self,
        program: &Program,
        query: &Query,
    ) -> Result<Vec<Binding>, EvalError> {
        let rewritten = magic_sets::rewrite(program, query);
        let db = Database::from_program(&rewritten)?;
        crate::eval::query(&db, query)
    }
}

/// Discriminated evaluation-time failure modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    /// The program or query mentioned a `Term::Bound` (pipeline
    /// interpolation), which needs the R229 REPL loop to resolve.
    UnresolvedPipelineValue {
        /// Variable name (bare, without the `$` sigil).
        name: String,
        /// Owning atom for diagnostics.
        atom: String,
    },
    /// The program's rule dependency graph contains a cycle that
    /// crosses at least one `not p(...)` edge — negation through
    /// recursion, which stratified semantics cannot assign a fixpoint
    /// to. Raised by [`Evaluator::run_stratified`] (and by
    /// [`Database::from_program`] on such a program) **before** any
    /// fixpoint iteration, so no half-derived tuples leak out.
    UnstratifiedNegation {
        /// The predicates that form the offending cycle, in the order
        /// the stratifier discovered them. Non-empty on construction.
        cycle: Vec<String>,
    },
}

// --------------------------------------------------------------------
// Internals
// --------------------------------------------------------------------

fn reject_bound(atom: &Atom) -> Result<(), EvalError> {
    for t in &atom.terms {
        if let Term::Bound(name) = t {
            return Err(EvalError::UnresolvedPipelineValue {
                name: name.clone(),
                atom: atom.to_string(),
            });
        }
    }
    Ok(())
}

/// End-to-end stratified evaluation: reject-bound, stratify, group
/// rules by their head predicate's stratum, then run a fresh seminaïve
/// fixpoint per stratum using the accumulated DB from lower strata as
/// its EDB. Called by both [`Database::from_program`] and
/// [`Evaluator::run_stratified`] — the two entry points are the same
/// pipeline, only their names differ so callers can name the shape of
/// their intent (constructor vs. explicit-negation-aware evaluation).
fn evaluate_stratified(program: &Program) -> Result<Database, EvalError> {
    // Refuse programs that mention pipeline interpolation — the shell
    // round that resolves `$expr` (R226.M8 + R229) is not landed yet.
    for atom in &program.facts {
        reject_bound(atom)?;
    }
    for rule in &program.rules {
        reject_bound(&rule.head)?;
        for body in &rule.body {
            reject_bound(body.atom())?;
        }
    }

    // Stratify — refuse negation through recursion before any tuple is
    // derived. On success `strata[p]` is the stratum index of predicate
    // `p` (0-based, densely packed).
    let strata = stratification::compute_strata(program).map_err(|e| match e {
        StratificationError::CycleThroughNegation { cycle } => {
            EvalError::UnstratifiedNegation { cycle }
        }
    })?;

    // Seed the EDB — the base of every stratum's fixpoint.
    let mut db = Database::default();
    for atom in &program.facts {
        let tuple = atom
            .as_ground()
            .expect("parser guaranteed fact atom is ground");
        db.insert(atom.predicate.clone(), tuple);
    }

    if program.rules.is_empty() {
        return Ok(db);
    }

    // Group rules by head-predicate stratum. `BTreeMap` gives an
    // ascending-order walk without a manual max-index compute step.
    let mut rules_by_stratum: BTreeMap<usize, Vec<&Rule>> = BTreeMap::new();
    for rule in &program.rules {
        let s = strata.get(&rule.head.predicate).copied().unwrap_or(0);
        rules_by_stratum.entry(s).or_default().push(rule);
    }

    // Run seminaïve stratum by stratum in ascending order. Each
    // stratum's fixpoint sees the previous strata as extensional (fully
    // materialised) input — the standard Apt-Blair-Walker construction.
    for (_stratum, rules) in &rules_by_stratum {
        seminaive_fixpoint(rules, &mut db);
    }

    Ok(db)
}

/// One seminaïve fixpoint over `rules` starting from `db`'s current
/// contents. Extracted so [`evaluate_stratified`] can call it once per
/// stratum, giving negation stratified semantics for free — a negated
/// body atom in stratum `k` always resolves against the completed
/// `db` of strata `< k` (its predicate is guaranteed by the stratifier
/// to sit strictly below `k`).
fn seminaive_fixpoint(rules: &[&Rule], db: &mut Database) {
    // First-round Δ is the base DB — every existing tuple is "new"
    // from this stratum's perspective (its rules have never fired
    // before). Subsequent rounds only see the strictly-fresh tuples
    // from the previous round.
    let mut delta_prev: HashMap<Key, TupleSet> = db.tables.clone();
    loop {
        let mut delta_new: HashMap<Key, TupleSet> = HashMap::new();
        for rule in rules {
            derive_round(rule, db, &delta_prev, &mut delta_new);
        }
        let mut had_new = false;
        for (key, set) in delta_new.iter_mut() {
            if let Some(existing) = db.tables.get(key) {
                set.retain(|t| !existing.contains(t));
            }
            if !set.is_empty() {
                had_new = true;
            }
        }
        if !had_new {
            break;
        }
        for (key, set) in &delta_new {
            let table = db.tables.entry(key.clone()).or_default();
            for t in set {
                table.insert(t.clone());
            }
        }
        delta_prev = delta_new;
    }
}

/// One seminaïve round for one rule: for each positive body-atom
/// pivot position `i`, match `body[i]` against `delta_prev` and every
/// other positive body atom against `db`; then filter the resulting
/// bindings by every negated body atom (retain a binding iff the
/// substituted negated atom does NOT match any tuple in `db`).
/// Collected derivations land in `delta_new`.
///
/// R226.M5 addition: negated goals never serve as pivots (a `not p`
/// conjunct doesn't produce new bindings — it only filters existing
/// ones) and are applied after the positive extension pass so every
/// variable in the negated atom is guaranteed to be bound before
/// membership is checked (safety condition).
fn derive_round(
    rule: &Rule,
    db: &Database,
    delta_prev: &HashMap<Key, TupleSet>,
    delta_new: &mut HashMap<Key, TupleSet>,
) {
    if rule.body.is_empty() {
        // Parser guarantees rules have non-empty bodies; a defensive
        // no-op keeps a future refactor from surprise-recursing.
        return;
    }

    // Only positive body atoms can be pivots — a negated goal does not
    // enumerate tuples (it filters). A rule with no positive goal is
    // range-unsafe (no variable can ever be bound); silently skip it
    // — R226.M9 will make it a compile-time diagnostic.
    let positive_positions: Vec<usize> = (0..rule.body.len())
        .filter(|&i| rule.body[i].is_positive())
        .collect();
    if positive_positions.is_empty() {
        return;
    }

    for &pivot in &positive_positions {
        let pivot_atom = rule.body[pivot].atom();
        let empty: TupleSet = HashSet::new();
        let pivot_tuples = delta_prev
            .get(&(pivot_atom.predicate.clone(), pivot_atom.arity()))
            .unwrap_or(&empty);
        for tuple in pivot_tuples {
            let Some(base_binding) = unify_atom(pivot_atom, tuple, &Binding::new()) else {
                continue;
            };
            let mut bindings = vec![base_binding];

            // Positive extension pass: iterate positive non-pivot body
            // atoms and join with the full DB.
            let mut ok = true;
            for (j, goal) in rule.body.iter().enumerate() {
                if j == pivot {
                    continue;
                }
                if let BodyGoal::Positive(a) = goal {
                    bindings = extend_with_db(a, db, bindings);
                    if bindings.is_empty() {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }

            // Negation filter pass: for every `not p(...)` conjunct,
            // retain only bindings whose substitution finds NO matching
            // tuple in `db`. Because the stratifier guarantees `p` sits
            // in a strictly lower stratum, `db` is complete for `p`
            // when this filter runs — closed-world negation is sound.
            for goal in &rule.body {
                if let BodyGoal::Negative(a) = goal {
                    bindings.retain(|b| !negation_matches(a, db, b));
                    if bindings.is_empty() {
                        break;
                    }
                }
            }
            if bindings.is_empty() {
                continue;
            }

            for binding in bindings {
                let head_tuple = ground_atom(&rule.head, &binding);
                let Some(head_tuple) = head_tuple else {
                    // Head had a free variable that no body atom bound.
                    // The `range restriction` violation is a legit
                    // Datalog compile-time error; at M1/M2 we silently
                    // skip the derivation (R226.M9 will make it a
                    // diagnostic).
                    continue;
                };
                let key = (rule.head.predicate.clone(), rule.head.arity());
                let already_in_db =
                    db.tables.get(&key).map(|s| s.contains(&head_tuple)).unwrap_or(false);
                if !already_in_db {
                    delta_new.entry(key).or_default().insert(head_tuple);
                }
            }
        }
    }
}

/// Check whether a negated body atom's relation contains any tuple
/// that unifies with the atom under the current binding. Returns
/// `true` when a match exists (i.e. the negation FAILS — the rule
/// must not fire for this binding). Returns `false` when no tuple
/// matches (the negation SUCCEEDS — the rule may fire).
///
/// If the atom has unbound variables at this point (a safety
/// violation), any match on the bound positions is enough to answer
/// "match found" — a conservative approach that preserves soundness
/// even when a caller writes a range-unsafe rule.
fn negation_matches(atom: &Atom, db: &Database, binding: &Binding) -> bool {
    let empty: TupleSet = HashSet::new();
    let candidates = db
        .tables
        .get(&(atom.predicate.clone(), atom.arity()))
        .unwrap_or(&empty);
    for tuple in candidates {
        if unify_atom(atom, tuple, binding).is_some() {
            return true;
        }
    }
    false
}

/// Extend a set of partial bindings by matching `atom` against every
/// tuple of its relation in `db`.
fn extend_with_db(atom: &Atom, db: &Database, bindings: Vec<Binding>) -> Vec<Binding> {
    let empty: TupleSet = HashSet::new();
    let candidates = db
        .tables
        .get(&(atom.predicate.clone(), atom.arity()))
        .unwrap_or(&empty);
    let mut out = Vec::new();
    for binding in &bindings {
        for tuple in candidates {
            if let Some(extended) = unify_atom(atom, tuple, binding) {
                out.push(extended);
            }
        }
    }
    out
}

/// Unify an atom's term list against a concrete tuple, extending
/// `base`. Returns `None` on any conflict (constant mismatch or a
/// variable already bound to a different value).
fn unify_atom(atom: &Atom, tuple: &[Value], base: &Binding) -> Option<Binding> {
    if atom.terms.len() != tuple.len() {
        return None;
    }
    let mut b = base.clone();
    for (term, val) in atom.terms.iter().zip(tuple.iter()) {
        match term {
            Term::Const(c) => {
                if c != val {
                    return None;
                }
            }
            Term::Var(name) => match b.get(name) {
                Some(existing) if existing != val => return None,
                Some(_) => {}
                None => {
                    b.insert(name.clone(), val.clone());
                }
            },
            Term::Bound(_) => {
                // Bound terms are rejected up front by `reject_bound`;
                // if one slips through we treat it as no-match to avoid
                // fabricating a result.
                return None;
            }
        }
    }
    Some(b)
}

/// Apply a binding to an atom, producing a ground tuple. Returns
/// `None` if any variable was not bound (violation of range
/// restriction).
fn ground_atom(atom: &Atom, binding: &Binding) -> Option<Vec<Value>> {
    let mut out = Vec::with_capacity(atom.terms.len());
    for term in &atom.terms {
        match term {
            Term::Const(v) => out.push(v.clone()),
            Term::Var(name) => match binding.get(name) {
                Some(v) => out.push(v.clone()),
                None => return None,
            },
            Term::Bound(_) => return None,
        }
    }
    Some(out)
}
