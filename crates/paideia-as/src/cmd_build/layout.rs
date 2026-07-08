//! Array-type layout helpers used by the module-level data pass.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Given a Let node's IR id, extracts the source-declared array type `[T; N]`
//! and returns element width / total BSS size / declared length.

use paideia_as_ast::{AstArena, NodeId as AstNodeId, NodeKind, TypeData};
use paideia_as_diagnostics::SourceMap;
use paideia_as_ir::IrNodeId;

use super::identifier::parse_integer_literal;

/// Phase 6 m5-005: Compute .bss size from array type if present in AST.
///
/// Given an IR Let node ID, attempts to extract the type annotation from the AST
/// and compute the total size in bytes for array types [T; N].
/// Returns 8 (default u64 size) if no type info or not an array type.
pub(super) fn compute_bss_size_from_type(
    ir_node_id: IrNodeId,
    ast_arena: &AstArena,
    source_map: &SourceMap,
    file_id: paideia_as_diagnostics::FileId,
) -> u64 {
    // Map IR node ID to AST node ID (1-to-1 mapping).
    let ast_node_id = match AstNodeId::new(ir_node_id.get()) {
        Some(nid) => nid,
        None => return 8,
    };

    // Get the Let item from AST.
    let ast_node = match ast_arena.get(ast_node_id) {
        Some(node) => node,
        None => return 8,
    };

    // PA10-006s: Handle both Let and StmtLet node kinds
    if ast_node.kind != NodeKind::Let && ast_node.kind != NodeKind::StmtLet {
        return 8;
    }

    // Extract the type annotation from Let.
    let ty_node_id = if ast_node.kind == NodeKind::Let {
        match ast_arena.item_data(ast_node_id) {
            Some(paideia_as_ast::ItemData::Let { ty, .. }) => match ty {
                Some(ty_id) => *ty_id,
                None => return 8,
            },
            _ => return 8,
        }
    } else {
        // StmtLet case
        match ast_arena.stmt_data(ast_node_id) {
            Some(paideia_as_ast::StmtData::Let { ty, .. }) => match ty {
                Some(ty_id) => *ty_id,
                None => return 8,
            },
            _ => return 8,
        }
    };

    // Check if this type is an array type [T; N].
    let type_node = match ast_arena.get(ty_node_id) {
        Some(node) => node,
        None => return 8,
    };

    if type_node.kind != NodeKind::TypeArray {
        return 8;
    }

    // Extract TypeData to check for Array variant.
    let type_data = match ast_arena.type_data(ty_node_id) {
        Some(td) => td,
        None => return 8,
    };

    if let TypeData::Array { length, .. } = type_data {
        // Extract the length literal from the AST.
        let length_node = match ast_arena.get(*length) {
            Some(node) => node,
            None => return 8,
        };

        if length_node.kind != NodeKind::ExprLiteral {
            return 8;
        }

        // Get the literal span and parse it.
        if let Some(paideia_as_ast::ExprData::Literal { lit }) = ast_arena.expr_data(*length) {
            if let Some(lit_node) = ast_arena.get(*lit) {
                let span = lit_node.span;
                let content = source_map.content(file_id);
                let start = span.byte_start() as usize;
                let len = span.byte_len() as usize;

                if start + len <= content.len() {
                    let literal_text = &content[start..start + len];
                    // Parse the array length.
                    if let Ok(array_len) = parse_integer_literal(literal_text) {
                        // PA10-006s: Use element byte width instead of hardcoded 8.
                        let element_width =
                            array_element_byte_width(ir_node_id, ast_arena, source_map, file_id)
                                .unwrap_or(8);
                        return (array_len as u64) * (element_width as u64);
                    }
                }
            }
        }
    }

    8
}

