//! paideia-lsp binary — stdio-based LSP server.

use paideia_lsp::{Backend, DocumentStore};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        store: DocumentStore::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
