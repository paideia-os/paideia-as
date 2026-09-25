//! R226.M4 magic-set rewriting (Beeri & Ramakrishnan 1991, §4.3).
//!
//! # What magic-set rewriting is for
//!
//! Bottom-up seminaïve evaluation (see [`crate::eval`]) computes every
//! derivable tuple of every IDB relation, whether or not the current
//! query cares about it. On a 10⁶-node graph running
//! `ancestor(alice, ?)`, that means materializing every ancestor pair
//! in the closure — 10¹² tuples in the worst case — when only the
//! `alice`-rooted slice ever affects the answer. Magic-set rewriting
//! reshapes the program so the fixpoint restricts itself to *just* the
//! tuples the query can consume, closing the gap between top-down
//! selectivity (goal-directed like Prolog) and bottom-up termination
//! (Datalog fixpoint).
//!
//! # Algorithm (supplementary-magic variant)
//!
//! Given a program `P` and a query `Q = p(t₁, …, tₙ)`:
//!
//! 1. **Adornment.** The query fixes an *adornment* on `p` — one flag
//!    per argument position: `b` (bound, the query supplied a constant
//!    there) or `f` (free). We BFS from `(p, α)` over IDB predicates,
//!    propagating adornments to body atoms using left-to-right
//!    Sideways Information Passing (SIP): at each body position, an
//!    argument is bound iff it is a constant or its variable already
//!    appeared in the head (in a bound position) or in some earlier
//!    body atom.
//!
//! 2. **Magic-guarded rules.** For each adorned rule
//!    `p^α(…) :- b₁, b₂, …, bₙ.`, prepend a magic-predicate goal to
//!    the body:
//!    `p(…) :- magic_p_α(bound_head_args), b₁, b₂, …, bₙ.`
//!    Rule heads keep the original predicate name (this is the
//!    "supplementary magic" simplification of Beeri-Ramakrishnan §4.3
//!    — correct, and simpler than fully adorning heads); the guard
//!    restricts the substitutions the rule ever considers to those
//!    the query cares about.
//!
//! 3. **Supplementary magic rules.** For each IDB body atom
//!    `bᵢ^βᵢ` at position `i` of an adorned rule, emit
//!    `magic_bᵢ_βᵢ(bound_bᵢ_args) :- magic_p_α(bound_head_args), b₁, …, b_{i-1}.`
//!    so the magic set of `bᵢ` grows exactly with the substitutions
//!    the SIP hands it. EDB atoms need no supplementary rule (their
//!    tuples are ground facts already).
//!
//! 4. **Query seed.** Emit `magic_p_α(v₁, …, vₖ).` as a ground fact,
//!    where `v₁ … vₖ` are the constants at the query's bound
//!    positions. This is the base case that ignites the fixpoint.
//!
//! # Why "supplementary" and not "full" adornment
//!
//! Full Beeri-Ramakrishnan also renames rule heads (`p^bf(…)` becomes
//! its own relation, distinct from `p^fb(…)`). Supplementary magic
//! collapses those into the single relation `p` — correct because the
//! magic guard restricts *which* rule instances fire, and slightly
//! less selective across queries that touch the same predicate under
//! multiple adornments (very rare in the shell's use cases). The
//! simpler shape keeps the [`Evaluator::run_query_via_magic_sets`]
//! helper's post-condition trivial: the original query runs against
//! the original predicate name, no re-naming at the query boundary.
//!
//! [`Evaluator::run_query_via_magic_sets`]: crate::eval::Evaluator::run_query_via_magic_sets
//!
//! # Degenerate cases
//!
//! * **Empty query.** No goal → nothing to rewrite; return `program`
//!   unchanged.
//! * **All-free query** (`p(?X, ?Y)`). Every tuple of `p` matters, so
//!   magic sets cannot restrict anything — return `program` unchanged.
//!   The [`Evaluator::run_query_via_magic_sets`] helper still runs
//!   correctly (its fixpoint = the naïve fixpoint), just with no
//!   speedup.
//! * **Ground-fact query** (predicate has no rules — EDB only). The
//!   BFS finds no rule heads matching the query predicate, so no
//!   guarded rules are emitted. The seed magic fact is still added
//!   (harmless, unused), the original facts survive, and the query
//!   returns the same answer as the naïve path.
//! * **Arity-0 adornment on a body atom.** If SIP leaves an IDB body
//!   atom with *no* bound arguments, the "magic" guard would have
//!   arity 0. We simply omit the guard for that atom — its magic
//!   filter would be constant-true anyway, so it degenerates to the
//!   unguarded rule (correct, just no filter for that atom).
//!
//! # Termination and complexity
//!
//! The adornment BFS terminates because there are only finitely many
//! `(predicate, adornment)` pairs (the powerset of argument positions
//! per predicate). The rewritten program is Datalog (no function
//! symbols), so the downstream seminaïve fixpoint terminates on its
//! finite Herbrand base. Rewriting is O(|rules| × |adorned| × |body|)
//! in program size; on the M1/M2 corpus (< 20 rules) this is
//! irrelevant, on the R226.M4-targeted 10⁶-node graphs it is still
//! negligible compared to the fixpoint itself.
//!
//! # Fingerprints
//!
//! The test corpus tags each fixture with `r226m4-magic-NN` so the
//! R220.M10 `@fingerprint` correlator can attribute pass/fail without
//! re-parsing the test name.

