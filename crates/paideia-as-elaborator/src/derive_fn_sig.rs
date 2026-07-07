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
) -> Option<TypeId> {
    let expr_data = ast.expr_data(lambda_ast_id)?;

    // Match on ExprData::Lambda
    let ExprData::Lambda {
        params,
        body: _,
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

    // Phase-1: Return type is Top (placeholder).
    // Phase-2+: compute from body type or extract from explicit annotation.
    let ret_ty = types.top();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder() {
        // Detailed tests in integration test suite
    }
}
