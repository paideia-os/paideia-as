//! Types: monotypes, type variables, and type schemes.
//!
//! The three shapes here are the entire type language of R225.M1.
//! Rows, records, and effect qualifiers arrive in later milestones and
//! will live in their own modules alongside this one.

use std::collections::HashSet;
use std::fmt;

/// A fresh type variable, minted by [`crate::infer::FreshVarGen`].
///
/// `TypeVar` values are opaque monotonic identifiers — the [`u32`]
/// payload has no meaning beyond distinguishing one variable from
/// another. Comparisons are by numeric identity, not by structural
/// role.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVar(pub u32);

/// A monotype: no top-level (or nested) universal quantifiers.
///
/// The three shapes match the M1 type grammar in the crate docs. The
/// [`Box`] on `Arrow` is required for the recursive definition; the
/// resulting `MonoType` is `Sized` and cheaply cloned only when the
/// tree is small, which the Damas-Milner substitution algorithm
/// happens to keep true in practice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonoType {
    /// A type variable (either free or later replaced by
    /// [`crate::subst::Substitution`]).
    Var(TypeVar),
    /// A type constant such as `Int` or `Str`. The payload is the
    /// name; M1 has no constant arity — every constant is nullary.
    Con(String),
    /// A function type `τ1 -> τ2`.
    Arrow(Box<MonoType>, Box<MonoType>),
}

impl MonoType {
    /// Collect every free type variable that appears in this monotype.
    ///
    /// A monotype has no binders, so *every* [`TypeVar`] it mentions
    /// is free. The result is a set (order-insensitive); callers that
    /// need a stable ordering should sort by [`TypeVar::0`].
    pub fn free_vars(&self) -> HashSet<TypeVar> {
        let mut out = HashSet::new();
        self.collect_free_vars(&mut out);
        out
    }

    fn collect_free_vars(&self, out: &mut HashSet<TypeVar>) {
        match self {
            Self::Var(v) => {
                out.insert(*v);
            }
            Self::Con(_) => {}
            Self::Arrow(a, b) => {
                a.collect_free_vars(out);
                b.collect_free_vars(out);
            }
        }
    }
}

impl fmt::Display for MonoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Variables print as lower-case letters `a`, `b`, ... for
            // the first 26, then `t<n>` after that. This keeps the
            // common small-tree case readable in test diagnostics
            // without pretending variables are alpha-normalised (they
            // are not — the id is the raw fresh counter).
            Self::Var(TypeVar(n)) => {
                if (*n as usize) < 26 {
                    let c = char::from(b'a' + *n as u8);
                    write!(f, "{c}")
                } else {
                    write!(f, "t{n}")
                }
            }
            Self::Con(name) => f.write_str(name),
            // Right-associative arrows: `a -> b -> c` renders as
            // `(a -> (b -> c))` — we always parenthesise so a reader
            // does not have to remember precedence when reading a
            // test failure message.
            Self::Arrow(a, b) => write!(f, "({a} -> {b})"),
        }
    }
}

/// A rank-1 type scheme: a monotype body prefixed by an outermost
/// sequence of universal quantifiers.
///
/// A scheme with an empty `quantified` list is a monotype dressed as a
/// scheme — the two are equivalent under [`crate::infer::instantiate`]
/// and [`crate::infer::generalize`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeScheme {
    /// The universally quantified variables — the `α` in `∀α. τ`.
    ///
    /// Duplicates are not enforced against; the algorithm never
    /// produces them. Order is preserved for deterministic pretty
    /// printing but has no semantic weight.
    pub quantified: Vec<TypeVar>,
    /// The scheme body.
    pub body: MonoType,
}

impl TypeScheme {
    /// Free type variables of the scheme: those free in the body but
    /// not in the [`Self::quantified`] list.
    ///
    /// This is the definition [`crate::infer::generalize`] uses to
    /// decide which variables of a monotype are eligible for
    /// quantification.
    pub fn free_vars(&self) -> HashSet<TypeVar> {
        let mut out = self.body.free_vars();
        for q in &self.quantified {
            out.remove(q);
        }
        out
    }
}

impl fmt::Display for TypeScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.quantified.is_empty() {
            write!(f, "{}", self.body)
        } else {
            f.write_str("forall")?;
            for q in &self.quantified {
                write!(f, " {}", MonoType::Var(*q))?;
            }
            write!(f, ". {}", self.body)
        }
    }
}
