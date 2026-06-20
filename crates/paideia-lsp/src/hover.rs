//! textDocument/hover handler.

use tower_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Position,
};

use crate::document::{DocumentStore, position_to_offset};
use paideia_as_elaborator::position_index::ByteOffset;

/// Return hover information for the token at the cursor.
///
/// Consults PositionIndex to retrieve elaborator results (type, lin_class, effects, capabilities).
/// Until walkers populate the index, returns None (no info available).
pub fn hover_at(store: &DocumentStore, params: &HoverParams) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let doc = store.get(uri)?;

    // Convert LSP position to byte offset
    let byte_offset = position_to_offset(&doc.text, position);

    // Query the position index for elaborator results
    let entry = doc.position_index.at(
        paideia_as_elaborator::position_index::FileId(0), // FileId mapping is left to elaborator
        ByteOffset(byte_offset as u32),
    )?;

    // Format hover response from the position entry
    let markdown = format_hover_from_entry(entry);

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    })
}

/// Information about a token at the cursor position.
#[derive(Clone, Debug)]
pub struct TokenInfo {
    /// The name of the token.
    pub name: String,
    /// Synthetic for m8-006; m8-007 wires real type-environment lookup.
    pub class: SubstructuralClass,
    /// The kind of the token.
    pub kind: TokenKind,
}

/// Substructural class classification.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubstructuralClass {
    /// Linear class: each binding used exactly once.
    Linear,
    /// Affine class: each binding used at most once.
    Affine,
    /// Ordered class: bindings may be reordered but not duplicated.
    Ordered,
    /// Unrestricted class: no resource constraints.
    Unrestricted,
}

/// Token kind classification.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// An identifier (variable, function, etc.).
    Identifier,
    /// A keyword (fn, let, val, type, ...).
    Keyword,
    /// A literal (number, string, etc.).
    Literal,
    /// An operator (+, -, *, etc.).
    Operator,
}

/// List of keywords in paideia-as.
const KEYWORDS: &[&str] = &[
    "fn",
    "let",
    "val",
    "type",
    "data",
    "class",
    "instance",
    "if",
    "then",
    "else",
    "match",
    "with",
    "where",
    "import",
    "export",
    "public",
    "private",
    "module",
    "do",
    "return",
    "for",
    "while",
    "break",
    "continue",
    "use",
    "linear",
    "affine",
    "ordered",
    "unrestricted",
];

/// Identify the token at the position.
///
/// Phase-2-m8-006 minimum: extract the word-like span at the position via a simple
/// alphanumeric scan; classify by lexical convention:
/// - Starts with "linear:" → Linear class, Identifier kind.
/// - Starts with "affine:" → Affine class.
/// - Starts with "ordered:" → Ordered class.
/// - Matches /[A-Z][a-zA-Z]*/ → Identifier (uppercase initial).
/// - Matches a keyword list (fn, let, val, type, ...) → Keyword.
/// - Else → Identifier with Unrestricted class.
pub fn identify_token_at(text: &str, position: Position) -> Option<TokenInfo> {
    let offset = super::document::position_to_offset(text, position);
    let bytes = text.as_bytes();

    if offset > bytes.len() {
        return None;
    }

    // Find the start of the word by scanning backwards.
    let mut start = offset;
    while start > 0 {
        let ch = bytes[start - 1] as char;
        if ch.is_alphanumeric() || ch == '_' || ch == ':' {
            start -= 1;
        } else {
            break;
        }
    }

    // Find the end of the word by scanning forwards.
    let mut end = offset;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if ch.is_alphanumeric() || ch == '_' || ch == ':' {
            end += 1;
        } else {
            break;
        }
    }

    // Extract the word.
    let word = &text[start..end];
    if word.is_empty() {
        return None;
    }

    // Classify the token.
    let class = classify_class(word);
    let kind = classify_kind(word, class);

    Some(TokenInfo {
        name: word.to_string(),
        class,
        kind,
    })
}

/// Classify the substructural class based on the token name.
fn classify_class(word: &str) -> SubstructuralClass {
    if word.starts_with("linear:") {
        SubstructuralClass::Linear
    } else if word.starts_with("affine:") {
        SubstructuralClass::Affine
    } else if word.starts_with("ordered:") {
        SubstructuralClass::Ordered
    } else {
        SubstructuralClass::Unrestricted
    }
}

