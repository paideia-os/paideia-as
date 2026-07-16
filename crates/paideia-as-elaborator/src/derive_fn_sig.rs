//! Derive Type::Fn from an ExprData::Lambda declaration.
//!
//! PA-r17-003b (#1038): reads lambda's declared params + return type +
//! effect row + cap set, lowers each via lower_type_ast, produces the
//! interned Type::Fn signature.

use paideia_as_ast::{AstArena, ExprData, NodeId};
use paideia_as_diagnostics::SourceMap;
use paideia_as_types::{TypeId, TypeInterner, CapSetId};
use paideia_as_effects::EffectInterner;
use paideia_as_types::CapSetInterner;

use crate::lower_type::lower_type_ast;
use crate::struct_registry::StructRegistry;
use crate::EnumRegistry;

/// Derive a Type::Fn signature from a Lambda expression in the AST.
///
/// Processes a lambda's parameter declarations and infers/derives the function
/// pointer type that represents this lambda. In phase-1, this extracts:
/// - Parameter types from pattern_type_hints
/// - Return type as Top (placeholder; phase-2+ will compute from body)
/// - Effect row as empty (phase-2+ will extract from lambda annotations)
/// - Capability set as empty (phase-2+ will extract from lambda annotations)
///
/// # Arguments
/// - `ast`: The AST arena containing the lambda node
/// - `source_map`: Source location mapping
/// - `lambda_ast_id`: The AST NodeId of the lambda expression
/// - `types`: The type interner (mutable)
/// - `effects`: The effect-row interner (mutable)
/// - `caps`: The capability-set interner (mutable)
/// - `registry`: The struct registry for named type lookup
/// - `enum_registry`: The enum registry for enum type lookup
///
/// Returns the interned TypeId of Type::Fn, or None if the lambda cannot be processed.
pub fn derive_fn_sig_from_lambda(
    ast: &AstArena,
    source_map: &SourceMap,
    lambda_ast_id: NodeId,
    types: &mut TypeInterner,
    effects: &mut EffectInterner,
    caps: &mut CapSetInterner,
    registry: &StructRegistry,
    enum_registry: &EnumRegistry,
) -> Option<TypeId> {
    let expr_data = ast.expr_data(lambda_ast_id)?;

    // Match on ExprData::Lambda
    let ExprData::Lambda {
        params,
        body,
        pipe_form: _,
        generic_params: _,
    } = expr_data
    else {
        return None;
    };

    // Phase-1: Extract parameter types from pattern_type_hints.
    // Each param NodeId should have a corresponding type stored in the arena's
    // pattern_type_hints table (set by the parser during lambda parsing).
    let mut param_types = Vec::new();

    for param_node_id in params {
        // Look up the type hint for this parameter
        if let Some(type_node_id) = ast.pattern_type_hints().get(*param_node_id) {
            // Lower the type node to a TypeId
            match lower_type_ast(
                ast,
                source_map,
                type_node_id,
                types,
                effects,
                caps,
                registry,
                enum_registry,
            ) {
                Ok(param_ty) => param_types.push(param_ty),
                Err(_diags) => {
                    // Phase-1: if lowering fails, skip this parameter or default to Top
                    // For now, return None to signal we couldn't derive the full signature
                    return None;
                }
            }
        } else {
            // Phase-1: if no type hint, we can't infer it yet
            // Return None to indicate incompleteness
            return None;
        }
    }

    // Phase-1 → Phase-2: Derive return type from lambda body literals.
    // Cases: bool literal, suffixed integer, bare param reference.
    // Fallback to Top for complex bodies.
    let ret_ty = derive_lambda_ret_type(ast, source_map, *body, params, types)
        .unwrap_or_else(|| types.top());

    // Phase-1: Effect row is empty.
    // Phase-2+: extract from lambda action block annotations or compute from body.
    let eff_row_id = effects.empty();

    // Phase-1: Capability set is empty.
    // Phase-2+: extract from lambda action block annotations or compute from body.
    let cap_set_id = CapSetId::EMPTY;

    // Intern the Type::Fn
    let fn_type = paideia_as_types::Type::Fn {
        params: param_types,
        ret: ret_ty,
        effects: eff_row_id,
        caps: cap_set_id,
    };

    Some(types.intern(fn_type))
}

/// Extract source text corresponding to an AST node.
///
/// Returns the source code substring for the given node's span, or None if
/// the span is out of bounds.
fn extract_source_text(
    ast: &AstArena,
    source_map: &SourceMap,
    node_id: NodeId,
) -> Option<String> {
    let node = ast.get(node_id)?;
    let span = node.span;
    let file_id = span.file();
    let source = source_map.content(file_id);

    let start = span.byte_start() as usize;
    let len = span.byte_len() as usize;
    if start + len > source.len() {
        return None;
    }

    let text = &source[start..start + len];
    Some(text.to_string())
}

/// Derive lambda return type from body-literal inspection.
///
/// Attempts to infer the return type by examining common body patterns:
/// - Bool literal ("true" or "false") → bool
/// - Suffixed integer (e.g., "42u32", "7i64") → uint(width) or sint(width)
/// - Bare parameter reference → None (not yet supported; requires param type lookup)
/// - Other expressions → None (fallback to Top)
///
/// Returns Some(TypeId) if inference succeeds, None to signal fallback to Top.
fn derive_lambda_ret_type(
    ast: &AstArena,
    source_map: &SourceMap,
    body: NodeId,
    _params: &[NodeId],
    types: &mut TypeInterner,
) -> Option<TypeId> {
    let node = ast.get(body)?;

    match node.kind {
        // Case 1 & 2: Literal expressions (bool or suffixed integer)
        paideia_as_ast::NodeKind::ExprLiteral => {
            let text = extract_source_text(ast, source_map, body)?;

            // Case 1: bool literal
            if text == "true" || text == "false" {
                return Some(types.bool_ty());
            }

            // Case 2: suffixed integer
            // Try each suffix from longest to shortest
            for (suffix, is_signed, width) in &[
                ("u64", false, 64u16), ("u32", false, 32u16),
                ("u16", false, 16u16), ("u8", false, 8u16),
                ("i64", true, 64u16), ("i32", true, 32u16),
                ("i16", true, 16u16), ("i8", true, 8u16),
            ] {
                if text.ends_with(suffix) {
                    return Some(if *is_signed {
                        types.sint(*width)
                    } else {
                        types.uint(*width)
                    });
                }
            }

            None
        }

        // Case 3: Bare parameter reference (not yet supported)
        paideia_as_ast::NodeKind::ExprPath => {
            // Would require looking up param type via pattern_type_hints
            // and lowering it; more invasive. For MVP, skip.
            None
        }

        // Other expression kinds: conservatively return None
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder() {
        // Detailed tests in integration test suite
    }
}
