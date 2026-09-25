//! Stage 2 of the R229 REPL turn pipeline: HM Algorithm W over the
//! lambda subset of the shell-ast surface. Wired in R225.M4.
//!
//! # What this module is
//!
//! [`type_check`] walks a [`SyntaxNode`] into a shell-hm [`Expr`], then
//! drives `paideia_as_shell_hm::infer` and reports its result as a
//! plain [`MonoType`] (success) or a [`TypeStageError`] (failure). The
//! caller in [`crate::turn::eval_turn`] surfaces success as
//! `ReplTurn::inferred_type = Some(mono)` and (for the two real HM
//! failures — [`TypeStageError::UnboundVar`] and
//! [`TypeStageError::UnifyFailed`]) as an early
//! `TurnResult::Error("type: …")`.
//!
//! # What the M4 subset covers
//!
//! Only the lambda-shaped fragment of the shell AST is HM-typed at
//! M4 — the pipeline / datalog / match / binop / unary-op / bool-
//! literal shapes are out of scope for this milestone and surface as
//! [`TypeStageError::UnsupportedNode`]. The recognised set is:
//!
//! * [`SyntaxNode::LitInt`]        → `Expr::Lit(Lit::Int)` → `Int`.
//! * [`SyntaxNode::LitStr`]        → `Expr::Lit(Lit::Str)` → `Str`.
//! * [`SyntaxNode::Var`] /
//!   [`SyntaxNode::Ident`]         → `Expr::Var(name)`.
//! * [`SyntaxNode::Lambda`]        → right-fold params into nested
//!   `Expr::Lam` (a `|x, y| body` lambda types as `a -> b -> t`).
//! * [`SyntaxNode::App`]           → left-fold args into nested
//!   `Expr::App`.
//! * [`SyntaxNode::Let`]           → `Expr::Let(name, value, body)`.
//! * [`SyntaxNode::FieldAccess`]   → `Expr::Field(base, field)`
//!   (row-polymorphic access via R225.M2).
//! * [`SyntaxNode::RecordExpr`]    → `Expr::RecordLit(fields)`.
//! * [`SyntaxNode::Group`]         → pass-through to `inner`.
//! * [`SyntaxNode::Cmd`] where
//!   `args.is_empty()` **and** the head is *not* [`SyntaxNode::Ident`]
//!   → pass-through to the head node. The pipeline parser wraps bare
//!   top-level expressions in `Cmd { name, args: [] }`, so `"42"` at
//!   the REPL reaches us as `Cmd { name: LitInt, args: [] }`; the
//!   Ident-head guard keeps us from misinterpreting a nullary command
//!   invocation (`"ls"`) as a variable reference — those defer to the
//!   executor's `Cmd` dispatch instead.
//!
//! Every other shape — including `Cmd` with a non-empty arg list or an
//! Ident head, `Pipe`, `Seq`, `Redirect`, `Background`, `DatalogBlock`,
//! `Atom`, `Rule`, `QVar`, `InterpVar`, `NotAtom`, `Match`, `BinOp`,
//! `UnaryOp`, `LitBool` — returns [`TypeStageError::UnsupportedNode`]
//! carrying the variant name. The caller in `eval_turn` interprets
//! `UnsupportedNode` as *no HM opinion*, sets `inferred_type: None`,
//! and continues the pipeline; it does **not** treat it as a turn
//! failure. (Only `UnboundVar` and `UnifyFailed` abort the turn.)
//!
//! # Why bool literals fall through
//!
//! shell-hm at R225.M3 defines exactly two literal shapes ([`Lit::Int`]
//! and [`Lit::Str`]); there is no `Lit::Bool` and no `Bool` type
//! constant. Extending the hm crate is out of scope for M4 (per the
//! ticket's "keep shell-hm additive" constraint), so `LitBool` maps to
//! `UnsupportedNode`. When R225 grows a `Bool` shape, this module and
//! the M4 test corpus grow one arm each.

use std::collections::HashMap;
use std::fmt;

use paideia_as_shell_ast::SyntaxNode;
use paideia_as_shell_hm::{
    infer, Expr, FreshVarGen, InferError, Lit, MonoType, TypeEnv,
};

