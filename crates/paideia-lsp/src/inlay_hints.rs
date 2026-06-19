//! textDocument/inlayHint handler.

use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Position};

use crate::document::DocumentStore;

/// Handle textDocument/inlayHint request.
///
/// Walks the text and produces inlay hints for `let` and `val` bindings.
/// Phase-2-m8-011 synthetic types based on variable name prefixes:
/// - If NAME starts with `linear:` → `: Linear<???>`
/// - If NAME starts with `affine:` → `: Affine<???>`
/// - Else → `: ???` (placeholder type)
pub fn inlay_hints_at(store: &DocumentStore, params: &InlayHintParams) -> Option<Vec<InlayHint>> {
    let uri = &params.text_document.uri;
    let doc = store.get(uri)?;

    let range = params.range;
    let hints = extract_inlay_hints(&doc.text, range);

    if hints.is_empty() {
        Some(vec![])
    } else {
        Some(hints)
    }
}

/// Extract inlay hints from source text within an optional range.
///
/// Looks for patterns like `let NAME =` or `val NAME =` and produces
/// hints with synthetic type annotations.
fn extract_inlay_hints(text: &str, _range: tower_lsp::lsp_types::Range) -> Vec<InlayHint> {
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

        // Check for `=`.
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }

        // Calculate position of the hint (right after the name).
        let (line, col) = byte_offset_to_line_col(text, name_start + name.len());

        // Determine synthetic type annotation.
        let hint_label = if name.starts_with("linear:") {
            ": Linear<???>".to_string()
        } else if name.starts_with("affine:") {
            ": Affine<???>".to_string()
        } else {
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
        let hints = extract_inlay_hints(text, range);
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
        let hints = extract_inlay_hints(text, range);
        assert_eq!(hints.len(), 3, "Expected 3 hints for 3 bindings");
    }

    #[test]
    fn inlay_hints_returns_empty_for_no_lets() {
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
        let hints = extract_inlay_hints(text, range);
        assert_eq!(hints.len(), 0);
    }

    #[test]
    fn inlay_hints_linear_prefix_emits_linear_type() {
        let text = "let linear:x = acquire()";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 25,
            },
        };
        let hints = extract_inlay_hints(text, range);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, ": Linear<???>"),
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn inlay_hints_affine_prefix_emits_affine_type() {
        let text = "val affine:cap = request()";
        let range = tower_lsp::lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 26,
            },
        };
        let hints = extract_inlay_hints(text, range);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, ": Affine<???>"),
            _ => panic!("Expected string label"),
        }
    }
}
