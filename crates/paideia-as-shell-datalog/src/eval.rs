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

use crate::aggregation::{self, AggregateResult, AggregationError};
use crate::ast::{AggregateQuery, Atom, BodyGoal, Program, Query, Rule, Term, Value};
use crate::fingerprint::{FingerprintSink, NullSink, QueryId};
use crate::magic_sets;
use crate::progress::{NullProgressSink, ProgressSink};
use crate::session_edb::SessionEdb;
use crate::stratification::{self, StratificationError};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

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
        // Non-Evaluator entry point — no injected progress sink, so
        // ticks go to the discarding [`NullProgressSink`]. Callers who
        // want to observe iteration progress must build an
        // [`Evaluator`] with [`Evaluator::with_progress_sink`] and run
        // through its query methods instead.
        evaluate_stratified(program, &NullProgressSink)
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

/// Query-driving façade over the module's free functions plus the
/// R226.M4 magic-set path.
///
/// # State (R226.M11 + R226.M10)
///
/// * `next_query_id` — a monotone per-evaluator counter used to
///   fingerprint each completed query. Two evaluators built via
///   [`Evaluator::new`] or [`Evaluator::default`] have independent
///   counters (both start at 0); a single evaluator threaded through a
///   REPL sees `0`, `1`, `2`, … across successive queries.
/// * `fingerprint_sink` — an injected [`FingerprintSink`] the
///   evaluator emits `dlg.<hex-id>.<result-count>` into after each
///   completed query. Defaults to [`NullSink`] so pre-M11 call sites
///   see no observable behaviour change; a caller who wants the
///   emissions supplies a real sink via
///   [`Evaluator::with_fingerprint_sink`].
/// * `progress_sink` — an injected [`ProgressSink`] the evaluator
///   ticks into once per seminaïve fixpoint iteration
///   (see the [`crate::fingerprint`] module doc for the tick semantics).
///   Defaults to [`NullProgressSink`] so pre-M10 call sites see no
///   observable behaviour change; a caller who wants the stream
///   supplies a real sink via [`Evaluator::with_progress_sink`].
///
/// The two sinks are independent — a caller may attach one without the
/// other. `Database::from_program` (the non-Evaluator entry point) uses
/// a silent sink for both.
///
/// # Concurrency
///
/// A single `Evaluator` is shared across threads through an `Arc` in
/// the wider shell. The counter is an [`AtomicU64`] and the sinks
/// carry a `Send + Sync` bound — a `&self` method can be called from
/// multiple threads concurrently. The counter's fetch-add uses
/// [`Ordering::Relaxed`]: fingerprint ids need to be unique per
/// evaluator (which relaxed fetch-add guarantees) but do not need to
/// participate in cross-thread happens-before ordering with anything
/// else — the sink is the visible boundary, and its own
/// synchronisation covers ordering of emitted tags.
///
/// # Error paths are silent
///
/// Fingerprints are emitted **only** on successful completion. Bound-
/// term rejection, [`EvalError::UnstratifiedNegation`], and
/// [`EvalError::AggregationError`] all return early without touching
/// the sink — a fingerprint therefore serves as proof of completion,
/// not merely of attempt.
///
/// Progress ticks are streamed *during* evaluation, so they may fire
/// before a subsequent failure (a program that stratifies but hits an
/// aggregation error, say, will already have ticked through its
/// fixpoint by the time the aggregator complains). Callers correlate
/// completion against fingerprints and per-step progress against ticks
/// — the two are deliberately different lifecycles.
pub struct Evaluator {
    next_query_id: AtomicU64,
    fingerprint_sink: Box<dyn FingerprintSink + Send + Sync>,
    progress_sink: Box<dyn ProgressSink + Send + Sync>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Evaluator {
    /// The sink is a trait object with no `Debug` bound, so we render
    /// only its presence — a caller inspecting the evaluator only
    /// cares about the counter's current value and that a sink exists
    /// at all (the concrete implementation is opaque by design).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Evaluator")
            .field("next_query_id", &self.next_query_id.load(Ordering::Relaxed))
            .field("fingerprint_sink", &"<dyn FingerprintSink>")
            .field("progress_sink", &"<dyn ProgressSink>")
            .finish()
    }
}

