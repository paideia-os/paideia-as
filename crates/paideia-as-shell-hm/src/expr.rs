//! Pure lambda expression AST, extended in R225.M2 with record
//! literals and field access.
//!
//! Seven constructors — variable, literal, lambda, application, let,
//! record literal, field access — cover the M2 surface. Helper
//! builders `v`, `i`, `s`, `lam`, `app`, `let_`, `record`, `field`
//! keep test corpora readable without a parser.

use std::collections::HashMap;

/// The two constant shapes M1 recognises.
///
/// `Int` and `Str` are stand-ins for the two ground types the
/// pipeline's HM checker needs to talk about at this stage. They are
/// nullary (no payload); the actual literal value is not carried
/// because inference only cares about the *type* of a literal.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Lit {
    /// Integer literal — inferred type `Int`.
    Int,
    /// String literal — inferred type `Str`.
    Str,
}

/// The pure lambda expression grammar (M1) plus record literals and
/// field access (M2).
///
/// This is the untyped syntactic surface consumed by
/// [`crate::infer::infer`]; type annotations are absent by design
/// (M2 still does Algorithm W, not bidirectional checking).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// A term variable — looked up in the [`crate::infer::TypeEnv`].
    Var(String),
    /// A literal constant of one of the shapes in [`Lit`].
    Lit(Lit),
    /// A lambda abstraction `\param. body`.
    Lam(String, Box<Expr>),
    /// A function application `f a`.
    App(Box<Expr>, Box<Expr>),
    /// A `let x = val in body` binding — the site of Damas-Milner
    /// generalisation.
    Let(String, Box<Expr>, Box<Expr>),
    /// A record literal `{name1 = e1, name2 = e2, ...}`. The
    /// [`HashMap`] key is the field name; the value is the
    /// sub-expression that computes the field's value. The empty map
    /// is the empty record `{}`.
    RecordLit(HashMap<String, Expr>),
    /// A field access `e.field` — projects `field` out of the record
    /// value produced by `e`.
    Field(Box<Expr>, String),
}

/// Build a variable reference — `v("x")` ≡ `Expr::Var("x".into())`.
pub fn v(name: &str) -> Expr {
    Expr::Var(name.to_owned())
}

/// Build an integer literal.
pub fn i() -> Expr {
    Expr::Lit(Lit::Int)
}

/// Build a string literal.
pub fn s() -> Expr {
    Expr::Lit(Lit::Str)
}

/// Build a lambda abstraction — `lam("x", body)` ≡ `\x. body`.
pub fn lam(param: &str, body: Expr) -> Expr {
    Expr::Lam(param.to_owned(), Box::new(body))
}

/// Build an application — `app(f, a)` ≡ `f a`.
pub fn app(f: Expr, a: Expr) -> Expr {
    Expr::App(Box::new(f), Box::new(a))
}

/// Build a let-binding — `let_("x", val, body)` ≡ `let x = val in body`.
///
/// Named with a trailing underscore because `let` is a Rust keyword;
/// the alias matches the convention used elsewhere in the workspace
/// for expression-builders whose names would otherwise collide.
pub fn let_(name: &str, val: Expr, body: Expr) -> Expr {
    Expr::Let(name.to_owned(), Box::new(val), Box::new(body))
}

/// Build a record literal — `record([("a", i()), ("b", s())])` ≡
/// `{a = 42, b = "hello"}`.
///
/// Accepts an iterable of `(name, expression)` pairs (any type that
/// coerces to `&str` for the name) so test call sites can use array
/// literals without allocating a [`HashMap`] by hand. If the same
/// field name appears twice, the later occurrence wins.
pub fn record<I, N>(fields: I) -> Expr
where
    I: IntoIterator<Item = (N, Expr)>,
    N: Into<String>,
{
    let mut map = HashMap::new();
    for (name, e) in fields {
        map.insert(name.into(), e);
    }
    Expr::RecordLit(map)
}

/// Build a field-access expression — `field(v("r"), "a")` ≡ `r.a`.
pub fn field(e: Expr, name: &str) -> Expr {
    Expr::Field(Box::new(e), name.to_owned())
}
