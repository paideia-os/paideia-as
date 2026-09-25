//! R226.M9 query-time type checking for Datalog programs and queries.
//!
//! # Position in the pipeline
//!
//! ```text
//!     Program (parsed by parser)  ─┐
//!     Query   (parsed by parser)  ─┤
//!                                   ├─▶ type_check::check_program ─▶ Ok / Err(Vec<TypeCheckError>)
//!     SchemaRegistry (built by     ─┘        │
//!     the caller — schema drives)             ▼
//!                                    Evaluator::run_query_typed
//! ```
//!
//! # What it is (and is not)
//!
//! A *schema-driven* type check: the caller builds a
//! [`SchemaRegistry`] mapping `(predicate, arity)` to a
//! [`PredicateSignature`], and this pass walks every atom in the
//! program (facts + rule heads + positive body goals) and query,
//! checking each constant term against the expected [`ValueType`] at
//! its position.
//!
//! It is **not** a full type-inference pass. Free variables and
//! pipeline `Bound` terms are accepted unconditionally — their type
//! is fixed by unification with the tuples the seminaïve fixpoint
//! actually derives, which is a *runtime* fact rather than a
//! signature-time one. (Type-flow through variables is R226.M10's
//! and later's territory, once the schema registry is a first-class
//! surface with per-slot inference.)
//!
//! # Why per-position, not per-atom?
//!
//! The R229 schema-registry contract (per FS §7.2) keys tables on
//! `(predicate, arity)` — the same discriminator the evaluator's
//! `HashMap<Key, TupleSet>` uses. A signature is a per-slot list,
//! not a name/type table, so `p(?X, 1)` and `p(?X, "hi")` under
//! `p/2 arg_types=[Ident, Num]` produce a `TypeMismatch` at position
//! 1 for the second — the diagnostic points at the exact slot the
//! caller wrote, which is what an LSP action needs.
//!
//! # Error collection
//!
//! Errors are collected across the whole program (not short-circuited
//! at the first mismatch) so a single check pass surfaces every
//! problem the caller must fix — a REPL that runs the check on every
//! keystroke should not oscillate on "fix one, discover the next".
//!
//! # Negative goals
//!
//! `not p(...)` bodies are checked identically to positive ones — the
//! sig lookup uses `(predicate, arity)`, and negation polarity is
//! irrelevant to type conformance. (The evaluator's stratification
//! and safety-condition machinery handles negation-specific concerns
//! separately.)
//!
//! # Fingerprints
//!
//! The `tests/type_check.rs` corpus tags each fixture with
//! `r226m9-tc-NN` so the R220.M10 `@fingerprint` correlator can
//! attribute pass/fail to a specific fixture without re-parsing its
//! name.

use crate::ast::{Atom, BodyGoal, Program, Term, Value};
use std::collections::HashMap;
use std::fmt;

/// The type universe a predicate slot can be constrained to.
///
/// [`ValueType::Any`] is the escape hatch — a slot registered as
/// `Any` accepts any [`Value`] variant. It is the default for
/// undeclared arg slots when a caller wants a partially-typed sig
/// (e.g. `p(Ident, Any, Num)` — position 1 is a wildcard).
///
/// The concrete variants correspond one-for-one with the [`Value`]
/// variants ([`Value::Ident`], [`Value::Str`], [`Value::Num`],
/// [`Value::Float`]) — there is no lossy narrowing across kinds.
/// `Num` and `Float` are kept distinct even though both are numeric:
/// the schema registry pins per-slot types, and mixing widths inside
/// one column is an error the R226.M6 aggregator already surfaces
/// (`AggregationError::NonHomogeneousNumeric`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueType {
    /// Bare identifier ([`Value::Ident`]).
    Ident,
    /// Quoted string literal ([`Value::Str`]).
    Str,
    /// Integer literal ([`Value::Num`]).
    Num,
    /// IEEE 754 double-precision literal ([`Value::Float`]).
    Float,
    /// Wildcard — accepts any concrete value. Serves as the default
    /// for undeclared arg slots.
    Any,
}

impl ValueType {
    /// The [`ValueType`] a given [`Value`] belongs to.
    ///
    /// Total (never returns `Any` — `Any` is a *constraint*, not a
    /// classification of any concrete value).
    pub fn of(value: &Value) -> ValueType {
        match value {
            Value::Ident(_) => ValueType::Ident,
            Value::Str(_) => ValueType::Str,
            Value::Num(_) => ValueType::Num,
            Value::Float(_) => ValueType::Float,
        }
    }

