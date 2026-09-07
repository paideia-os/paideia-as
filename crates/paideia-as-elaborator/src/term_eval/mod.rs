//! Typed-term evaluator for macro bodies.
//!
//! Implements a small-step evaluator over [`Term`] to enable pure computation
//! in macro expansions. Phase-2-m5 supports:
//! - Literals (integer + bool).
//! - Let bindings: `let x = e1 in e2`.
//! - Pattern-match on `TermHead`: `match t with | TermHead::Lambda => e1 | _ => e2`.
//! - Calls to the reflect_api functions: `kind(t)`, `children(t)`, `span(t)`.
//! - Splice operation: `splice(t)` (phase-2-m6+).
//! - Conditionals (`if cond then e1 else e2`).
//! - Arithmetic on integers (`+`, `-`, `*`).
//! - Identifier lookup from the environment.
//!
//! # Design
//!
//! The evaluator is pure functional — no side effects, no mutation of the arena,
//! no capability-requiring operations. It dispatches on the AST node kind and
//! evaluates bottom-up, threading an environment of bindings through let and
//! match contexts.
//!
//! Function abstraction and application are deferred to a later issue if needed;
//! most macro bodies pattern-match on the input term and return a constant.
//!
//! # Module layout
//!
//! Split into per-arm submodules in the phase-2 God-file refactor
//! (issue #1412 under umbrella #1405). Zero behavior change from the
//! prior single-file version; every previously public path is preserved
//! at `crate::term_eval::*` via `pub use` here:
//!
//! - [`value`]     — `Value`, `EvalResult`, `Env`, `DEFAULT_FUEL`,
//!   `DEFAULT_STACK_DEPTH`.
//! - [`diag`]      — diagnostic constructors (T500-T524, M0311).
//! - [`literal`]   — literal decoders and `ExprLiteral` / `ExprPath` arms.
//! - [`arith`]     — `ExprInfix` arithmetic + equality folder.
//! - [`control`]   — `ExprIf`, `ExprBlock`, `StmtLet`, `ExprQuote` arms.
//! - [`match_arm`] — `ExprMatch` on `TermHead`.
//! - [`call`]      — `ExprCall` builtin dispatch (kind / children / span
//!   / splice / elab).

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

mod arith;
mod call;
mod control;
mod diag;
mod literal;
mod match_arm;
mod value;

#[cfg(test)]
mod tests_basic;
#[cfg(test)]
mod tests_calls;
#[cfg(test)]
mod tests_control;
#[cfg(test)]
mod tests_limits;

pub use value::{Env, EvalResult, Value, DEFAULT_FUEL, DEFAULT_STACK_DEPTH};

use diag::{depth_exceeded_diag, fuel_exhausted_diag};

/// Evaluate an AST node (the macro body expression) in `env`.
///
/// Phase-2-m5 supports:
/// - Literals (Int, Bool)
/// - Path (single-segment identifiers for lookup)
/// - Infix (`+`, `-`, `*`, `==` on integers)
/// - If/Then/Else
/// - Let x = e1 in e2
/// - Match t with | Head1 => e1 | _ => e2
/// - Calls to reflect-api functions: kind, children, span, elab
///
/// Tracks fuel (evaluation steps) and stack depth to detect infinite loops
/// and unbounded recursion. Returns M0311 if fuel is exhausted or depth limit exceeded.
///
/// Returns a `Diagnostic` on type mismatch, undefined identifier, or
/// non-exhaustive match.
#[allow(clippy::result_large_err)]
pub fn eval<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
) -> EvalResult<'a> {
    // Fetch the node data.
    let node_data = arena.get(expr_id).ok_or_else(|| {
        Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 503).expect("valid T code"),
        )
        .message("internal error: invalid node ID")
        .finish()
    })?;

    let span = node_data.span;

    // Check fuel: if exhausted, fail fast.
    if env.fuel == 0 {
        return Err(fuel_exhausted_diag(span, DEFAULT_FUEL));
    }
    env.fuel -= 1;

    // Check depth: if exceeded, fail fast.
    if env.depth >= env.max_depth {
        return Err(depth_exceeded_diag(span, env.depth));
    }
    env.depth += 1;

    // Evaluate the node; decrement depth on exit.
    let result = eval_inner(arena, expr_id, node_data, span, env, type_cache);
    env.depth -= 1;
    result
}

/// Internal evaluator body: dispatches on the node kind after fuel and depth checks.
///
/// Each match arm forwards to a per-kind helper in one of the submodules; the
/// helpers reuse the recursive `eval` entry point above so fuel and depth
/// accounting stay centralised here.
#[allow(clippy::result_large_err)]
fn eval_inner<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    node_data: &paideia_as_ast::NodeData,
    span: Span,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
) -> EvalResult<'a> {
    match node_data.kind {
        NodeKind::ExprLiteral => literal::eval_literal(span),
        NodeKind::ExprPath => literal::eval_path(arena, expr_id, span, env),
        NodeKind::ExprInfix => arith::eval_infix(arena, expr_id, env, type_cache, span),
        NodeKind::ExprIf => control::eval_if(arena, expr_id, env, type_cache, span),
        NodeKind::ExprBlock => control::eval_block(arena, expr_id, env, type_cache, span),
        NodeKind::StmtLet => control::eval_let(arena, expr_id, env, type_cache),
        NodeKind::ExprMatch => match_arm::eval_match(arena, expr_id, env, type_cache, span),
        NodeKind::ExprCall => call::eval_call(arena, expr_id, env, type_cache, span),
        NodeKind::ExprQuote => control::eval_quote(arena, expr_id, span),

        _ => Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 521).expect("valid T code"),
        )
        .message("unsupported expression kind in evaluator")
        .with_span(span)
        .finish()),
    }
}
