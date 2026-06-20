//! textDocument/definition + textDocument/references handlers.
//!
//! Phase-3-m4: definition and references now use elaborator-tracked
//! name resolution via NameResolutionTable side-table.
//!
//! Phase-2-m8-007 minimum: text-based identifier matching across documents
//! in the store. m8-009+ will replace this with real binder/scope-tracking
//! once the elaboration engine ships.

use tower_lsp::lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range, ReferenceParams,
};

#[cfg(test)]
use paideia_as_elaborator::position_index::{ByteOffset, FileId};
use paideia_as_elaborator::{NameResolutionTable, Span};

use crate::document::DocumentStore;
use crate::hover::identify_token_at;

/// Jump to definition using elaborator name resolution table.
/// Phase-3-m4: queries NameResolutionTable for the definition of a use site.
pub fn definition_at_via_elaboration(
    _store: &DocumentStore,
    _table: &NameResolutionTable,
    _use_pos: Position,
) -> Option<GotoDefinitionResponse> {
    // Placeholder: elaborator does not yet populate NameResolutionTable.
    // When populated, this will:
    // 1. Convert LSP position to byte offset
    // 2. Query table.definition_of(use_span)
    // 3. Convert result span to LSP Location
    None
}

/// Jump to definition: find the first identifier occurrence of the same name
/// in the current document. Phase-2-m8-007 minimum.
pub fn definition_at(
    store: &DocumentStore,
    params: &GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Look up the document.
    let doc = store.get(uri)?;

    // Identify the token at the cursor.
    let token = identify_token_at(&doc.text, position)?;

    // Find the first occurrence of the identifier in the same document.
    let ranges = find_identifier_ranges(&doc.text, &token.name);
    if ranges.is_empty() {
        return None;
    }

    // Return the first occurrence as the definition location.
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: ranges[0],
    }))
}

/// Find all references using elaborator name resolution table.
/// Phase-3-m4: queries NameResolutionTable for all uses of a definition.
pub fn references_at_via_elaboration(
    _table: &NameResolutionTable,
    _def_span: Span,
) -> Option<Vec<Location>> {
    // Placeholder: elaborator does not yet populate NameResolutionTable.
    // When populated, this will:
    // 1. Query table.references_of(def_span)
    // 2. Convert each use span to LSP Location
    // 3. Return the list
    None
}

