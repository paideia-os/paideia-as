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

use crate::ast::{Atom, Program, Query, Rule, Term, Value};
use crate::magic_sets;
use std::collections::{HashMap, HashSet};

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
    pub fn from_program(program: &Program) -> Result<Self, EvalError> {
        // Refuse programs that mention pipeline interpolation — the
        // shell round that resolves `$expr` (R226.M8 + R229) is not
        // landed yet.
        for atom in &program.facts {
            reject_bound(atom)?;
        }
        for rule in &program.rules {
            reject_bound(&rule.head)?;
            for body in &rule.body {
                reject_bound(body)?;
            }
        }

        // Seed the EDB.
        let mut db = Database::default();
        for atom in &program.facts {
            let tuple = atom
                .as_ground()
                .expect("parser guaranteed fact atom is ground");
            db.insert(atom.predicate.clone(), tuple);
        }

        // Empty program with no rules — nothing further to derive.
        if program.rules.is_empty() {
            return Ok(db);
        }

        // First-round Δ is the EDB itself (every base tuple is "new").
        let mut delta_prev: HashMap<Key, TupleSet> = db.tables.clone();

        loop {
            let mut delta_new: HashMap<Key, TupleSet> = HashMap::new();
            for rule in &program.rules {
                derive_round(rule, &db, &delta_prev, &mut delta_new);
            }
            // Remove tuples already present in db so we do not "grow"
            // Δ with re-derivations.
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
            // Merge Δ into db, and Δ becomes the next round's Δ_prev.
            for (key, set) in &delta_new {
                let table = db.tables.entry(key.clone()).or_default();
                for t in set {
                    table.insert(t.clone());
                }
            }
            delta_prev = delta_new;
        }

        Ok(db)
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

/// One seminaïve round for one rule: for each body-atom pivot position
/// `i`, match `body[i]` against `delta_prev` and every `body[j != i]`
/// against `db`, collecting derived head tuples into `delta_new`.
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
    for pivot in 0..rule.body.len() {
        let empty: TupleSet = HashSet::new();
        let pivot_tuples = delta_prev
            .get(&(rule.body[pivot].predicate.clone(), rule.body[pivot].arity()))
            .unwrap_or(&empty);
        for tuple in pivot_tuples {
            let Some(base_binding) = unify_atom(&rule.body[pivot], tuple, &Binding::new()) else {
                continue;
            };
            let mut bindings = vec![base_binding];
            let mut ok = true;
            for (j, body_atom) in rule.body.iter().enumerate() {
                if j == pivot {
                    continue;
                }
                bindings = extend_with_db(body_atom, db, bindings);
                if bindings.is_empty() {
                    ok = false;
                    break;
                }
            }
            if !ok {
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
