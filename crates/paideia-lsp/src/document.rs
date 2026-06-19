//! In-memory document store and incremental editing.

use std::collections::HashMap;
use std::sync::RwLock;

use tower_lsp::lsp_types::{Position, TextDocumentContentChangeEvent, Url};

/// A document in the store.
#[derive(Clone, Debug)]
pub struct Document {
    /// Document URI.
    pub uri: Url,
    /// Document version number.
    pub version: i32,
    /// Document text content.
    pub text: String,
}

impl Document {
    /// Create a new document.
    pub fn new(uri: Url, version: i32, text: String) -> Self {
        Self { uri, version, text }
    }

    /// Apply an incremental edit. If the range is None, the change.text
    /// is the new full document (whole-document sync fallback).
    pub fn apply_change(&mut self, change: &TextDocumentContentChangeEvent) {
        if let Some(range) = change.range {
            let start_offset = position_to_offset(&self.text, range.start);
            let end_offset = position_to_offset(&self.text, range.end);
            self.text
                .replace_range(start_offset..end_offset, &change.text);
        } else {
            // Whole-document replacement.
            self.text = change.text.clone();
        }
    }
}

/// Convert an LSP Position to a byte offset in the text.
///
/// Phase-2-m8-003 simplification: treat position.character as UTF-8 byte offset (CORRECT
/// for ASCII; INCORRECT for multi-byte text). Document the simplification; m8-004+ adds
/// UTF-16 code-unit awareness once we wire structured diagnostics.
pub fn position_to_offset(text: &str, position: Position) -> usize {
    let mut current_line = 0;
    let mut line_start = 0;

    for (i, ch) in text.char_indices() {
        if current_line == position.line as usize {
            // We're on the right line; advance by character count.
            for (chars_in, (j, _)) in text[line_start..].char_indices().enumerate() {
                if chars_in == position.character as usize {
                    return line_start + j;
                }
            }
            return text.len();
        }
        if ch == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }

    if current_line == position.line as usize {
        line_start
    } else {
        text.len()
    }
}

/// The document store.
#[derive(Default, Debug)]
pub struct DocumentStore {
    docs: RwLock<HashMap<Url, Document>>,
}

impl DocumentStore {
    /// Create a new empty document store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a new document.
    pub fn open(&self, uri: Url, version: i32, text: String) {
        let mut docs = self.docs.write().unwrap();
        docs.insert(uri.clone(), Document::new(uri, version, text));
    }

    /// Apply changes to an open document. Returns true if successful, false if document not found.
    pub fn change(
        &self,
        uri: &Url,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
    ) -> bool {
        let mut docs = self.docs.write().unwrap();
        if let Some(doc) = docs.get_mut(uri) {
            for change in changes {
                doc.apply_change(change);
            }
            doc.version = version;
            true
        } else {
            false
        }
    }

    /// Close a document. Returns true if the document existed, false otherwise.
    pub fn close(&self, uri: &Url) -> bool {
        let mut docs = self.docs.write().unwrap();
        docs.remove(uri).is_some()
    }

    /// Get a document by URI. Returns a clone of the document or None if not found.
    pub fn get(&self, uri: &Url) -> Option<Document> {
        let docs = self.docs.read().unwrap();
        docs.get(uri).cloned()
    }

    /// Get the number of open documents.
    pub fn len(&self) -> usize {
        self.docs.read().unwrap().len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.docs.read().unwrap().is_empty()
    }

    /// Iterate over all documents in the store, yielding (URI, Document) pairs.
    pub fn iter(&self) -> Vec<(Url, Document)> {
        let docs = self.docs.read().unwrap();
        docs.iter()
            .map(|(uri, doc)| (uri.clone(), doc.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Range;

    #[test]
    fn apply_change_whole_document_replaces_text() {
        let mut doc = Document::new(
            Url::parse("file:///test.pax").unwrap(),
            1,
            "old".to_string(),
        );
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "new".to_string(),
        };
        doc.apply_change(&change);
        assert_eq!(doc.text, "new");
    }

    #[test]
    fn apply_change_incremental_replaces_range() {
        let mut doc = Document::new(
            Url::parse("file:///test.pax").unwrap(),
            1,
            "hello world".to_string(),
        );
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 6,
                },
                end: Position {
                    line: 0,
                    character: 11,
                },
            }),
            range_length: Some(5),
            text: "paideia".to_string(),
        };
        doc.apply_change(&change);
        assert_eq!(doc.text, "hello paideia");
    }

    #[test]
    fn position_to_offset_handles_first_line() {
        let text = "abc";
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 0,
                    character: 0
                }
            ),
            0
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 0,
                    character: 1
                }
            ),
            1
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 0,
                    character: 3
                }
            ),
            3
        );
    }

    #[test]
    fn position_to_offset_handles_subsequent_lines() {
        let text = "abc\ndef\nghi";
        // First line: "abc"
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 0,
                    character: 0
                }
            ),
            0
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 0,
                    character: 3
                }
            ),
            3
        );
        // Second line: "def"
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 1,
                    character: 0
                }
            ),
            4
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 1,
                    character: 2
                }
            ),
            6
        );
        // Third line: "ghi"
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 2,
                    character: 0
                }
            ),
            8
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 2,
                    character: 2
                }
            ),
            10
        );
    }

    #[test]
    fn document_store_open_change_close_lifecycle() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();

        // Initially empty
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        // Open a document
        store.open(uri.clone(), 1, "hello".to_string());
        assert_eq!(store.len(), 1);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "hello");
        assert_eq!(doc.version, 1);

        // Change the document
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            }),
            range_length: Some(5),
            text: "goodbye".to_string(),
        }];
        let success = store.change(&uri, 2, &changes);
        assert!(success);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "goodbye");
        assert_eq!(doc.version, 2);

        // Close the document
        let closed = store.close(&uri);
        assert!(closed);
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.get(&uri).is_none());
    }

    #[test]
    fn snapshot_three_inserts_at_end() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();

        store.open(uri.clone(), 1, "abc".to_string());

        // Insert "d" at (0, 3)
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 3,
                },
                end: Position {
                    line: 0,
                    character: 3,
                },
            }),
            range_length: Some(0),
            text: "d".to_string(),
        }];
        store.change(&uri, 2, &changes);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "abcd");

        // Insert "e" at (0, 4)
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 4,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            }),
            range_length: Some(0),
            text: "e".to_string(),
        }];
        store.change(&uri, 3, &changes);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "abcde");

        // Insert "f" at (0, 5)
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 5,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            }),
            range_length: Some(0),
            text: "f".to_string(),
        }];
        store.change(&uri, 4, &changes);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "abcdef");
    }

    #[test]
    fn snapshot_replace_middle() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();

        store.open(uri.clone(), 1, "hello".to_string());

        // Replace "ell" with "X": range (0, 1)..(0, 4)
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            }),
            range_length: Some(3),
            text: "X".to_string(),
        }];
        store.change(&uri, 2, &changes);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "hXo");
    }

    #[test]
    fn snapshot_multi_line_edit() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.pax").unwrap();

        store.open(uri.clone(), 1, "abc\ndef\n".to_string());

        // Insert "X" at (1, 0) (beginning of second line)
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 1,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 0,
                },
            }),
            range_length: Some(0),
            text: "X".to_string(),
        }];
        store.change(&uri, 2, &changes);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text, "abc\nXdef\n");
    }
}
