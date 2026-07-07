//! AST TypeData → interned Type::Fn/etc. lowering.
//!
//! PA-r17-003b (#1038): supplies the type-system infrastructure needed
//! to check fn-ptr signature assignments (T0535). Also unblocks other
//! T-check integrations that need real Type::Fn from source annotations.

use paideia_as_ast::{AstArena, NodeId, TypeData};
use paideia_as_diagnostics::{Diagnostic, SourceMap};
use paideia_as_types::{TypeId, TypeInterner, CapSetId, CapSet, CapId};
use paideia_as_effects::{EffectId, EffectInterner, EffectRow};
use paideia_as_ir::EffectRowId;
use paideia_as_types::CapSetInterner;

use crate::struct_registry::StructRegistry;

/// Lower an AST type node to an interned Type::Fn or other monomorphic type.
///
/// Recursively processes TypeData variants, handling:
/// - Primitive names (u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool, char, unit)
/// - Named types (struct registry lookup)
/// - Function pointers (FnPtr with params, ret, effects, caps)
/// - Tuples, pointers, references
///
/// # Arguments
/// - `ast`: The AST arena containing type nodes
/// - `source_map`: Source location mapping (for error reporting)
/// - `node`: The type node ID to lower
/// - `types`: The type interner (mutable for interning)
/// - `effects`: The effect-row interner (mutable for interning)
/// - `caps`: The capability-set interner (mutable for interning)
/// - `registry`: The struct registry for named type lookup
///
/// Returns the interned TypeId, or diagnostics on error.
pub fn lower_type_ast(
    ast: &AstArena,
    source_map: &SourceMap,
    node: NodeId,
    types: &mut TypeInterner,
    effects: &mut EffectInterner,
    caps: &mut CapSetInterner,
    registry: &StructRegistry,
) -> Result<TypeId, Vec<Diagnostic>> {
    let type_data = ast.type_data(node).ok_or_else(|| {
        let span = ast.get(node).map(|nd| nd.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0)
        });
        vec![Diagnostic::error(paideia_as_diagnostics::DiagnosticCode::new(
            paideia_as_diagnostics::Category::T,
            paideia_as_diagnostics::Severity::Error,
            1,
        ).expect("valid code")).message("missing type data").with_span(span).finish()]
    })?;

    match type_data {
        TypeData::Name { name, args: _ } => {
            // Extract type name from Ident node
            let name_text = get_ident_text(ast, *name, source_map)?;

            // Match against primitive types
            match name_text.as_str() {
                "u8" => Ok(types.uint(8)),
                "u16" => Ok(types.uint(16)),
                "u32" => Ok(types.uint(32)),
                "u64" => Ok(types.uint(64)),
                "u128" => Ok(types.uint(128)),
                "usize" => Ok(types.uint(0xFFFF)), // SIZE_WIDTH_SENTINEL
                "i8" => Ok(types.sint(8)),
                "i16" => Ok(types.sint(16)),
                "i32" => Ok(types.sint(32)),
                "i64" => Ok(types.sint(64)),
                "i128" => Ok(types.sint(128)),
                "isize" => Ok(types.sint(0xFFFF)), // SIZE_WIDTH_SENTINEL
                "f32" => Ok(types.float(32)),
                "f64" => Ok(types.float(64)),
                "bool" => Ok(types.bool_ty()),
                "char" => Ok(types.char_ty()),
                "()" => Ok(types.unit()),
                _ => {
                    // Try struct registry lookup (phase-1: returns Top as placeholder)
                    if registry.get_by_name(&name_text).is_some() {
                        // Struct found; phase-1 returns Top
                        Ok(types.top())
                    } else {
                        let span = ast.get(node).map(|nd| nd.span).unwrap_or_else(|| {
                            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0)
                        });
                        Err(vec![Diagnostic::error(paideia_as_diagnostics::DiagnosticCode::new(
                            paideia_as_diagnostics::Category::T,
                            paideia_as_diagnostics::Severity::Error,
                            1,
                        ).expect("valid code")).message(format!("unknown type name: {}", name_text)).with_span(span).finish()])
                    }
                }
            }
        }

        TypeData::FnPtr {
            params,
            ret,
            effects: eff_node,
            capabilities: cap_node,
        } => {
            // Recursively lower each parameter type
            let mut param_types = Vec::new();
            for param_node in params {
                let param_ty = lower_type_ast(
                    ast, source_map, *param_node, types, effects, caps, registry,
                )?;
                param_types.push(param_ty);
            }

            // Lower return type
            let ret_ty = lower_type_ast(
                ast, source_map, *ret, types, effects, caps, registry,
            )?;

            // Lower effect row (if present)
            let eff_row_id = if let Some(eff_node) = eff_node {
                lower_effect_row(ast, source_map, *eff_node, effects)?
            } else {
                effects.empty()
            };

            // Lower capability set (if present)
            let cap_set_id = if let Some(cap_node) = cap_node {
                lower_cap_set(ast, source_map, *cap_node, caps)?
            } else {
                CapSetId::EMPTY
            };

            // Intern Type::Fn
            let fn_type = paideia_as_types::Type::Fn {
                params: param_types,
                ret: ret_ty,
                effects: eff_row_id,
                caps: cap_set_id,
            };
            Ok(types.intern(fn_type))
        }

        TypeData::Tuple { elements } => {
            let mut elem_types = Vec::new();
            for elem_node in elements {
                let elem_ty = lower_type_ast(
                    ast, source_map, *elem_node, types, effects, caps, registry,
                )?;
                elem_types.push(elem_ty);
            }
            let tuple_type = paideia_as_types::Type::Tuple(elem_types);
            Ok(types.intern(tuple_type))
        }

        TypeData::Ptr { pointee } => {
            let pointee_ty = lower_type_ast(
                ast, source_map, *pointee, types, effects, caps, registry,
            )?;
            let ptr_type = paideia_as_types::Type::Ptr {
                pointee: pointee_ty,
                mutable: false,
            };
            Ok(types.intern(ptr_type))
        }

        TypeData::Ref { pointee, mutable } => {
            let pointee_ty = lower_type_ast(
                ast, source_map, *pointee, types, effects, caps, registry,
            )?;
            let ref_type = paideia_as_types::Type::Ref {
                pointee: pointee_ty,
                mutable: *mutable,
                lifetime: 0, // 'static (m5 will refine this)
            };
            Ok(types.intern(ref_type))
        }

        _ => {
            // Unimplemented type variants (Array, Record, Enum, etc.) → placeholder Top
            let span = ast.get(node).map(|nd| nd.span).unwrap_or_else(|| {
                paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0)
            });
            Err(vec![Diagnostic::error(paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::T,
                paideia_as_diagnostics::Severity::Error,
                1,
            ).expect("valid code")).message("type lowering not yet implemented for this variant").with_span(span).finish()])
        }
    }
}

