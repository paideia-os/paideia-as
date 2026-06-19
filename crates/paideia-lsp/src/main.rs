//! paideia-lsp binary — stdio-based LSP server.

use paideia_lsp::incremental::IncrementalEngine;
use paideia_lsp::{Backend, DocumentStore, ParseCache};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        store: DocumentStore::new(),
        cache: ParseCache::with_default_capacity(),
        engine: IncrementalEngine::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
