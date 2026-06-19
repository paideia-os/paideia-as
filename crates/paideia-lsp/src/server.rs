//! LSP server backend implementation.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

/// The paideia-lsp backend implementing the Language Server Protocol.
pub struct Backend {
    /// Client for communicating with the editor.
    pub client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "paideia-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: capabilities(),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "paideia-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// Server capabilities per editor-support.md §1.1.
///
/// Phase-2-m8-001: Advertises text-document sync (incremental), hover,
/// definition, references, completion, code action, formatting, and
/// semantic tokens. However, handlers are no-op stubs; real handlers
/// ship in m8-003..010.
pub fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend::default(),
                range: Some(false),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_advertises_text_document_sync_incremental() {
        let caps = capabilities();
        assert!(matches!(
            caps.text_document_sync,
            Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL
            ))
        ));
    }

    #[test]
    fn capabilities_advertises_hover_definition_references_completion() {
        let caps = capabilities();
        assert!(caps.hover_provider.is_some());
        assert!(caps.definition_provider.is_some());
        assert!(caps.references_provider.is_some());
        assert!(caps.completion_provider.is_some());
    }

    #[test]
    fn capabilities_advertises_code_action_formatting_semantic_tokens() {
        let caps = capabilities();
        assert!(caps.code_action_provider.is_some());
        assert!(caps.document_formatting_provider.is_some());
        assert!(caps.semantic_tokens_provider.is_some());
    }
}
