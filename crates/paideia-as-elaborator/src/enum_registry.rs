//! Enum-declaration registry: walks AST for `enum` items, mints
//! per-enum EnumTypeId, decodes each variant's (name, payload types).
//!
//! Mirrors the StructRegistry pattern for consistency.

use paideia_as_ast::{AstArena, EnumVariant, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{DiagnosticSink, SourceMap};
use paideia_as_ir::enum_layout::EnumTypeId;
use std::collections::HashMap;

/// Registry of enum declarations indexed by name, mapping to EnumTypeId
/// and storing variant descriptors (name, payload type nodes).
#[derive(Debug, Clone)]
pub struct EnumRegistry {
    /// Map from enum name to EnumTypeId.
    pub by_name: HashMap<String, EnumTypeId>,
    /// Map from EnumTypeId to list of (variant_name, payload_type_nodes).
    pub variants: HashMap<EnumTypeId, Vec<(String, Vec<NodeId>)>>,
}

impl EnumRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            by_name: HashMap::new(),
            variants: HashMap::new(),
        }
    }

    /// Get the EnumTypeId for an enum by name.
    pub fn get_by_name(&self, name: &str) -> Option<EnumTypeId> {
        self.by_name.get(name).copied()
    }

    /// Get the variant descriptors for an EnumTypeId.
    pub fn get_variants(&self, type_id: EnumTypeId) -> Option<&Vec<(String, Vec<NodeId>)>> {
        self.variants.get(&type_id)
    }
}

/// Build the enum registry by walking the AST for all Enum items.
///
/// Walks the AST arena in NodeId order (1..=len), identifies NodeKind::Enum items,
/// extracts enum names and variant information, and populates both by_name
/// and variants maps.
///
/// # Errors
///
/// Silently skips enums if names or variant names cannot be extracted from source text.
pub fn build_enum_registry(
    ast: &AstArena,
    source_map: &SourceMap,
    _sink: &mut dyn DiagnosticSink,
) -> EnumRegistry {
    let mut registry = EnumRegistry::empty();
    let mut next_id: u32 = 1;

    // Walk all nodes; for each NodeKind::Enum item:
    for i in 0..ast.len() {
        if let Some(node_id) = NodeId::new((i + 1) as u32) {
            if let Some(node) = ast.get(node_id) {
                if node.kind == NodeKind::Enum {
                    if let Some(ItemData::Enum { name, variants, .. }) = ast.item_data(node_id) {
                        // Extract enum name from AST
                        let enum_name = match extract_source_text(ast, source_map, *name) {
                            Some(name) => name,
                            None => {
                                // Skip enum if we can't extract its name
                                continue;
                            }
                        };

                        let type_id = EnumTypeId(next_id);
                        next_id += 1;

                        // Process each variant in the enum
                        let mut variant_descriptors: Vec<(String, Vec<NodeId>)> = Vec::new();
                        for variant in variants {
                            let (variant_name, payload_types) = match variant {
                                EnumVariant::Unit { name } => {
                                    let variant_name_text =
                                        match extract_source_text(ast, source_map, *name) {
                                            Some(text) => text,
                                            None => continue,
                                        };
                                    (variant_name_text, vec![])
                                }
                                EnumVariant::Tuple { name, payload } => {
                                    let variant_name_text =
                                        match extract_source_text(ast, source_map, *name) {
                                            Some(text) => text,
                                            None => continue,
                                        };
                                    (variant_name_text, payload.clone())
                                }
                                EnumVariant::Record { name, fields } => {
                                    let variant_name_text =
                                        match extract_source_text(ast, source_map, *name) {
                                            Some(text) => text,
                                            None => continue,
                                        };
                                    let field_types: Vec<NodeId> =
                                        fields.iter().map(|(_, ty)| *ty).collect();
                                    (variant_name_text, field_types)
                                }
                            };

                            variant_descriptors.push((variant_name, payload_types));
                        }

                        // Store the enum in registry
                        registry.by_name.insert(enum_name, type_id);
                        registry.variants.insert(type_id, variant_descriptors);
                    }
                }
            }
        }
    }

    registry
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_registry_empty() {
        let registry = EnumRegistry::empty();
        assert!(registry.by_name.is_empty());
        assert!(registry.variants.is_empty());
    }

    #[test]
    fn test_enum_registry_get_by_name_not_found() {
        let registry = EnumRegistry::empty();
        assert_eq!(registry.get_by_name("NonExistent"), None);
    }

    #[test]
    fn test_enum_registry_get_variants_not_found() {
        let registry = EnumRegistry::empty();
        let type_id = EnumTypeId(999);
        assert_eq!(registry.get_variants(type_id), None);
    }

    #[test]
    fn test_enum_registry_insert_and_retrieve() {
        let mut registry = EnumRegistry::empty();
        let type_id = EnumTypeId(1);
        let variants = vec![("Ok".to_string(), vec![])];

        registry.by_name.insert("Result".to_string(), type_id);
        registry.variants.insert(type_id, variants.clone());

        assert_eq!(registry.get_by_name("Result"), Some(type_id));
        assert_eq!(registry.get_variants(type_id), Some(&variants));
    }
}
