//! Address-of pre-emit pass + T0535 fn-ptr assignment checks.
//!
//! Extracted from `cmd_build.rs` (2026-09-07 refactor, issue #1401).
//!
//! PA-R17-003 / #988: Resolve `&fn_name` operands and populate
//! AddrOfSideTable. Runs after the SymbolTable is populated and before the
//! data-table loop.
//!
//! PA-r17-003b (#1038) / PA-r17-1076 (#1076) / #1074: T0535 signature check
//! is wired in this pass for both top-level and record-field fn-ptr LHS
//! types.
//!
//! #988 v2: keyed by rhs_id (Borrow node) instead of let_id to disambiguate
//! multiple Borrow per Let (needed for record literals with multiple fnptr
//! fields).
//!
//! #1310: `[u64; N]` arrays of `&sym` (Borrow) elements — a table of
//! symbol addresses — are recognised and populated separately since T0535
//! (a fn-ptr scalar LHS check) does not apply to array elements.

use std::collections::HashMap;

use paideia_as_ast::{AstArena, ItemData, NodeId as AstNodeId, StmtData};
use paideia_as_diagnostics::{DiagnosticSink, FileId, SourceMap, VecSink};
use paideia_as_effects::EffectInterner;
use paideia_as_elaborator::{EnumRegistry, LoweringResult, StructRegistry};
use paideia_as_ir::IrNodeId;
use paideia_as_types::{CapSetInterner, Subst, TypeInterner};

use super::addr_of;
use super::addr_of::extract_var_name_from_operand;

