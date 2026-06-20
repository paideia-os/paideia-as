//! textDocument/inlayHint handler.

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Position};

use crate::document::DocumentStore;
use paideia_as_elaborator::position_index::ByteOffset;

/// Handle textDocument/inlayHint request.
///
/// For each `let` / `val` binding without an explicit type annotation:
/// 1. Find the binding's identifier byte offset.
/// 2. Query PositionIndex.at(file, ident_offset).
/// 3. If a TypeId is returned, format it as `: <type>` inlay hint.
///
/// Phase-3 scaffold: PositionIndex isn't populated yet; the inlay-hint pass
/// wires the lookup. Until walkers populate, returns empty Vec.
pub fn inlay_hints_at(store: &DocumentStore, params: &InlayHintParams) -> Option<Vec<InlayHint>> {
    let uri = &params.text_document.uri;
    let doc = store.get(uri)?;

    let range = params.range;
    let hints = extract_inlay_hints(&doc.text, range, &doc.position_index);

    if hints.is_empty() {
        Some(vec![])
    } else {
        Some(hints)
    }
}

/// Extract inlay hints from source text within an optional range.
///
/// Looks for patterns like `let NAME =` or `val NAME =` without explicit type annotations
/// and produces hints by querying the position index for elaborator results.
fn extract_inlay_hints(
    text: &str,
    _range: tower_lsp::lsp_types::Range,
    position_index: &paideia_as_elaborator::position_index::PositionIndex,
) -> Vec<InlayHint> {
    let mut hints = vec![];
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for "let" or "val" keywords.
        let keyword_start = i;
        let is_let = i + 3 <= bytes.len() && &bytes[i..i + 3] == b"let";
        let is_val = i + 3 <= bytes.len() && &bytes[i..i + 3] == b"val";

        if !is_let && !is_val {
            i += 1;
            continue;
        }

        // Ensure it's a word boundary (not part of a longer identifier).
        let before_ok = keyword_start == 0
            || !bytes[keyword_start - 1].is_ascii_alphanumeric()
                && bytes[keyword_start - 1] != b'_';
        let after_ok = keyword_start + 3 >= bytes.len()
            || !bytes[keyword_start + 3].is_ascii_alphanumeric()
                && bytes[keyword_start + 3] != b'_';

        if !before_ok || !after_ok {
            i += 1;
            continue;
        }

        // Advance past "let" or "val".
        i += 3;

        // Skip whitespace.
        while i < bytes.len() && (bytes[i] as char).is_whitespace() && bytes[i] != b'\n' {
            i += 1;
        }

        // Extract the binder name.
        let name_start = i;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_alphanumeric() || c == '_' || c == ':' {
                i += 1;
            } else {
                break;
            }
        }

        if name_start == i {
            // No identifier found.
            continue;
        }

        let name = &text[name_start..i];

        // Skip whitespace.
        while i < bytes.len() && (bytes[i] as char).is_whitespace() && bytes[i] != b'\n' {
            i += 1;
        }

        // Check for explicit type annotation (`:` after name).
        if i < bytes.len() && bytes[i] == b':' {
            // Skip bindings with explicit type annotations.
            continue;
        }

        // Check for `=`.
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }

        // Calculate position of the hint (right after the name).
        let (line, col) = byte_offset_to_line_col(text, name_start + name.len());

        // Query position index for elaborator type results.
        let hint_label = if let Some(entry) = position_index.at(
            paideia_as_elaborator::position_index::FileId(0),
            ByteOffset(name_start as u32),
        ) {
            // Found elaborator result; use TypeId if available.
            if let Some(type_id) = entry.type_id {
                format!(": {}", type_id)
            } else {
                // Type not yet inferred; use placeholder.
                ": ???".to_string()
            }
        } else {
            // Position index not yet populated by walkers.
            // TODO: Phase-3-m4 walkers will populate this.
            ": ???".to_string()
        };

        hints.push(InlayHint {
            position: Position {
                line: line as u32,
                character: col as u32,
            },
            label: InlayHintLabel::String(hint_label),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        });
    }

    hints
}

/// Convert byte offset to (line, column) coordinates.
fn byte_offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for ch in text.chars() {
        if current_offset >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_offset += ch.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_elaborator::position_index::{FileId, PositionEntry, PositionIndex};
    use paideia_as_types::TypeId;

    fn empty_index() -> PositionIndex {
        PositionIndex::new()
    }

    fn index_with_type(offset: u32, type_id: TypeId) -> PositionIndex {
        let mut index = PositionIndex::new();
        index.insert(
            FileId(0),
            PositionEntry {
                span_start: ByteOffset(offset),
                span_end: ByteOffset(offset + 10),
                type_id: Some(type_id),
                lin_class: None,
                effect_row_id: None,
                cap_set_id: None,
                region_id: None,
            },
        );
        index.finish();
        index
    }

    #[test]
    fn inlay_hints_returns_empty_for_uncovered_document() {
        let text = "42 + 3.14";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 9,
            },
        };
        let index = empty_index();
        let hints = extract_inlay_hints(text, range, &index);
        assert_eq!(hints.len(), 0);
    }

    #[test]
    fn inlay_hints_skips_explicit_annotations() {
        let text = "let x: Int = 42";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 15,
            },
        };
        let index = empty_index();
        let hints = extract_inlay_hints(text, range, &index);
        // Should skip this binding because it has an explicit type annotation.
        assert_eq!(hints.len(), 0);
    }

    #[test]
    fn inlay_hints_format_renders_type_after_binding() {
        let text = "let x = 42";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 10,
            },
        };
        // Create an index with a TypeId at the binding position.
        // TypeId(1) will format as "t1".
        let type_id = TypeId::new(1).expect("valid TypeId");
        let index = index_with_type(4, type_id); // Position of "x" in "let x = 42"
        let hints = extract_inlay_hints(text, range, &index);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, ": t1"),
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn inlay_hints_emits_type_placeholder_after_let_binder() {
        let text = "let x = 42";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 10,
            },
        };
        let index = empty_index();
        let hints = extract_inlay_hints(text, range, &index);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, ": ???"),
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn inlay_hints_handles_multiple_lets() {
        let text = "let x = 42\nlet y = 3.14\nval z = true";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 15,
            },
        };
        let index = empty_index();
        let hints = extract_inlay_hints(text, range, &index);
        assert_eq!(hints.len(), 3, "Expected 3 hints for 3 bindings");
    }
}
