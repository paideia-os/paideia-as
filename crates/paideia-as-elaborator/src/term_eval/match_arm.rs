//! `ExprMatch` evaluator for the term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). The evaluator matches
//! only on the `TermHead` discriminant of a scrutinee `Value::Term`; the
//! expected head is encoded in each arm's pattern-node span byte_start.

use paideia_as_ast::reflect::TermHead;
use paideia_as_ast::{AstArena, ExprData, NodeId};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use super::diag::{nonexhaustive_match_diag, type_mismatch_diag};
use super::value::{Env, EvalResult, Value};

/// Evaluate an `ExprMatch` node.
#[allow(clippy::result_large_err)]
pub(super) fn eval_match<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
    span: Span,
) -> EvalResult<'a> {
    if let Some(ExprData::Match {
        scrutinee,
        arms,
        attrs: _,
    }) = arena.expr_data(expr_id)
    {
        let scrutinee_val = super::eval(arena, *scrutinee, env, type_cache)?;

        // Scrutinee must be a Term for pattern matching on TermHead.
        let scrutinee_term = match scrutinee_val {
            Value::Term(t) => t,
            _ => return Err(type_mismatch_diag("term", &scrutinee_val, span)),
        };

        let scrutinee_head = scrutinee_term.head();

        // Try each arm's pattern.
        for arm in arms {
            let pattern_data = arena.get(arm.pattern).ok_or_else(|| {
                Diagnostic::error(
                    DiagnosticCode::new(Category::T, Severity::Error, 513)
                        .expect("valid T code"),
                )
                .message("invalid pattern node")
                .finish()
            })?;

            let _pattern_kind = pattern_data.kind;

            // Pattern matching on TermHead.
            // Convention: pattern node's span.byte_start encodes the expected head.
            let expected_head_code = pattern_data.span.byte_start();

            let matches = match expected_head_code {
                0 => scrutinee_head == TermHead::Lambda,
                1 => scrutinee_head == TermHead::Literal,
                2 => scrutinee_head == TermHead::Quote,
                3 => scrutinee_head == TermHead::Call,
                4 => true, // wildcard (_)
                _ => false,
            };

            if matches {
                // Evaluate the arm body.
                return super::eval(arena, arm.body, env, type_cache);
            }
        }

        // No arm matched.
        Err(nonexhaustive_match_diag(span))
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 514).expect("valid T code"),
        )
        .message("expected Match expression data")
        .finish())
    }
}
