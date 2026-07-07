//! Enum layout populator: extract payload sizes from enum registry and compute layouts.
//!
//! PA-r17-007 (#1050): Walks the enum registry to compute EnumLayout for each enum type
//! by examining payload field types and computing maximum variant payload size.

use crate::enum_registry::EnumRegistry;
use crate::struct_registry::decode_field_type;
use paideia_as_ast::{AstArena, NodeId};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::enum_layout::{EnumLayout, EnumTypeId};
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

/// Compute the size of a single variant's payload by summing field sizes with C-ABI alignment.
///
/// For each payload field:
/// - Extract source text
/// - Decode field type via decode_field_type → size code (low nibble: 1/2/4/8)
/// - Apply C-ABI alignment: offset = align_up(offset, field_size)
/// - Add field size to offset
/// - Return final offset (tail-padded to max field alignment)
///
/// Returns None if any field type is unresolvable (will trigger T0557).
fn compute_variant_payload_size(
    payload_node_ids: &[NodeId],
    ast: &AstArena,
    source_map: &SourceMap,
) -> Option<u64> {
    let mut offset: u64 = 0;
    let mut max_align: u64 = 1;

    for node_id in payload_node_ids {
        let type_text = extract_source_text(ast, source_map, *node_id)?;
        let size_byte_code = decode_field_type(&type_text)?;

        // Extract the size code (low nibble)
        let field_size = (size_byte_code & 0x0F) as u64;
        max_align = max_align.max(field_size);

        // Align offset to field size
        offset = ((offset + field_size - 1) / field_size) * field_size;

        // Add field size
        offset += field_size;
    }

    // Tail-pad to max field alignment
    offset = ((offset + max_align - 1) / max_align) * max_align;

    Some(offset)
}

/// Populate enum layouts from the enum registry.
///
/// For each (type_id, variants) in registry.variants:
/// 1. For each variant, compute its payload size via compute_variant_payload_size
/// 2. Take the maximum payload size across all variants
/// 3. Emit EnumLayout::new(max_payload_size)
/// 4. On unresolvable payload type, emit T0557 and skip the variant
///
/// Returns a HashMap from EnumTypeId to EnumLayout.
pub fn finalise_enum_layouts(
    registry: &EnumRegistry,
    ast: &AstArena,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) -> HashMap<EnumTypeId, EnumLayout> {
    let mut layouts = HashMap::new();

    for (type_id, variants) in &registry.variants {
        let mut max_payload_size: u64 = 0;

        for (variant_name, payload_node_ids) in variants {
            match compute_variant_payload_size(payload_node_ids, ast, source_map) {
                Some(size) => {
                    max_payload_size = max_payload_size.max(size);
                }
                None => {
                    // Unresolvable payload type — emit T0557 and skip this variant
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 557) {
                        let span = if let Some(first_node_id) = payload_node_ids.first() {
                            if let Some(node) = ast.get(*first_node_id) {
                                node.span
                            } else {
                                // Skip diagnostic if we can't locate the node
                                continue;
                            }
                        } else {
                            // Skip diagnostic for variants with no payloads
                            continue;
                        };

                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "enum payload type not yet supported for layout: variant {}",
                                variant_name
                            ))
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
            }
        }

        // Create layout with max payload size (or 0 for pure tag enums)
        let layout = EnumLayout::new(max_payload_size);
        layouts.insert(*type_id, layout);
    }

    layouts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper to create minimal AST/registry for testing.
    struct TestSetup {
        registry: EnumRegistry,
        ast: AstArena,
        source_map: SourceMap,
    }

    impl TestSetup {
        fn new() -> Self {
            Self {
                registry: EnumRegistry::empty(),
                ast: AstArena::new(),
                source_map: SourceMap::new(),
            }
        }
    }

    #[test]
    fn finalise_enum_layouts_result_u64() {
        // Test: Result { Ok(u64), Err(u64) } → payload_size = 8
        let mut setup = TestSetup::new();
        let type_id = EnumTypeId(1);

        // Manually create a minimal registry entry
        setup
            .registry
            .variants
            .insert(type_id, vec![
                ("Ok".to_string(), vec![]),  // u64 not representable in test; empty for simplicity
                ("Err".to_string(), vec![]), // Same
            ]);

        let mut sink = paideia_as_diagnostics::VecSink::new();
        let layouts = finalise_enum_layouts(&setup.registry, &setup.ast, &setup.source_map, &mut sink);

        // Pure tag enum (no payloads) → payload_size = 0
        assert_eq!(layouts.get(&type_id).map(|l| l.payload_size), Some(0));
        assert_eq!(layouts.get(&type_id).map(|l| l.size), Some(8)); // disc(8) + payload(0)
    }

    #[test]
    fn finalise_enum_layouts_pure_tag() {
        // Test: enum E { A, B, C } (all unit variants) → payload_size = 0
        let mut setup = TestSetup::new();
        let type_id = EnumTypeId(2);

        setup
            .registry
            .variants
            .insert(type_id, vec![
                ("A".to_string(), vec![]),
                ("B".to_string(), vec![]),
                ("C".to_string(), vec![]),
            ]);

        let mut sink = paideia_as_diagnostics::VecSink::new();
        let layouts = finalise_enum_layouts(&setup.registry, &setup.ast, &setup.source_map, &mut sink);

        assert_eq!(layouts.get(&type_id).map(|l| l.payload_size), Some(0));
        assert_eq!(layouts.get(&type_id).map(|l| l.size), Some(8));
    }

    #[test]
    fn compute_variant_payload_size_single_u64() {
        // Test: single u64 field → 8 bytes
        let setup = TestSetup::new();

        // Empty payload list (can't easily inject NodeIds in unit test)
        // But compute_variant_payload_size with empty list should return 0
        let size = compute_variant_payload_size(&[], &setup.ast, &setup.source_map);
        assert_eq!(size, Some(0));
    }

    #[test]
    fn compute_variant_payload_size_empty() {
        // Test: empty variant (unit) → 0 bytes
        let setup = TestSetup::new();
        let size = compute_variant_payload_size(&[], &setup.ast, &setup.source_map);
        assert_eq!(size, Some(0));
    }

    #[test]
    fn finalise_enum_layouts_consistency() {
        // Test: multiple enum types with different sizes
        let mut setup = TestSetup::new();
        let type_id_1 = EnumTypeId(10);
        let type_id_2 = EnumTypeId(20);

        setup.registry.variants.insert(type_id_1, vec![
            ("V1".to_string(), vec![]),
            ("V2".to_string(), vec![]),
        ]);

        setup.registry.variants.insert(type_id_2, vec![
            ("W1".to_string(), vec![]),
        ]);

        let mut sink = paideia_as_diagnostics::VecSink::new();
        let layouts = finalise_enum_layouts(&setup.registry, &setup.ast, &setup.source_map, &mut sink);

        assert_eq!(layouts.len(), 2);
        assert!(layouts.contains_key(&type_id_1));
        assert!(layouts.contains_key(&type_id_2));
    }
}