use crate::ast::{Atom, BodyGoal, Program, Query, Rule, Term, Value};
use std::collections::{HashSet, VecDeque};

/// Adornment on a predicate occurrence: `true` per position that the
/// SIP has bound (either a query constant, or a variable inherited
/// from the head's bound positions or from a preceding body atom).
type Adornment = Vec<bool>;

/// Rewrite `program` for the specific `query` using the Beeri-Ramakrishnan
/// magic-set algorithm (see module documentation for the full picture).
///
/// The result is a fresh `Program` — original rules, guarded copies,
/// supplementary magic rules, and the query-seed magic fact all live
/// in the returned value. The input `program` is untouched, so a REPL
/// can cache the original AST and rewrite once per query without
/// losing the naïve baseline.
///
/// # Correctness
///
/// For every substitution σ that the naïve fixpoint would derive
/// against the original program for `query`, the rewritten program's
/// fixpoint derives the same σ. Extra magic-predicate tuples appear
/// in the DB but never affect the query result — the query is stated
/// against the original predicate names, and those magic predicates
/// are simply not among the goal atoms.
///
/// # Selectivity
///
/// When the query has at least one bound argument and the target
/// predicate is IDB, the rewritten program's DB is (weakly) smaller
/// than the naïve DB, often dramatically so. On the R226.M4
/// test corpus, `r226m4-magic-04` and `r226m4-magic-05` assert this
/// numerically.
pub fn rewrite(program: &Program, query: &Query) -> Program {
    // Empty query — nothing to do.
    if query.goals.is_empty() {
        return program.clone();
    }

    // Only the first goal is the magic target; extra goals become
    // additional filter atoms the caller's `query()` step evaluates on
    // top of the fixpoint DB. This matches how BR91 §4.3 states the
    // rewriting for a single "query predicate."
    let q_head = &query.goals[0];
    let q_adornment: Adornment = q_head
        .terms
        .iter()
        .map(|t| matches!(t, Term::Const(_)))
        .collect();

    // No bound positions → magic sets cannot restrict anything; the
    // "rewrite" is trivially the original program. See module doc.
    if !q_adornment.iter().any(|&b| b) {
        return program.clone();
    }

    // IDB predicates: those that appear as a rule head. EDB predicates
    // (only in facts / only in bodies) need no supplementary magic —
    // their tuples are already ground.
    let idb: HashSet<String> = program.rules.iter().map(|r| r.head.predicate.clone()).collect();

    // ----------------------------------------------------------------
    // Step 1 — adornment BFS.
    // ----------------------------------------------------------------
    let mut adorned: HashSet<(String, Adornment)> = HashSet::new();
    let mut worklist: VecDeque<(String, Adornment)> = VecDeque::new();
    worklist.push_back((q_head.predicate.clone(), q_adornment.clone()));

    while let Some((pred, adr)) = worklist.pop_front() {
        if !adorned.insert((pred.clone(), adr.clone())) {
            continue;
        }
        for rule in &program.rules {
            if rule.head.predicate != pred || rule.head.terms.len() != adr.len() {
                continue;
            }
            propagate_sip(rule, &adr, &idb, |bi_pred, bi_beta| {
                worklist.push_back((bi_pred, bi_beta));
            });
        }
    }

    // ----------------------------------------------------------------
    // Step 2 + 3 — rewrite rules and emit supplementary magic rules.
    // ----------------------------------------------------------------
    let mut new_rules: Vec<Rule> = Vec::new();
    let mut new_facts: Vec<Atom> = program.facts.clone();

    for rule in &program.rules {
        // Collect the adornments that apply to this rule's head.
        // We rewrite once per (rule, adornment) pair so a predicate
        // used under multiple adornments produces multiple guarded
        // rule instances (each with its own magic filter).
        let head_adornments: Vec<&Adornment> = adorned
            .iter()
            .filter(|(p, a)| p == &rule.head.predicate && a.len() == rule.head.terms.len())
            .map(|(_, a)| a)
            .collect();

        for adr in head_adornments {
            let magic_guard = build_magic_atom(&rule.head, adr);

            // Guarded rewrite of the original rule — the magic guard
            // is a fresh positive body goal; the original body's
            // polarities (positive OR negative) pass through unchanged.
            let mut new_body: Vec<BodyGoal> = Vec::new();
            if let Some(g) = &magic_guard {
                new_body.push(BodyGoal::Positive(g.clone()));
            }
            new_body.extend(rule.body.iter().cloned());
            new_rules.push(Rule {
                head: rule.head.clone(),
                body: new_body,
            });

            // Supplementary magic rules — one per IDB *positive* body
            // atom whose adornment has at least one bound position.
            // Negated body atoms are skipped here: they cannot adorn
            // (a `not p(...)` conjunct doesn't bind variables) and
            // their filter role belongs to the guarded rule above,
            // not to any supplementary magic rule.
            let mut bound_vars: HashSet<String> = HashSet::new();
            for (i, term) in rule.head.terms.iter().enumerate() {
                if adr[i] {
                    if let Term::Var(name) = term {
                        bound_vars.insert(name.clone());
                    }
                }
            }
            let mut prev_body: Vec<Atom> = Vec::new();
            for bi in &rule.body {
                if !bi.is_positive() {
                    // Negated atoms don't adorn and don't bind — skip.
                    continue;
                }
                let bi_atom = bi.atom();
                let beta_i = compute_adornment(bi_atom, &bound_vars);
                if idb.contains(&bi_atom.predicate)
                    && adorned.contains(&(bi_atom.predicate.clone(), beta_i.clone()))
                {
                    if let Some(bi_magic) = build_magic_atom(bi_atom, &beta_i) {
                        let mut mrule_body: Vec<BodyGoal> = Vec::new();
                        if let Some(g) = &magic_guard {
                            mrule_body.push(BodyGoal::Positive(g.clone()));
                        }
                        mrule_body.extend(
                            prev_body.iter().cloned().map(BodyGoal::Positive),
                        );
                        // A supplementary rule must have at least one
                        // body atom (Datalog rules cannot have empty
                        // bodies — parser convention). If no guard and
                        // no prior body atoms, promote the magic head
                        // to a fact so the fixpoint still ignites.
                        if mrule_body.is_empty() {
                            if bi_magic.is_ground() {
                                new_facts.push(bi_magic);
                            }
                            // Non-ground with no body → cannot seed;
                            // silently drop. The guarded rewrite of
                            // the enclosing rule will still fire
                            // whenever the top-level magic set is
                            // populated, so correctness is preserved.
                        } else {
                            new_rules.push(Rule {
                                head: bi_magic,
                                body: mrule_body,
                            });
                        }
                    }
                }
                prev_body.push(bi_atom.clone());
                for t in &bi_atom.terms {
                    if let Term::Var(name) = t {
                        bound_vars.insert(name.clone());
                    }
                }
            }
        }
    }

    // ----------------------------------------------------------------
    // Step 4 — query seed.
    // ----------------------------------------------------------------
    let seed_terms: Vec<Term> = q_head
        .terms
        .iter()
        .zip(q_adornment.iter())
        .filter(|&(_, &b)| b)
        .map(|(t, _)| t.clone())
        .collect();
    if !seed_terms.is_empty() {
        let seed = Atom {
            predicate: magic_pred_name(&q_head.predicate, &q_adornment),
            terms: seed_terms,
        };
        // The bound positions of a query are Const by construction
        // (`q_adornment` marks Const → true); is_ground holds.
        if seed.is_ground() {
            new_facts.push(seed);
        }
    }

    Program {
        rules: new_rules,
        facts: new_facts,
    }
}

