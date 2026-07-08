//! Root-module inner-attribute extractors: `#![bits = N]` and `#![target_features = "…"]`.
//! Split out of `cmd_build.rs` (2026-07-08).

use paideia_as_ast::{AstArena, NodeId as AstNodeId};

/// Phase 15 m2-002a: Extract the `#![bits = N]` inner attribute from the root module.
///
/// Returns the bits value (32 or 64) if present, otherwise None.
pub(super) fn extract_root_module_bits(
    root_id: Option<AstNodeId>,
    arena: &AstArena,
) -> Option<u8> {
    let root = root_id?;
    let node = arena.get(root)?;

    // The root should be a Module node.
    if node.kind != paideia_as_ast::NodeKind::Module {
        return None;
    }

    // Extract the Module's inner_attrs.
    let item_data = arena.item_data(root)?;
    if let paideia_as_ast::ItemData::Module { inner_attrs, .. } = item_data {
        // Look for #![bits = N]
        for attr in inner_attrs {
            if let paideia_as_ast::ItemAttribute::InnerAttr { name, value } = attr {
                // The name is an Ident node; check if it says "bits"
                if let Some(name_node) = arena.get(*name) {
                    let name_span = name_node.span;
                    // For now, we would need the source content to extract the text.
                    // As a simpler approach, check if the name is exactly "bits"
                    // by looking at the span length (should be 4 bytes for "bits").
                    if name_span.byte_len() == 4 {
                        // This is a heuristic; ideally the parser would normalize this.
                        // Extract the actual bits value from the AttrValue.
                        if let paideia_as_ast::AttrValue::Int(bits) = value {
                            if *bits == 32 || *bits == 64 {
                                return Some(*bits as u8);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// PA-r16-004-backtrack-a (#1033): Extract the `#![target_features = "..."]`
/// inner attribute from the root module.
///
/// The root_id points to a synthetic root Structure node created by parse_source_file,
/// which contains the module-level #![...] inner attributes.
///
/// Returns a HashSet of enabled CPU features parsed from the comma-separated
/// string value. Unknown tokens are silently skipped (the parser already emits
/// P0241 diagnostics for them). Returns an empty set if the attribute is absent.
pub(super) fn extract_root_module_features(
    root_id: Option<AstNodeId>,
    arena: &AstArena,
    source_map: &paideia_as_diagnostics::SourceMap,
    file: paideia_as_diagnostics::FileId,
) -> std::collections::HashSet<paideia_as_ir::instruction::CpuFeature> {
    let root = match root_id {
        Some(r) => r,
        None => return std::collections::HashSet::new(),
    };

    let node = match arena.get(root) {
        Some(n) => n,
        None => return std::collections::HashSet::new(),
    };

    // The root should be a synthetic Structure node (created by parse_source_file).
    if node.kind != paideia_as_ast::NodeKind::Structure {
        return std::collections::HashSet::new();
    }

    // Extract the Structure's inner_attrs directly.
    let item_data = match arena.item_data(root) {
        Some(data) => data,
        None => return std::collections::HashSet::new(),
    };

    let inner_attrs = if let paideia_as_ast::ItemData::Structure { inner_attrs, .. } = item_data {
        inner_attrs.clone()
    } else {
        return std::collections::HashSet::new();
    };

    // Process the inner_attrs from the Structure node
    {
        let content_ref = source_map.content(file);

        // Look for #![target_features = "..."]
        for attr in inner_attrs {
            if let paideia_as_ast::ItemAttribute::InnerAttr { name, value } = attr {
                // The name is an Ident node; check if it says "target_features"
                if let Some(name_node) = arena.get(name) {
                    let name_span = name_node.span;
                    let name_start = name_span.byte_start() as usize;
                    let name_len = name_span.byte_len() as usize;

                    if name_start + name_len <= content_ref.len() {
                        let name_text = &content_ref[name_start..name_start + name_len];
                        if name_text == "target_features" {
                            // Extract the string value
                            if let paideia_as_ast::AttrValue::Str(str_id) = value {
                                if let Some(str_node) = arena.get(str_id) {
                                    let str_span = str_node.span;
                                    let start = str_span.byte_start() as usize;
                                    let len = str_span.byte_len() as usize;

                                    if start + len <= content_ref.len() {
                                        let str_text = &content_ref[start..start + len];
                                        // Remove surrounding quotes from the string literal
                                        let content = if str_text.starts_with('"') && str_text.ends_with('"') {
                                            &str_text[1..str_text.len() - 1]
                                        } else {
                                            str_text
                                        };

                                        // Parse comma-separated tokens
                                        let mut features = std::collections::HashSet::new();
                                        for token in content.split(',') {
                                            let trimmed = token.trim();
                                            if !trimmed.is_empty() {
                                                if let Some(feature) =
                                                    paideia_as_ir::instruction::CpuFeature::from_token(trimmed)
                                                {
                                                    features.insert(feature);
                                                }
                                                // Unknown tokens are silently skipped (parser already emitted P0241)
                                            }
                                        }
                                        return features;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    std::collections::HashSet::new()
}