    /// Does a value of type `self` satisfy a slot declared as `expected`?
    ///
    /// `expected == Any` is universally satisfied. Otherwise the two
    /// must match exactly — no coercion (Datalog values compare by
    /// exact equality; see [`Value`] doc).
    pub fn satisfies(self, expected: ValueType) -> bool {
        expected == ValueType::Any || self == expected
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValueType::Ident => "Ident",
            ValueType::Str => "Str",
            ValueType::Num => "Num",
            ValueType::Float => "Float",
            ValueType::Any => "Any",
        })
    }
}

/// Declared signature for a `(predicate, arity)` pair.
///
/// `arg_types.len()` must equal `arity` — the registry's `register`
/// entry point enforces this on insertion (a mismatched sig is a
/// caller bug, not an evaluator bug, and would silently misreport
/// positions if allowed through).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredicateSignature {
    /// Predicate name.
    pub predicate: String,
    /// Arity (number of arg slots).
    pub arity: usize,
    /// Per-slot expected type, in source-position order.
    pub arg_types: Vec<ValueType>,
}

impl PredicateSignature {
    /// Build a signature. Panics if `arg_types.len() != arity` — a
    /// mismatched sig is a caller bug (a debug-time misuse) rather
    /// than a runtime error path: the registry itself has no notion
    /// of "partial signature" to fall back on.
    pub fn new(
        predicate: impl Into<String>,
        arity: usize,
        arg_types: Vec<ValueType>,
    ) -> Self {
        assert_eq!(
            arg_types.len(),
            arity,
            "PredicateSignature::new: arity {arity} does not match arg_types.len() {}",
            arg_types.len(),
        );
        Self {
            predicate: predicate.into(),
            arity,
            arg_types,
        }
    }
}

/// Registry of predicate signatures, keyed on `(name, arity)`.
///
/// Two distinct arities of the same predicate name are two distinct
/// entries — the same discriminator the evaluator uses for its
/// tuple index. A missing key is reported as
/// [`TypeCheckError::UnknownPredicate`] per the R226.M9 spec
/// (untyped predicates are rejected, not silently accepted — this
/// forces every schema-driven program to be fully declared).
#[derive(Clone, Debug, Default)]
pub struct SchemaRegistry {
    sigs: HashMap<(String, usize), PredicateSignature>,
}

impl SchemaRegistry {
    /// Empty registry — every predicate reference is unknown.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a signature. Later inserts shadow earlier
    /// ones (the last write wins for the same key) — matching how a
    /// caller building a registry from multiple schema fragments
    /// expects "later declarations override".
    pub fn register(&mut self, sig: PredicateSignature) {
        let key = (sig.predicate.clone(), sig.arity);
        self.sigs.insert(key, sig);
    }

    /// Fetch the signature for a `(name, arity)` pair, if declared.
    pub fn lookup(&self, pred: &str, arity: usize) -> Option<&PredicateSignature> {
        self.sigs.get(&(pred.to_owned(), arity))
    }

    /// Number of registered signatures. Handy for tests.
    pub fn len(&self) -> usize {
        self.sigs.len()
    }

    /// True iff no signatures are registered.
    pub fn is_empty(&self) -> bool {
        self.sigs.is_empty()
    }
}

/// One diagnostic produced by [`check_program`] or by the query-side
/// check in [`crate::eval::Evaluator::run_query_typed`].
///
/// * [`TypeCheckError::UnknownPredicate`] — the atom's
///   `(predicate, arity)` has no registered signature. The R226.M9
///   contract rejects untyped predicates rather than silently
///   accepting them.
/// * [`TypeCheckError::ArityMismatch`] — a sig with a differing
///   arity is registered under the same predicate name. In practice
///   `SchemaRegistry::lookup` keys on `(name, arity)` so this is
///   only produced when a caller explicitly looks up a sig by name
///   alone (currently unused inside this module; reserved for
///   future name-first API shapes).
/// * [`TypeCheckError::TypeMismatch`] — a constant term at
///   `position` inside `predicate` is of type `got`, but the sig
///   expects `expected`. `position` is 0-based and points at the
///   exact slot the caller wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeCheckError {
    /// An atom's `(predicate, arity)` pair has no registered
    /// signature.
    UnknownPredicate {
        /// Predicate name as written.
        predicate: String,
        /// Arity of the offending atom.
        arity: usize,
    },
    /// A signature exists under `predicate` but for a different
    /// arity than the program uses.
    ArityMismatch {
        /// Predicate name.
        predicate: String,
        /// Arity the registry declared.
        expected: usize,
        /// Arity the program's atom used.
        got: usize,
    },
    /// A constant term's type does not match the declared slot type.
    TypeMismatch {
        /// Predicate the offending atom names.
        predicate: String,
        /// 0-based slot index.
        position: usize,
        /// The [`ValueType`] the sig declared for the slot.
        expected: ValueType,
        /// The [`ValueType`] of the constant term the program supplied.
        got: ValueType,
    },
}