/// Failure modes for [`type_check`].
///
/// Split three ways so the caller (currently
/// [`crate::turn::eval_turn`]) can decide independently for each shape
/// whether to abort the turn or continue. At M4 the wiring is:
///
/// * [`TypeStageError::UnsupportedNode`] — the AST shape is outside
///   the HM subset. The caller does *not* abort; it records
///   `inferred_type: None` and proceeds to stage 3. This preserves
///   the R229.M2 / R229.M3 executor contract for the pipeline
///   sub-language (Cmd / Pipe / Seq / DatalogBlock have their own
///   sub-language checkers upstream in stage 4).
/// * [`TypeStageError::UnboundVar`] — HM saw a `Var` that isn't in
///   `state.type_env`. The caller aborts with `type: unbound …`.
/// * [`TypeStageError::UnifyFailed`] — HM's unifier rejected a
///   constraint. The caller aborts with `type: <unifier message>`.
///
/// Kept as an enum (rather than a `String` newtype) so the R229.M5
/// diagnostics layer can pattern-match on the failure shape without
/// re-parsing the rendered message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeStageError {
    /// The unifier rejected a constraint. Payload is the wrapped
    /// `UnifyError::to_string()` — a stable rendering that the R229.M5
    /// diagnostics layer will replace with a structured span-carrying
    /// form; kept as a plain string here so we do not leak the hm
    /// crate's internal enum surface across this crate boundary.
    UnifyFailed(String),
    /// The environment does not bind the referenced identifier.
    /// Payload is the unbound name.
    UnboundVar(String),
    /// The AST shape lies outside the M4 HM subset (see the module
    /// doc for the enumerated support list). Payload is a short
    /// variant tag or reason phrase so a diagnostic can name what was
    /// skipped without dumping the whole subtree.
    UnsupportedNode(String),
}

impl fmt::Display for TypeStageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnifyFailed(msg) => write!(f, "unify failed: {msg}"),
            Self::UnboundVar(name) => write!(f, "unbound variable `{name}`"),
            Self::UnsupportedNode(what) => write!(f, "unsupported node: {what}"),
        }
    }
}

impl std::error::Error for TypeStageError {}

/// Typecheck a REPL turn's root AST node under `env`.
///
/// Walks `node` into a shell-hm [`Expr`] (see the module doc for the
/// recognised subset) and drives [`infer`]. Returns the inferred
/// [`MonoType`] on success; on failure, returns a [`TypeStageError`]
/// that the caller ([`crate::turn::eval_turn`]) triages per the enum's
/// per-variant contract.
///
/// The [`FreshVarGen`] is minted per call — separate turns do not
/// share a variable counter, matching the R225.M1 note that "distinct
/// runs may reuse ids — the generator is not meant to be a global
/// registry".
pub fn type_check(node: &SyntaxNode, env: &TypeEnv) -> Result<MonoType, TypeStageError> {
    let expr = lower_to_expr(node)?;
    let mut fresh = FreshVarGen::new();
    match infer(env, &expr, &mut fresh) {
        Ok((_, mono)) => Ok(mono),
        Err(InferError::UnboundVar(name)) => Err(TypeStageError::UnboundVar(name)),
        Err(InferError::UnifyError(e)) => Err(TypeStageError::UnifyFailed(e.to_string())),
    }
}

