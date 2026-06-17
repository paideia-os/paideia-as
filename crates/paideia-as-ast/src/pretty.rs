//! Pretty-printing for item trees.
//!
//! [`print_item`] produces a structured indented dump of an item node,
//! showing the variant name and child NodeIds. This is useful for snapshot
//! testing in the parser.

use crate::{AstArena, ItemData, NodeId};

/// Pretty-print an item node as a structured indented dump.
///
/// Produces output like:
/// ```text
/// Module { name: n1, sig: None, body: n2, doc: None }
///   Structure { items: [n3, n4], doc: None }
///     Let { name: n5, ty: None, value: n6, doc: None }
/// ```
///
/// Non-item child nodes (Placeholder, Expr, Type, etc.) are printed by ID only.
pub fn print_item(arena: &AstArena, id: NodeId) -> String {
    let mut output = String::new();
    print_item_internal(arena, id, 0, &mut output);
    output
}

fn print_item_internal(arena: &AstArena, id: NodeId, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);

    let Some(item) = arena.item_data(id) else {
        // Not an item node; just print the ID.
        use std::fmt::Write;
        let _ = writeln!(output, "{}(non-item: {})", indent, id);
        return;
    };

    let line = match item {
        ItemData::Module {
            name,
            sig,
            body,
            doc,
        } => {
            format!(
                "Module {{ name: {}, sig: {:?}, body: {}, doc: {:?} }}",
                name, sig, body, doc
            )
        }
        ItemData::Signature { name, body, doc } => {
            format!(
                "Signature {{ name: {}, body: {}, doc: {:?} }}",
                name, body, doc
            )
        }
        ItemData::Structure { items, doc } => {
            let items_str = items
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Structure {{ items: [{}], doc: {:?} }}", items_str, doc)
        }
        ItemData::Functor { params, body, doc } => {
            let params_str = params
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Functor {{ params: [{}], body: {}, doc: {:?} }}",
                params_str, body, doc
            )
        }
        ItemData::FunctorParam { name, sig } => {
            format!("FunctorParam {{ name: {}, sig: {} }}", name, sig)
        }
        ItemData::Effect { name, ops, doc } => {
            let ops_str = ops
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Effect {{ name: {}, ops: [{}], doc: {:?} }}",
                name, ops_str, doc
            )
        }
        ItemData::OpSig {
            name,
            ty,
            effect_set,
        } => {
            format!(
                "OpSig {{ name: {}, ty: {}, effect_set: {:?} }}",
                name, ty, effect_set
            )
        }
        ItemData::Capability { name, body, doc } => {
            format!(
                "Capability {{ name: {}, body: {}, doc: {:?} }}",
                name, body, doc
            )
        }
        ItemData::Let {
            name,
            ty,
            value,
            doc,
        } => {
            format!(
                "Let {{ name: {}, ty: {:?}, value: {}, doc: {:?} }}",
                name, ty, value, doc
            )
        }
        ItemData::Struct { name, fields, doc } => {
            let fields_str = fields
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Struct {{ name: {}, fields: [{}], doc: {:?} }}",
                name, fields_str, doc
            )
        }
        ItemData::Enum {
            name,
            variants,
            doc,
        } => {
            let variants_str = variants
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Enum {{ name: {}, variants: [{}], doc: {:?} }}",
                name, variants_str, doc
            )
        }
        ItemData::UnsafeBlock {
            effects,
            capabilities,
            justification,
            block,
        } => {
            let effects_str = effects
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let caps_str = capabilities
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let block_str = block
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "UnsafeBlock {{ effects: [{}], capabilities: [{}], justification: {}, block: [{}] }}",
                effects_str, caps_str, justification, block_str
            )
        }
        ItemData::NonItem => "NonItem".to_string(),
    };

    use std::fmt::Write;
    let _ = writeln!(output, "{}{}", indent, line);

    // Recurse into child items.
    match item {
        ItemData::Module { body, .. } => {
            if let Some(body_data) = arena.item_data(*body)
                && !matches!(body_data, ItemData::NonItem)
            {
                print_item_internal(arena, *body, depth + 1, output);
            }
        }
        ItemData::Signature { body, .. } => {
            if let Some(body_data) = arena.item_data(*body)
                && !matches!(body_data, ItemData::NonItem)
            {
                print_item_internal(arena, *body, depth + 1, output);
            }
        }
        ItemData::Structure { items, .. } => {
            for &item_id in items {
                if let Some(item_data) = arena.item_data(item_id)
                    && !matches!(item_data, ItemData::NonItem)
                {
                    print_item_internal(arena, item_id, depth + 1, output);
                }
            }
        }
        ItemData::Functor { params, body, .. } => {
            for &param_id in params {
                if let Some(param_data) = arena.item_data(param_id)
                    && !matches!(param_data, ItemData::NonItem)
                {
                    print_item_internal(arena, param_id, depth + 1, output);
                }
            }
            if let Some(body_data) = arena.item_data(*body)
                && !matches!(body_data, ItemData::NonItem)
            {
                print_item_internal(arena, *body, depth + 1, output);
            }
        }
        ItemData::Effect { ops, .. } => {
            for &op_id in ops {
                if let Some(op_data) = arena.item_data(op_id)
                    && !matches!(op_data, ItemData::NonItem)
                {
                    print_item_internal(arena, op_id, depth + 1, output);
                }
            }
        }
        // Other variants don't have nested item children.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemData, NodeKind};
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn print_module_with_body() {
        let mut arena = AstArena::new();

        // Allocate identifiers and structure
        let module_name = arena.alloc(NodeKind::Ident, span());
        let let_name = arena.alloc(NodeKind::Ident, span());
        let let_value = arena.alloc(NodeKind::Placeholder, span()); // Expression placeholder

        let let_id = arena.alloc_item(
            NodeKind::Let,
            span(),
            ItemData::Let {
                name: let_name,
                ty: None,
                value: let_value,
                doc: None,
            },
        );

        let structure_id = arena.alloc_item(
            NodeKind::Structure,
            span(),
            ItemData::Structure {
                items: vec![let_id],
                doc: None,
            },
        );

        let module_id = arena.alloc_item(
            NodeKind::Module,
            span(),
            ItemData::Module {
                name: module_name,
                sig: None,
                body: structure_id,
                doc: None,
            },
        );

        let output = print_item(&arena, module_id);
        assert!(output.contains("Module {"));
        assert!(output.contains("Structure {"));
        assert!(output.contains("Let {"));
    }

    #[test]
    fn print_non_item_node() {
        let mut arena = AstArena::new();
        let placeholder_id = arena.alloc(NodeKind::Placeholder, span());
        let output = print_item(&arena, placeholder_id);
        assert!(output.contains("non-item"));
    }

    #[test]
    fn print_out_of_range_id() {
        let arena = AstArena::new();
        let stray_id = NodeId::new(999).unwrap();
        let output = print_item(&arena, stray_id);
        assert!(output.contains("non-item"));
    }
}
