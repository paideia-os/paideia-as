//! textDocument/completion handler.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, InsertTextFormat,
    Position,
};

use crate::document::DocumentStore;

/// Return completion items for the cursor position.
///
/// Phase-2-m8-009: Lexical completion only. Classifies the context and returns:
/// - Statement: keywords + identifiers
/// - MemberAccess: placeholder members ("field1", "field2", "field3")
/// - TypeAnnotation: standard types + identifiers (uppercase heuristic)
/// - Identifier: keywords + identifiers
pub fn completion_at(
    store: &DocumentStore,
    params: &CompletionParams,
) -> Option<CompletionResponse> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let doc = store.get(uri)?;

    let context = classify_context(&doc.text, position);

    let mut items = vec![];

    match context {
        CompletionContext::Statement => {
            items.extend(keyword_completions());
            items.extend(identifier_completions(&doc.text));
        }
        CompletionContext::MemberAccess => {
            items.extend(member_completions(&doc.text, position));
        }
        CompletionContext::TypeAnnotation => {
            items.extend(type_completions());
            // Also add identifiers that look like type names (uppercase start).
            let mut idents = identifier_completions(&doc.text);
            idents.retain(|item| {
                item.label
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
            });
            items.extend(idents);
        }
        CompletionContext::Identifier => {
            items.extend(identifier_completions(&doc.text));
            items.extend(keyword_completions());
        }
    }

    if items.is_empty() {
        None
    } else {
        Some(CompletionResponse::Array(items))
    }
}

/// Classify the cursor context to determine what kind of completion fits.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CompletionContext {
    /// Top-level / statement position — keywords + identifiers.
    Statement,
    /// After a `.` — member completion.
    MemberAccess,
    /// After a `:` — type annotation; suggest types.
    TypeAnnotation,
    /// Default (identifier prefix being typed).
    Identifier,
}

/// Classify context by examining text backwards from position.
///
/// Phase-2-m8-009 simplification: lexical scan only. Real semantic completion
/// lands when the elaborator's per-position type queries come online.
pub fn classify_context(text: &str, position: Position) -> CompletionContext {
    let offset = super::document::position_to_offset(text, position);
    let bytes = text.as_bytes();

    if offset > bytes.len() {
        return CompletionContext::Identifier;
    }

    // Scan backwards from the cursor.
    // Skip current whitespace.
    let mut scan_pos = offset;
    while scan_pos > 0 && bytes[scan_pos - 1].is_ascii_whitespace() {
        scan_pos -= 1;
    }

    // Check immediately before cursor.
    if scan_pos > 0 {
        let prev_char = bytes[scan_pos - 1] as char;
        if prev_char == '.' {
            return CompletionContext::MemberAccess;
        }
        if prev_char == ':' {
            return CompletionContext::TypeAnnotation;
        }
    }

    // Check if the line is empty or only whitespace (statement context).
    let line_start = {
        let mut start = offset;
        while start > 0 && bytes[start - 1] != b'\n' {
            start -= 1;
        }
        start
    };

    let line_text = std::str::from_utf8(&bytes[line_start..offset])
        .unwrap_or("")
        .trim();

    if line_text.is_empty() {
        return CompletionContext::Statement;
    }

    CompletionContext::Identifier
}

/// Standard paideia-as keywords.
pub const KEYWORDS: &[&str] = &[
    "fn",
    "let",
    "val",
    "type",
    "sig",
    "structure",
    "functor",
    "pack",
    "unpack",
    "module",
    "in",
    "with",
    "handle",
    "perform",
    "use",
    "import",
    "if",
    "then",
    "else",
    "match",
    "case",
    "of",
    "do",
    "linear",
    "affine",
    "ordered",
    "true",
    "false",
];

/// Standard paideia-as types.
pub const STD_TYPES: &[&str] = &["Int", "Bool", "String", "Unit", "List", "Option", "Result"];

/// Generate completion items for keywords.
pub fn keyword_completions() -> Vec<CompletionItem> {
    KEYWORDS
        .iter()
        .map(|kw| CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some(kw.to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        })
        .collect()
}

