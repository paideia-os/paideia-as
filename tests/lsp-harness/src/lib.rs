//! LSP harness: library for programmatic end-to-end testing of LSP handlers.
//!
//! This harness exercises paideia-lsp's public API directly (no JSON-RPC stdio),
//! testing diagnostic publication, hover, definition, and references handlers
//! against in-memory documents stored in a DocumentStore.

use tower_lsp::lsp_types::Url;

pub use paideia_lsp::ParseCache;
pub use paideia_lsp::diagnostics::{diagnose_document, diagnose_document_with_cache};
pub use paideia_lsp::document::{Document, DocumentStore};
pub use paideia_lsp::hover::{SubstructuralClass, TokenKind, hover_at};
pub use paideia_lsp::navigation::{definition_at, references_at};

/// Create a test URL from a filename.
pub fn test_url(filename: &str) -> Url {
    Url::from_file_path(format!("/tmp/test/{}", filename)).expect("valid file URL")
}

/// Helper to create and open a document in the store.
pub fn create_document(store: &DocumentStore, filename: &str, text: &str) -> Url {
    let uri = test_url(filename);
    store.open(uri.clone(), 1, text.to_string());
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_creates_valid_file_url() {
        let url = test_url("test.pax");
        assert!(url.to_file_path().is_ok());
    }

    #[test]
    fn create_document_opens_document_in_store() {
        let store = DocumentStore::new();
        let uri = create_document(&store, "test.pax", "fn main() {}");
        assert!(store.get(&uri).is_some());
    }
}
