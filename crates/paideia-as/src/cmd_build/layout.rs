//! Array-type layout helpers used by the module-level data pass.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Given a Let node's IR id, extracts the source-declared array type `[T; N]`
//! and returns element width / total BSS size / declared length.

use paideia_as_ast::{AstArena, ExprData, ItemData, NodeId as AstNodeId, NodeKind, TypeData};
use paideia_as_diagnostics::{SourceMap, Span};
use paideia_as_ir::IrNodeId;

use super::identifier::parse_integer_literal;

/// Issue #1313: a `[T; N]` array length that could not be sized at emit-time.
///
/// Carries enough to build a diagnostic at the call site (which owns the
/// `DiagnosticSink`): the span of the offending length expression and a
/// human-readable explanation.
pub(super) struct UnresolvedArrayLength {
    pub span: Span,
    pub message: String,
}

/// Phase 6 m5-005: Compute .bss size from array type if present in AST.
///
/// Given an IR Let node ID, attempts to extract the type annotation from the AST
/// and compute the total size in bytes for array types [T; N].
///
/// Returns `Ok(8)` (default u64 size) if there is no type info or the
/// declaration is not an array type — those are legitimate scalar defaults,
/// not sizing failures.
///
/// For an array type `[T; N]`, `N` is resolved as follows (issue #1313):
///   1. an integer literal — sized directly;
///   2. a single-identifier reference to a module-level, non-`mut` `let NAME
///      (: TY)? = <integer literal>` in the same file — resolved by folding
///      that constant;
///   3. anything else (a binary expression, an unresolved/mutable/absent
///      identifier, ...) — `Err(UnresolvedArrayLength)` naming the length
///      expression. This function never silently substitutes a fallback size
///      for a declared array whose length it cannot determine; the 8-byte
///      default is reserved for declarations that aren't arrays at all.
pub(super) fn compute_bss_size_from_type(
    ir_node_id: IrNodeId,
    ast_arena: &AstArena,
    source_map: &SourceMap,
    file_id: paideia_as_diagnostics::FileId,
) -> Result<u64, UnresolvedArrayLength> {
    // Map IR node ID to AST node ID (1-to-1 mapping).
    let ast_node_id = match AstNodeId::new(ir_node_id.get()) {
        Some(nid) => nid,
        None => return Ok(8),
    };

    // Get the Let item from AST.
    let ast_node = match ast_arena.get(ast_node_id) {
        Some(node) => node,
        None => return Ok(8),
    };

    // PA10-006s: Handle both Let and StmtLet node kinds
    if ast_node.kind != NodeKind::Let && ast_node.kind != NodeKind::StmtLet {
        return Ok(8);
    }

    // Extract the type annotation from Let.
    let ty_node_id = if ast_node.kind == NodeKind::Let {
        match ast_arena.item_data(ast_node_id) {
            Some(paideia_as_ast::ItemData::Let { ty, .. }) => match ty {
                Some(ty_id) => *ty_id,
                None => return Ok(8),
            },
            _ => return Ok(8),
        }
    } else {
        // StmtLet case
        match ast_arena.stmt_data(ast_node_id) {
            Some(paideia_as_ast::StmtData::Let { ty, .. }) => match ty {
                Some(ty_id) => *ty_id,
                None => return Ok(8),
            },
            _ => return Ok(8),
        }
    };

    // Check if this type is an array type [T; N].
    let type_node = match ast_arena.get(ty_node_id) {
        Some(node) => node,
        None => return Ok(8),
    };

    if type_node.kind != NodeKind::TypeArray {
        return Ok(8);
    }

    // Extract TypeData to check for Array variant.
    let type_data = match ast_arena.type_data(ty_node_id) {
        Some(td) => td,
        None => return Ok(8),
    };

    if let TypeData::Array { length, .. } = type_data {
        // Extract the length node from the AST.
        let length_node = match ast_arena.get(*length) {
            Some(node) => node,
            None => return Ok(8),
        };

        let element_width =
            array_element_byte_width(ir_node_id, ast_arena, source_map, file_id).unwrap_or(8);

        if length_node.kind == NodeKind::ExprLiteral {
            // Get the literal span and parse it.
            if let Some(ExprData::Literal { lit }) = ast_arena.expr_data(*length) {
                if let Some(lit_node) = ast_arena.get(*lit) {
                    let span = lit_node.span;
                    let content = source_map.content(file_id);
                    let start = span.byte_start() as usize;
                    let len = span.byte_len() as usize;

                    if start + len <= content.len() {
                        let literal_text = &content[start..start + len];
                        if let Ok(array_len) = parse_integer_literal(literal_text) {
                            return Ok((array_len as u64) * (element_width as u64));
                        }
                    }
                }
            }

            return Err(UnresolvedArrayLength {
                span: length_node.span,
                message: "array length literal could not be parsed; cannot determine .bss size"
                    .to_string(),
            });
        }

        // Issue #1313: a non-literal length. Try to resolve a single bare
        // identifier against a module-level constant `let` in this file
        // (`pub let CONST : u64 = <literal>`); anything more (a binary
        // expression, a multi-segment path, an unresolvable name) is a hard
        // error rather than a silent 8-byte guess.
        if length_node.kind == NodeKind::ExprPath {
            if let Some(ExprData::Path { segments }) = ast_arena.expr_data(*length) {
                if segments.len() == 1 {
                    if let Some(name_node) = ast_arena.get(segments[0]) {
                        let span = name_node.span;
                        let content = source_map.content(file_id);
                        let start = span.byte_start() as usize;
                        let len = span.byte_len() as usize;
                        if start + len <= content.len() {
                            let ident_text = &content[start..start + len];
                            if let Some(const_val) =
                                resolve_module_const_u64(ident_text, ast_arena, source_map, file_id)
                            {
                                return Ok(const_val * element_width as u64);
                            }
                        }
                    }
                }
            }
        }

        let content = source_map.content(file_id);
        let span = length_node.span;
        let start = span.byte_start() as usize;
        let len = span.byte_len() as usize;
        let snippet = if start + len <= content.len() {
            &content[start..start + len]
        } else {
            "<unknown>"
        };
        return Err(UnresolvedArrayLength {
            span,
            message: format!(
                "array length `{snippet}` is not an integer literal or a resolvable module-level \
                 constant (a non-`mut` top-level `let NAME = <literal>` in this file); cannot \
                 determine .bss size for this declaration"
            ),
        });
    }

    Ok(8)
}

