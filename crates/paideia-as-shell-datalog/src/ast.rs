//! R226.M1 AST for the shell's embedded Datalog.
//!
//! Four node kinds: `Term` (an atom's argument slot), `Atom`
//! (predicate + argument list), `Rule` (head atom + body conjunction),
//! `Program` (rules + ground facts). Queries are a separate top-level
//! shape (`Query = conjunction of goal atoms`) so a REPL can pass a
//! program once and re-query without re-parsing.
//!
//! # Value universe
//!
//! Values are one of `Ident` (bare symbols like `alice`, `bob`), `Str`
//! (quoted strings), `Num` (integer literals), or `Float` (IEEE 754
//! double-precision literals). Floats were folded in as an R226.M6
//! follow-up (issue #1460) so `sum`/`avg` can reduce over a
//! non-integer numeric column.
//!
//! # `Value::Float` equality and hashing
//!
//! Rust's `f64` deliberately does not implement `Eq` or `Hash` — NaN
//! violates reflexivity under IEEE 754 equality and floats have no
//! natural total order. The Datalog engine, however, keeps ground
//! tuples in `HashSet<Vec<Value>>` (see [`crate::eval::Database`] and
//! [`crate::session_edb::SessionEdb`]) and reserves the right to
//! compare bindings by exact equality. So we bracket the whole
//! variant behind bit-pattern equality: two `Float(a)` and `Float(b)`
//! values are considered equal iff `a.to_bits() == b.to_bits()`, and
//! `Hash` mixes in `to_bits()` too. Consequences:
//!
//! * A canonical `Value::Float(f64::NAN)` compares equal to another
//!   `Value::Float(f64::NAN)` produced the same way (same bit
//!   pattern), so a substitution carrying NaN round-trips through the
//!   fact table. This is the pragmatic choice; the alternative (NaN
//!   != NaN, matching raw IEEE 754) would make NaN-bearing tuples
//!   disappear from the DB and silently break downstream aggregation.
//! * A NaN with a different payload bit pattern (rare — comes from
//!   specific FMA/quiet-signaling paths, not arithmetic on
//!   `Value::Float`) compares unequal even to itself in that other
//!   payload, but that inequality is still reflexive per bit-pattern,
//!   so `Eq` holds.
//! * `+0.0` and `-0.0` have distinct bit patterns and therefore
//!   compare unequal as `Value::Float`. If a caller wants numeric
//!   equality they normalise before wrapping (`f.abs() + 0.0`).
//!
//! # `Term::Bound` — pipeline interpolation stub
//!
//! `$expr` lexes as `TokenKind::InterpVar`. At M1/M2 the parser accepts
//! it (so shell scripts round-trip through the AST) but the evaluator
//! refuses to run a program that mentions a `Bound` term — pipeline-
//! value resolution needs the R229 REPL loop, which is not landed yet.
//! Treating `Bound` as a first-class term at the AST level (rather than
//! smuggling it inside `Const`) keeps R226.M8 (session-local EDB
//! assert/retract) from having to rewrite the AST when the wiring
//! finally arrives.

use std::fmt;
use std::hash::{Hash, Hasher};

/// Concrete, ground values a predicate slot can hold. Comparison by
/// exact equality — no unification-time coercion (per the schema
/// registry's principle that predicate slots are typed). See the
/// module doc for the `Value::Float` equality / hashing contract
/// (bit-pattern based, so `HashSet<Vec<Value>>` works with NaN).
#[derive(Clone, Debug)]
pub enum Value {
    /// Bare symbol like `alice`, `bob`. The most common shape at M1/M2
    /// where predicates read like Prolog fixtures.
    Ident(String),
    /// Quoted string literal. The parser passes the raw lexer inner
    /// text through unchanged (escape resolution is deferred to
    /// R226.M2 in the published plan — the crates.io grammar layer).
    Str(String),
    /// Integer literal.
    Num(i64),
    /// IEEE 754 double-precision literal. Equality and hashing use
    /// `to_bits()` so the variant is safe inside `HashSet<Vec<Value>>`
    /// — see the module doc for the NaN and signed-zero caveats.
    Float(f64),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Ident(a), Value::Ident(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            // Bit-pattern equality — see module doc. Reflexive
            // (`x.to_bits() == x.to_bits()` always), symmetric, and
            // transitive, so `Eq` below is sound.
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Discriminant first so hashes never accidentally collide
        // across variants (a `Num(0)` and a `Float(0.0)` must not
        // land in the same bucket even if both hashed their inner as
        // `u64`).
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Ident(s) => s.hash(state),
            Value::Str(s) => s.hash(state),
            Value::Num(n) => n.hash(state),
            // `to_bits` gives us a hashable representation compatible
            // with the `PartialEq` impl above. Same NaN caveat.
            Value::Float(f) => f.to_bits().hash(state),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Ident(s) => f.write_str(s),
            Value::Str(s) => write!(f, "{:?}", s),
            Value::Num(n) => write!(f, "{}", n),
            // `{}` on an `f64` renders `3.14`, `-0.5`, `inf`, `NaN`.
            // Adequate for REPL output; a schema-registry-driven
            // formatter (R226.M9) can override per column.
            Value::Float(x) => write!(f, "{}", x),
        }
    }
}