/// Generate completion items for standard types.
pub fn type_completions() -> Vec<CompletionItem> {
    STD_TYPES
        .iter()
        .map(|ty| CompletionItem {
            label: ty.to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            insert_text: Some(ty.to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        })
        .collect()
}

/// Extract identifiers from the document text.
///
/// Scans for word-like sequences (alphanumeric + '_') and deduplicates.
/// Heuristic: avoids suggesting keywords themselves.
pub fn identifier_completions(text: &str) -> Vec<CompletionItem> {
    let mut identifiers = std::collections::HashSet::new();

    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_alphanumeric() || c == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &text[start..i];
            // Skip keywords to avoid duplication.
            if !KEYWORDS.contains(&word) {
                identifiers.insert(word.to_string());
            }
        } else {
            i += 1;
        }
    }

    identifiers
        .into_iter()
        .map(|ident| CompletionItem {
            label: ident.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some(ident),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        })
        .collect()
}

/// Generate placeholder member completions after `.`.
///
/// Phase-2-m8-009 stub: returns generic placeholders. Real member completion
/// lands when the elaborator wires type queries for known members.
pub fn member_completions(_text: &str, _position: Position) -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "field1".to_string(),
            kind: Some(CompletionItemKind::FIELD),
            insert_text: Some("field1".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: "field2".to_string(),
            kind: Some(CompletionItemKind::FIELD),
            insert_text: Some("field2".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: "field3".to_string(),
            kind: Some(CompletionItemKind::FIELD),
            insert_text: Some("field3".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::TextDocumentIdentifier;

    // Unit tests for classify_context
    #[test]
    fn classify_context_returns_member_access_after_dot() {
        let text = "myValue.";
        let position = Position {
            line: 0,
            character: 8,
        };
        let context = classify_context(text, position);
        assert_eq!(context, CompletionContext::MemberAccess);
    }

    #[test]
    fn classify_context_returns_type_annotation_after_colon() {
        let text = "let x :";
        let position = Position {
            line: 0,
            character: 7,
        };
        let context = classify_context(text, position);
        assert_eq!(context, CompletionContext::TypeAnnotation);
    }

    #[test]
    fn classify_context_returns_statement_on_empty_line() {
        let text = "";
        let position = Position {
            line: 0,
            character: 0,
        };
        let context = classify_context(text, position);
        assert_eq!(context, CompletionContext::Statement);
    }

    // Snapshot tests
    #[test]
    fn snapshot_keyword_completions_include_fn_let_val_type() {
        let items = keyword_completions();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"fn"));
        assert!(labels.contains(&"let"));
        assert!(labels.contains(&"val"));
        assert!(labels.contains(&"type"));
    }

    #[test]
    fn snapshot_type_completions_include_int_bool_string() {
        let items = type_completions();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"Int"));
        assert!(labels.contains(&"Bool"));
        assert!(labels.contains(&"String"));
    }

    #[test]
    fn snapshot_identifier_completions_extracts_from_text() {
        let text = "let myVar = 42\nlet anotherVar = myVar";
        let items = identifier_completions(text);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"myVar"));
        assert!(labels.contains(&"anotherVar"));
    }

    #[test]
    fn snapshot_member_completions_after_dot() {
        let text = "myValue.";
        let position = Position {
            line: 0,
            character: 8,
        };
        let items = member_completions(text, position);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"field1"));
        assert!(labels.contains(&"field2"));
        assert!(labels.contains(&"field3"));
    }

    #[test]
    fn snapshot_completion_at_end_of_file() {
        let store = crate::document::DocumentStore::new();
        let uri = tower_lsp::lsp_types::Url::parse("file:///test.pax").unwrap();
        store.open(uri.clone(), 1, "let x = ".to_string());

        let params = CompletionParams {
            text_document_position: tower_lsp::lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 8,
                },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };
        let result = completion_at(&store, &params);
        assert!(result.is_some());
    }

    #[test]
    fn identifier_completions_skips_keywords() {
        let text = "fn myFunc() { let x = 5 }";
        let items = identifier_completions(text);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        // "fn" and "let" are keywords, should not appear in identifier completions
        assert!(!labels.contains(&"fn"));
        assert!(!labels.contains(&"let"));
        // But "myFunc" and "x" should appear
        assert!(labels.contains(&"myFunc"));
        assert!(labels.contains(&"x"));
    }
}
