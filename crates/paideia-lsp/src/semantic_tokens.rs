//! textDocument/semanticTokens handler.

use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensParams, SemanticTokensResult,
};

use crate::completion::KEYWORDS;
use crate::document::DocumentStore;

/// Token types per editor-support.md §1.1 + PaideiaOS-specific
/// extensions for capability binding sites and effect rows.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::TYPE,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE,
];

// Indexes (the wire form uses indexes into TOKEN_TYPES).
/// Token type index for keywords.
pub const TT_KEYWORD: u32 = 0;
/// Token type index for types.
pub const TT_TYPE: u32 = 1;
/// Token type index for functions.
pub const TT_FUNCTION: u32 = 2;
/// Token type index for variables.
pub const TT_VARIABLE: u32 = 3;
/// Token type index for parameters.
pub const TT_PARAMETER: u32 = 4;
/// Token type index for comments.
pub const TT_COMMENT: u32 = 5;
/// Token type index for strings.
pub const TT_STRING: u32 = 6;
/// Token type index for numbers.
pub const TT_NUMBER: u32 = 7;
/// Token type index for operators.
pub const TT_OPERATOR: u32 = 8;
/// Token type index for capabilities (PaideiaOS-specific via NAMESPACE slot).
pub const TT_CAPABILITY: u32 = 9;

/// Handle textDocument/semanticTokens/full request.
///
/// Tokenizes the document and returns LSP SemanticTokens in delta-encoded form.
pub fn semantic_tokens_at(
    store: &DocumentStore,
    params: &SemanticTokensParams,
) -> Option<SemanticTokensResult> {
    let uri = &params.text_document.uri;
    let doc = store.get(uri)?;

    let tokens = tokenise(&doc.text);

    if tokens.is_empty() {
        return Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: vec![],
        }));
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: tokens,
    }))
}

/// Tokenise the source into a sequence of SemanticToken structs.
///
/// LSP encodes semantic tokens as u32 tuples:
/// [delta_line, delta_start, length, type, modifiers]
///
/// This function walks the source and emits tokens for:
/// - Keywords from the KEYWORDS list → TT_KEYWORD.
/// - Identifiers starting with uppercase → TT_TYPE.
/// - Identifiers starting with "cap:" prefix → TT_CAPABILITY (phase-2-m8-011 synthetic).
/// - Identifiers followed by `(` → TT_FUNCTION.
/// - Other identifiers → TT_VARIABLE.
/// - Numeric literals → TT_NUMBER.
/// - `"..."` → TT_STRING.
/// - `//` line comments → TT_COMMENT.
fn tokenise(text: &str) -> Vec<SemanticToken> {
    let mut tokens = vec![];
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut line = 0u32;
    let mut col = 0u32;

    while i < chars.len() {
        let ch = chars[i];

        // Handle newlines.
        if ch == '\n' {
            line += 1;
            col = 0;
            i += 1;
            continue;
        }

        // Handle whitespace.
        if ch.is_whitespace() {
            col += 1;
            i += 1;
            continue;
        }

        // Handle line comments.
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            let start_col = col;
            let comment_start = i;
            // Scan to end of line.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            let length = (i - comment_start) as u32;
            add_token(&mut tokens, line, start_col, length, TT_COMMENT);
            continue;
        }

        // Handle string literals.
        if ch == '"' {
            let start_col = col;
            i += 1;
            col += 1;
            let string_start = i - 1;
            // Scan to closing quote (simplified: no escape handling).
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\n' {
                    line += 1;
                    col = 0;
                } else {
                    col += 1;
                }
                i += 1;
            }
            if i < chars.len() && chars[i] == '"' {
                i += 1;
                col += 1;
            }
            let length = (i - string_start) as u32;
            add_token(&mut tokens, line, start_col, length, TT_STRING);
            continue;
        }

        // Handle numeric literals.
        if ch.is_ascii_digit() {
            let start_col = col;
            let num_start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            let length = (i - num_start) as u32;
            add_token(&mut tokens, line, start_col, length, TT_NUMBER);
            continue;
        }

        // Handle operators.
        if "+-*/<>=!&|".contains(ch) {
            col += 1;
            i += 1;
            continue;
        }

        // Handle identifiers and keywords.
        if ch.is_alphabetic() || ch == '_' {
            let start_col = col;
            let ident_start = i;
            while i < chars.len() {
                let c = chars[i];
                if c.is_alphanumeric() || c == '_' || c == ':' {
                    i += 1;
                    col += 1;
                } else {
                    break;
                }
            }

            let ident: String = chars[ident_start..i].iter().collect();
            let length = (i - ident_start) as u32;

            // Determine token type.
            let token_type = if KEYWORDS.contains(&ident.as_str()) {
                TT_KEYWORD
            } else if ident.starts_with("cap:") {
                TT_CAPABILITY
            } else if ident
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
            {
                TT_TYPE
            } else {
                // Check if this is a function call (followed by '(').
                let mut lookahead = i;
                while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                    lookahead += 1;
                }
                if lookahead < chars.len() && chars[lookahead] == '(' {
                    TT_FUNCTION
                } else {
                    TT_VARIABLE
                }
            };

            add_token(&mut tokens, line, start_col, length, token_type);
            continue;
        }

        // Skip any other character.
        col += 1;
        i += 1;
    }

    tokens
}