// --------------------------------------------------------------------
// Internals
// --------------------------------------------------------------------

/// Walk `rule`'s body left-to-right under the given head adornment,
/// invoking `on_idb_body` with each IDB body atom's predicate and its
/// SIP-derived adornment. Used only by the adornment BFS in step 1 —
/// step 2/3 duplicate the same walk because they need `prev_body` and
/// `magic_guard` context that this callback shape does not carry.
fn propagate_sip<F>(rule: &Rule, adr: &Adornment, idb: &HashSet<String>, mut on_idb_body: F)
where
    F: FnMut(String, Adornment),
{
    let mut bound_vars: HashSet<String> = HashSet::new();
    for (i, term) in rule.head.terms.iter().enumerate() {
        if adr[i] {
            if let Term::Var(name) = term {
                bound_vars.insert(name.clone());
            }
        }
    }
    // Only positive body atoms adorn IDB predicates and contribute to
    // `bound_vars` — a `not p(...)` goal is a filter over already-bound
    // variables, per stratified-negation safety.
    for bi in &rule.body {
        if !bi.is_positive() {
            continue;
        }
        let bi_atom = bi.atom();
        let beta_i = compute_adornment(bi_atom, &bound_vars);
        if idb.contains(&bi_atom.predicate) {
            on_idb_body(bi_atom.predicate.clone(), beta_i);
        }
        for t in &bi_atom.terms {
            if let Term::Var(name) = t {
                bound_vars.insert(name.clone());
            }
        }
    }
}

