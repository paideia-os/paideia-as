//! Struct-declaration registry: walks AST for `struct` items, mints
//! per-struct RecordTypeId, decodes each field's (name, size_byte_code).
//!
//! PA-r17-010a (#1070): pre-lower pass that closes the gap where
//! RecordLayoutTable was never populated in production. Consumes
//! ItemData::Struct.fields (Vec<(NodeId, NodeId)>) landed in #1071.

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::record_layout::RecordTypeId;
use std::collections::HashMap;

/// Registry of struct declarations indexed by name, mapping to RecordTypeId
/// and storing field descriptors (name, byte_code).
///
/// The byte_code encodes field type and signedness:
/// - Bit 4: sign flag (0 = unsigned, 1 = signed)
/// - Bits 0-3: size code (1 = u8, 2 = u16, 4 = u32, 8 = u64)
#[derive(Debug, Clone)]
pub struct StructRegistry {
    /// Map from struct name to RecordTypeId.
    pub by_name: HashMap<String, RecordTypeId>,
    /// Map from RecordTypeId to list of (field_name, byte_code) descriptors.
    pub fields: HashMap<RecordTypeId, Vec<(String, u8)>>,
}

impl StructRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            by_name: HashMap::new(),
            fields: HashMap::new(),
        }
    }

    /// Get the RecordTypeId for a struct by name.
    pub fn get_by_name(&self, name: &str) -> Option<RecordTypeId> {
        self.by_name.get(name).copied()
    }

    /// Get the field descriptors for a RecordTypeId.
    pub fn get_fields(&self, type_id: RecordTypeId) -> Option<&Vec<(String, u8)>> {
        self.fields.get(&type_id)
    }
}