/// A single slot inside an atom's argument list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Term {
    /// Logic variable (`?X` in surface syntax; `X` at the AST level —
    /// the sigil is not stored). Scoped to the enclosing rule / query.
    Var(String),
    /// Ground constant literal.
    Const(Value),
    /// Pipeline-value interpolation (`$name`). Stored as the bare name;
    /// M2's evaluator refuses to run a program that mentions one. See
    /// module doc for the deferral rationale.
    Bound(String),
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Var(n) => write!(f, "?{}", n),
            Term::Const(v) => write!(f, "{}", v),
            Term::Bound(n) => write!(f, "${}", n),
        }
    }
}

/// A predicate applied to a term list. The arity is `terms.len()`;
/// the evaluator's per-predicate index keys on `(predicate, arity)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Atom {
    /// Predicate name (a bare identifier from `TokenKind::Ident`).
    pub predicate: String,
    /// Argument list, in source order.
    pub terms: Vec<Term>,
}

impl Atom {
    /// Construct an atom from a name and term list.
    pub fn new(predicate: impl Into<String>, terms: Vec<Term>) -> Self {
        Self {
            predicate: predicate.into(),
            terms,
        }
    }

    /// Arity — number of argument slots.
    #[inline]
    pub fn arity(&self) -> usize {
        self.terms.len()
    }

    /// True iff every argument is `Term::Const(_)`. Ground atoms are
    /// candidates for the extensional database (EDB).
    pub fn is_ground(&self) -> bool {
        self.terms.iter().all(|t| matches!(t, Term::Const(_)))
    }

    /// Extract the ground tuple for an atom that `is_ground()` returns
    /// `true` on. Returns `None` for a non-ground atom.
    pub fn as_ground(&self) -> Option<Vec<Value>> {
        self.terms
            .iter()
            .map(|t| match t {
                Term::Const(v) => Some(v.clone()),
                _ => None,
            })
            .collect()
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.predicate)?;
        for (i, t) in self.terms.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{}", t)?;
        }
        f.write_str(")")
    }
}

/// A single conjunct in a rule body: either a positive goal
/// (`p(?X, ?Y)`) that must match a tuple in the DB, or a negative
/// goal (`not p(?X, ?Y)`) that must NOT match any tuple in the DB
/// (closed-world negation, stratified per R226.M5).
///
/// The wrapper is preferred over adding a `Negated(Atom)` variant to
/// `Term` or `Atom` — it keeps `Atom` (and its ground-tuple, arity,
/// display machinery) polarity-free, and localises the polarity flag
/// on the body-conjunct axis where the evaluator's stratification and
/// safety checks actually live.
///
/// The R226.M5 evaluator refuses a program that requires negation
/// through recursion (a cycle in the predicate dependency graph that
/// carries any negated edge); it accepts a program whose negations
/// can be stratified — evaluated bottom-up, one stratum at a time —
/// so every negated goal is checked against a completed relation
/// from a strictly lower stratum. See `crate::stratification`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BodyGoal {
    /// A positive body atom: must match a tuple in the DB.
    Positive(Atom),
    /// A negative body atom (`not p(...)` in surface syntax): must
    /// NOT match any tuple in the DB under the current substitution.
    /// Safety condition: every variable that appears in the atom must
    /// also appear positively earlier in the same rule body.
    Negative(Atom),
}

impl BodyGoal {
    /// The inner atom, regardless of polarity. Handy when a caller
    /// only cares about the predicate name / arity / term shape (e.g.
    /// the stratification dependency walk).
    #[inline]
    pub fn atom(&self) -> &Atom {
        match self {
            BodyGoal::Positive(a) | BodyGoal::Negative(a) => a,
        }
    }

    /// True iff this goal is a `not p(...)` conjunct.
    #[inline]
    pub fn is_negated(&self) -> bool {
        matches!(self, BodyGoal::Negative(_))
    }

    /// True iff this goal is a plain positive atom.
    #[inline]
    pub fn is_positive(&self) -> bool {
        matches!(self, BodyGoal::Positive(_))
    }
}

impl fmt::Display for BodyGoal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BodyGoal::Positive(a) => write!(f, "{}", a),
            BodyGoal::Negative(a) => write!(f, "not {}", a),
        }
    }
}

/// A rule: `head` is derivable whenever every atom in `body` is
/// derivable under a consistent substitution.
///
/// **Convention:** `head => body_1, body_2, …` — head first, body
/// after, `=>` as separator. This is the direct positional translation
/// of Prolog's `head :- body_1, body_2, …` — see the crate-level doc's
/// "Concrete syntax accepted at M1" section for the rationale (the
/// lexer emits `=>` as `FatArrow`; `:-` is not a lexical token).
///
/// A rule with `body.is_empty()` is stored as a `Program::facts` entry
/// during parsing (not as a `Rule`), so a `Rule` here always has at
/// least one body atom.
///
/// Each body conjunct is a `BodyGoal`, so `not p(...)` conjuncts (the
/// R226.M5 stratified-negation surface) can sit alongside positive
/// atoms without disturbing the `Atom` type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Head atom — the conclusion the rule derives.
    pub head: Atom,
    /// Body conjunction — one or more goals (positive or negative)
    /// that must all hold under a consistent substitution.
    pub body: Vec<BodyGoal>,
}