/// Classify the token kind based on the token name and class.
fn classify_kind(word: &str, _class: SubstructuralClass) -> TokenKind {
    if KEYWORDS.contains(&word) {
        TokenKind::Keyword
    } else if word.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
        TokenKind::Literal
    } else if word
        .chars()
        .all(|c| !c.is_alphanumeric() && c != '_' && c != ':')
    {
        TokenKind::Operator
    } else {
        TokenKind::Identifier
    }
}

/// Format the hover markdown for a token.
///
/// Legacy format used during phase-2-m8-006 when PositionIndex was unavailable.
/// Retained for backward compatibility; new code should use format_hover_from_entry.
pub fn format_hover_markdown(token: &TokenInfo) -> String {
    format!(
        "**{name}**\n\n- kind: {kind:?}\n- class: {class:?}\n- type: _(elaboration deferred to m8-007)_\n- effects: _(deferred)_\n- capabilities: _(deferred)_",
        name = token.name,
        kind = token.kind,
        class = token.class,
    )
}

/// Format hover markdown from an elaborator position index entry.
///
/// Renders type, linearity class, effect row, and capability set into markdown.
/// Until walkers populate the index, entries contain None values; the function
/// renders "no info available" for missing fields.
pub fn format_hover_from_entry(
    entry: &paideia_as_elaborator::position_index::PositionEntry,
) -> String {
    let type_str = if let Some(_type_id) = entry.type_id {
        // TODO: format_type_id to get human-readable type name
        "unknown type".to_string()
    } else {
        "no info available".to_string()
    };

    let class_str = entry
        .lin_class
        .map(|lc| format!("{:?}", lc))
        .unwrap_or_else(|| "no info available".to_string());

    let effects_str = entry
        .effect_row_id
        .map(|_id| "!{}".to_string())
        .unwrap_or_else(|| "no info available".to_string());

    let caps_str = entry
        .cap_set_id
        .map(|_id| "@{}".to_string())
        .unwrap_or_else(|| "no info available".to_string());

    format!(
        "**Position**\n\n- type: {}\n- class: {}\n- effects: {}\n- capabilities: {}",
        type_str, class_str, effects_str, caps_str
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{TextDocumentIdentifier, TextDocumentPositionParams};

    #[test]
    fn identify_token_at_returns_identifier_for_alpha_word() {
        let text = "hello world";
        let position = Position {
            line: 0,
            character: 0,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "hello");
        assert_eq!(token.kind, TokenKind::Identifier);
        assert_eq!(token.class, SubstructuralClass::Unrestricted);
    }

    #[test]
    fn identify_token_at_returns_linear_class_for_linear_prefix() {
        let text = "linear:x uses_once";
        let position = Position {
            line: 0,
            character: 0,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "linear:x");
        assert_eq!(token.class, SubstructuralClass::Linear);
        assert_eq!(token.kind, TokenKind::Identifier);
    }

    #[test]
    fn identify_token_at_returns_keyword_for_fn() {
        let text = "fn main() { }";
        let position = Position {
            line: 0,
            character: 0,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "fn");
        assert_eq!(token.kind, TokenKind::Keyword);
        assert_eq!(token.class, SubstructuralClass::Unrestricted);
    }

    #[test]
    fn format_hover_markdown_contains_class_and_kind() {
        let token = TokenInfo {
            name: "test".to_string(),
            class: SubstructuralClass::Affine,
            kind: TokenKind::Identifier,
        };
        let markdown = format_hover_markdown(&token);
        assert!(markdown.contains("test"));
        assert!(markdown.contains("Affine"));
        assert!(markdown.contains("Identifier"));
        assert!(markdown.contains("elaboration deferred to m8-007"));
    }

    #[test]
    fn hover_at_returns_none_for_unknown_uri() {
        let store = crate::document::DocumentStore::new();
        let uri = tower_lsp::lsp_types::Url::parse("file:///unknown.pax").unwrap();
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 0,
                },
            },
            work_done_progress_params: Default::default(),
        };
        let result = hover_at(&store, &params);
        assert!(result.is_none());
    }

    #[test]
    fn snapshot_hover_linear_identifier() {
        let text = "linear:x + affine:y";
        let position = Position {
            line: 0,
            character: 2,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "linear:x");
        assert_eq!(token.class, SubstructuralClass::Linear);
        let markdown = format_hover_markdown(&token);
        assert!(markdown.contains("linear:x"));
        assert!(markdown.contains("Linear"));
    }

    #[test]
    fn snapshot_hover_unrestricted_identifier() {
        let text = "myVar = 42";
        let position = Position {
            line: 0,
            character: 2,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "myVar");
        assert_eq!(token.class, SubstructuralClass::Unrestricted);
        let markdown = format_hover_markdown(&token);
        assert!(markdown.contains("myVar"));
        assert!(markdown.contains("Unrestricted"));
    }

    #[test]
    fn snapshot_hover_keyword() {
        let text = "let x = 5";
        let position = Position {
            line: 0,
            character: 0,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "let");
        assert_eq!(token.kind, TokenKind::Keyword);
        let markdown = format_hover_markdown(&token);
        assert!(markdown.contains("let"));
        assert!(markdown.contains("Keyword"));
    }

    #[test]
    fn snapshot_hover_at_end_of_line() {
        let text = "hello";
        let position = Position {
            line: 0,
            character: 5,
        };
        let token = identify_token_at(text, position).unwrap();
        assert_eq!(token.name, "hello");
        assert_eq!(token.kind, TokenKind::Identifier);
    }

    #[test]
    fn snapshot_hover_position_out_of_range_returns_none() {
        let text = "  ";
        let position = Position {
            line: 0,
            character: 0,
        };
        let result = identify_token_at(text, position);
        assert!(result.is_none());
    }

    #[test]
    fn hover_format_empty_position_returns_no_info() {
        use paideia_as_elaborator::position_index::PositionEntry;

        let entry = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(10),
            type_id: None,
            lin_class: None,
            effect_row_id: None,
            cap_set_id: None,
        };

        let markdown = format_hover_from_entry(&entry);
        assert!(markdown.contains("no info available"));
    }

    #[test]
    fn hover_format_with_type_and_class_renders_4_line_string() {
        use paideia_as_elaborator::position_index::PositionEntry;
        use paideia_as_ir::LinClass;
        use paideia_as_types::TypeId;

        let entry = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(10),
            type_id: TypeId::new(1),
            lin_class: Some(LinClass::Linear),
            effect_row_id: None,
            cap_set_id: None,
        };

        let markdown = format_hover_from_entry(&entry);
        // Should contain all 4 fields: type, class, effects, capabilities
        assert!(markdown.contains("- type:"));
        assert!(markdown.contains("- class:"));
        assert!(markdown.contains("- effects:"));
        assert!(markdown.contains("- capabilities:"));
        // Should contain the Linear class
        assert!(markdown.contains("Linear"));
    }

    #[test]
    fn hover_lookup_with_populated_index_returns_formatted_result() {
        use paideia_as_elaborator::position_index::{FileId, PositionEntry};
        use paideia_as_ir::LinClass;
        use paideia_as_types::TypeId;

        // Create a position index with a single entry
        let mut index = paideia_as_elaborator::position_index::PositionIndex::new();
        let entry = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(10),
            type_id: TypeId::new(42),
            lin_class: Some(LinClass::Unrestricted),
            effect_row_id: Some(1),
            cap_set_id: Some(2),
        };

        index.insert(FileId(0), entry);
        index.finish();

        // Query at position 5 (within the span [0, 10))
        let result = index.at(FileId(0), ByteOffset(5));
        assert!(result.is_some());

        let entry = result.unwrap();
        let markdown = format_hover_from_entry(entry);

        // Should contain all four fields
        assert!(markdown.contains("- type:"));
        assert!(markdown.contains("- class:"));
        assert!(markdown.contains("- effects:"));
        assert!(markdown.contains("- capabilities:"));
        // Should contain the Unrestricted class
        assert!(markdown.contains("Unrestricted"));
    }
}
