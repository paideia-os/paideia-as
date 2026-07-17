//! Closure-vs-fn-ptr dispatch — issue #994 piece 2.
//!
//! Converts function-local (`StmtLet`) bindings whose declared type is a
//! closure type (`|T1, T2| -> R !{E} @{C}`, `TypeData::Closure`) so that the
//! IR represents them as `IrKind::ClosureCons` wrapping the lambda body,
//! rather than a bare `IrKind::Lambda`. This is the signal `emit_closure_cons`
//! and `emit_block_body`'s ClosureCons arm (#1233 Phase B) key off of.
//!
//! Also fires T0571 when a lambda annotated with a plain function-pointer
//! type (`fn(T) -> R`, `TypeData::FnPtr`) actually captures free variables —
//! that combination is a contradiction (a bare code pointer has nowhere to
//! store an environment) and should have used closure-type syntax instead.
//!
//! # Why not module-level (`ItemData::Let`) bindings?
//!
//! `ClosureCons` materialises a 16-byte fat pointer on the *caller's stack
//! frame* (see `emit_closure_cons` / `precompute_caller_frame`), which
//! requires an enclosing `Lambda` whose frame layout has been precomputed.
//! Module-level lets have no such enclosing frame, and (since nothing at
//! module scope can ever go out of scope) a module-level closure can never
//! actually capture anything free — it structurally degenerates to a bare
//! code pointer. So module-level closure-typed lets are deliberately left as
//! plain `Lambda` nodes; only function-local (`StmtLet`) bindings are
//! candidates for conversion here.
//!
//! # Ordering requirement
//!
//! Must run AFTER the `LinearityWalker` pass (which populates
//! `arena.captures()` — issue #994 piece 1) and BEFORE `EmitWalker::walk`
//! (whose `register_closure_body_symbols` / `precompute_caller_frame`
//! pre-passes assume any `ClosureCons` nodes and `ClosureMetaTable` entries
//! already exist).

use std::collections::HashMap;

use paideia_as_ast::{AstArena, NodeId, NodeKind, StmtData, TypeData};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, Span};
use paideia_as_ir::{CaptureKind as MetaCaptureKind, CaptureMeta, ClosureMeta, IrArena, IrKind, IrNodeId};

use crate::check_fn_ptr_sig::T_FN_PTR_SIG_MISMATCH;
use crate::check_lambda::T_FN_PTR_WITH_CAPTURES;

/// Construct a DiagnosticCode in the T (Type) category.
fn t_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T diagnostic code")
}

/// Count a lambda's actual arity (number of parameters).
///
/// Mirrors the two representations `register_nested_lambda_params`
/// (emit_visit_lambda.rs) already knows how to register:
/// - **Flattened**: `arena.lambda_params()` populated with all params directly.
/// - **Curried nesting**: `fn (a) (b) -> body` lowers to `Lambda(Lambda(body))`
///   — arity is 1 + however many nested single-child Lambdas follow.
fn count_lambda_arity(ir: &IrArena, lambda_id: IrNodeId) -> usize {
    if let Some(params) = ir.lambda_params().get(lambda_id) {
        if !params.is_empty() {
            return params.len();
        }
    }
    let mut count = 1usize;
    let mut current = lambda_id;
    loop {
        let children = ir.children(current);
        let Some(&body_id) = children.first() else {
            break;
        };
        let Some(body_node) = ir.get(body_id) else {
            break;
        };
        if body_node.kind == IrKind::Lambda {
            count += 1;
            current = body_id;
        } else {
            break;
        }
    }
    count
}