impl fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeCheckError::UnknownPredicate { predicate, arity } => write!(
                f,
                "type check: unknown predicate {predicate}/{arity} (no registered signature)"
            ),
            TypeCheckError::ArityMismatch {
                predicate,
                expected,
                got,
            } => write!(
                f,
                "type check: arity mismatch for {predicate}: signature declares arity {expected}, program uses arity {got}"
            ),
            TypeCheckError::TypeMismatch {
                predicate,
                position,
                expected,
                got,
            } => write!(
                f,
                "type check: {predicate} position {position} expected {expected}, got {got}"
            ),
        }
    }
}

impl std::error::Error for TypeCheckError {}

/// Type-check every atom mentioned in `program` (facts + rule heads
/// + every body goal, positive and negative alike) against
/// `registry`. Returns `Ok(())` iff every atom passes; otherwise
/// returns every collected diagnostic — the pass does not
/// short-circuit at the first error.
///
/// See [`check_atom`] for the per-atom rules; this driver only
/// stitches together the walk order.
pub fn check_program(
    program: &Program,
    registry: &SchemaRegistry,
) -> Result<(), Vec<TypeCheckError>> {
    let mut errors = Vec::new();
    for fact in &program.facts {
        check_atom(fact, registry, &mut errors);
    }
    for rule in &program.rules {
        check_atom(&rule.head, registry, &mut errors);
        for goal in &rule.body {
            // Both polarities checked — negation does not affect type
            // conformance (only stratification + safety do).
            match goal {
                BodyGoal::Positive(a) | BodyGoal::Negative(a) => {
                    check_atom(a, registry, &mut errors);
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Type-check a single atom against `registry`, appending any
/// diagnostics to `errors`.
///
/// # Rules
///
/// 1. Look up `(atom.predicate, atom.arity())`. Missing →
///    [`TypeCheckError::UnknownPredicate`], no further checks (the
///    sig is what tells us the expected per-slot types; nothing to
///    check against).
/// 2. Belt-and-braces guard: if the lookup somehow returned a sig of
///    the wrong arity (a caller registered a bad sig without going
///    through [`PredicateSignature::new`]), emit
///    [`TypeCheckError::ArityMismatch`] and skip per-slot checks.
///    In the current call shape (`lookup` keys on `(name, arity)`)
///    this branch is unreachable; keeping it collapses one future
///    API-shape refactor into an already-tested code path.
/// 3. Per-slot: for each [`Term::Const(v)`], check
///    `ValueType::of(v).satisfies(expected)`. On mismatch emit
///    [`TypeCheckError::TypeMismatch`] with the exact 0-based
///    `position`. [`Term::Var`] and [`Term::Bound`] slots pass
///    unconditionally (their type is set by fixpoint unification /
///    pipeline resolution, not by the signature check).
pub fn check_atom(
    atom: &Atom,
    registry: &SchemaRegistry,
    errors: &mut Vec<TypeCheckError>,
) {
    let arity = atom.arity();
    let Some(sig) = registry.lookup(&atom.predicate, arity) else {
        errors.push(TypeCheckError::UnknownPredicate {
            predicate: atom.predicate.clone(),
            arity,
        });
        return;
    };
    // Belt-and-braces — `lookup` keys on `(name, arity)`, so a match
    // here always has `sig.arity == arity`. Kept as a diagnostic path
    // rather than a `debug_assert!` because the same helper is the
    // designated hook for the future name-first API shape.
    if sig.arity != arity {
        errors.push(TypeCheckError::ArityMismatch {
            predicate: atom.predicate.clone(),
            expected: sig.arity,
            got: arity,
        });
        return;
    }
    for (position, term) in atom.terms.iter().enumerate() {
        let expected = sig.arg_types[position];
        if let Term::Const(v) = term {
            let got = ValueType::of(v);
            if !got.satisfies(expected) {
                errors.push(TypeCheckError::TypeMismatch {
                    predicate: atom.predicate.clone(),
                    position,
                    expected,
                    got,
                });
            }
        }
        // Term::Var and Term::Bound: no static type — pass.
    }
}

// ------------------------------------------------------------------
// Unit tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Atom, Rule, Term, Value};

    fn sig(pred: &str, arg_types: Vec<ValueType>) -> PredicateSignature {
        PredicateSignature::new(pred, arg_types.len(), arg_types)
    }

    #[test]
    fn value_type_of_covers_every_variant() {
        assert_eq!(ValueType::of(&Value::Ident("a".into())), ValueType::Ident);
        assert_eq!(ValueType::of(&Value::Str("a".into())), ValueType::Str);
        assert_eq!(ValueType::of(&Value::Num(1)), ValueType::Num);
    }

    #[test]
    fn any_slot_accepts_every_value_type() {
        assert!(ValueType::Ident.satisfies(ValueType::Any));
        assert!(ValueType::Str.satisfies(ValueType::Any));
        assert!(ValueType::Num.satisfies(ValueType::Any));
    }

    #[test]
    fn concrete_slot_requires_exact_match() {
        assert!(ValueType::Ident.satisfies(ValueType::Ident));
        assert!(!ValueType::Num.satisfies(ValueType::Ident));
        assert!(!ValueType::Str.satisfies(ValueType::Num));
    }

    #[test]
    fn registry_last_write_wins() {
        let mut reg = SchemaRegistry::new();
        reg.register(sig("p", vec![ValueType::Ident]));
        reg.register(sig("p", vec![ValueType::Num]));
        assert_eq!(reg.len(), 1);
        assert_eq!(
            reg.lookup("p", 1).unwrap().arg_types,
            vec![ValueType::Num]
        );
    }

    #[test]
    fn check_atom_var_and_bound_pass() {
        let mut reg = SchemaRegistry::new();
        reg.register(sig("p", vec![ValueType::Ident, ValueType::Num]));
        let atom = Atom::new(
            "p",
            vec![Term::Var("X".into()), Term::Bound("y".into())],
        );
        let mut errors = Vec::new();
        check_atom(&atom, &reg, &mut errors);
        assert!(errors.is_empty(), "var/bound slots should not error, got {errors:?}");
    }

    #[test]
    fn check_atom_reports_exact_position() {
        let mut reg = SchemaRegistry::new();
        reg.register(sig(
            "p",
            vec![ValueType::Ident, ValueType::Ident, ValueType::Ident],
        ));
        // p(a, 42, c) — position 1 is Num where Ident was expected.
        let atom = Atom::new(
            "p",
            vec![
                Term::Const(Value::Ident("a".into())),
                Term::Const(Value::Num(42)),
                Term::Const(Value::Ident("c".into())),
            ],
        );
        let mut errors = Vec::new();
        check_atom(&atom, &reg, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TypeCheckError::TypeMismatch {
                predicate,
                position,
                expected,
                got,
            } => {
                assert_eq!(predicate, "p");
                assert_eq!(*position, 1);
                assert_eq!(*expected, ValueType::Ident);
                assert_eq!(*got, ValueType::Num);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn check_program_collects_multiple_errors() {
        let mut reg = SchemaRegistry::new();
        reg.register(sig("p", vec![ValueType::Ident]));
        reg.register(sig("q", vec![ValueType::Num]));
        // p(1). q("hi"). — both type-mismatch; r(a). — unknown predicate.
        let program = Program {
            rules: vec![Rule {
                head: Atom::new("r", vec![Term::Const(Value::Ident("a".into()))]),
                body: vec![BodyGoal::Positive(Atom::new(
                    "p",
                    vec![Term::Const(Value::Num(1))],
                ))],
            }],
            facts: vec![
                Atom::new("q", vec![Term::Const(Value::Str("hi".into()))]),
            ],
        };
        let err = check_program(&program, &reg).unwrap_err();
        // 1 fact-level mismatch + 1 head-atom UnknownPredicate + 1 body mismatch.
        assert_eq!(err.len(), 3, "collected errors = {err:?}");
    }
}
