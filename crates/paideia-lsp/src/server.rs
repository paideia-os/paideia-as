//! LSP server backend implementation.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::cache::ParseCache;
use crate::diagnostics::to_lsp_diagnostic;
use crate::document::{DocumentStore, position_to_offset};
use crate::dsl_embed::{DslDiagnosticHandle, current_handle, install_router_handle};
use crate::incremental::IncrementalEngine;

/// Resolve `FormatOptions` for a document URI by walking ancestors from
/// the document's parent directory looking for `.pdxfmt` (FMT-11
/// discovery semantics). On any failure (non-`file:` URI, URL that does
/// not map to a path, missing or malformed `.pdxfmt`), fall back to
/// [`paideia_fmt::FormatOptions::default`].
///
/// FMT-16 (paideia-as#1580). Kept as a free function so the two
/// formatting handlers (`textDocument/formatting` and
/// `textDocument/rangeFormatting`) share the same discovery contract.
fn load_fmt_options(uri: &Url) -> paideia_fmt::FormatOptions {
    let mut opts = paideia_fmt::FormatOptions::default();
    let Ok(path) = uri.to_file_path() else {
        return opts;
    };
    let start = match path.parent() {
        Some(p) => p.to_path_buf(),
        None => return opts,
    };
    if let Some((_found, cfg)) = paideia_fmt::config::discover(&start) {
        paideia_fmt::config::apply_to_options(&cfg, &mut opts);
    }
    opts
}

/// Compute the full-document LSP range for `text`.
///
/// Walks a position-end that always sits at (last_line, last_line_len).
/// Pulled out of the former `formatting` body so the AST and
/// range-formatting handlers share a single implementation.
fn full_document_range(text: &str) -> Range {
    let line_count = text.lines().count() as u32;
    let last_line_len = text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: line_count,
            character: last_line_len,
        },
    }
}

/// Expand `(start, end)` (byte offsets into `text`) to the smallest
/// line-aligned region whose extent fully contains the range.
///
/// FMT-16: this is the "smallest enclosing node" approximation for a
/// mid-expression `textDocument/rangeFormatting` request. The LSP spec
/// (3.17 §textDocument_rangeFormatting) permits — and in practice
/// requires — the server to widen the range to the nearest structural
/// boundary when it can't scope formatting to an arbitrary byte range.
/// Full lines are a conservative pre-FMT-9/10 approximation of
/// `smallest-enclosing-AST-node`: they are guaranteed to be a superset
/// of any single-line enclosing item and compose cleanly with the
/// `format_whitespace`-based idempotent pipeline today.
///
/// FIXME(fmt-16-ast-span): once FMT-9/10 land per-node formatting we
/// should replace this helper with an AST walk against the arena
/// produced by `paideia_fmt::format` and pick the smallest
/// [`paideia_as_ast::NodeData::span`] whose `[byte_start, byte_end)`
/// fully contains `(start, end)`.
fn widen_to_full_lines(text: &str, start: usize, end: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let s = start.min(n);
    let e = end.min(n).max(s);

    // Walk backwards from `s` to the start of its line.
    let mut line_start = s;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    // Walk forwards from `e` to the end of its line (exclusive of the
    // terminating '\n' when there is one; inclusive to EOF otherwise).
    // When `e` already sits at a line boundary we leave it alone — this
    // keeps `widen_to_full_lines(text, 0, text.len()) == (0, text.len())`
    // the identity, which keeps the whole-file test straightforward.
    let mut line_end = e;
    while line_end < n && bytes[line_end] != b'\n' {
        line_end += 1;
    }

    (line_start, line_end)
}