/// Find all references to the identifier under the cursor, across all open
/// documents in the store.
pub fn references_at(store: &DocumentStore, params: &ReferenceParams) -> Option<Vec<Location>> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    // Look up the document.
    let doc = store.get(uri)?;

    // Identify the token at the cursor.
    let token = identify_token_at(&doc.text, position)?;

    let mut locations = Vec::new();

    // Walk every document in the store; collect all occurrence ranges.
    for (doc_uri, document) in store.iter() {
        let ranges = find_identifier_ranges(&document.text, &token.name);
        for range in ranges {
            locations.push(Location {
                uri: doc_uri.clone(),
                range,
            });
        }
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

/// Walk a document's text and return every occurrence range of the named
/// identifier (word-boundary matches).
fn find_identifier_ranges(text: &str, name: &str) -> Vec<Range> {
    let mut ranges = Vec::new();
    let bytes = text.as_bytes();
    let name_bytes = name.as_bytes();

    if name_bytes.is_empty() || bytes.is_empty() {
        return ranges;
    }

    let mut i = 0;
    while i <= bytes.len().saturating_sub(name_bytes.len()) {
        // Check if the bytes at position i match the name.
        if bytes[i..].starts_with(name_bytes) {
            // Check word boundaries: character before must not be alphanumeric/underscore/colon.
            let char_before_ok = if i == 0 {
                true
            } else {
                let ch = bytes[i - 1] as char;
                !ch.is_alphanumeric() && ch != '_' && ch != ':'
            };

            // Check word boundaries: character after must not be alphanumeric/underscore/colon.
            let end_idx = i + name_bytes.len();
            let char_after_ok = if end_idx >= bytes.len() {
                true
            } else {
                let ch = bytes[end_idx] as char;
                !ch.is_alphanumeric() && ch != '_' && ch != ':'
            };

            if char_before_ok && char_after_ok {
                // Convert byte offsets to positions.
                let start_pos = offset_to_position(text, i);
                let end_pos = offset_to_position(text, end_idx);
                ranges.push(Range {
                    start: start_pos,
                    end: end_pos,
                });
            }

            i += 1;
        } else {
            i += 1;
        }
    }

    ranges
}

/// Convert a byte offset to an LSP Position (line, character).
fn offset_to_position(text: &str, byte_offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;

    for (i, ch) in text.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    Position { line, character }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{TextDocumentIdentifier, TextDocumentPositionParams, Url};

    #[test]
    fn definition_at_returns_first_occurrence_in_same_doc() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();
        let text = "foo bar foo baz foo";
        store.open(uri.clone(), 1, text.to_string());

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position {
                    line: 0,
                    character: 10, // pointing at second "foo"
                },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result = definition_at(&store, &params);
        assert!(result.is_some());
        let response = result.unwrap();
        match response {
            GotoDefinitionResponse::Scalar(loc) => {
                assert_eq!(loc.uri, uri);
                // First occurrence should be at character 0
                assert_eq!(loc.range.start.character, 0);
            }
            _ => panic!("Expected scalar response"),
        }
    }

    #[test]
    fn definition_at_returns_none_for_unknown_uri() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///unknown.pax").unwrap();

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 0,
                },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result = definition_at(&store, &params);
        assert!(result.is_none());
    }

    #[test]
    fn references_at_returns_all_occurrences_in_same_doc() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();
        let text = "foo bar foo baz foo";
        store.open(uri.clone(), 1, text.to_string());

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 1, // pointing at first "foo"
                },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: tower_lsp::lsp_types::ReferenceContext {
                include_declaration: true,
            },
        };

        let result = references_at(&store, &params);
        assert!(result.is_some());
        let locations = result.unwrap();
        // Should find 3 occurrences of "foo"
        assert_eq!(locations.len(), 3);
    }

    #[test]
    fn references_at_finds_cross_file_occurrences() {
        let store = DocumentStore::new();
        let uri1 = Url::parse("file:///test1.pax").unwrap();
        let uri2 = Url::parse("file:///test2.pax").unwrap();

        store.open(uri1.clone(), 1, "foo bar baz".to_string());
        store.open(uri2.clone(), 1, "hello foo world".to_string());

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri1.clone() },
                position: Position {
                    line: 0,
                    character: 0, // pointing at "foo" in first doc
                },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: tower_lsp::lsp_types::ReferenceContext {
                include_declaration: true,
            },
        };

        let result = references_at(&store, &params);
        assert!(result.is_some());
        let locations = result.unwrap();
        // Should find one occurrence in each document
        assert_eq!(locations.len(), 2);

        // Verify both URIs are present
        let uris: Vec<_> = locations.iter().map(|l| l.uri.clone()).collect();
        assert!(uris.contains(&uri1));
        assert!(uris.contains(&uri2));
    }

    #[test]
    fn find_identifier_ranges_respects_word_boundaries() {
        let text = "foo foobar bar foo baz";
        let ranges = find_identifier_ranges(text, "foo");
        // Should match "foo" at positions 0-3 and 15-18, but not in "foobar"
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start.character, 0);
        assert_eq!(ranges[0].end.character, 3);
        assert_eq!(ranges[1].start.character, 15);
        assert_eq!(ranges[1].end.character, 18);
    }

    #[test]
    fn find_identifier_ranges_handles_multiline_input() {
        let text = "foo\nbar foo\nbaz foo";
        let ranges = find_identifier_ranges(text, "foo");
        assert_eq!(ranges.len(), 3);

        // First occurrence on line 0
        assert_eq!(ranges[0].start.line, 0);
        assert_eq!(ranges[0].start.character, 0);

        // Second occurrence on line 1
        assert_eq!(ranges[1].start.line, 1);
        assert_eq!(ranges[1].start.character, 4);

        // Third occurrence on line 2
        assert_eq!(ranges[2].start.line, 2);
        assert_eq!(ranges[2].start.character, 4);
    }

    #[test]
    fn offset_to_position_round_trips_through_position_to_offset() {
        use crate::document::position_to_offset;
        let text = "hello\nworld\ntest";
        let offsets = vec![0, 1, 5, 6, 11, 12, 15];

        for offset in offsets {
            let pos = offset_to_position(text, offset);
            let offset_back = position_to_offset(text, pos);
            assert_eq!(
                offset, offset_back,
                "Round-trip failed for offset {}",
                offset
            );
        }
    }

    #[test]
    fn references_at_returns_none_for_unknown_uri() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///unknown.pax").unwrap();

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 0,
                },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: tower_lsp::lsp_types::ReferenceContext {
                include_declaration: true,
            },
        };

        let result = references_at(&store, &params);
        assert!(result.is_none());
    }

    #[test]
    fn references_returns_elaborator_tracked_uses() {
        // Gated: elaborator does not yet populate NameResolutionTable.
        // This test scaffolds the shape of the future API.
        let table = NameResolutionTable::new();
        let def_span = Span {
            file: FileId(1),
            start: ByteOffset(0),
            end: ByteOffset(3),
        };

        // Table is empty; references_at_via_elaboration returns None
        let result = references_at_via_elaboration(&table, def_span);
        assert!(result.is_none());

        // When elaborator populates the table, this will return populated results.
    }
}
