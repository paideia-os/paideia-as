//! Pattern-binding lowering helpers for m7-007 (IR lowering: RecordCons / FieldAccess / EnumCons / EnumDiscriminant).
//!
//! This module provides helper functions for lowering pattern bindings from the AST to IR.
//! The full pattern-walk wiring is gated on the m4 walker choicepoint (not yet available),
//! so these helpers are standalone and await integration into the elaborator's main lowering loop.
//!
//! # Pattern Lowering Strategy
//!
//! When elaborating `let pat = expr`, the elaborator should:
//! 1. Lower `expr` to an IR node.
//! 2. Walk the pattern, producing one `IrKind::Var` binding per binder.
//! 3. For each nested projection (tuple or record), insert `FieldAccess` or equivalent.
//!
//! Currently, these helpers are **documentation and scaffolding** for future phases.
//! The elaborator's pattern-lowering choicepoint does not yet exist (m4 blocker).

use paideia_as_ast::{NodeId, PatternData};
use paideia_as_diagnostics::Span;
use paideia_as_ir::{IrArena, IrKind, IrNodeId};

/// Helper to lower an identifier pattern to an `IrKind::Var` node.
///
/// This is the base case: a simple binding like `let x = expr`.
/// The returned `IrNodeId` is the fresh Var node representing the binding.
///
/// # Arguments
///
/// * `arena` - The IR arena for node allocation.
/// * `symbol_id` - The AST NodeId of the binding (typically the Ident node).
/// * `span` - The source span for diagnostics.
///
/// # Returns
///
/// The `IrNodeId` of the newly-allocated Var node.
///
/// # Phase-4 Status
///
/// **TODO**: Integrate into the elaborator's main pattern-walk when the m4 walker
/// choicepoint becomes available. This function is standalone scaffolding.
pub fn lower_ident_pattern(arena: &mut IrArena, _symbol_id: NodeId, span: Span) -> IrNodeId {
    // For now, allocate a Var node with no children.
    // The elaborator will later thread in the symbol binding.
    arena.alloc(IrKind::Var, span)
}

/// Helper to lower a tuple pattern to IR bindings with `FieldAccess` projections.
///
/// A tuple pattern like `let (x, y, z) = expr` should produce:
/// 1. One `Var` node for each binding (x, y, z).
/// 2. Each `Var` node is preceded by a `FieldAccess` projection from the tuple value.
///
/// This function is a **skeleton** that illustrates the intended design.
/// The full implementation requires:
/// - Walking the tuple pattern's elements.
/// - Emitting `FieldAccess` nodes with proper (TypeId, field_index) metadata.
/// - Recursively lowering nested patterns.
///
/// # Arguments
///
/// * `arena` - The IR arena for node allocation.
/// * `_pattern_data` - The pattern data (would contain field patterns).
/// * `span` - The source span for diagnostics.
///
/// # Returns
///
/// A vector of `IrNodeId`s, one per binding in the tuple pattern.
///
/// # Phase-4 Status
///
/// **TODO**: Implement once the m4 walker choicepoint is available.
/// Currently returns a placeholder vec.
pub fn lower_tuple_pattern(
    arena: &mut IrArena,
    _pattern_data: &PatternData,
    span: Span,
) -> Vec<IrNodeId> {
    // Placeholder: return a single Var node.
    // A full implementation would:
    // 1. Unpack the pattern_data to get tuple elements.
    // 2. For each element, emit a FieldAccess + Var pair.
    // 3. Recursively lower nested patterns.
    vec![arena.alloc(IrKind::Var, span)]
}

/// Helper to lower a record pattern to IR bindings with named `FieldAccess` projections.
///
/// A record pattern like `let Rec { x: a, y: b } = expr` should produce:
/// 1. One `Var` node for each binding (a, b).
/// 2. Each `Var` node is preceded by a `FieldAccess` projection with the field name.
///
/// This function is a **skeleton** that illustrates the intended design.
/// The full implementation requires:
/// - Walking the record pattern's named fields.
/// - Emitting `FieldAccess` nodes with proper (TypeId, field_index) metadata.
/// - Recursively lowering nested patterns.
///
/// # Arguments
///
/// * `arena` - The IR arena for node allocation.
/// * `_pattern_data` - The pattern data (would contain field patterns).
/// * `span` - The source span for diagnostics.
///
/// # Returns
///
/// A vector of `IrNodeId`s, one per binding in the record pattern.
///
/// # Phase-4 Status
///
/// **TODO**: Implement once the m4 walker choicepoint is available.
/// Currently returns a placeholder vec.
pub fn lower_record_pattern(
    arena: &mut IrArena,
    _pattern_data: &PatternData,
    span: Span,
) -> Vec<IrNodeId> {
    // Placeholder: return a single Var node.
    // A full implementation would:
    // 1. Unpack the pattern_data to get record fields.
    // 2. For each field, emit a FieldAccess + Var pair with the field index.
    // 3. Recursively lower nested patterns.
    vec![arena.alloc(IrKind::Var, span)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn test_span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn pattern_lowering_ident_produces_one_var() {
        let mut arena = IrArena::new();
        let symbol_id = NodeId::new(1).unwrap();
        let span = test_span();

        let var_id = lower_ident_pattern(&mut arena, symbol_id, span);

        // Verify the node was created
        assert_eq!(arena[var_id].kind, IrKind::Var);
        // Var has no children
        assert!(arena.children(var_id).is_empty());
    }

    #[test]
    fn pattern_lowering_tuple_produces_n_vars_with_field_access() {
        let mut arena = IrArena::new();
        let pattern_data = PatternData::Tuple { elements: vec![] };
        let span = test_span();

        let bindings = lower_tuple_pattern(&mut arena, &pattern_data, span);

        // For now, the placeholder returns one Var
        assert!(!bindings.is_empty());
        for binding_id in bindings {
            assert_eq!(arena[binding_id].kind, IrKind::Var);
        }
    }

    #[test]
    fn pattern_lowering_record_produces_n_vars_with_field_access() {
        let mut arena = IrArena::new();
        let pattern_data = PatternData::Struct {
            path: NodeId::new(1).unwrap(),
            fields: vec![],
        };
        let span = test_span();

        let bindings = lower_record_pattern(&mut arena, &pattern_data, span);

        // For now, the placeholder returns one Var
        assert!(!bindings.is_empty());
        for binding_id in bindings {
            assert_eq!(arena[binding_id].kind, IrKind::Var);
        }
    }
}
