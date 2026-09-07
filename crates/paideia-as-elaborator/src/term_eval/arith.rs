//! Arithmetic + equality folder for the term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). Handles the
//! `NodeKind::ExprInfix` arm: `+`, `-`, `*`, `==` on integers, where the
//! operator token is encoded in the op node's `span.byte_start`.

use paideia_as_ast::{AstArena, ExprData, NodeId};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use super::diag::type_mismatch_diag;
use super::value::{Env, EvalResult, Value};

/// Evaluate an `ExprInfix` node.
#[allow(clippy::result_large_err)]
pub(super) fn eval_infix<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
    span: Span,
) -> EvalResult<'a> {
    if let Some(ExprData::Infix { lhs, op, rhs }) = arena.expr_data(expr_id) {
        let lhs_val = super::eval(arena, *lhs, env, type_cache)?;
        let rhs_val = super::eval(arena, *rhs, env, type_cache)?;

        // Extract the operator name from the op node.
        // Heuristic: byte_start of the op's span encodes the operator.
        // Convention:
        // - byte_start == 0: +
        // - byte_start == 1: -
        // - byte_start == 2: *
        // - byte_start == 3: ==
        let op_data = arena.get(*op).ok_or_else(|| {
            Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 506).expect("valid T code"),
            )
            .message("invalid operator node")
            .finish()
        })?;

        let op_kind = op_data.span.byte_start();

        match (lhs_val, rhs_val) {
            (Value::Int(l), Value::Int(r)) => match op_kind {
                0 => Ok(Value::Int(l + r)),   // +
                1 => Ok(Value::Int(l - r)),   // -
                2 => Ok(Value::Int(l * r)),   // *
                3 => Ok(Value::Bool(l == r)), // ==
                _ => Err(Diagnostic::error(
                    DiagnosticCode::new(Category::T, Severity::Error, 507)
                        .expect("valid T code"),
                )
                .message("unknown operator")
                .with_span(op_data.span)
                .finish()),
            },
            (Value::Int(_), other) => Err(type_mismatch_diag("int", &other, span)),
            (other, _) => Err(type_mismatch_diag("int", &other, span)),
        }
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 508).expect("valid T code"),
        )
        .message("expected Infix expression data")
        .finish())
    }
}
