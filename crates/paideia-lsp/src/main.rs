//! paideia-lsp binary — stdio-based LSP server.

use paideia_lsp::incremental::IncrementalEngine;
use paideia_lsp::{Backend, DocumentStore, DslDiagnosticHandle, ParseCache, install_router_handle};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    // R220.M12: install the hosted-DSL diagnostic router once at server
    // startup so a hosted DSL (@dsl_parser body per R220.M3) that emits
    // via `Elab.elab_error` / `Elab.elab_warn` reaches this session's
    // diagnostic stream.  Idempotent — safe if a client relaunches the
    // binary inside the same test harness.
    install_router_handle(DslDiagnosticHandle::new());

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
