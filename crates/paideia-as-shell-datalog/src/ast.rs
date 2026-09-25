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
//! (quoted strings), or `Num` (integer literals — floats are deferred
//! until the schema registry pins numeric column types at R226.M9).
//! The narrow universe is deliberate: at M1/M2 the evaluator's job is
//! to demonstrate fixpoint correctness, not to be a full expression
//! runtime. R226.M6 will widen the universe for aggregation operands.
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

/// Concrete, ground values a predicate slot can hold. Comparison by
/// exact equality — no unification-time coercion (per the schema
/// registry's principle that predicate slots are typed).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// Bare symbol like `alice`, `bob`. The most common shape at M1/M2
    /// where predicates read like Prolog fixtures.
    Ident(String),
    /// Quoted string literal. The parser passes the raw lexer inner
    /// text through unchanged (escape resolution is deferred to
    /// R226.M2 in the published plan — the crates.io grammar layer).
    Str(String),
    /// Integer literal. Floats are deferred; see module doc.
    Num(i64),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Ident(s) => f.write_str(s),
            Value::Str(s) => write!(f, "{:?}", s),
            Value::Num(n) => write!(f, "{}", n),
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Head atom — the conclusion the rule derives.
    pub head: Atom,
    /// Body conjunction — one or more atoms that must all hold.
    pub body: Vec<Atom>,
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
