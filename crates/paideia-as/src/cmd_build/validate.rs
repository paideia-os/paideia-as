//! Late attribute-validation passes: P0284 / P0286 / U1620 anchor.
//!
//! Extracted from `cmd_build.rs` (2026-09-07 refactor, issue #1401).
//! These passes fire after symbol name resolution and before the address-of
//! pre-emit pass. Each pass walks the AST arena once and emits diagnostics
//! for Let bindings whose attribute placement is invalid.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{DiagnosticSink, VecSink};

/// Phase 19 PA19-r19-010: P0284 validation pass.
/// Check all Let bindings with `@link_section` directive. If the value is
/// lambda-shaped, emit P0284 error and reject (deferred to pa-r19-010b).
pub(super) fn validate_link_section_on_non_lambda(
    arena: &AstArena,
    sink: &mut VecSink,
) {
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(paideia_as_ast::ItemData::Let {
                        link_section: Some(_),
                        value: value_id,
                        ..
                    }) = arena.item_data(ast_id)
                    {
                        // Check if the value is a lambda (ExprLambda)
                        if let Some(value_node) = arena.get(*value_id) {
                            if value_node.kind == paideia_as_ast::NodeKind::ExprLambda {
                                // Emit P0284: lambda bindings cannot use @link_section
                                let code = paideia_as_diagnostics::DiagnosticCode::new(
                                    paideia_as_diagnostics::Category::P,
                                    paideia_as_diagnostics::Severity::Error,
                                    284,
                                ).expect("valid P0284 code");
                                let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                    .message("lambda-shaped bindings cannot use @link_section")
                                    .with_span(value_node.span)
                                    .finish();
                                let _ = sink.emit(diag);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Phase 19 PA19-r19-001: P0286 validation pass.
/// Check all Let bindings with `@abi` directive. If the value is NOT lambda-shaped,
/// emit P0286 error and reject.
pub(super) fn validate_abi_on_lambda(arena: &AstArena, sink: &mut VecSink) {
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(paideia_as_ast::ItemData::Let {
                        abi: Some(_),
                        value: value_id,
                        ..
                    }) = arena.item_data(ast_id)
                    {
                        // Check if the value is a lambda (ExprLambda)
                        if let Some(value_node) = arena.get(*value_id) {
                            if value_node.kind != paideia_as_ast::NodeKind::ExprLambda {
                                // Emit P0286: @abi is only valid on lambda-shaped bindings
                                let code = paideia_as_diagnostics::DiagnosticCode::new(
                                    paideia_as_diagnostics::Category::P,
                                    paideia_as_diagnostics::Severity::Error,
                                    286,
                                ).expect("valid P0286 code");
                                let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                    .message("@abi is only valid on function-shaped bindings")
                                    .with_span(value_node.span)
                                    .finish();
                                let _ = sink.emit(diag);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// v0.21-001 (#1277, closes #1011): U1620 gate — MS x64 emit path.
///
/// Historical: PA19-r19-006 narrowed this gate to only pass Path / Literal /
/// Infix(+, ident, literal) bodies with ≤4 params. That narrowing was
/// defensive at r19 close because several body-shape lowerings hardcoded
/// SysV RDI as the parameter source register (emit_bitnot_lambda,
/// emit_cast_lambda_with_shape, emit_double_lambda, emit_shl_*_lambda).
///
/// v0.21-001 opens the callee-side of the MS x64 ABI:
/// - The hardcoded-RDI arms now route through `param_index_to_reg_for_abi`
///   (see emit_arith_lambda.rs / emit_lambda.rs).
/// - The generic Var-expr lowerer (emit_var_assign_expr_to_reg) resolves
///   parameter registers via local_bindings, which
///   `register_nested_lambda_params` populates with MS_ARG_REGS when
///   `@abi("ms")` is set.
/// - Args 5+ live above the shadow space; the callee-side `param_index_
///   to_reg_for_abi` returns None for idx≥4, so `register_nested_lambda_
///   params` silently skips them. A body reference to one fires T0540
///   ("Var … not found in bindings") — a cleaner diagnostic than a blanket
///   U1620 up-front rejection, and correct for the common case (`efi_main`
///   only reads image_handle in RCX and ignores rest).
///
/// The only remaining U1620 case is deliberately empty: every AST shape
/// reaches a lowering arm, and shape-specific diagnostics fire from the
/// elaborator when they can't complete. Leave the pass in place as an
/// anchor for future narrow rejections (e.g. XMM float args, aggregate
/// return by-hidden-pointer) rather than deleting the site.
pub(super) fn validate_ms_x64_emit_anchor(arena: &AstArena, sink: &mut VecSink) {
    let _ = &sink; // reserved for future narrow rejections
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(paideia_as_ast::ItemData::Let {
                        abi: Some(paideia_as_ast::CallingConvention::Ms),
                        value: _,
                        ..
                    }) = arena.item_data(ast_id)
                    {
                        // No shape rejections at this pass. All previously-U1620'd
                        // shapes now route through the elaborator's ABI-aware
                        // lowering. Downstream diagnostics (T0540, T0521) fire
                        // for genuinely-unsupported patterns (stack-arg reads).
                    }
                }
            }
        }
    }
}
