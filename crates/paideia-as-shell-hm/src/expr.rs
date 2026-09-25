//! Pure lambda expression AST.
//!
//! Five constructors — variable, literal, lambda, application, let —
//! is the entire M1 surface. Helper builders `v`, `i`, `s`, `lam`,
//! `app`, `let_` keep test corpora readable without a parser.

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

/// The pure lambda expression grammar.
///
/// This is the untyped syntactic surface consumed by
/// [`crate::infer::infer`]; type annotations are absent by design
/// (M1 does Algorithm W, not bidirectional checking).
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