/// Issue #1313: resolve a bare identifier used as an array length against a
/// module-level constant declared in this file: `[pub] let NAME (: TY)? =
/// <integer literal>`. Only non-`mut` bindings qualify — a `let mut` is a
/// mutable storage location, not a compile-time constant, and folding its
/// initializer into a size would be misleading in the same way the 8-byte
/// fallback was.
///
/// Returns `None` if no such top-level binding exists, it's mutable, or its
/// value isn't an integer literal (e.g. itself another expression) — the
/// caller treats that as unresolved and reports a diagnostic rather than
/// guessing.
fn resolve_module_const_u64(
    name: &str,
    ast_arena: &AstArena,
    source_map: &SourceMap,
    file_id: paideia_as_diagnostics::FileId,
) -> Option<u64> {
    let content = source_map.content(file_id);

    for i in 0..ast_arena.len() {
        let Some(candidate_id) = AstNodeId::new((i + 1) as u32) else {
            continue;
        };
        let Some(node) = ast_arena.get(candidate_id) else {
            continue;
        };
        if node.kind != NodeKind::Let {
            continue;
        }
        let Some(ItemData::Let {
            mutable,
            name: name_id,
            value,
            ..
        }) = ast_arena.item_data(candidate_id)
        else {
            continue;
        };
        if *mutable {
            continue;
        }
        let Some(name_node) = ast_arena.get(*name_id) else {
            continue;
        };
        let span = name_node.span;
        let start = span.byte_start() as usize;
        let len = span.byte_len() as usize;
        if start + len > content.len() || &content[start..start + len] != name {
            continue;
        }

        let Some(value_node) = ast_arena.get(*value) else {
            continue;
        };
        if value_node.kind != NodeKind::ExprLiteral {
            continue;
        }
        let Some(ExprData::Literal { lit }) = ast_arena.expr_data(*value) else {
            continue;
        };
        let Some(lit_node) = ast_arena.get(*lit) else {
            continue;
        };
        let lspan = lit_node.span;
        let lstart = lspan.byte_start() as usize;
        let llen = lspan.byte_len() as usize;
        if lstart + llen > content.len() {
            continue;
        }
        if let Ok(v) = parse_integer_literal(&content[lstart..lstart + llen]) {
            return Some(v as u64);
        }
    }

    None
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
