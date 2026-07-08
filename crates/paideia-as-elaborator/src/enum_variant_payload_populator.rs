//! Enum variant payload type populator: extract payload record types from variants.
//!
//! Issue #1054: Walks the enum registry to populate EnumVariantPayloadTable with
//! RecordTypeId for each variant's payload, enabling type-directed code generation.

use crate::enum_registry::EnumRegistry;
use crate::struct_registry::StructRegistry;
use paideia_as_ast::{AstArena, NodeId};
use paideia_as_diagnostics::SourceMap;
use paideia_as_ir::enum_layout::EnumTypeId;
use paideia_as_ir::record_layout::RecordTypeId;
use std::collections::HashMap;

/// Extract source text for an AST node by its span.
///
/// Returns None if the node doesn't exist, the span is invalid, or the span
/// extends beyond the file content.
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

/// Populate enum variant payload types from the enum registry and struct registry.
///
/// For each (type_id, variants) in registry.variants:
/// 1. For each variant, check payload_nodes length
/// 2. If exactly 1 payload node, extract source text and look up in struct_registry
/// 3. Store the result: Some(RecordTypeId) if found, None otherwise
/// 4. If 0 or multiple payload nodes, store None
///
/// Returns a HashMap from (EnumTypeId, variant_idx) to Option<RecordTypeId>.
pub fn finalise_enum_variant_payloads(
    enum_registry: &EnumRegistry,
    struct_registry: &StructRegistry,
    ast: &AstArena,
    source_map: &SourceMap,
) -> HashMap<(EnumTypeId, u32), Option<RecordTypeId>> {
    let mut out = HashMap::new();

    for (enum_id, variants) in &enum_registry.variants {
        for (i, (_name, payload_nodes)) in variants.iter().enumerate() {
            let variant_idx = i as u32;

            let payload = if payload_nodes.len() == 1 {
                // Single payload node: try to look up in struct registry
                if let Some(type_text) = extract_source_text(ast, source_map, payload_nodes[0]) {
                    struct_registry.by_name.get(&type_text).copied()
                } else {
                    None
                }
            } else {
                // Zero or multiple payload nodes: None
                None
            };

            out.insert((*enum_id, variant_idx), payload);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::VecSink;

    /// Test helper to create minimal registries and AST for testing.
    struct TestSetup {
        enum_registry: EnumRegistry,
        struct_registry: StructRegistry,
        ast: AstArena,
        source_map: SourceMap,
    }

    impl TestSetup {
        fn new() -> Self {
            Self {
                enum_registry: EnumRegistry::empty(),
                struct_registry: StructRegistry::empty(),
                ast: AstArena::new(),
                source_map: SourceMap::new(),
            }
        }
    }

    #[test]
    fn unit_variant_yields_none() {
        // Test: enum E { A } (unit variant, no payload) → None
        let mut setup = TestSetup::new();
        let enum_id = EnumTypeId(1);

        setup
            .enum_registry
            .variants
            .insert(enum_id, vec![("A".to_string(), vec![])]);

        let payloads = finalise_enum_variant_payloads(
            &setup.enum_registry,
            &setup.struct_registry,
            &setup.ast,
            &setup.source_map,
        );

        // Empty payload_nodes → None
        assert_eq!(payloads.get(&(enum_id, 0)), Some(&None));
    }

    #[test]
    fn multiple_payload_nodes_yields_none() {
        // Test: enum E { V(u64, u32) } (multi-field tuple variant) → None
        let mut setup = TestSetup::new();
        let enum_id = EnumTypeId(1);

        // Create two dummy NodeIds (we won't look them up since we can't build AST in unit test)
        setup.enum_registry.variants.insert(
            enum_id,
            vec![(
                "V".to_string(),
                vec![NodeId::new(1).unwrap(), NodeId::new(2).unwrap()],
            )],
        );

        let payloads = finalise_enum_variant_payloads(
            &setup.enum_registry,
            &setup.struct_registry,
            &setup.ast,
            &setup.source_map,
        );

        // Multiple payload_nodes → None
        assert_eq!(payloads.get(&(enum_id, 0)), Some(&None));
    }

    #[test]
    fn unknown_record_type_yields_none() {
        // Test: enum E { V(UnknownType) } where UnknownType not in struct registry → None
        let mut setup = TestSetup::new();
        let enum_id = EnumTypeId(1);

        // Single payload node that won't resolve
        setup.enum_registry.variants.insert(
            enum_id,
            vec![("V".to_string(), vec![NodeId::new(1).unwrap()])],
        );

        let payloads = finalise_enum_variant_payloads(
            &setup.enum_registry,
            &setup.struct_registry,
            &setup.ast,
            &setup.source_map,
        );

        // Node doesn't exist in AST, extract_source_text returns None → None
        assert_eq!(payloads.get(&(enum_id, 0)), Some(&None));
    }

    #[test]
    fn idempotent() {
        // Test: two invocations produce equal maps
        let mut setup = TestSetup::new();
        let enum_id = EnumTypeId(1);

        setup
            .enum_registry
            .variants
            .insert(enum_id, vec![("A".to_string(), vec![])]);

        let payloads_1 = finalise_enum_variant_payloads(
            &setup.enum_registry,
            &setup.struct_registry,
            &setup.ast,
            &setup.source_map,
        );

        let payloads_2 = finalise_enum_variant_payloads(
            &setup.enum_registry,
            &setup.struct_registry,
            &setup.ast,
            &setup.source_map,
        );

        assert_eq!(payloads_1, payloads_2);
    }

    #[test]
    fn multiple_variants_tracked_separately() {
        // Test: enum E { A, B, C } with different payload patterns
        let mut setup = TestSetup::new();
        let enum_id = EnumTypeId(1);

        setup.enum_registry.variants.insert(
            enum_id,
            vec![
                ("A".to_string(), vec![]),                        // Unit: None
                ("B".to_string(), vec![NodeId::new(1).unwrap()]), // Single node: None (not resolvable)
                ("C".to_string(), vec![]),                        // Unit: None
            ],
        );

        let payloads = finalise_enum_variant_payloads(
            &setup.enum_registry,
            &setup.struct_registry,
            &setup.ast,
            &setup.source_map,
        );

        // All should map to None (no resolvable record types)
        assert_eq!(payloads.len(), 3);
        assert_eq!(payloads.get(&(enum_id, 0)), Some(&None));
        assert_eq!(payloads.get(&(enum_id, 1)), Some(&None));
        assert_eq!(payloads.get(&(enum_id, 2)), Some(&None));
    }
}