/// Recursive AST → hm `Expr` walker. Returns
/// [`TypeStageError::UnsupportedNode`] the moment it hits a shape
/// outside the M4 subset (see the module doc). The error is *not*
/// synthesised into a placeholder `Expr` — HM is not being asked to
/// speak about shapes it does not model.
fn lower_to_expr(node: &SyntaxNode) -> Result<Expr, TypeStageError> {
    match node {
        SyntaxNode::LitInt { .. } => Ok(Expr::Lit(Lit::Int)),
        SyntaxNode::LitStr { .. } => Ok(Expr::Lit(Lit::Str)),
        SyntaxNode::LitBool { .. } => Err(TypeStageError::UnsupportedNode(
            "lit-bool (shell-hm has no Bool literal at R225.M3)".to_owned(),
        )),

        // Var (lambda context) and Ident (pipeline context / datalog
        // constant) both denote a term-level name; treat identically.
        SyntaxNode::Var { name, .. } | SyntaxNode::Ident { name, .. } => {
            Ok(Expr::Var(name.clone()))
        }

        // `{|a, b| body}` curries right-to-left so
        //   |a, b| body  →  lam a. lam b. body
        // The empty-params case is degenerate at the parser level
        // (`{||body}` still yields params=[]), so a lambda with no
        // params just types as the body's type — matching the intent
        // of "no thunk wrapper introduced".
        SyntaxNode::Lambda { params, body, .. } => {
            let body_expr = lower_to_expr(body)?;
            Ok(params.iter().rev().fold(body_expr, |acc, p| {
                Expr::Lam(p.clone(), Box::new(acc))
            }))
        }

        // `f a b c` → App(App(App(f, a), b), c). Left-associative
        // application matches the shell-hm inference (`App` arm
        // unifies `t_f` with `t_arg -> t_res`, one arg at a time).
        SyntaxNode::App { func, args, .. } => {
            let mut acc = lower_to_expr(func)?;
            for a in args {
                let ae = lower_to_expr(a)?;
                acc = Expr::App(Box::new(acc), Box::new(ae));
            }
            Ok(acc)
        }

        SyntaxNode::Let { name, value, body, .. } => {
            let val = lower_to_expr(value)?;
            let bd = lower_to_expr(body)?;
            Ok(Expr::Let(name.clone(), Box::new(val), Box::new(bd)))
        }

        SyntaxNode::FieldAccess { base, field, .. } => {
            let b = lower_to_expr(base)?;
            Ok(Expr::Field(Box::new(b), field.clone()))
        }

        // Record literal — walk each field's value into an `Expr` and
        // hand the map to `Expr::RecordLit`. Duplicate field names on
        // the AST side are impossible (the parser rejects them) so we
        // do not defend against them here.
        SyntaxNode::RecordExpr { fields, .. } => {
            let mut map: HashMap<String, Expr> = HashMap::with_capacity(fields.len());
            for f in fields {
                map.insert(f.name.clone(), lower_to_expr(&f.value)?);
            }
            Ok(Expr::RecordLit(map))
        }

        // Parenthesised sub-expression: pass through. The R229 colorer
        // and pretty-printer care about the explicit grouping, but for
        // HM inference `(e)` and `e` are the same term.
        SyntaxNode::Group { inner, .. } => lower_to_expr(inner),

        // Pipeline-parser artifact: a bare top-level expression like
        // `42` or `"hello"` or `{|x|x}` reaches us as
        // `Cmd { name: <the expr>, args: [] }` because the pipeline
        // grammar wraps every stage in `Cmd`. Unwrap that shape when
        // it is safe:
        //
        //   * args must be empty (a real command like `head -n 5`
        //     stays untyped — argparse in stage 4 handles it),
        //   * the head must NOT be `Ident`. A bare identifier like
        //     `ls` is a nullary command invocation, not a variable
        //     reference; typing it as `Var("ls")` would trip
        //     `UnboundVar` and abort turns that R229.M3's Cmd
        //     dispatcher is meant to handle successfully.
        SyntaxNode::Cmd { name, args, .. } if args.is_empty() => match name.as_ref() {
            SyntaxNode::Ident { .. } => Err(TypeStageError::UnsupportedNode(
                "cmd (nullary ident head — defers to command dispatch)".to_owned(),
            )),
            other => lower_to_expr(other),
        },

        // Anything else: pipeline sub-language shapes (Cmd with args,
        // Pipe, Seq, Redirect, Background), datalog shapes
        // (DatalogBlock, Atom, Rule, QVar, InterpVar, NotAtom), and
        // the lambda-side shapes that M4 does not yet model (Match,
        // BinOp, UnaryOp). All surface as UnsupportedNode with a short
        // variant tag so `eval_turn` records inferred_type: None and
        // continues without aborting the turn.
        other => Err(TypeStageError::UnsupportedNode(
            variant_tag(other).to_owned(),
        )),
    }
}

/// Short single-word variant name for [`TypeStageError::UnsupportedNode`]
/// payloads. Duplicates the shape of `crate::turn::variant_name` but
/// kept private here so a change to one does not silently propagate to
/// the other — the two callers have subtly different rendering
/// contracts (executor-fallback message vs. type-stage diagnostic).
fn variant_tag(n: &SyntaxNode) -> &'static str {
    match n {
        SyntaxNode::Cmd { .. } => "cmd",
        SyntaxNode::Pipe { .. } => "pipe",
        SyntaxNode::Seq { .. } => "seq",
        SyntaxNode::Redirect { .. } => "redirect",
        SyntaxNode::Background { .. } => "background",
        SyntaxNode::Group { .. } => "group",
        SyntaxNode::DatalogBlock { .. } => "datalog-block",
        SyntaxNode::Atom { .. } => "atom",
        SyntaxNode::Rule { .. } => "rule",
        SyntaxNode::QVar { .. } => "qvar",
        SyntaxNode::InterpVar { .. } => "interp-var",
        SyntaxNode::NotAtom { .. } => "not-atom",
        SyntaxNode::Lambda { .. } => "lambda",
        SyntaxNode::App { .. } => "app",
        SyntaxNode::Var { .. } => "var",
        SyntaxNode::Let { .. } => "let",
        SyntaxNode::Match { .. } => "match",
        SyntaxNode::BinOp { .. } => "binop",
        SyntaxNode::UnaryOp { .. } => "unaryop",
        SyntaxNode::FieldAccess { .. } => "field-access",
        SyntaxNode::RecordExpr { .. } => "record",
        SyntaxNode::LitStr { .. } => "lit-str",
        SyntaxNode::LitInt { .. } => "lit-int",
        SyntaxNode::LitBool { .. } => "lit-bool",
        SyntaxNode::Ident { .. } => "ident",
    }
}