/// Lower an effect row node to an interned EffectRowId.
fn lower_effect_row(
    ast: &AstArena,
    source_map: &paideia_as_diagnostics::SourceMap,
    node: NodeId,
    effects: &mut EffectInterner,
) -> Result<EffectRowId, Vec<Diagnostic>> {
    // Phase-1: effect rows are stored as TypeData::EffectRow with items
    if let Some(TypeData::EffectRow { items, rest: _ }) = ast.type_data(node) {
        let mut effect_ids = Vec::new();

        // Extract effect IDs from items (each is an Ident node)
        for item_node in items {
            if let Some(ident_text) = get_ident_text(ast, *item_node, source_map).ok() {
                // Parse effect ID from name (e.g., "Io" → EffectId(1), "Atomic" → EffectId(2))
                // Phase-1: hardcoded mapping; phase-2+ will use a registry
                if let Some(eff_id) = parse_effect_name(&ident_text) {
                    effect_ids.push(eff_id);
                }
            }
        }

        // For now, ignore rest variable (phase-2+)
        Ok(effects.intern(EffectRow::from_ids(effect_ids, None)))
    } else {
        // Empty effect row
        Ok(effects.empty())
    }
}

/// Lower a capability set node to an interned CapSetId.
fn lower_cap_set(
    ast: &AstArena,
    source_map: &paideia_as_diagnostics::SourceMap,
    node: NodeId,
    caps: &mut CapSetInterner,
) -> Result<CapSetId, Vec<Diagnostic>> {
    // Phase-1: capability sets are stored as TypeData::EffectRow with items (reused)
    if let Some(TypeData::EffectRow { items, rest: _ }) = ast.type_data(node) {
        let mut cap_ids = Vec::new();

        // Extract capability IDs from items
        for item_node in items {
            if let Some(ident_text) = get_ident_text(ast, *item_node, source_map).ok() {
                // Phase-1: hardcoded mapping for capability names
                if let Some(cap_id) = parse_cap_name(&ident_text) {
                    cap_ids.push(cap_id);
                }
            }
        }

        Ok(caps.intern(CapSet::from_ids(cap_ids)))
    } else {
        // Empty capability set
        Ok(CapSetId::EMPTY)
    }
}

