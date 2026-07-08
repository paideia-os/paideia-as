//! Struct-declaration registry: walks AST for `struct` items, mints
//! per-struct RecordTypeId, decodes each field's (name, size_byte_code).
//!
//! PA-r17-010a (#1070): pre-lower pass that closes the gap where
//! RecordLayoutTable was never populated in production. Consumes
//! ItemData::Struct.fields (Vec<(NodeId, NodeId)>) landed in #1071.

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind, TypeData};
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
    /// Map from RecordTypeId to list of AST NodeIds for field types.
    /// Used for T0535 fn-ptr field checking. Index aligns with fields vec.
    pub field_type_nodes: HashMap<RecordTypeId, Vec<NodeId>>,
}

impl StructRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            by_name: HashMap::new(),
            fields: HashMap::new(),
            field_type_nodes: HashMap::new(),
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
                        let mut field_type_node_ids: Vec<NodeId> = Vec::new();
                        for (field_name_id, field_type_id) in fields {
                            // Extract field name
                            let field_name = match extract_source_text(ast, source_map, *field_name_id) {
                                Some(name) => name,
                                None => {
                                    // Skip field if we can't extract its name
                                    continue;
                                }
                            };

                            // Check if this is a function-pointer type
                            if let Some(type_data) = ast.type_data(*field_type_id) {
                                if matches!(type_data, TypeData::FnPtr { .. }) {
                                    // Record the field type node ID for T0535 checking
                                    field_type_node_ids.push(*field_type_id);
                                    // Function-pointer fields are encoded as 8-byte pointers
                                    field_descriptors.push((field_name, 0x08));
                                    continue;
                                }
                            }

                            // Extract field type text for non-FnPtr types
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
                                    // Record the field type node ID for T0535 checking
                                    field_type_node_ids.push(*field_type_id);
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
                        registry.field_type_nodes.insert(type_id, field_type_node_ids);
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

    #[test]
    fn test_build_struct_registry_alignment_with_unsupported_field() {
        use paideia_as_ast::AstArena;
        use paideia_as_diagnostics::{SourceMap, VecSink, FileId, Span};
        use std::path::PathBuf;

        // Create minimal source content: "struct S { good: u64, bad: f32 }"
        // Layout: S at 7, good at 11, u64 at 17, bad at 23, f32 at 28
        let source = "struct S { good: u64, bad: f32 }";
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());

        // Create AST arena
        let mut ast = AstArena::new();

        // Allocate nodes for struct name, field names, and field types
        // All nodes need NodeKind, span, and optional data.
        let struct_name_id = ast.alloc(NodeKind::Ident, Span::new(file_id, 7, 1));
        let field_good_name_id = ast.alloc(NodeKind::Ident, Span::new(file_id, 11, 4));
        let field_good_type_id = ast.alloc(NodeKind::Placeholder, Span::new(file_id, 17, 3));
        let field_bad_name_id = ast.alloc(NodeKind::Ident, Span::new(file_id, 23, 3));
        // f32 is unsupported by decode_field_type, so it will trigger T0552 skip path
        let field_bad_type_id = ast.alloc(NodeKind::Placeholder, Span::new(file_id, 28, 3));

        // Create ItemData::Struct with both fields
        let struct_data = ItemData::Struct {
            name: struct_name_id,
            generic_params: Vec::new(),
            fields: vec![
                (field_good_name_id, field_good_type_id),
                (field_bad_name_id, field_bad_type_id),
            ],
            attributes: Vec::new(),
            doc: None,
        };

        // Allocate the struct item node
        ast.alloc_item(NodeKind::Struct, Span::new(file_id, 0, 33), struct_data);

        // Build the registry
        let mut sink = VecSink::new();
        let registry = build_struct_registry(&ast, &source_map, &mut sink);

        // Verify the registry has exactly one struct
        assert_eq!(registry.by_name.len(), 1);
        let type_id = registry.get_by_name("S").expect("struct S should be in registry");

        // CRITICAL: Verify fields and field_type_nodes have the same length
        // Before the fix, field_type_nodes would have 2 entries while fields has 1
        let fields_count = registry.fields.get(&type_id).map(|v| v.len());
        let field_type_nodes_count = registry.field_type_nodes.get(&type_id).map(|v| v.len());

        assert_eq!(fields_count, Some(1), "should have 1 accepted field (u64)");
        assert_eq!(field_type_nodes_count, Some(1), "field_type_nodes should match fields length");
        assert_eq!(fields_count, field_type_nodes_count, "fields and field_type_nodes must be aligned");

        // Verify the accepted field is the 'good' one
        if let Some(fields) = registry.fields.get(&type_id) {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0, "good", "the accepted field should be 'good'");
            assert_eq!(fields[0].1, 0x08, "good: u64 should encode as 0x08");
        }
    }
}
