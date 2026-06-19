//! LSP server backend implementation.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::cache::ParseCache;
use crate::document::DocumentStore;
use crate::incremental::IncrementalEngine;

/// The paideia-lsp backend implementing the Language Server Protocol.
pub struct Backend {
    /// Client for communicating with the editor.
    pub client: Client,
    /// In-memory document store.
    pub store: DocumentStore,
    /// Parse cache for incremental elaboration.
    pub cache: ParseCache,
    /// Incremental elaboration engine.
    pub engine: IncrementalEngine,
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

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        self.store
            .open(uri.clone(), params.text_document.version, text.clone());
        self.engine.set_document(uri.to_string().as_str(), &text);
        let diagnostics =
            crate::diagnostics::diagnose_document_with_cache(&uri, &text, &self.cache);
        self.client
            .publish_diagnostics(uri, diagnostics, Some(params.text_document.version))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.store
            .change(&uri, params.text_document.version, &params.content_changes);
        let Some(doc) = self.store.get(&uri) else {
            return;
        };
        self.engine
            .set_document(uri.to_string().as_str(), &doc.text);
        let diagnostics =
            crate::diagnostics::diagnose_document_with_cache(&uri, &doc.text, &self.cache);
        self.client
            .publish_diagnostics(uri, diagnostics, Some(doc.version))
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.store.close(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        Ok(crate::hover::hover_at(&self.store, &params))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(crate::completion::completion_at(&self.store, &params))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(crate::navigation::definition_at(&self.store, &params))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        Ok(crate::navigation::references_at(&self.store, &params))
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        let actions = crate::code_action::code_actions_at(&self.store, &params);
        Ok(actions.map(|acts| {
            acts.into_iter()
                .map(CodeActionOrCommand::CodeAction)
                .collect()
        }))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(doc) = self.store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let opts = paideia_fmt::FormatOptions::default();
        let formatted = paideia_fmt::format(&doc.text, &opts);
        if formatted == doc.text {
            return Ok(Some(vec![]));
        }
        let line_count = doc.text.lines().count() as u32;
        let last_line_len = doc.text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
        let full_range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count,
                character: last_line_len,
            },
        };
        Ok(Some(vec![TextEdit {
            range: full_range,
            new_text: formatted,
        }]))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        Ok(crate::semantic_tokens::semantic_tokens_at(
            &self.store,
            &params,
        ))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        Ok(crate::inlay_hints::inlay_hints_at(&self.store, &params))
    }
}

/// Server capabilities per editor-support.md §1.1.
///
/// Phase-2-m8-011: Advertises text-document sync (incremental), hover,
/// definition, references, completion, code action, formatting,
/// semantic tokens, and inlay hints.
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
        inlay_hint_provider: Some(OneOf::Left(true)),
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

    #[test]
    fn capabilities_advertises_inlay_hint_provider() {
        let caps = capabilities();
        assert!(caps.inlay_hint_provider.is_some());
    }
}