/// Compute the SIP adornment for an atom given the currently-bound
/// variables. A `Const` slot is always bound; a `Var` slot is bound
/// iff its name is in `bound_vars`; a `Bound` (pipeline
/// interpolation) slot is treated as free at rewrite time — the
/// evaluator would reject any program that contains one before we
/// get to query it.
fn compute_adornment(atom: &Atom, bound_vars: &HashSet<String>) -> Adornment {
    atom.terms
        .iter()
        .map(|t| match t {
            Term::Const(_) => true,
            Term::Var(name) => bound_vars.contains(name),
            Term::Bound(_) => false,
        })
        .collect()
}

/// Build the `magic_<pred>_<adornment>(bound_args…)` guard atom for a
/// given source atom + adornment. Returns `None` when the adornment
/// has no bound positions (an arity-0 magic predicate is legal in the
/// AST but useless as a filter — see the module doc's "degenerate
/// cases").
fn build_magic_atom(source: &Atom, adornment: &Adornment) -> Option<Atom> {
    let args: Vec<Term> = source
        .terms
        .iter()
        .zip(adornment.iter())
        .filter(|&(_, &b)| b)
        .map(|(t, _)| t.clone())
        .collect();
    if args.is_empty() {
        return None;
    }
    Some(Atom {
        predicate: magic_pred_name(&source.predicate, adornment),
        terms: args,
    })
}

/// Canonical name for a magic predicate: `magic_<pred>_<adornment>`
/// where `<adornment>` is the position-wise `b`/`f` string. The
/// double-underscore-free shape matches the format the design doc
/// (`design/terminal/semantic-shell-language-plan.md` §R226.M4) and
/// task prompt call out; a user's own `magic_foo_bf` predicate would
/// collide, so document callers should reserve the `magic_` prefix
/// for the rewriter (there is no lexer-level namespace guard).
fn magic_pred_name(pred: &str, adornment: &Adornment) -> String {
    let adr: String = adornment
        .iter()
        .map(|&b| if b { 'b' } else { 'f' })
        .collect();
    format!("magic_{}_{}", pred, adr)
}

// --------------------------------------------------------------------
// Compile-only sanity checks: use every public re-export path so a
// downstream user of this module can rely on the same symbols the
// tests exercise, without depending on the test corpus itself.
// --------------------------------------------------------------------
#[allow(dead_code)]
fn _compile_check(program: &Program, query: &Query) -> Program {
    let _ = Value::Ident("_".into());
    rewrite(program, query)
}