/// PA-R17-003 / #988 / #1310 / #1074: address-of pre-emit + T0535 checks.
///
/// This is a no-op when `lowering.ir.is_empty()`.
pub(super) fn run_addr_of_pass(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    registry: &StructRegistry,
    enum_registry: &EnumRegistry,
    types: &mut TypeInterner,
    effects: &mut EffectInterner,
    caps: &mut CapSetInterner,
    lowering: &mut LoweringResult,
    sink: &mut VecSink,
) {
    if lowering.ir.is_empty() {
        return;
    }

    let arena_len = lowering.ir.len();

    // PA-r17-1076 (#1076): Build a binding name → AST node ID map for T0535 checks.
    // Scan the AST arena to map binding names to their Let node IDs.
    // This enables derive_fn_sig_from_source_binding to read the LHS type annotation.
    let mut binding_name_to_ast_id: HashMap<String, AstNodeId> = HashMap::new();
    let content_ref = source_map.content(file);

    for i in 0..arena.len() {
        if let Some(ast_id) = AstNodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                // Handle ItemData::Let (module-level)
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(paideia_as_ast::ItemData::Let { name: name_id, .. }) =
                        arena.item_data(ast_id)
                    {
                        if let Some(name_node) = arena.get(*name_id) {
                            let span = name_node.span;
                            let start = span.byte_start() as usize;
                            let len = span.byte_len() as usize;
                            if start + len <= content_ref.len() {
                                let binding_text = content_ref[start..start + len].to_string();
                                // First-match semantics: if binding name exists, don't overwrite
                                binding_name_to_ast_id.entry(binding_text).or_insert(ast_id);
                            }
                        }
                    }
                }
                // Handle StmtData::Let (statement-level)
                else if node.kind == paideia_as_ast::NodeKind::StmtLet {
                    if let Some(paideia_as_ast::StmtData::Let { name: name_id, .. }) =
                        arena.stmt_data(ast_id)
                    {
                        if let Some(name_node) = arena.get(*name_id) {
                            let span = name_node.span;
                            let start = span.byte_start() as usize;
                            let len = span.byte_len() as usize;
                            if start + len <= content_ref.len() {
                                let binding_text = content_ref[start..start + len].to_string();
                                // First-match semantics: if binding name exists, don't overwrite
                                binding_name_to_ast_id.entry(binding_text).or_insert(ast_id);
                            }
                        }
                    }
                }
            }
        }
    }

    // Collect all address-of entries first to avoid borrow issues.
    // Also collect let_id for T0535 checking.
    // #1074: Extended to track record-field context: Option<(rc_ir_id, field_idx0)>
    let mut addr_of_entries: Vec<(IrNodeId, IrNodeId, String, Option<(IrNodeId, usize)>)> = Vec::new();

    // #1310: ArrayLit elements with Borrow (`&sym`) children — a table of symbol
    // addresses such as `pub let _klog_files : [u64; 205] = [&name_file_0, ...]`.
    // Tracked separately from `addr_of_entries` because that vector is also the
    // T0535 fn-ptr-assignment check queue, and T0535 is about a *scalar* LHS type
    // being a function pointer; it has no meaning for `[u64; N]` array elements.
    // Only the AddrOfSideTable insertion is needed here so the ArrayLit data-packing
    // pass below can resolve each element's target symbol.
    let mut array_addr_of_entries: Vec<(IrNodeId, String)> = Vec::new();

    for i in 1..=arena_len as u32 {
        if let Some(let_id) = IrNodeId::new(i) {
            if let Some(node) = lowering.ir.get(let_id) {
                if node.kind == paideia_as_ir::IrKind::Let {
                    let children: Vec<_> = lowering.ir.children(let_id).iter().copied().collect();
                    // Look for an RHS with IrKind::Borrow
                    for rhs_id in &children {
                        if let Some(rhs_node) = lowering.ir.get(*rhs_id) {
                            if rhs_node.kind == paideia_as_ir::IrKind::Borrow {
                                // Locate the Borrow's single child (the operand).
                                let borrow_children: Vec<_> = lowering.ir.children(*rhs_id).iter().copied().collect();
                                if let Some(operand_id) = borrow_children.first() {
                                    // Use helper to extract and validate var_name
                                    if let Some(var_name) = extract_var_name_from_operand(
                                        *operand_id,
                                        lowering,
                                        source_map,
                                        file,
                                        addr_of::AddrOfPolicy::FunctionOnly,
                                        sink,
                                    ) {
                                        // #988 v2: Push (let_id, rhs_id, var_name, None) for T0535 checking
                                        // #1074: Tag as None (not from record field)
                                        addr_of_entries.push((let_id, *rhs_id, var_name, None));
                                    }
                                }
                            }
                        }
                    }

                    // #988 v2: Also handle RecordCons fields with Borrow children
                    for rhs_id in &children {
                        if let Some(rhs_node) = lowering.ir.get(*rhs_id) {
                            if rhs_node.kind == paideia_as_ir::IrKind::RecordCons {
                                // Skip type_name child (index 0), process field children
                                let field_children: Vec<_> = lowering.ir.children(*rhs_id).iter().copied().collect();
                                for (field_idx, &field_id) in field_children.iter().enumerate() {
                                    if field_idx == 0 {
                                        // Skip type_name at index 0
                                        continue;
                                    }
                                    if let Some(field_node) = lowering.ir.get(field_id) {
                                        if field_node.kind == paideia_as_ir::IrKind::Borrow {
                                            // Get the Borrow's operand
                                            let borrow_children: Vec<_> = lowering.ir.children(field_id).iter().copied().collect();
                                            if let Some(operand_id) = borrow_children.first() {
                                                if let Some(var_name) = extract_var_name_from_operand(
                                                    *operand_id,
                                                    lowering,
                                                    source_map,
                                                    file,
                                                    addr_of::AddrOfPolicy::FunctionOrObject,
                                                    sink,
                                                ) {
                                                    // Push (let_id, borrow_id, var_name, Some((rc_ir_id, field_idx0))) for record field
                                                    // field_idx is 1-based (0 is type_name), so field_idx0 = field_idx - 1
                                                    addr_of_entries.push((let_id, field_id, var_name, Some((*rhs_id, field_idx - 1))));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // #1310: Also handle ArrayLit elements with Borrow children — a
                    // module-level array of symbol addresses, e.g.
                    // `pub let _klog_files : [u64; 205] = [&name_file_0, &name_file_1, ...]`.
                    // No T0535 check is queued for these (see note on
                    // `array_addr_of_entries` above); only the symbol name is resolved
                    // so the AddrOfSideTable can be populated ahead of the ArrayLit
                    // data-packing pass.
                    for rhs_id in &children {
                        if let Some(rhs_node) = lowering.ir.get(*rhs_id) {
                            if rhs_node.kind == paideia_as_ir::IrKind::ArrayLit {
                                let array_children: Vec<_> = lowering.ir.children(*rhs_id).iter().copied().collect();
                                for &elem_id in &array_children {
                                    if let Some(elem_node) = lowering.ir.get(elem_id) {
                                        if elem_node.kind == paideia_as_ir::IrKind::Borrow {
                                            let borrow_children: Vec<_> = lowering.ir.children(elem_id).iter().copied().collect();
                                            if let Some(operand_id) = borrow_children.first() {
                                                if let Some(var_name) = extract_var_name_from_operand(
                                                    *operand_id,
                                                    lowering,
                                                    source_map,
                                                    file,
                                                    addr_of::AddrOfPolicy::FunctionOrObject,
                                                    sink,
                                                ) {
                                                    array_addr_of_entries.push((elem_id, var_name));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // #1310: Populate the AddrOfSideTable for ArrayLit-of-Borrow elements. Kept
    // separate from the T0535 loop below since these entries never carry a
    // fn-ptr-assignment check.
    for (elem_id, var_name) in array_addr_of_entries {
        lowering.ir.addr_of_mut().insert(
            elem_id,
            paideia_as_ir::AddrOfMeta::new(var_name),
        );
    }

    // Now populate the AddrOfSideTable and perform T0535 checks
    // #988 v2: Keyed by rhs_id (Borrow node) not let_id
    // #1074: Handle both top-level and record-field cases
    for (let_id, rhs_id, var_name, record_context) in addr_of_entries {
        lowering.ir.addr_of_mut().insert(
            rhs_id,
            paideia_as_ir::AddrOfMeta::new(var_name.clone()),
        );

        match record_context {
            None => {
                // PA-r17-003b (#1038): T0535 signature check for top-level let bindings
                // Get the type annotation NodeId from the AST Let node
                let ast_let_id = AstNodeId::new(let_id.get()).unwrap();

                // Let nodes can be either ItemData::Let (module-level) or StmtData::Let (statement-level)
                let type_annotation_node_id = if let Some(item_data) = arena.item_data(ast_let_id) {
                    match item_data {
                        ItemData::Let { ty: Some(ty_node), .. } => Some(*ty_node),
                        _ => None,
                    }
                } else if let Some(stmt_data) = arena.stmt_data(ast_let_id) {
                    match stmt_data {
                        StmtData::Let { ty: Some(ty_node), .. } => Some(*ty_node),
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(lhs_type_node) = type_annotation_node_id {
                    // Lower the LHS type annotation to a TypeId
                    if let Ok(lhs_tid) = paideia_as_elaborator::lower_type::lower_type_ast(
                        arena,
                        source_map,
                        lhs_type_node,
                        types,
                        effects,
                        caps,
                        registry,
                        enum_registry,
                    ) {
                        // Look up the lambda via symbol table
                        if let Some(symbol) = lowering.ir.symbols().lookup_by_name(&var_name) {
                            // Convert IR node ID to AST node ID (they're the same)
                            let lambda_ast_id = AstNodeId::new(symbol.ir_node.get()).unwrap();

                            // PA-r17-1076 (#1076): Derive the RHS signature from the source binding's annotation
                            // Look up source binding's AST node ID via var_name
                            let source_let_ast_id = binding_name_to_ast_id
                                .get(&var_name)
                                .copied()
                                .unwrap_or(ast_let_id); // Fallback to current behavior if not found

                            // Use derive_fn_sig_from_source_binding to read LHS type annotation (including effects/caps)
                            if let Some(rhs_tid) = paideia_as_elaborator::derive_fn_sig::derive_fn_sig_from_source_binding(
                                arena,
                                source_map,
                                source_let_ast_id,
                                lambda_ast_id,
                                types,
                                effects,
                                caps,
                                registry,
                                enum_registry,
                            ) {
                                // T0535 check only applies to fn-ptr LHS types.
                                if matches!(types.get(lhs_tid), paideia_as_types::Type::Fn { .. }) {
                                    // Check fn-ptr assignment compatibility
                                    let mut subst = Subst::new();
                                    let span = lowering.ir.get(rhs_id).map(|n| n.span).unwrap_or_else(|| {
                                        paideia_as_diagnostics::Span::new(
                                            paideia_as_diagnostics::FileId::new(1).unwrap(),
                                            0,
                                            0,
                                        )
                                    });
                                    let diags = paideia_as_elaborator::check_fn_ptr_assignment(
                                        types,
                                        &mut subst,
                                        effects,
                                        caps,
                                        lhs_tid,
                                        rhs_tid,
                                        span,
                                    );
                                    // Push diagnostics to sink
                                    for diag in diags {
                                        let _ = sink.emit(diag);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some((rc_ir_id, field_idx0)) => {
                // #1074: T0535 signature check for record-field fn-ptr assignments
                // Get the RecordTypeId from the IR's record_layout_table
                if let Some(record_type_id) = lowering.ir.record_layout_table().get(rc_ir_id) {
                    // Get the field type node for this field index
                    if let Some(field_type_nodes) = registry.field_type_nodes.get(&record_type_id) {
                        if field_idx0 < field_type_nodes.len() {
                            let field_ty_node = field_type_nodes[field_idx0];

                            // Lower the field type
                            if let Ok(field_ty_tid) = paideia_as_elaborator::lower_type::lower_type_ast(
                                arena,
                                source_map,
                                field_ty_node,
                                types,
                                effects,
                                caps,
                                registry,
                                enum_registry,
                            ) {
                                // Check if the field type is a function-pointer
                                if matches!(types.get(field_ty_tid), paideia_as_types::Type::Fn { .. }) {
                                    // Look up the lambda via symbol table
                                    if let Some(symbol) = lowering.ir.symbols().lookup_by_name(&var_name) {
                                        // Convert IR node ID to AST node ID
                                        let lambda_ast_id = AstNodeId::new(symbol.ir_node.get()).unwrap();

                                        // PA-r17-1076 (#1076): Derive the RHS signature from the source binding's annotation
                                        // Look up source binding's AST node ID via var_name
                                        let source_let_ast_id = binding_name_to_ast_id
                                            .get(&var_name)
                                            .copied()
                                            .unwrap_or(lambda_ast_id); // Fallback if not found

                                        // Use derive_fn_sig_from_source_binding to read LHS type annotation (including effects/caps)
                                        if let Some(rhs_tid) = paideia_as_elaborator::derive_fn_sig::derive_fn_sig_from_source_binding(
                                            arena,
                                            source_map,
                                            source_let_ast_id,
                                            lambda_ast_id,
                                            types,
                                            effects,
                                            caps,
                                            registry,
                                            enum_registry,
                                        ) {
                                            // Check fn-ptr assignment compatibility
                                            let mut subst = Subst::new();
                                            let span = lowering.ir.get(rhs_id).map(|n| n.span).unwrap_or_else(|| {
                                                paideia_as_diagnostics::Span::new(
                                                    paideia_as_diagnostics::FileId::new(1).unwrap(),
                                                    0,
                                                    0,
                                                )
                                            });
                                            let diags = paideia_as_elaborator::check_fn_ptr_assignment(
                                                types,
                                                &mut subst,
                                                effects,
                                                caps,
                                                field_ty_tid,
                                                rhs_tid,
                                                span,
                                            );
                                            // Push diagnostics to sink
                                            for diag in diags {
                                                let _ = sink.emit(diag);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