impl Evaluator {
    /// Fresh evaluator with a discarding [`NullSink`], a discarding
    /// [`NullProgressSink`], and a query counter at 0. Identical to
    /// [`Evaluator::default`]; preserved because "new evaluator, then
    /// use it" reads more naturally at most call sites than the
    /// `Default` trait detour.
    pub fn new() -> Self {
        Self {
            next_query_id: AtomicU64::new(0),
            fingerprint_sink: Box::new(NullSink),
            progress_sink: Box::new(NullProgressSink),
        }
    }

    /// Builder-style replacement of the fingerprint sink.
    ///
    /// The counter is preserved across the swap — a caller that
    /// upgrades from a silent to a collecting sink mid-session does
    /// not silently reset the ids that later fingerprints depend on
    /// for uniqueness. A caller that wants a fresh id sequence should
    /// build a fresh evaluator instead.
    ///
    /// Leaves the progress sink untouched — the two channels are
    /// independent; wire each one explicitly.
    pub fn with_fingerprint_sink(
        mut self,
        sink: Box<dyn FingerprintSink + Send + Sync>,
    ) -> Self {
        self.fingerprint_sink = sink;
        self
    }

    /// Builder-style replacement of the progress sink.
    ///
    /// Companion to [`Self::with_fingerprint_sink`]: swap in a real
    /// sink to observe per-iteration progress; leave it defaulted to
    /// suppress the stream entirely. Chainable: `Evaluator::new()
    /// .with_fingerprint_sink(...).with_progress_sink(...)` is the
    /// intended shape for a REPL that wants both channels.
    ///
    /// Leaves the fingerprint sink and query counter untouched — the
    /// two channels are independent.
    pub fn with_progress_sink(
        mut self,
        sink: Box<dyn ProgressSink + Send + Sync>,
    ) -> Self {
        self.progress_sink = sink;
        self
    }