/// R220.M12: drain any hosted-DSL diagnostics that accumulated in the
/// router during document elaboration and interleave them (LSP-shape)
/// with the native diagnostics computed by `diagnose_document_with_cache`.
///
/// A no-op when no router handle is installed or when nothing hosted was
/// emitted — the fast path for source files with no `@dsl_parser`
/// attachments.  Called from `did_open` / `did_change` immediately
/// before `publish_diagnostics`.
///
/// FIXME(per-doc-scope): under concurrent document editing the router
/// slot is process-wide, so two documents that both trigger hosted
/// diagnostics could observe each other's output.  R229 introduces
/// per-document `with_current_handle` scoping.  For today's
/// single-threaded diagnose path this is fine.
fn merge_hosted(mut native: Vec<Diagnostic>, source_text: &str) -> Vec<Diagnostic> {
    let Some(handle) = current_handle() else {
        return native;
    };
    let hosted = handle.drain();
    if hosted.is_empty() {
        return native;
    }
    for d in hosted {
        native.push(to_lsp_diagnostic(&d, source_text));
    }
    native
}

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
        // R220.M12: ensure the router handle is installed at least once
        // per Backend construction.  In the binary path, `main.rs` also
        // installs it (idempotent); test harnesses that build a Backend
        // directly get the same guarantee here.
        let _ = install_router_handle(DslDiagnosticHandle::new());

        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        self.store
            .open(uri.clone(), params.text_document.version, text.clone());
        self.engine.set_document(uri.to_string().as_str(), &text);
        let native =
            crate::diagnostics::diagnose_document_with_cache(&uri, &text, &self.cache);
        let diagnostics = merge_hosted(native, &text);
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
        let native =
            crate::diagnostics::diagnose_document_with_cache(&uri, &doc.text, &self.cache);
        let diagnostics = merge_hosted(native, &doc.text);
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
        let opts = load_fmt_options(&params.text_document.uri);
        // FMT-16 (paideia-as#1580): the LSP format-on-save path drives the
        // AST pipeline `paideia_fmt::format`. The function's documented
        // fallback contract (lex / parse failure → verbatim source) means
        // we never need a secondary error branch here — a parse-broken
        // file simply yields `formatted == doc.text` and we return an
        // empty edit list.
        let formatted = match paideia_fmt::format(&doc.text, &opts) {
            Ok(s) => s,
            // `FormatError::Reserved` is the only variant today and is
            // never returned by `format`; keep the arm total.
            Err(_) => return Ok(Some(vec![])),
        };
        if formatted == doc.text {
            return Ok(Some(vec![]));
        }
        Ok(Some(vec![TextEdit {
            range: full_document_range(&doc.text),
            new_text: formatted,
        }]))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let Some(doc) = self.store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let opts = load_fmt_options(&params.text_document.uri);

        // Run the AST pipeline on the full document. The format function
        // is byte-identical on parse failure (verbatim fallback) and —
        // today, through FMT-15 — byte-identical on parse success too.
        // Both branches collapse into the `formatted == doc.text` short
        // circuit below. When FMT-9/10 begin to emit structural edits,
        // the slice-back path takes over.
        let formatted = match paideia_fmt::format(&doc.text, &opts) {
            Ok(s) => s,
            Err(_) => return Ok(Some(vec![])),
        };
        if formatted == doc.text {
            return Ok(Some(vec![]));
        }

        // Byte offsets for the user-requested range in the ORIGINAL
        // text.
        let orig_start = position_to_offset(&doc.text, params.range.start);
        let orig_end = position_to_offset(&doc.text, params.range.end);
        if orig_end < orig_start {
            // Malformed range from the client: no safe edit to compute.
            return Ok(Some(vec![]));
        }

        // Smallest-enclosing-node approximation (see `widen_to_full_lines`
        // for the FMT-9/10 upgrade note). We compute the enclosing
        // region in the original byte space so the follow-up patch —
        // once a per-node formatter API lands — can slice from the
        // formatted output across that window. Today the computation
        // only gates the no-edit guard (`encl_end > formatted.len()`);
        // the returned `TextEdit` always carries the ORIGINAL
        // user-requested range per the FMT-16 "slice back the diff"
        // discipline, so the edit never widens beyond the user's
        // intent.
        let (_encl_start, encl_end) = widen_to_full_lines(&doc.text, orig_start, orig_end);

        // If the formatted output's byte extent at the enclosing region
        // can't be sliced (text got longer AND the orig range extends
        // past the formatted length), fall back to no edit — safer than
        // a wrong one. The whole-file `formatting` handler still catches
        // the full-document case.
        if orig_end > formatted.len() || encl_end > formatted.len() {
            return Ok(Some(vec![]));
        }

        // In the FMT-15 byte-identical era `formatted[orig_start..orig_end]`
        // equals `doc.text[orig_start..orig_end]`, so the slice-back is
        // a semantic no-op. FMT-9/10 will produce real differences; the
        // same code path then returns a non-empty structural edit.
        let new_text = formatted
            .get(orig_start..orig_end)
            .unwrap_or("")
            .to_string();
        if new_text.as_bytes() == &doc.text.as_bytes()[orig_start..orig_end] {
            return Ok(Some(vec![]));
        }

        Ok(Some(vec![TextEdit {
            range: params.range,
            new_text,
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
        // FMT-16 (paideia-as#1580): advertise rangeFormatting so clients
        // can send `textDocument/rangeFormatting` requests against the
        // AST pipeline.
        document_range_formatting_provider: Some(OneOf::Left(true)),
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

    /// FMT-16 (paideia-as#1580): `textDocument/rangeFormatting` must be
    /// advertised so clients route sub-range format requests at the AST
    /// pipeline.
    #[test]
    fn capabilities_advertises_document_range_formatting_provider() {
        let caps = capabilities();
        assert!(caps.document_range_formatting_provider.is_some());
    }

    /// FMT-16: `widen_to_full_lines` is the smallest-enclosing-node
    /// approximation. These unit tests pin the three structural cases
    /// (whole file, mid-line start, mid-line end) at the function
    /// boundary.
    #[test]
    fn widen_to_full_lines_identity_on_whole_file() {
        let text = "let a = 1\nlet b = 2\n";
        let (s, e) = widen_to_full_lines(text, 0, text.len());
        assert_eq!(s, 0);
        assert_eq!(e, text.len());
    }

    #[test]
    fn widen_to_full_lines_expands_mid_line_start() {
        let text = "let a = 1\nlet b = 2\n";
        // Range (4..7) sits mid-first-line; widens to [0..9).
        let (s, e) = widen_to_full_lines(text, 4, 7);
        assert_eq!(s, 0);
        assert_eq!(e, 9);
    }

    #[test]
    fn widen_to_full_lines_expands_mid_line_end() {
        let text = "let a = 1\nlet b = 2\n";
        // Range (10..13) sits mid-second-line; widens to [10..19).
        let (s, e) = widen_to_full_lines(text, 10, 13);
        assert_eq!(s, 10);
        assert_eq!(e, 19);
    }

    #[test]
    fn widen_to_full_lines_clamps_out_of_range() {
        // `"abc"` is a single line (no '\n'), so an out-of-range request
        // clamps to the end then widens the full-line window back to
        // (0, 3) — the whole document.
        let text = "abc";
        let (s, e) = widen_to_full_lines(text, 100, 200);
        assert_eq!(s, 0);
        assert_eq!(e, 3);
    }

    /// FMT-16: `load_fmt_options` resolves to defaults when the URI is
    /// not a file-URL (e.g. a `vscode-untitled://` or `untitled:` URL
    /// from a scratch buffer). Guards the handler fall-back path.
    #[test]
    fn load_fmt_options_non_file_uri_returns_defaults() {
        let uri = Url::parse("untitled:scratch").expect("parse");
        let opts = load_fmt_options(&uri);
        let default = paideia_fmt::FormatOptions::default();
        assert_eq!(opts.max_width, default.max_width);
        assert_eq!(opts.indent_width, default.indent_width);
    }
}