/// Convert closure-typed, function-local `let` bindings from `Lambda` to
/// `ClosureCons`, and fire T0571 for fn-ptr-typed lambdas with captures.
///
/// Returns the number of bindings converted to `ClosureCons` (useful for
/// tests / diagnostics; not required for correctness).
pub fn convert_closure_lets(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    sink: &mut dyn DiagnosticSink,
) -> usize {
    // Pass 1 (immutable borrow of `ast`): classify every StmtLet-with-Lambda-RHS
    // binding by its type annotation's discriminant.
    let mut closure_lets: Vec<(NodeId, NodeId, usize)> = Vec::new(); // (let_ast_id, lambda_ast_id, declared_arity)
    let mut fn_ptr_lets: Vec<(NodeId, NodeId)> = Vec::new(); // (let_ast_id, lambda_ast_id)

    for i in 1..=ast.len() {
        let Some(ast_id) = NodeId::new(i as u32) else { continue };
        let Some(node) = ast.get(ast_id) else { continue };
        if node.kind != NodeKind::StmtLet {
            continue;
        }
        let Some(StmtData::Let { ty: Some(ty_node), value, .. }) = ast.stmt_data(ast_id) else {
            continue;
        };
        let value_id = *value;
        let Some(value_node) = ast.get(value_id) else { continue };
        if value_node.kind != NodeKind::ExprLambda {
            continue;
        }

        match ast.type_data(*ty_node) {
            Some(TypeData::Closure { params, .. }) => {
                closure_lets.push((ast_id, value_id, params.len()));
            }
            Some(TypeData::FnPtr { .. }) => fn_ptr_lets.push((ast_id, value_id)),
            _ => {}
        }
    }

    // Pass 2: T0571 — fn-ptr annotation on a lambda that actually captures.
    for (let_ast_id, lambda_ast_id) in &fn_ptr_lets {
        let Some(&lambda_ir_id) = ast_to_ir.get(lambda_ast_id) else { continue };
        if ir.captures().has_captures(lambda_ir_id) {
            let span: Span = ast
                .get(*lambda_ast_id)
                .or_else(|| ast.get(*let_ast_id))
                .map(|n| n.span)
                .unwrap_or_else(|| Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0));
            let _ = sink.emit(
                Diagnostic::error(t_code(T_FN_PTR_WITH_CAPTURES))
                    .message(
                        "lambda captures free variables but is annotated as fn-ptr — \
                         did you mean to use closure type |T| -> R instead?"
                            .to_string(),
                    )
                    .with_span(span)
                    .finish(),
            );
        }
    }

    // Pass 3: ClosureCons conversion for closure-typed lets.
    let mut converted = 0usize;
    for (let_ast_id, lambda_ast_id, declared_arity) in closure_lets {
        let (Some(&let_ir_id), Some(&lambda_ir_id)) =
            (ast_to_ir.get(&let_ast_id), ast_to_ir.get(&lambda_ast_id))
        else {
            continue;
        };

        // T0535: the closure type's declared parameter count must match the
        // lambda literal's actual arity. A mismatch here can't be converted
        // to a well-formed ClosureCons (its body would expect a different
        // register/frame shape than its callers assume), so skip conversion
        // once the diagnostic fires.
        let actual_arity = count_lambda_arity(ir, lambda_ir_id);
        if actual_arity != declared_arity {
            let span = ir.get(lambda_ir_id).map(|n| n.span).unwrap_or_else(|| {
                paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0)
            });
            let _ = sink.emit(
                Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                    .message(format!(
                        "arity mismatch: closure type declares {} parameter(s), lambda has {}",
                        declared_arity, actual_arity
                    ))
                    .with_span(span)
                    .finish(),
            );
            continue;
        }

        // Mangled body symbol: MUST match register_closure_body_symbols's own
        // recomputation (emit_walker.rs) byte-for-byte, since emit_closure_cons
        // uses this name (not a fresh lookup) for the `lea [rip + name]` target.
        let binding_name = ir
            .binding_names()
            .get(let_ir_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("_let_{}", let_ir_id.get()));
        let mangled_name = format!("closure_{}_{}", binding_name, lambda_ir_id.get());

        // Build CaptureMeta entries (name + sequential 8-byte offset) from the
        // piece-1 CapturesTable analysis, in the same (symbol-sorted, hence
        // deterministic) order analyze_captures produced them.
        let mut captures_meta = Vec::new();
        let mut offset: i32 = 0;
        if let Some(analyzed) = ir.captures().get(lambda_ir_id) {
            for cap in analyzed {
                let name = IrNodeId::new(cap.symbol)
                    .and_then(|sym_id| ir.binding_names().get(sym_id))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("_sym_{}", cap.symbol));
                let kind = if cap.kind == 2 {
                    MetaCaptureKind::Consume
                } else {
                    MetaCaptureKind::Reference
                };
                captures_meta.push(CaptureMeta { name, offset, kind });
                offset += 8;
            }
        }
        let env_size = captures_meta.len() as u64 * 8;

        ir.closure_meta_mut().insert(
            lambda_ir_id,
            ClosureMeta {
                mangled_name,
                captures: captures_meta,
                env_size,
            },
        );

        // Wrap the lambda: alloc a new ClosureCons node with the lambda as its
        // sole child, then rewrite the Let's child list to point at it instead
        // of directly at the lambda.
        let span = ir.get(lambda_ir_id).map(|n| n.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0)
        });
        let cc_id = ir.alloc_with_children(IrKind::ClosureCons, span, [lambda_ir_id]);

        if let Some(let_children) = ir.children_mut(let_ir_id) {
            for child in let_children.iter_mut() {
                if *child == lambda_ir_id {
                    *child = cc_id;
                }
            }
        }

        converted += 1;
    }

    converted
}
