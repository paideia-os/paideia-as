//! AST → IR side-table population passes.
//!
//! Extracted from `cmd_build.rs` (2026-09-07 refactor, issue #1401).
//! Each function walks the AST arena and populates one IR side-table.
//! All functions preserve the original single-pass, no-early-exit semantics
//! and are called sequentially by the `run()` orchestrator in `mod.rs`.
//!
//! Nothing here is `pub`; every entry is `pub(super)` to the `cmd_build`
//! module and never crosses the crate boundary.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{FileId, SourceMap};
use paideia_as_elaborator::LoweringResult;

use super::identifier::parse_integer_literal;

/// Phase-5-m1-001: Extract literal values from AST and populate the IR's literal_values table.
/// This enables emit_walker to look up literal values during lambda lowering.
pub(super) fn populate_literal_values(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let content_ref = source_map.content(file);

    // Walk AST to find all ExprLiteral nodes and extract their numeric values
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::ExprLiteral {
                    if let Some(paideia_as_ast::ExprData::Literal { lit }) =
                        arena.expr_data(ast_id)
                    {
                        // The 'lit' is a Placeholder node that contains the literal's span
                        if let Some(lit_node) = arena.get(*lit) {
                            let span = lit_node.span;
                            let start = span.byte_start() as usize;
                            let len = span.byte_len() as usize;
                            if start + len <= content_ref.len() {
                                let literal_text = &content_ref[start..start + len];
                                // Try to parse the literal: boolean literals first, then numeric
                                // Handle boolean: true (1), false (0)
                                // Handle numeric formats: decimal, hex (0x...), binary (0b...), octal (0o...)
                                let value = if literal_text == "true" {
                                    Some(1i64)
                                } else if literal_text == "false" {
                                    Some(0i64)
                                } else {
                                    parse_integer_literal(literal_text).ok()
                                };

                                if let Some(val) = value {
                                    // Map AST node ID to IR node ID (1-to-1 mapping)
                                    // The KEY is the ExprLiteral node ID (ast_id), not the Placeholder child ID,
                                    // because the IR Literal node ID = ast_id (1-to-1 mapping).
                                    let ir_lit_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                        .expect("valid ir node id from ast expr literal node");
                                    lowering.ir.literal_values_mut().insert(ir_lit_id, val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Phase 6 m2-004: Extract binding names from AST Let nodes and populate the IR's binding_names table.
/// This enables emit_walker to use actual binding names (_start, _anchor, etc.) instead of generic _let_<nodeid>.
pub(super) fn populate_module_let_names(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let content_ref = source_map.content(file);

    // Walk AST to find all Let nodes and extract their binding names
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(paideia_as_ast::ItemData::Let { name: name_id, .. }) =
                        arena.item_data(ast_id)
                    {
                        // Get the Ident node for the binding name
                        if let Some(name_node) = arena.get(*name_id) {
                            let span = name_node.span;
                            let start = span.byte_start() as usize;
                            let len = span.byte_len() as usize;
                            if start + len <= content_ref.len() {
                                let binding_text = content_ref[start..start + len].to_string();
                                // Map AST Let node ID to IR Let node ID (1-to-1 mapping)
                                let ir_let_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                    .expect("valid ir node id from ast let node");
                                lowering
                                    .ir
                                    .binding_names_mut()
                                    .insert(ir_let_id, binding_text);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Phase 6 m2-004b: Extract binding names from local StmtLet nodes and populate the IR's binding_names table.
/// Local `let` statements inside function/match-arm bodies use NodeKind::StmtLet, which must be handled
/// separately from module-level ItemData::Let. This ensures every local let gets a proper binding name entry.
pub(super) fn populate_stmt_let_names(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let content_ref = source_map.content(file);

    // Walk AST to find all StmtLet nodes and extract their binding names
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::StmtLet {
                    if let Some(paideia_as_ast::StmtData::Let { name: name_id, .. }) =
                        arena.stmt_data(ast_id)
                    {
                        // Get the Ident node for the binding name
                        if let Some(name_node) = arena.get(*name_id) {
                            let span = name_node.span;
                            let start = span.byte_start() as usize;
                            let len = span.byte_len() as usize;
                            if start + len <= content_ref.len() {
                                let binding_text = content_ref[start..start + len].to_string();
                                // Map AST StmtLet node ID to IR Let node ID (1-to-1 mapping)
                                let ir_let_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                    .expect("valid ir node id from ast stmtlet node");
                                lowering
                                    .ir
                                    .binding_names_mut()
                                    .insert(ir_let_id, binding_text);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// PA904: Extract public flag from AST Let nodes and populate the IR's public_lets table.
/// This enables the elaborator to mark symbols as global when they have explicit `pub` visibility.
pub(super) fn populate_public_lets(
    arena: &AstArena,
    lowering: &mut LoweringResult,
) {
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(_node) = arena.get(ast_id) {
                if let Some(paideia_as_ast::ItemData::Let { public, .. }) =
                    arena.item_data(ast_id)
                {
                    if *public {
                        // Map AST Let node ID to IR Let node ID (1-to-1 mapping)
                        let ir_let_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                            .expect("valid ir node id from ast let node");
                        lowering.ir.public_lets_mut().insert(ir_let_id);
                    }
                }
            }
        }
    }
}

/// PA8-m1-001c: Extract lambda parameter binding names from AST Lambda nodes
/// and populate the IR's binding_names and lambda_params tables. This enables
/// emit_walker to use actual parameter names (e.g., "foo", "bar") instead of
/// generic "_param_<index>".
pub(super) fn populate_lambda_params(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let content_ref = source_map.content(file);

    // Walk AST to find all Lambda nodes and extract their parameter binding names
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(_node) = arena.get(ast_id) {
                if let Some(paideia_as_ast::ExprData::Lambda { params, .. }) =
                    arena.expr_data(ast_id)
                {
                    // Map Lambda IR node ID to parameter node IDs
                    let ir_lambda_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                        .expect("valid ir node id from ast lambda");
                    let ir_param_ids: Vec<paideia_as_ir::IrNodeId> = params
                        .iter()
                        .filter_map(|param_id| paideia_as_ir::IrNodeId::new(param_id.get()))
                        .collect();
                    lowering
                        .ir
                        .lambda_params_mut()
                        .insert(ir_lambda_id, ir_param_ids);

                    // Each parameter is a Pattern node ID
                    // We need to extract the binding name from each pattern
                    for param_id in params {
                        // Check if this pattern is a simple Ident (most common case)
                        if let Some(paideia_as_ast::PatternData::Ident {
                            name: name_id, ..
                        }) = arena.pattern_data(*param_id)
                        {
                            // Get the Ident node for the parameter binding name
                            if let Some(name_node) = arena.get(*name_id) {
                                let span = name_node.span;
                                let start = span.byte_start() as usize;
                                let len = span.byte_len() as usize;
                                if start + len <= content_ref.len() {
                                    let binding_text =
                                        content_ref[start..start + len].to_string();
                                    // Map AST pattern node ID to IR param node ID
                                    // (AST and IR use same node IDs per lowering)
                                    let ir_param_id =
                                        paideia_as_ir::IrNodeId::new(param_id.get())
                                            .expect("valid ir node id from ast param");
                                    lowering
                                        .ir
                                        .binding_names_mut()
                                        .insert(ir_param_id, binding_text);
                                }
                            }
                        }
                        // For non-Ident patterns (e.g., wildcard, destructuring),
                        // we fall back to synthetic _param_<index> in emit_walker.
                    }
                }
            }
        }
    }
}

/// PA-r17-004: Populate binding_names for use-site Var IR nodes so that
/// emit_identity_lambda (and future Var-body lowering) can resolve
/// parameter references via LocalBindingTable.
///
/// Variable references in the source (like `fn(a) -> a`) are represented as ExprPath
/// in the AST. During lowering, these become Var IR nodes. We extract the variable name
/// from the last segment of the ExprPath and populate binding_names for the corresponding
/// IR node with that name.
pub(super) fn populate_var_binding_names(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let content_ref = source_map.content(file);

    // Walk AST to find all ExprPath nodes (variable references)
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::ExprPath {
                    // Extract the last segment of the path (the identifier)
                    if let Some(paideia_as_ast::ExprData::Path { segments }) = arena.expr_data(ast_id) {
                        if !segments.is_empty() {
                            let last_segment_id = segments[segments.len() - 1];
                            if let Some(segment_node) = arena.get(last_segment_id) {
                                // The segment should be an Ident node
                                if segment_node.kind == paideia_as_ast::NodeKind::Ident {
                                    let span = segment_node.span;
                                    let start = span.byte_start() as usize;
                                    let len = span.byte_len() as usize;
                                    if start + len <= content_ref.len() {
                                        let ident_text = content_ref[start..start + len].to_string();
                                        // Map AST ExprPath node ID to IR Var node ID
                                        let ir_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                            .expect("valid ir node id from ast exprpath");
                                        // Only populate if not already set
                                        if lowering.ir.binding_names().get(ir_id).is_none() {
                                            lowering.ir.binding_names_mut().insert(ir_id, ident_text);
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

/// PA10-002: Extract string and byte string literals from AST nodes
/// and populate the IR's literal_bytes table. This enables the emitter
/// to intern byte sequences and emit .rodata symbols with relocations.
/// Issue #1012: Also extract InlineBytes literals (@guid, @include_bytes).
pub(super) fn populate_literal_bytes(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let _content_ref = source_map.content(file);

    // Walk AST to find all ExprString, ExprByteString, and ExprInlineBytes nodes
    // and extract their payloads
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                match node.kind {
                    paideia_as_ast::NodeKind::ExprString => {
                        if let Some(paideia_as_ast::ExprData::StringLiteral(bytes)) =
                            arena.expr_data(ast_id)
                        {
                            let ir_string_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                .expect("valid ir node id from ast string literal");
                            lowering
                                .ir
                                .literal_bytes_mut()
                                .insert(ir_string_id, bytes.clone());
                        }
                    }
                    paideia_as_ast::NodeKind::ExprByteString => {
                        if let Some(paideia_as_ast::ExprData::ByteStringLiteral(bytes)) =
                            arena.expr_data(ast_id)
                        {
                            let ir_bytestring_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                .expect("valid ir node id from ast byte string literal");
                            lowering
                                .ir
                                .literal_bytes_mut()
                                .insert(ir_bytestring_id, bytes.clone());
                        }
                    }
                    paideia_as_ast::NodeKind::ExprInlineBytes => {
                        if let Some(paideia_as_ast::ExprData::InlineBytes(bytes)) =
                            arena.expr_data(ast_id)
                        {
                            let ir_inline_bytes_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                .expect("valid ir node id from ast inline bytes literal");
                            lowering
                                .ir
                                .literal_bytes_mut()
                                .insert(ir_inline_bytes_id, bytes.clone());
                        }
                    }
                    paideia_as_ast::NodeKind::ExprInlineStr => {
                        if let Some(paideia_as_ast::ExprData::InlineStr(bytes)) =
                            arena.expr_data(ast_id)
                        {
                            let ir_inline_str_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                .expect("valid ir node id from ast inline str literal");
                            lowering
                                .ir
                                .literal_bytes_mut()
                                .insert(ir_inline_str_id, bytes.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
