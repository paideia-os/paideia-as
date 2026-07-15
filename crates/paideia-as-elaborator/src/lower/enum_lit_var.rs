//! #1198: rewrite bare enum-variant Vars (unit-payload only) to EnumCons.
//!
//! Pre-#1198, bare enum literals like `A` in `f(A, ...)` lowered to
//! `IrKind::Var` but were never re-classified. `emit_call_args_and_call`'s
//! Var arm couldn't resolve them (not local binding, not module Object) and
//! silently no-opped for arg_idx=0 → silent miscompile.
//!
//! This pass runs after populate_enum_cons_info (which handles the qualified
//! `EnumType::Variant` shape via 2-segment paths). It scans for bare
//! `IrKind::Var` nodes whose source text matches a unit enum variant and
//! rewrites them.

use crate::enum_registry::EnumRegistry;
use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrKind, IrNodeId};
use std::collections::HashMap;
use paideia_as_ast::NodeId;

use super::text_extract::extract_source_text_for_record_cons;

pub(super) fn populate_enum_lit_var_rewrites(
    ir: &mut IrArena,
    ast: &AstArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    enum_registry: &EnumRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    use std::collections::HashSet;

    // 1. Inverse map: IrNodeId -> NodeId.
    let ir_to_ast: HashMap<IrNodeId, NodeId> =
        ast_to_ir.iter().map(|(&ast_id, &ir_id)| (ir_id, ast_id)).collect();

    // 2. Unit variant lookup: name -> Vec<(EnumTypeId, variant_index)>.
    let mut unit_variant_lookup: HashMap<String, Vec<(paideia_as_ir::enum_layout::EnumTypeId, u32)>> = HashMap::new();
    for (&type_id, variants) in &enum_registry.variants {
        for (idx, (variant_name, payload_types)) in variants.iter().enumerate() {
            if payload_types.is_empty() {
                // Unit-variant filter: only index if payload is empty
                unit_variant_lookup
                    .entry(variant_name.clone())
                    .or_insert_with(Vec::new)
                    .push((type_id, idx as u32));
            }
        }
    }

    // 3. Shadow set: names bound by lets/params anywhere in AST.
    //    Reuse pattern from text_extract::build_binding_type_map.
    let mut shadow_names: HashSet<String> = HashSet::new();
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };
        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        // Process item-level Let bindings (ItemData::Let).
        if ast_node.kind == paideia_as_ast::NodeKind::Let {
            if let Some(paideia_as_ast::ItemData::Let { name, .. }) = ast.item_data(ast_id) {
                if let Some(binding_name) =
                    extract_source_text_for_record_cons(ast, source_map, *name)
                {
                    shadow_names.insert(binding_name);
                }
            }
        }

        // Process statement-level Let bindings (StmtData::Let).
        if ast_node.kind == paideia_as_ast::NodeKind::StmtLet {
            if let Some(paideia_as_ast::StmtData::Let { name, .. }) = ast.stmt_data(ast_id) {
                if let Some(binding_name) =
                    extract_source_text_for_record_cons(ast, source_map, *name)
                {
                    shadow_names.insert(binding_name);
                }
            }
        }

        // Lambda parameter names (allows bare variant in fn(A: SomeEnum) -> ... context).
        if ast_node.kind == paideia_as_ast::NodeKind::ExprLambda {
            if let Some(paideia_as_ast::ExprData::Lambda { params, .. }) = ast.expr_data(ast_id) {
                for &pat_id in params {
                    if ast.get(pat_id).map(|n| n.kind) == Some(paideia_as_ast::NodeKind::PatIdent) {
                        if let Some(param_name) =
                            extract_source_text_for_record_cons(ast, source_map, pat_id)
                        {
                            shadow_names.insert(param_name);
                        }
                    }
                }
            }
        }
    }

    // 4. Collect rewrites (immutable read of ir).
    let mut rewrites: Vec<(IrNodeId, paideia_as_ir::EnumConsInfo)> = Vec::new();
    let mut ambig: Vec<(paideia_as_diagnostics::Span, String)> = Vec::new();

    for i in 1..=ir.len() as u32 {
        let Some(app_id) = IrNodeId::new(i) else { continue };
        let Some(node) = ir.get(app_id) else { continue };
        if node.kind != IrKind::App { continue }
        let children = ir.children(app_id).to_vec();
        // Skip children[0] (callee); iterate children[1..] (args).
        for &arg_id in children.iter().skip(1) {
            let Some(arg) = ir.get(arg_id) else { continue };
            if arg.kind != IrKind::Var { continue }
            let Some(&ast_id) = ir_to_ast.get(&arg_id) else { continue };
            let Some(text) = extract_source_text_for_record_cons(ast, source_map, ast_id)
                else { continue };
            if shadow_names.contains(&text) { continue }

            // #1202: Qualified 2-segment shape (Type::Variant) — unambiguous, no shadow check.
            if let Some((seg0, seg1)) = text.split_once("::") {
                if let Some(&type_id) = enum_registry.by_name.get(seg0) {
                    if let Some(variants) = enum_registry.variants.get(&type_id) {
                        if let Some((idx, _)) = variants.iter().enumerate()
                            .find(|(_, (name, payload))| name == seg1 && payload.is_empty())
                        {
                            rewrites.push((arg_id, paideia_as_ir::EnumConsInfo {
                                type_id,
                                variant_index: idx as u32,
                            }));
                            continue;
                        }
                    }
                }
            }

            match unit_variant_lookup.get(text.as_str()) {
                None => {}
                Some(v) if v.len() == 1 => {
                    let (type_id, variant_index) = v[0];
                    rewrites.push((arg_id, paideia_as_ir::EnumConsInfo { type_id, variant_index }));
                }
                Some(v) => {
                    ambig.push((arg.span, format!(
                        "Ambiguous bare enum variant '{}': matches {} enum type(s)",
                        text, v.len()
                    )));
                }
            }
        }
    }

    // 5. Apply rewrites.
    for (arg_id, info) in rewrites {
        if let Some(n) = ir.get_mut(arg_id) { n.kind = IrKind::EnumCons; }
        ir.enum_cons_info_mut().insert(arg_id, info);
    }

    // 6. Emit T0555 for ambiguities.
    for (span, msg) in ambig {
        if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 555) {
            let diag = Diagnostic::error(code)
                .message(msg)
                .with_span(span)
                .finish();
            let _ = sink.emit(diag);
        }
    }
}