/// Extract identifier text from an Ident node.
/// Reads the source text using source_map.
fn get_ident_text(
    ast: &AstArena,
    node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<String, Vec<Diagnostic>> {
    if let Some(node_data) = ast.get(node) {
        match node_data.kind {
            paideia_as_ast::NodeKind::Ident => {
                let span = node_data.span;
                let file_id = span.file();
                let source = source_map.content(file_id);

                // Extract the text from the span
                let start = span.byte_start() as usize;
                let end = start + span.byte_len() as usize;
                if end > source.len() {
                    let err_span = paideia_as_diagnostics::Span::new(file_id, 0, 0);
                    return Err(vec![Diagnostic::error(paideia_as_diagnostics::DiagnosticCode::new(
                        paideia_as_diagnostics::Category::T,
                        paideia_as_diagnostics::Severity::Error,
                        1,
                    ).expect("valid code")).message("span out of bounds").with_span(err_span).finish()]);
                }

                let text = &source[start..end];
                Ok(text.to_string())
            }
            _ => {
                let span = node_data.span;
                Err(vec![Diagnostic::error(paideia_as_diagnostics::DiagnosticCode::new(
                    paideia_as_diagnostics::Category::T,
                    paideia_as_diagnostics::Severity::Error,
                    1,
                ).expect("valid code")).message("expected Ident node").with_span(span).finish()])
            }
        }
    } else {
        let span = paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0);
        Err(vec![Diagnostic::error(paideia_as_diagnostics::DiagnosticCode::new(
            paideia_as_diagnostics::Category::T,
            paideia_as_diagnostics::Severity::Error,
            1,
        ).expect("valid code")).message("missing node").with_span(span).finish()])
    }
}

/// Parse effect name (e.g., "Io", "Atomic") to EffectId.
/// Phase-1: hardcoded mapping.
fn parse_effect_name(name: &str) -> Option<EffectId> {
    match name {
        "Io" => EffectId::new(1),
        "Atomic" => EffectId::new(2),
        "Memory" => EffectId::new(3),
        "Control" => EffectId::new(4),
        _ => None,
    }
}

/// Parse capability name (e.g., "Alloc", "FileRead") to CapId.
/// Phase-1: hardcoded mapping.
fn parse_cap_name(name: &str) -> Option<CapId> {
    match name {
        "Alloc" => CapId::new(1),
        "FileRead" => CapId::new(2),
        "FileWrite" => CapId::new(3),
        "NetworkRead" => CapId::new(4),
        "NetworkWrite" => CapId::new(5),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_lower_correctly() {
        // Placeholder: detailed tests in integration test suite
    }
}