/// Build the struct registry by walking the AST for all Struct items.
///
/// Walks the AST arena in NodeId order (1..=len), identifies NodeKind::Struct items,
/// extracts struct names and field type information, and populates both by_name
/// and fields maps.
///
/// # Errors
///
/// Emits T0552 diagnostic for unsupported field types. The field is skipped but
/// the registry continues processing other fields and structs.
pub fn build_struct_registry(
    ast: &AstArena,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) -> StructRegistry {
    let mut registry = StructRegistry::empty();
    let mut next_id: u32 = 1;

    // Walk all nodes; for each NodeKind::Struct item:
    for i in 0..ast.len() {
        if let Some(node_id) = NodeId::new((i + 1) as u32) {
            if let Some(node) = ast.get(node_id) {
                if node.kind == NodeKind::Struct {
                    if let Some(ItemData::Struct { name, fields, .. }) = ast.item_data(node_id) {
                        // Extract struct name from AST
                        let struct_name = match extract_source_text(ast, source_map, *name) {
                            Some(name) => name,
                            None => {
                                // Skip struct if we can't extract its name
                                continue;
                            }
                        };

                        let type_id = RecordTypeId(next_id);
                        next_id += 1;

                        // Process each field in the struct
                        let mut field_descriptors: Vec<(String, u8)> = Vec::new();
                        for (field_name_id, field_type_id) in fields {
                            // Extract field name
                            let field_name = match extract_source_text(ast, source_map, *field_name_id) {
                                Some(name) => name,
                                None => {
                                    // Skip field if we can't extract its name
                                    continue;
                                }
                            };

                            // Extract field type text
                            let type_text = match extract_source_text(ast, source_map, *field_type_id) {
                                Some(text) => text,
                                None => {
                                    // Skip field if we can't extract its type
                                    continue;
                                }
                            };

                            // Decode the field type to byte_code
                            match decode_field_type(&type_text) {
                                Some(code) => {
                                    field_descriptors.push((field_name, code));
                                }
                                None => {
                                    // Emit T0552 diagnostic for unsupported field type
                                    if let Some(field_type_node) = ast.get(*field_type_id) {
                                        if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                                            let diag = Diagnostic::error(code)
                                                .message(format!(
                                                    "Unsupported struct field type: {}",
                                                    type_text
                                                ))
                                                .with_span(field_type_node.span)
                                                .finish();
                                            let _ = sink.emit(diag);
                                        }
                                    }
                                    // Skip this field but continue processing others
                                }
                            }
                        }

                        // Store the struct in registry
                        registry.by_name.insert(struct_name, type_id);
                        registry.fields.insert(type_id, field_descriptors);
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

/// Decode source text like "u32" → 0x04.
///
/// Bit 4 = signed flag (0 = unsigned, 1 = signed).
/// Low 4 bits = size code (1 = u8, 2 = u16, 4 = u32, 8 = u64).
/// Pointers are encoded as 0x08 (8 bytes, unsigned).
pub fn decode_field_type(text: &str) -> Option<u8> {
    match text.trim() {
        "u8" => Some(0x01),
        "i8" => Some(0x11),
        "u16" => Some(0x02),
        "i16" => Some(0x12),
        "u32" => Some(0x04),
        "i32" => Some(0x14),
        "u64" => Some(0x08),
        "i64" => Some(0x18),
        s if s.starts_with('*') => Some(0x08), // pointer, 8 bytes unsigned
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_field_type_unsigned() {
        assert_eq!(decode_field_type("u8"), Some(0x01));
        assert_eq!(decode_field_type("u16"), Some(0x02));
        assert_eq!(decode_field_type("u32"), Some(0x04));
        assert_eq!(decode_field_type("u64"), Some(0x08));
    }

    #[test]
    fn test_decode_field_type_signed() {
        assert_eq!(decode_field_type("i8"), Some(0x11));
        assert_eq!(decode_field_type("i16"), Some(0x12));
        assert_eq!(decode_field_type("i32"), Some(0x14));
        assert_eq!(decode_field_type("i64"), Some(0x18));
    }

    #[test]
    fn test_decode_field_type_pointer() {
        assert_eq!(decode_field_type("*u64"), Some(0x08));
        assert_eq!(decode_field_type("*i32"), Some(0x08));
    }

    #[test]
    fn test_decode_field_type_unsupported() {
        assert_eq!(decode_field_type("f32"), None);
        assert_eq!(decode_field_type("f64"), None);
        assert_eq!(decode_field_type("bool"), None);
        assert_eq!(decode_field_type("unknown"), None);
    }

    #[test]
    fn test_decode_field_type_whitespace_trimmed() {
        assert_eq!(decode_field_type("  u32  "), Some(0x04));
        assert_eq!(decode_field_type("\tu64\t"), Some(0x08));
    }

    #[test]
    fn test_struct_registry_empty() {
        let registry = StructRegistry::empty();
        assert!(registry.by_name.is_empty());
        assert!(registry.fields.is_empty());
    }

    #[test]
    fn test_struct_registry_get_by_name_not_found() {
        let registry = StructRegistry::empty();
        assert_eq!(registry.get_by_name("NonExistent"), None);
    }

    #[test]
    fn test_struct_registry_get_fields_not_found() {
        let registry = StructRegistry::empty();
        let type_id = RecordTypeId(999);
        assert_eq!(registry.get_fields(type_id), None);
    }

    #[test]
    fn test_struct_registry_insert_and_retrieve() {
        let mut registry = StructRegistry::empty();
        let type_id = RecordTypeId(1);
        let fields = vec![
            ("x".to_string(), 0x08),
            ("y".to_string(), 0x08),
        ];

        registry.by_name.insert("Point".to_string(), type_id);
        registry.fields.insert(type_id, fields.clone());

        assert_eq!(registry.get_by_name("Point"), Some(type_id));
        assert_eq!(registry.get_fields(type_id), Some(&fields));
    }

    #[test]
    fn test_build_struct_registry_empty_ast() {
        use paideia_as_ast::AstArena;
        use paideia_as_diagnostics::{SourceMap, VecSink};

        // Create an empty AST arena
        let ast = AstArena::new();
        let mut source_map = SourceMap::new();

        // Build the registry from an empty AST
        let mut sink = VecSink::new();
        let registry = build_struct_registry(&ast, &source_map, &mut sink);

        // Verify the registry is empty
        assert_eq!(registry.by_name.len(), 0);
        assert_eq!(registry.fields.len(), 0);
    }
}
