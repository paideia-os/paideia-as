//! Control-flow + binding evaluators (if / block / let / quote) for the
//! term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405).

use paideia_as_ast::{AstArena, ExprData, NodeId, StmtData, Term};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use super::diag::type_mismatch_diag;
use super::value::{Env, EvalResult, Value};

/// Evaluate an `ExprIf` node.
#[allow(clippy::result_large_err)]
pub(super) fn eval_if<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
    span: Span,
) -> EvalResult<'a> {
    if let Some(ExprData::If {
        cond,
        then_block,
        else_block,
    }) = arena.expr_data(expr_id)
    {
        let cond_val = super::eval(arena, *cond, env, type_cache)?;
        match cond_val {
            Value::Bool(true) => super::eval(arena, *then_block, env, type_cache),
            Value::Bool(false) => {
                if let Some(else_id) = else_block {
                    super::eval(arena, *else_id, env, type_cache)
                } else {
                    Ok(Value::Unit)
                }
            }
            _ => Err(type_mismatch_diag("bool", &cond_val, span)),
        }
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 509).expect("valid T code"),
        )
        .message("expected If expression data")
        .finish())
    }
}

/// Evaluate an `ExprBlock` node: run statements, then return the tail
/// value (or `Unit` when the block has no tail).
#[allow(clippy::result_large_err)]
pub(super) fn eval_block<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
    _span: Span,
) -> EvalResult<'a> {
    if let Some(ExprData::Block { stmts, tail }) = arena.expr_data(expr_id) {
        // Evaluate all statements (typically let bindings).
        for &stmt_id in stmts {
            let _ = super::eval(arena, stmt_id, env, type_cache)?;
        }
        // Evaluate tail if present.
        if let Some(tail_id) = tail {
            super::eval(arena, *tail_id, env, type_cache)
        } else {
            Ok(Value::Unit)
        }
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 510).expect("valid T code"),
        )
        .message("expected Block expression data")
        .finish())
    }
}

/// Evaluate a `StmtLet` node: bind the variable and return `Unit`.
#[allow(clippy::result_large_err)]
pub(super) fn eval_let<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
) -> EvalResult<'a> {
    if let Some(StmtData::Let {
        mutable: _,
        name,
        ty: _,
        value,
        atomic: _,
    }) = arena.stmt_data(expr_id)
    {
        let val = super::eval(arena, *value, env, type_cache)?;
        // Bind the name (for now, assume it's a simple identifier).
        // Use the name node's span byte_start as the variable key.
        let name_data = arena.get(*name).ok_or_else(|| {
            Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 511).expect("valid T code"),
            )
            .message("invalid name node")
            .finish()
        })?;
        let var_name = format!("_var_{}", name_data.span.byte_start());
        env.bind(var_name, val);
        Ok(Value::Unit)
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 512).expect("valid T code"),
        )
        .message("expected Let statement data")
        .finish())
    }
}

/// Evaluate an `ExprQuote` node: return the body as a `Term` value.
#[allow(clippy::result_large_err)]
pub(super) fn eval_quote<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    _span: Span,
) -> EvalResult<'a> {
    if let Some(ExprData::Quote { body }) = arena.expr_data(expr_id) {
        Ok(Value::Term(Term::new(arena, *body)))
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 523).expect("valid T code"),
        )
        .message("expected Quote expression data")
        .finish())
    }
}
