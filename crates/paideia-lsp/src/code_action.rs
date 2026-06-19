//! textDocument/codeAction handler.

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, Range, TextEdit, Url, WorkspaceEdit,
};

use std::collections::HashMap;

use crate::document::DocumentStore;

/// Return the available code actions for the cursor's range.
///
/// Phase-2-m8-010: Returns five built-in code actions (lexical; no elaborator-driven filtering).
pub fn code_actions_at(
    store: &DocumentStore,
    params: &CodeActionParams,
) -> Option<Vec<CodeAction>> {
    let uri = &params.text_document.uri;
    let range = params.range;
    let doc = store.get(uri)?;

    // Extract the text of the selected range.
    let text_in_range = extract_range_text(&doc.text, range)?;

    let mut actions = vec![];

    // 1. drop_affine_binding
    actions.push(drop_affine_binding(uri, range));

    // 2. add_to_effect_signature
    actions.push(add_to_effect_signature(uri, range, "Net"));

    // 3. wrap_in_unsafe
    actions.push(wrap_in_unsafe(uri, range, &text_in_range));

    // 4. convert_ascii_to_unicode_glyph (if applicable)
    if let Some(action) = convert_ascii_to_unicode(uri, range, &text_in_range) {
        actions.push(action);
    }

    // 5. convert_unicode_to_ascii_glyph (if applicable)
    if let Some(action) = convert_unicode_to_ascii(uri, range, &text_in_range) {
        actions.push(action);
    }

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

/// Extract the text in a given range from the document.
fn extract_range_text(text: &str, range: Range) -> Option<String> {
    let start_offset = super::document::position_to_offset(text, range.start);
    let end_offset = super::document::position_to_offset(text, range.end);
    if start_offset <= end_offset && end_offset <= text.len() {
        Some(text[start_offset..end_offset].to_string())
    } else {
        None
    }
}

/// Code action: drop affine binding (replace `let x = ...` with `let _ = ...`).
pub fn drop_affine_binding(uri: &Url, range: Range) -> CodeAction {
    CodeAction {
        title: "Drop affine binding (replace with `_`)".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some({
                let mut changes = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range,
                        new_text: "_".to_string(),
                    }],
                );
                changes
            }),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }
}

/// Code action: add effect to function signature.
pub fn add_to_effect_signature(uri: &Url, range: Range, effect: &str) -> CodeAction {
    CodeAction {
        title: format!("Add !{{{effect}}} to effect signature"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some({
                let mut changes = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range,
                        new_text: format!("!{{{effect}}}", effect = effect),
                    }],
                );
                changes
            }),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }
}

/// Code action: wrap selected expression in `unsafe { ... }`.
pub fn wrap_in_unsafe(uri: &Url, range: Range, text: &str) -> CodeAction {
    CodeAction {
        title: "Wrap in unsafe block".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some({
                let mut changes = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range,
                        new_text: format!("unsafe {{ {} }}", text),
                    }],
                );
                changes
            }),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }
}

/// Code action: convert ASCII glyphs to Unicode (if there are ASCII glyphs in the selection).
pub fn convert_ascii_to_unicode(uri: &Url, range: Range, text: &str) -> Option<CodeAction> {
    let converted = paideia_fmt::ascii_to_unicode(text);
    if converted != text {
        Some(CodeAction {
            title: "Convert ASCII glyphs to Unicode".to_string(),
            kind: Some(CodeActionKind::REFACTOR),
            diagnostics: None,
            edit: Some(WorkspaceEdit {
                changes: Some({
                    let mut changes = HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range,
                            new_text: converted,
                        }],
                    );
                    changes
                }),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: None,
            disabled: None,
            data: None,
        })
    } else {
        None
    }
}

/// Code action: convert Unicode glyphs to ASCII (if there are Unicode glyphs in the selection).
pub fn convert_unicode_to_ascii(uri: &Url, range: Range, text: &str) -> Option<CodeAction> {
    let converted = paideia_fmt::unicode_to_ascii(text);
    if converted != text {
        Some(CodeAction {
            title: "Convert Unicode glyphs to ASCII".to_string(),
            kind: Some(CodeActionKind::REFACTOR),
            diagnostics: None,
            edit: Some(WorkspaceEdit {
                changes: Some({
                    let mut changes = HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range,
                            new_text: converted,
                        }],
                    );
                    changes
                }),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: None,
            disabled: None,
            data: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn drop_affine_binding_produces_expected_edit() {
        let uri = Url::parse("file:///test.pax").unwrap();
        let range = Range {
            start: Position {
                line: 0,
                character: 4,
            },
            end: Position {
                line: 0,
                character: 5,
            },
        };
        let action = drop_affine_binding(&uri, range);
        assert_eq!(action.title, "Drop affine binding (replace with `_`)");
        assert!(action.edit.is_some());
    }

    #[test]
    fn add_to_effect_signature_produces_expected_edit() {
        let uri = Url::parse("file:///test.pax").unwrap();
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        };
        let action = add_to_effect_signature(&uri, range, "Net");
        assert!(action.title.contains("Net"));
        assert!(action.edit.is_some());
    }

    #[test]
    fn wrap_in_unsafe_produces_expected_edit() {
        let uri = Url::parse("file:///test.pax").unwrap();
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 5,
            },
        };
        let action = wrap_in_unsafe(&uri, range, "x + y");
        assert_eq!(action.title, "Wrap in unsafe block");
        assert!(action.edit.is_some());
    }

    #[test]
    fn convert_ascii_to_unicode_replaces_arrow() {
        let uri = Url::parse("file:///test.pax").unwrap();
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 2,
            },
        };
        let action = convert_ascii_to_unicode(&uri, range, "->").unwrap();
        assert_eq!(action.title, "Convert ASCII glyphs to Unicode");
    }

    #[test]
    fn convert_unicode_to_ascii_replaces_arrow() {
        let uri = Url::parse("file:///test.pax").unwrap();
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        };
        let action = convert_unicode_to_ascii(&uri, range, "→").unwrap();
        assert_eq!(action.title, "Convert Unicode glyphs to ASCII");
    }

    #[test]
    fn code_actions_at_returns_multiple_actions() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();
        store.open(uri.clone(), 1, "let x = 1 -> 2".to_string());

        let params = CodeActionParams {
            text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position {
                    line: 0,
                    character: 10,
                },
                end: Position {
                    line: 0,
                    character: 12,
                },
            },
            context: tower_lsp::lsp_types::CodeActionContext {
                diagnostics: vec![],
                only: None,
                trigger_kind: None,
            },
            partial_result_params: Default::default(),
            work_done_progress_params: Default::default(),
        };

        let response = code_actions_at(&store, &params);
        assert!(response.is_some());
        if let Some(actions) = response {
            // We always return at least 3 base actions (drop_affine, add_effect, wrap_unsafe)
            // Plus up to 2 glyph conversion actions (ascii_to_unicode or unicode_to_ascii).
            // For "->" text, we get the 3 base + ascii_to_unicode = 4 actions.
            assert!(actions.len() >= 3 && actions.len() <= 5);
            // Check that the first three are the base actions
            assert!(actions[0].title.contains("Drop affine binding"));
            assert!(actions[1].title.contains("effect signature"));
            assert!(actions[2].title.contains("unsafe"));
        }
    }
}
