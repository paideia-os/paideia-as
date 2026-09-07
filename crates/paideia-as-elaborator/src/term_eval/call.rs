//! Builtin call evaluator (`ExprCall`) for the term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). The callee node's
//! `span.byte_start` selects one of the reflect-api builtins:
//! - 0 → `kind(t)`   — return the term's `TermHead`.
//! - 1 → `children(t)` — return the term's immediate children.
//! - 2 → `span(t)`   — return an encoded span integer.
//! - 3 → `splice(t)` — pass through the term's node id.
//! - 4 → `elab(t)`   — elaborate the term via `elab_builtin::elab`.

use paideia_as_ast::{AstArena, ExprData, NodeId, Term};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use super::diag::type_mismatch_diag;
use super::value::{Env, EvalResult, Value};

/// Evaluate an `ExprCall` node against the reflect-api builtin surface.
#[allow(clippy::result_large_err)]
pub(super) fn eval_call<'a>(
    arena: &'a AstArena,
    expr_id: NodeId,
    env: &mut Env<'a>,
    type_cache: &mut crate::reflect_api::TypeCache,
    span: Span,
) -> EvalResult<'a> {
    if let Some(ExprData::Call { callee, args }) = arena.expr_data(expr_id) {
        // Determine the callee name.
        let callee_data = arena.get(*callee).ok_or_else(|| {
            Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 515).expect("valid T code"),
            )
            .message("invalid callee node")
            .finish()
        })?;

        // Heuristic: callee's span.byte_start encodes the builtin function.
        // Convention:
        // - 0: kind
        // - 1: children
        // - 2: span
        // - 3: splice
        // - 4: elab
        let builtin_code = callee_data.span.byte_start();

        match builtin_code {
            0 => {
                // kind(t) builtin
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 516)
                            .expect("valid T code"),
                    )
                    .message("kind() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                match arg_val {
                    Value::Term(t) => Ok(Value::Head(t.head())),
                    _ => Err(type_mismatch_diag("term", &arg_val, span)),
                }
            }

            1 => {
                // children(t) builtin
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 517)
                            .expect("valid T code"),
                    )
                    .message("children() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                match arg_val {
                    Value::Term(t) => {
                        let child_terms: Vec<Value> = t
                            .children()
                            .iter()
                            .map(|child| Value::Term(*child))
                            .collect();
                        Ok(Value::List(child_terms))
                    }
                    _ => Err(type_mismatch_diag("term", &arg_val, span)),
                }
            }

            2 => {
                // span(t) builtin
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 518)
                            .expect("valid T code"),
                    )
                    .message("span() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                match arg_val {
                    Value::Term(t) => {
                        // Return a representation of the span.
                        // For now, return a Value encoding the byte_start and byte_len.
                        let s = t.span();
                        let encoded =
                            (s.byte_start() as i64) * 1000000 + (s.byte_len() as i64);
                        Ok(Value::Int(encoded))
                    }
                    _ => Err(type_mismatch_diag("term", &arg_val, span)),
                }
            }

            3 => {
                // splice(t) builtin (phase-2-m6+)
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 522)
                            .expect("valid T code"),
                    )
                    .message("splice() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                // Use the splice module to handle splicing.
                // For m2-006, this is a pass-through: return the Term's NodeId
                // wrapped in a Value::Term, with the call site span for diagnostics.
                crate::splice::splice(arg_val, span).map(|node_id| {
                    // Wrap the spliced node ID back into a Value::Term.
                    Value::Term(Term::new(arena, node_id))
                })
            }

            4 => {
                // elab(t) builtin (phase-2-m8+)
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 524)
                            .expect("valid T code"),
                    )
                    .message("elab() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                crate::elab_builtin::elab(arena, arg_val, type_cache, span)
            }

            _ => Err(Diagnostic::error(
                DiagnosticCode::new(Category::T, Severity::Error, 519).expect("valid T code"),
            )
            .message("unknown builtin function")
            .with_span(callee_data.span)
            .finish()),
        }
    } else {
        Err(Diagnostic::error(
            DiagnosticCode::new(Category::T, Severity::Error, 520).expect("valid T code"),
        )
        .message("expected Call expression data")
        .finish())
    }
}