    /// Consume and return the next fingerprint id, advancing the
    /// counter by one. Ordering is [`Ordering::Relaxed`] — see the
    /// struct-level concurrency note.
    ///
    /// Exposed for tests and for callers that want to correlate an
    /// out-of-band trace event with the id an upcoming query will
    /// carry. Ordinary evaluator users never need to call it directly
    /// — the query paths call it themselves before emitting.
    pub fn next_query_id(&self) -> QueryId {
        QueryId(self.next_query_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Emit a completed query's fingerprint. `id` must have been
    /// minted by a preceding [`Self::next_query_id`] on THIS
    /// evaluator; `result_count` is the summary scalar (substitution
    /// count, group count, or total tuple count depending on the query
    /// shape — see the [`crate::fingerprint`] module doc).
    ///
    /// Kept private so every emission call site in this module goes
    /// through the same formatter — a future format change touches one
    /// line rather than six.
    fn emit_fingerprint(&self, id: QueryId, result_count: usize) {
        self.fingerprint_sink.emit(&id.format_tag(result_count));
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
        // R226.M10 — route through the progress-aware pipeline so
        // per-iteration ticks reach `self.progress_sink`. The base
        // `Database::from_program` uses a silent sink and is
        // unchanged, so non-Evaluator callers keep their behaviour.
        let db = evaluate_stratified(program, self.progress_sink.as_ref())?;
        let bindings = crate::eval::query(&db, query)?;
        // R226.M11 — fingerprint the completed query. Id is minted
        // AFTER the fallible work so that a failure to materialise the
        // fixpoint or evaluate the query does not burn a slot in the
        // counter for an emission that never happens; the counter
        // then reflects "successful queries" 1:1, which is what the
        // correlator wants to attribute results against.
        let id = self.next_query_id();
        self.emit_fingerprint(id, bindings.len());
        Ok(bindings)
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
        let db = evaluate_stratified(program, self.progress_sink.as_ref())?;
        // R226.M11 — `run_stratified` returns a materialised database
        // rather than a substitution set, so the scalar summary is the
        // total tuple count across every predicate the fixpoint
        // produced. Same id-after-success discipline as `run_query`.
        let id = self.next_query_id();
        self.emit_fingerprint(id, db.total_tuple_count());
        Ok(db)
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
        // R226.M10 — route the rewritten program's fixpoint through the
        // Evaluator's progress sink; the M4 rewrite is answer-preserving
        // but changes the intermediate DB size, so a caller observing
        // progress here sees exactly the iteration profile magic-set
        // rewriting produced.
        let db = evaluate_stratified(&rewritten, self.progress_sink.as_ref())?;
        let bindings = crate::eval::query(&db, query)?;
        // R226.M11 — the magic-set path answers the same query as
        // `run_query` and by construction returns the same
        // substitution set, so the fingerprint's `result_count` field
        // matches the naïve path bit-for-bit; only the query id
        // distinguishes them in the correlator log.
        let id = self.next_query_id();
        self.emit_fingerprint(id, bindings.len());
        Ok(bindings)
    }

    /// Materialize `program` (under stratified negation if needed),
    /// enumerate every substitution that satisfies `query.goals`, and
    /// reduce the substitution set under `query.agg` on
    /// `query.target_var`, partitioned by `query.group_by`.
    ///
    /// The returned map keys on the group-by tuple (empty for
    /// ungrouped queries) and values on the per-group
    /// [`AggregateResult`]. See [`crate::aggregation`] for empty-
    /// input, grouping, and numeric-coercion rules.
    ///
    /// # Error routes
    ///
    /// * [`EvalError::UnresolvedPipelineValue`] — a body atom or the
    ///   program itself mentions `$name` (pipeline interpolation);
    ///   the R229 resolver is not landed yet.
    /// * [`EvalError::UnstratifiedNegation`] — the program's
    ///   dependency graph carries a cycle through negation; the
    ///   stratifier rejects it before any tuple is derived.
    /// * [`EvalError::AggregationError`] — a non-numeric target value
    ///   was observed while running `sum` or `avg`.
    pub fn run_aggregate_query(
        &self,
        program: &Program,
        query: &AggregateQuery,
    ) -> Result<HashMap<Vec<Value>, AggregateResult>, EvalError> {
        // Refuse pipeline interpolation up front — mirrors the query
        // path, so both entry points agree on the deferral.
        for goal in &query.goals {
            reject_bound(goal.atom())?;
        }
        // Materialize the fixpoint (stratified path handles both the
        // negation-free and the negation-bearing cases). Routed through
        // `self.progress_sink` so R226.M10 ticks reach the caller even
        // on the aggregate path.
        let db = evaluate_stratified(program, self.progress_sink.as_ref())?;
        // Enumerate every substitution satisfying the body — same
        // positive-then-negative pass order the fixpoint uses inside
        // `derive_round`, so aggregation and derivation agree on
        // which bindings are "valid" for a given body.
        let substitutions = enumerate_body(&db, &query.goals);
        let groups = aggregation::evaluate(
            query.agg,
            &query.target_var,
            &query.group_by,
            &substitutions,
        )
        .map_err(EvalError::AggregationError)?;
        // R226.M11 — aggregate `result_count` is the number of
        // groups. Ungrouped queries always emit exactly one row (the
        // empty-key group), so the fingerprint reads `.1` for them
        // rather than the substitution count; group-by queries emit
        // the group cardinality, which is the scalar a caller
        // benchmarking a group-by rewrite would most want to see.
        let id = self.next_query_id();
        self.emit_fingerprint(id, groups.len());
        Ok(groups)
    }

    // ------------------------------------------------------------------
    // R226.M8 — session-local EDB overlay entry points.
    //
    // Each `_with_session` variant mirrors its base counterpart but
    // seeds the seminaïve fixpoint with `session`'s tuples *in addition
    // to* `program.facts` before iterating. Session facts are treated
    // as extensional (they participate as first-round Δ pivots the way
    // program facts do) so any IDB rule can derive from them.
    //
    // No new error routes: an unstratifiable program is still rejected
    // by the stratifier before any tuple is seeded, and a `Term::Bound`
    // in the program is still refused up front by `reject_bound`.
    // ------------------------------------------------------------------

    /// R226.M8 — run [`run_query`](Self::run_query) with a session EDB
    /// overlay merged into the initial database.
    ///
    /// The overlay is additive: identical tuples in `program.facts` and
    /// `session` deduplicate through the underlying `HashSet`. IDB
    /// rules derive from session facts just as they do from program
    /// facts — the evaluator draws no distinction once seeded.
    ///
    /// A `session` that [`SessionEdb::is_empty`] is a valid input and
    /// produces exactly the answer set [`run_query`](Self::run_query)
    /// would.
    pub fn run_query_with_session(
        &self,
        program: &Program,
        query: &Query,
        session: &SessionEdb,
    ) -> Result<Vec<Binding>, EvalError> {
        let db = evaluate_stratified_with_session(
            program,
            session,
            self.progress_sink.as_ref(),
        )?;
        let bindings = crate::eval::query(&db, query)?;
        // R226.M11 — session-overlay path shares the substitution
        // shape of `run_query`, so `bindings.len()` is the natural
        // scalar. Same id-after-success discipline as elsewhere.
        let id = self.next_query_id();
        self.emit_fingerprint(id, bindings.len());
        Ok(bindings)
    }

    /// R226.M8 — run [`run_stratified`](Self::run_stratified) with a
    /// session EDB overlay merged into the initial database.
    ///
    /// Returns the materialised [`Database`] (session facts + program
    /// facts + every IDB tuple derived from either). The caller then
    /// runs [`query`] against it directly.
    pub fn run_stratified_with_session(
        &self,
        program: &Program,
        session: &SessionEdb,
    ) -> Result<Database, EvalError> {
        let db = evaluate_stratified_with_session(
            program,
            session,
            self.progress_sink.as_ref(),
        )?;
        // R226.M11 — stratified-with-session mirrors `run_stratified`
        // in shape: no query is run, only a DB is materialised, so the
        // scalar summary is the total tuple count across every
        // materialised relation.
        let id = self.next_query_id();
        self.emit_fingerprint(id, db.total_tuple_count());
        Ok(db)
    }

    /// R226.M8 — run [`run_aggregate_query`](Self::run_aggregate_query)
    /// with a session EDB overlay merged into the initial database.
    ///
    /// Aggregation runs over the substitution set produced by the
    /// merged DB — so session facts count toward `count`, `sum`, and
    /// friends exactly the way program facts do.
    pub fn run_aggregate_query_with_session(
        &self,
        program: &Program,
        query: &AggregateQuery,
        session: &SessionEdb,
    ) -> Result<HashMap<Vec<Value>, AggregateResult>, EvalError> {
        for goal in &query.goals {
            reject_bound(goal.atom())?;
        }
        let db = evaluate_stratified_with_session(
            program,
            session,
            self.progress_sink.as_ref(),
        )?;
        let substitutions = enumerate_body(&db, &query.goals);
        let groups = aggregation::evaluate(
            query.agg,
            &query.target_var,
            &query.group_by,
            &substitutions,
        )
        .map_err(EvalError::AggregationError)?;
        // R226.M11 — group cardinality, same scalar as the session-
        // free aggregate path.
        let id = self.next_query_id();
        self.emit_fingerprint(id, groups.len());
        Ok(groups)
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
    /// R226.M6 aggregation failure — carries the underlying
    /// [`AggregationError`] (currently only
    /// [`AggregationError::NonNumericTarget`]) so a REPL can render
    /// the offending value without unwrapping a boxed error.
    ///
    /// Wrapping (rather than flattening the inner variants into
    /// `EvalError` directly) keeps the aggregation-only failure modes
    /// namespaced under their own module — a future non-numeric-avg
    /// distinct-from-non-numeric-sum split, for instance, does not
    /// reshape `EvalError`.
    AggregationError(AggregationError),
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
/// its EDB. Called by both [`Database::from_program`] and every
/// [`Evaluator`] method that materialises a fixpoint — the two entry
/// points share the same pipeline; only the injected `progress` sink
/// differs (silent for `Database::from_program`, the evaluator's own
/// sink for the `Evaluator::*` methods).
///
/// Progress emission (R226.M10):
///
/// * A stratum with any prior-round tuples emits at least one tick
///   (see [`seminaive_fixpoint`] for the tick contract).
/// * A rules-free program short-circuits before entering any stratum;
///   a single seed tick `(1, 0, |facts|)` is emitted iff the program
///   supplied any facts, so a "facts-only" query still receives one
///   progress event while a genuinely empty program (no facts, no
///   rules) emits nothing — there is no work to report on.
fn evaluate_stratified(
    program: &Program,
    progress: &dyn ProgressSink,
) -> Result<Database, EvalError> {
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
        // R226.M10 — no rules means no fixpoint runs; emit a single
        // seed tick so a facts-only program still reports its EDB
        // size to a subscribed caller. An empty program (no facts
        // either) emits nothing — there is no work to report on.
        emit_seed_tick_if_nonempty(&db, progress);
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
    // The progress sink is shared across strata; the iteration counter
    // inside [`seminaive_fixpoint`] restarts per stratum so a caller
    // can attribute a slow stratum to its own iteration count rather
    // than reading a monotonic global.
    for (_stratum, rules) in &rules_by_stratum {
        seminaive_fixpoint(rules, &mut db, progress);
    }

    Ok(db)
}

/// R226.M10 — emit a single seed tick reflecting the current DB size,
/// but only if the DB has any tuples. Called by the rules-free paths
/// of [`evaluate_stratified`] and [`evaluate_stratified_with_session`]
/// so a facts-only program still reports its EDB size to a subscribed
/// caller while a genuinely empty program (no facts either) emits
/// nothing — matching the "work to report on" contract in the
/// [`crate::progress`] module doc.
fn emit_seed_tick_if_nonempty(db: &Database, progress: &dyn ProgressSink) {
    let total = db.total_tuple_count();
    if total > 0 {
        progress.tick(1, 0, total);
    }
}

/// R226.M8 — stratified evaluation with an additional session-EDB
/// overlay seeded before the fixpoint runs.
///
/// Same pipeline as [`evaluate_stratified`] (reject-bound, stratify,
/// seed EDB, seminaïve per stratum) with one added step between "seed"
/// and "seminaïve": every `(predicate, arity) → HashSet<tuple>` entry
/// in `session` is inserted into `db`. Session tuples are treated as
/// extensional — they participate as first-round Δ pivots the way
/// program facts do, so any IDB rule can derive from them.
///
/// The two evaluation paths (with vs. without session) share every
/// derivation invariant: stratification order, negation stratum
/// closure, safety-condition drops. The only observable difference is
/// the EDB the fixpoint starts from.
///
/// R226.M10 progress emission is threaded through identically to the
/// non-session path — the shared [`seminaive_fixpoint`] driver ticks
/// once per iteration into the injected sink.
fn evaluate_stratified_with_session(
    program: &Program,
    session: &SessionEdb,
    progress: &dyn ProgressSink,
) -> Result<Database, EvalError> {
    // Refuse programs that mention pipeline interpolation — identical
    // policy to `evaluate_stratified`. Session tuples are already
    // ground `Vec<Value>`s (no `Term::Bound` possible), so no separate
    // check is needed for the overlay.
    for atom in &program.facts {
        reject_bound(atom)?;
    }
    for rule in &program.rules {
        reject_bound(&rule.head)?;
        for body in &rule.body {
            reject_bound(body.atom())?;
        }
    }

    let strata = stratification::compute_strata(program).map_err(|e| match e {
        StratificationError::CycleThroughNegation { cycle } => {
            EvalError::UnstratifiedNegation { cycle }
        }
    })?;

    // Seed EDB with program facts + session overlay. Order is
    // irrelevant — both go through `Database::insert`, which
    // deduplicates through the underlying `HashSet`.
    let mut db = Database::default();
    for atom in &program.facts {
        let tuple = atom
            .as_ground()
            .expect("parser guaranteed fact atom is ground");
        db.insert(atom.predicate.clone(), tuple);
    }
    for ((pred, _arity), set) in session.facts_ref() {
        for tuple in set {
            db.insert(pred.clone(), tuple.clone());
        }
    }

    if program.rules.is_empty() {
        // Same seed-tick discipline as the session-free path.
        emit_seed_tick_if_nonempty(&db, progress);
        return Ok(db);
    }

    let mut rules_by_stratum: BTreeMap<usize, Vec<&Rule>> = BTreeMap::new();
    for rule in &program.rules {
        let s = strata.get(&rule.head.predicate).copied().unwrap_or(0);
        rules_by_stratum.entry(s).or_default().push(rule);
    }

    for (_stratum, rules) in &rules_by_stratum {
        seminaive_fixpoint(rules, &mut db, progress);
    }

    Ok(db)
}

/// One seminaïve fixpoint over `rules` starting from `db`'s current
/// contents. Extracted so [`evaluate_stratified`] can call it once per
/// stratum, giving negation stratified semantics for free — a negated
/// body atom in stratum `k` always resolves against the completed
/// `db` of strata `< k` (its predicate is guaranteed by the stratifier
/// to sit strictly below `k`).
///
/// # R226.M10 tick contract
///
/// After each round merges its delta into `db`, this driver emits one
/// tick `(iteration, delta_count, db.total_tuple_count())` into
/// `progress`. The iteration counter is 1-based and monotonic within
/// one call (a multi-stratum program restarts the counter per stratum
/// because each stratum makes a fresh call). The terminating round —
/// the first one whose merged delta is empty — still ticks, so a
/// caller always sees at least one event as long as the loop entered
/// (which requires either non-empty `delta_prev`, i.e. any tuples in
/// `db` when the fixpoint started, or a rule capable of firing without
/// bindings — the current pivot loop requires the former).
fn seminaive_fixpoint(rules: &[&Rule], db: &mut Database, progress: &dyn ProgressSink) {
    // First-round Δ is the base DB — every existing tuple is "new"
    // from this stratum's perspective (its rules have never fired
    // before). Subsequent rounds only see the strictly-fresh tuples
    // from the previous round.
    let mut delta_prev: HashMap<Key, TupleSet> = db.tables.clone();
    let mut iter_count: usize = 0;
    loop {
        let mut delta_new: HashMap<Key, TupleSet> = HashMap::new();
        for rule in rules {
            derive_round(rule, db, &delta_prev, &mut delta_new);
        }
        // Filter against the existing DB and count the strictly-new
        // tuples so the tick's `delta_tuples` reports what actually
        // grew the DB this round (not gross derivations, which double-
        // count any tuple two rules produced independently).
        let mut delta_count: usize = 0;
        for (key, set) in delta_new.iter_mut() {
            if let Some(existing) = db.tables.get(key) {
                set.retain(|t| !existing.contains(t));
            }
            delta_count += set.len();
        }
        // Merge before ticking so `db.total_tuple_count()` reflects the
        // post-merge state — the value a caller would observe if the
        // fixpoint halted at this tick.
        for (key, set) in &delta_new {
            let table = db.tables.entry(key.clone()).or_default();
            for t in set {
                table.insert(t.clone());
            }
        }
        iter_count += 1;
        progress.tick(iter_count, delta_count, db.total_tuple_count());
        if delta_count == 0 {
            break;
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

/// Enumerate every substitution that satisfies a rule-body-shaped
/// conjunction against `db`. Same positive-then-negative pass order
/// as [`derive_round`] so aggregation and derivation agree on which
/// bindings a given body admits.
///
/// * Start with a single empty binding.
/// * For each positive body goal in source order, extend by matching
///   against the tuples in the corresponding relation.
/// * For each negative body goal (applied *after* the positive pass,
///   so every variable in the negated atom is bound), retain only
///   bindings whose substitution finds NO matching tuple.
///
/// A body with no positive goals produces the singleton empty
/// binding vector (there is no way to bind any variable, so the
/// negative-only filter is a range-restriction violation — the
/// aggregator's group-by check subsequently drops such bindings).
///
/// R226.M6-only helper; the fixpoint pivot loop uses its own per-
/// pivot enumeration inside [`derive_round`] and cannot re-use this
/// function directly (the pivot pins one body atom to `delta_prev`
/// instead of `db`).
pub(crate) fn enumerate_body(db: &Database, goals: &[BodyGoal]) -> Vec<Binding> {
    let mut bindings = vec![Binding::new()];
    for goal in goals {
        if let BodyGoal::Positive(a) = goal {
            bindings = extend_with_db(a, db, bindings);
            if bindings.is_empty() {
                return bindings;
            }
        }
    }
    for goal in goals {
        if let BodyGoal::Negative(a) = goal {
            bindings.retain(|b| !negation_matches(a, db, b));
            if bindings.is_empty() {
                break;
            }
        }
    }
    bindings
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
