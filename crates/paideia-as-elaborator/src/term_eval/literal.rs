//! Literal parsing + literal / path evaluation for the term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). Holds:
//! - Byte-level literal decoders used by unit tests (`parse_int_literal`,
//!   `parse_bool_literal`, `extract_op_name`).
//! - Dispatch arms for `NodeKind::ExprLiteral` and `NodeKind::ExprPath`.

use paideia_as_ast::{AstArena, ExprData, NodeId};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use super::diag::undef_ident_diag;
use super::value::{Env, EvalResult, Value};

/// Parse an integer literal from a span's byte range.
///
/// In unit tests, the span's byte_start and byte_len encode the literal value
/// directly (e.g., span with byte_start=42, byte_len=0 encodes the int 42).
/// This is a test-only convention; in the real elaborator, text would be
/// recovered from the source buffer via the SourceMap.
pub(super) fn parse_int_literal(span: Span) -> Option<i64> {
    let byte_start = span.byte_start();
    if byte_start > i64::MAX as u32 {
        None
    } else {
        Some(byte_start as i64)
    }
}

/// Parse a boolean literal from a span's byte range.
///
/// Convention: byte_len == 1 means `true`, byte_len >= 2 means `false`.
#[allow(dead_code)]
pub(super) fn parse_bool_literal(span: Span) -> bool {
    span.byte_len() == 1
}

/// Try to extract an operator name from a simple identifier node.
#[allow(dead_code)]
pub(super) fn extract_op_name(_arena: &AstArena, _op_id: NodeId) -> Option<String> {
    // In the arena, an operator is typically an Ident node. We would need
    // to recover the text from the source buffer, but for testing purposes,
    // we'll use a heuristic: the span byte_start encodes the operator.
    // For now, return None and let the caller handle unknown ops.
    None
}

/// Evaluate an `ExprLiteral` node.
///
/// Disambiguates between integer and boolean using `byte_len`:
/// - `byte_len == 0`: integer, value in `byte_start`.
/// - `byte_len == 1`: boolean true.
/// - `byte_len >= 2`: boolean false.
#[allow(clippy::result_large_err)]
pub(super) fn eval_literal<'a>(span: Span) -> EvalResult<'a> {
    if span.byte_len() == 0 {
        if let Some(n) = parse_int_literal(span) {
            Ok(Value::Int(n))
        } else {
            Err(Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 506).expect("valid T code"),
            )
            .message("integer literal out of range")
            .with_span(span)
            .finish())
        }
    } else {
        Ok(Value::Bool(span.byte_len() == 1))
    }
}

/// Evaluate an `ExprPath` node (single-segment identifier lookup).
#[allow(clippy::result_large_err)]
pub(super) fn eval_path<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    span: Span,
    env: &Env<'a>,
) -> EvalResult<'a> {
    if let Some(ExprData::Path { segments }) = arena.expr_data(expr_id) {
        if segments.len() == 1 {
            let seg_id = segments[0];
            if let Some(seg_data) = arena.get(seg_id) {
                // Use the segment's span byte_start as a synthetic name key.
                let name_key = format!("_var_{}", seg_data.span.byte_start());
                env.lookup(&name_key)
                    .ok_or_else(|| undef_ident_diag(&name_key, span))
            } else {
                Err(Diagnostic::error(
                    DiagnosticCode::new(Category::T, Severity::Error, 503)
                        .expect("valid T code"),
                )
                .message("invalid segment node")
                .finish())
            }
        } else {
            Err(Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 504).expect("valid T code"),
            )
            .message("multi-segment paths not yet supported in evaluator")
            .with_span(span)
            .finish())
        }
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 505).expect("valid T code"),
        )
        .message("expected Path expression data")
        .finish())
    }
}