/// A Datalog program: rules (with non-empty bodies) plus ground facts.
///
/// Rules with empty bodies are lifted into `facts` at parse time so
/// the fixpoint evaluator can seed the database in one pass without
/// re-checking each rule for degeneracy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    /// Rules with at least one body atom.
    pub rules: Vec<Rule>,
    /// Ground facts (rules whose body was empty at parse time, plus
    /// any bare `p(a, b).` inputs).
    pub facts: Vec<Atom>,
}

impl Program {
    /// Empty program: no rules, no facts. Query results against it are
    /// always empty (the `t26_m2_08` fixture pins this).
    pub fn empty() -> Self {
        Self::default()
    }
}

/// A query: a conjunction of goal atoms. Free variables in the goals
/// become the columns of the result set.
///
/// Kept separate from `Program` so a REPL can materialize the fixpoint
/// once and re-query the same DB with different goals without reparsing
/// the underlying rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// Conjunctive goal list.
    pub goals: Vec<Atom>,
}

/// R226.M6 aggregation function selector.
///
/// The five SQL classics. Every aggregate takes a single target
/// variable (bound by some positive body goal) and optionally a
/// group-by tuple. Numeric coercion for [`Aggregate::Sum`] and
/// [`Aggregate::Avg`] is checked at evaluation time
/// ([`crate::aggregation::AggregationError::NonNumericTarget`]) — the
/// AST does not attempt static typing until R226.M9 pins predicate
/// slot types via the schema registry.
///
/// Kept as a `Copy` enum (never a `String`) so a caller can pattern-
/// match the selector without a heap allocation, and so a
/// misspelled selector is a parse-time error rather than an
/// evaluate-time surprise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Aggregate {
    /// `count(?x)` — number of substitutions whose `?x` slot is bound.
    /// Always emits an `AggregateResult::Count(u64)`; on zero input
    /// (ungrouped) emits `Count(0)` — the identity, not `Empty`.
    Count,
    /// `sum(?x)` — integer sum of `?x` across substitutions. Non-
    /// numeric `?x` values produce `AggregationError::NonNumericTarget`.
    /// On zero input (ungrouped) emits `Sum(0)` — the additive identity.
    Sum,
    /// `min(?x)` — minimum `?x` under the total order defined by
    /// [`crate::aggregation`] (Num < Str < Ident; natural order within
    /// a kind). On zero input emits `AggregateResult::Empty` — there
    /// is no meaningful minimum of an empty set.
    Min,
    /// `max(?x)` — dual of `Min`.
    Max,
    /// `avg(?x)` — arithmetic mean of `?x` as an `f64`. Non-numeric
    /// `?x` values produce `AggregationError::NonNumericTarget`. On
    /// zero input emits `AggregateResult::Empty` — `0/0` has no value.
    Avg,
}

impl fmt::Display for Aggregate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Aggregate::Count => "count",
            Aggregate::Sum => "sum",
            Aggregate::Min => "min",
            Aggregate::Max => "max",
            Aggregate::Avg => "avg",
        })
    }
}

/// R226.M6 aggregation query — an aggregate applied to the bindings
/// produced by a body of goal atoms, optionally partitioned by a
/// group-by tuple.
///
/// Kept distinct from [`Query`] (which returns raw bindings) so a
/// caller dispatches on the *shape of intent*: `Query` for
/// enumeration, `AggregateQuery` for reduction. A REPL that offers
/// both surfaces routes through two entry points on
/// [`crate::eval::Evaluator`] — no runtime discrimination inside
/// either path.
///
/// # Body shape
///
/// `goals` is a `Vec<BodyGoal>` — identical to `Rule::body` — so
/// stratified negation (`not p(...)`, R226.M5) works transparently in
/// aggregation queries. The evaluator enumerates every substitution
/// that satisfies the conjunction, honouring the same
/// positive-then-negative pass order the fixpoint uses, and hands the
/// resulting substitution set to [`crate::aggregation::evaluate`].
///
/// # Safety condition
///
/// `target_var` must appear in some positive body goal — otherwise no
/// substitution ever binds it, and the aggregation collapses to the
/// ungrouped zero-input case. Every name in `group_by` is subject to
/// the same rule. R226.M9 will make either violation a compile-time
/// diagnostic; at M6 the aggregator silently drops any substitution
/// whose group-by tuple is not fully bound (mirroring the
/// range-restriction skip in [`crate::eval`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregateQuery {
    /// Which aggregate to compute.
    pub agg: Aggregate,
    /// Free variable to reduce over.
    pub target_var: String,
    /// Body of goals producing the substitution set. Same grammar as
    /// a rule body, including `not p(...)` conjuncts.
    pub goals: Vec<BodyGoal>,
    /// Zero or more variables to partition by. Empty means one
    /// ungrouped output row.
    pub group_by: Vec<String>,
}