/// PA10-006s: Extract array element byte width from AST type annotation.
///
/// Given an IR Let node ID, attempts to extract the type annotation from the AST
/// and determine the per-element width for array types [T; N].
/// Returns None if not an array or type info unavailable; caller should default to 8.
///
/// Element width mapping:
/// - u8, i8, bool → 1
/// - u16, i16 → 2
/// - u32, i32, char → 4
/// - u64, i64, usize, isize → 8
pub(super) fn array_element_byte_width(
    ir_node_id: IrNodeId,
    ast_arena: &AstArena,
    source_map: &SourceMap,
    file_id: paideia_as_diagnostics::FileId,
) -> Option<u8> {
    // Map IR node ID to AST node ID (1-to-1 mapping).
    let ast_node_id = AstNodeId::new(ir_node_id.get())?;

    // Get the Let item from AST.
    let ast_node = ast_arena.get(ast_node_id)?;

    // PA10-006s: Handle both Let and StmtLet (statement let) node kinds
    if ast_node.kind != NodeKind::Let && ast_node.kind != NodeKind::StmtLet {
        return None;
    }

    // Extract the type annotation from Let.
    let ty_node_id = if ast_node.kind == NodeKind::Let {
        match ast_arena.item_data(ast_node_id) {
            Some(paideia_as_ast::ItemData::Let { ty, .. }) => match ty {
                Some(ty_id) => ty_id,
                None => return None,
            },
            _ => return None,
        }
    } else {
        // StmtLet case
        match ast_arena.stmt_data(ast_node_id) {
            Some(paideia_as_ast::StmtData::Let { ty, .. }) => match ty {
                Some(ty_id) => ty_id,
                None => return None,
            },
            _ => return None,
        }
    };

    // Check if this type is an array type [T; N].
    let type_node = match ast_arena.get(*ty_node_id) {
        Some(node) => node,
        None => return None,
    };

    if type_node.kind != NodeKind::TypeArray {
        return None;
    }

    // Extract TypeData to get the element type.
    let type_data = match ast_arena.type_data(*ty_node_id) {
        Some(td) => td,
        None => return None,
    };

    if let TypeData::Array { element, .. } = type_data {
        // Get the element type node
        let elem_type_node = ast_arena.get(*element)?;

        // Determine the byte width based on element type kind
        match elem_type_node.kind {
            NodeKind::TypeName => {
                // Extract the type name from the source
                let span = elem_type_node.span;
                let content = source_map.content(file_id);
                let start = span.byte_start() as usize;
                let len = span.byte_len() as usize;

                if start + len <= content.len() {
                    let type_name = &content[start..start + len];
                    match type_name {
                        "u8" | "i8" | "bool" => Some(1),
                        "u16" | "i16" => Some(2),
                        "u32" | "i32" | "char" => Some(4),
                        "u64" | "i64" | "usize" | "isize" => Some(8),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    }
}

/// PA-R12-001 (issue #910): Extract declared array length N from a `[T; N]`
/// type annotation. Returns None if no annotation or non-Array or unparseable.
pub(super) fn declared_array_len_from_type(
    ir_node_id: IrNodeId,
    ast_arena: &AstArena,
    source_map: &SourceMap,
    file_id: paideia_as_diagnostics::FileId,
) -> Option<u64> {
    let ast_node_id = AstNodeId::new(ir_node_id.get())?;
    let ast_node = ast_arena.get(ast_node_id)?;
    if ast_node.kind != NodeKind::Let && ast_node.kind != NodeKind::StmtLet {
        return None;
    }
    let ty_node_id = if ast_node.kind == NodeKind::Let {
        match ast_arena.item_data(ast_node_id) {
            Some(paideia_as_ast::ItemData::Let { ty: Some(t), .. }) => *t,
            _ => return None,
        }
    } else {
        match ast_arena.stmt_data(ast_node_id) {
            Some(paideia_as_ast::StmtData::Let { ty: Some(t), .. }) => *t,
            _ => return None,
        }
    };
    if ast_arena.get(ty_node_id)?.kind != NodeKind::TypeArray {
        return None;
    }
    let TypeData::Array { length, .. } = ast_arena.type_data(ty_node_id)? else {
        return None;
    };
    let length_node = ast_arena.get(*length)?;
    if length_node.kind != NodeKind::ExprLiteral {
        return None;
    }
    let paideia_as_ast::ExprData::Literal { lit } = ast_arena.expr_data(*length)? else {
        return None;
    };
    let lit_node = ast_arena.get(*lit)?;
    let span = lit_node.span;
    let start = span.byte_start() as usize;
    let len = span.byte_len() as usize;
    let content = source_map.content(file_id);
    if start + len > content.len() {
        return None;
    }
    parse_integer_literal(&content[start..start + len])
        .ok()
        .map(|n| n as u64)
}
