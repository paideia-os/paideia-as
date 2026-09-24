//! Builtin call evaluator (`ExprCall`) for the term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). The callee node's
//! `span.byte_start` selects one of the reflect-api builtins:
//! - 0 → `kind(t)`     — return the term's `TermHead`.
//! - 1 → `children(t)` — return the term's immediate children.
//! - 2 → `span(t)`     — return an encoded span integer.
//! - 3 → `splice(t)`   — pass through the term's node id.
//! - 4 → `elab(t)`     — elaborate the term via `elab_builtin::elab`.
//! - 5 → `elab_error(t)` — forward a hosted-DSL error diagnostic to the
//!                         `paideia-as-reflection` router (R220.M9 wiring
//!                         completed in R220.M12).
//! - 6 → `elab_warn(t)`  — forward a hosted-DSL warning to the same
//!                         router.  Returns `Value::Unit`.

use paideia_as_ast::{AstArena, ExprData, NodeId, Term};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_reflection::{ElabError, ElabWarn, current_handle, hosted_error_code, hosted_warn_code};

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

            5 => {
                // Elab.elab_error(t) — hosted-DSL error forwarder.
                //
                // R220.M9 landed the router (`paideia-as-reflection::dsl_diag`);
                // R220.M12 wires this dispatcher into it.  When a router
                // handle is installed (LSP server / `paideia-as check`
                // startup), forward the payload there so the diagnostic
                // reaches the editor / SARIF sink alongside native passes;
                // otherwise fall back to a plain elaborator diagnostic so
                // the error is still visible.
                //
                // FIXME(hosted-str-value): `Value` does not yet carry `Str`
                // or `Span` variants (R229 scope), so the payload we can
                // reach at this seam is the arg-Term's span + a synthetic
                // message.  Full hosted-DSL usability requires the R229
                // Value-expansion round; the router pathway lands today so
                // the wiring is testable end-to-end from the dispatcher.
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 525)
                            .expect("valid T code"),
                    )
                    .message("elab_error() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                let (msg, arg_span) = match &arg_val {
                    Value::Term(t) => ("hosted-DSL error".to_string(), t.span()),
                    other => (other.display(), span),
                };
                let payload = ElabError {
                    message: msg,
                    span: arg_span,
                };
                if let Some(handle) = current_handle() {
                    // Push into the router — dropped overflow is treated
                    // as "handle already full" and non-fatal at this seam;
                    // the sink's own bail policy has already fired.
                    let _ = handle.emit_elab_error(&payload, hosted_error_code(9001));
                    // `elab_error : Never` — return an error to preserve
                    // the "does-not-return" surface language contract.
                    Err(Diagnostic::error(
                        DiagnosticCode::new(Category::F, Severity::Error, 1200)
                            .expect("valid F code"),
                    )
                    .message(payload.message)
                    .with_span(payload.span)
                    .finish())
                } else {
                    // No router installed — surface as a native elaborator
                    // diagnostic so hosted-DSL errors are never silently
                    // dropped when run outside `paideia-as check` / LSP.
                    Err(Diagnostic::error(
                        DiagnosticCode::new(Category::F, Severity::Error, 1200)
                            .expect("valid F code"),
                    )
                    .message(payload.message)
                    .with_span(payload.span)
                    .finish())
                }
            }

            6 => {
                // Elab.elab_warn(t) — hosted-DSL warning forwarder.
                // Symmetric to arm 5; returns `Value::Unit` to model the
                // `elab_warn : ()` surface-language return type.
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        DiagnosticCode::new(Category::T, Severity::Error, 526)
                            .expect("valid T code"),
                    )
                    .message("elab_warn() expects exactly 1 argument")
                    .with_span(span)
                    .finish());
                }
                let arg_val = super::eval(arena, args[0], env, type_cache)?;
                let (msg, arg_span) = match &arg_val {
                    Value::Term(t) => ("hosted-DSL warning".to_string(), t.span()),
                    other => (other.display(), span),
                };
                let payload = ElabWarn {
                    message: msg,
                    span: arg_span,
                };
                if let Some(handle) = current_handle() {
                    let _ = handle.emit_elab_warn(&payload, hosted_warn_code(9001));
                }
                // Warnings do not abort evaluation; return unit.
                Ok(Value::Unit)
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