/// Add a token to the list, computing delta_line and delta_start from the current position.
fn add_token(tokens: &mut Vec<SemanticToken>, line: u32, col: u32, length: u32, token_type: u32) {
    if tokens.is_empty() {
        tokens.push(SemanticToken {
            delta_line: 0,
            delta_start: col,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });
    } else {
        let last = &tokens[tokens.len() - 1];
        let delta_line = line.saturating_sub(last.delta_line);
        let delta_start = if delta_line == 0 {
            col.saturating_sub(last.delta_start + last.length)
        } else {
            col
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_keywords_emits_keyword_token() {
        let tokens = tokenise("fn");
        assert!(!tokens.is_empty());
        assert_eq!(tokens[0].token_type, TT_KEYWORD);
        assert_eq!(tokens[0].length, 2);
    }

    #[test]
    fn tokenise_uppercase_identifier_emits_type_token() {
        let tokens = tokenise("MyType");
        assert!(!tokens.is_empty());
        assert_eq!(tokens[0].token_type, TT_TYPE);
        assert_eq!(tokens[0].length, 6);
    }

    #[test]
    fn tokenise_cap_prefix_emits_capability_token() {
        let tokens = tokenise("cap:myCapability");
        assert!(!tokens.is_empty());
        assert_eq!(tokens[0].token_type, TT_CAPABILITY);
        assert_eq!(tokens[0].length, 16);
    }

    #[test]
    fn tokenise_number_literal_emits_number_token() {
        let tokens = tokenise("42");
        assert!(!tokens.is_empty());
        assert_eq!(tokens[0].token_type, TT_NUMBER);
        assert_eq!(tokens[0].length, 2);
    }

    #[test]
    fn snapshot_tokenise_multi_token_document() {
        let text = r#"fn myFunc(x : Int) : Bool
  let value = 42
  "hello world"
  // comment
  true
"#;
        let tokens = tokenise(text);

        // Verify we got tokens.
        assert!(!tokens.is_empty());

        // Count token types.
        let keyword_count = tokens.iter().filter(|t| t.token_type == TT_KEYWORD).count();
        let type_count = tokens.iter().filter(|t| t.token_type == TT_TYPE).count();
        let function_count = tokens
            .iter()
            .filter(|t| t.token_type == TT_FUNCTION)
            .count();
        let variable_count = tokens
            .iter()
            .filter(|t| t.token_type == TT_VARIABLE)
            .count();
        let number_count = tokens.iter().filter(|t| t.token_type == TT_NUMBER).count();
        let string_count = tokens.iter().filter(|t| t.token_type == TT_STRING).count();
        let comment_count = tokens.iter().filter(|t| t.token_type == TT_COMMENT).count();

        assert!(keyword_count > 0, "Expected at least one keyword token");
        assert!(type_count > 0, "Expected at least one type token");
        assert!(function_count > 0, "Expected at least one function token");
        assert!(variable_count > 0, "Expected at least one variable token");
        assert!(number_count > 0, "Expected at least one number token");
        assert!(string_count > 0, "Expected at least one string token");
        assert!(comment_count > 0, "Expected at least one comment token");
    }
}
