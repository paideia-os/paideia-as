//! paideia-lsp: Language Server Protocol implementation for paideia-as.
//!
//! LSP server wrapping the elaborator.
//! Design: design/toolchain/editor-support.md in the PaideiaOS monorepo.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod cache;
pub mod code_action;
pub mod completion;
pub mod diagnostics;
pub mod document;
pub mod dsl_embed;
pub mod hover;
pub mod incremental;
pub mod inlay_hints;
pub mod navigation;
pub mod semantic_tokens;
pub mod server;
pub mod workspace;

pub use cache::{CacheEntry, ParseCache, content_hash};
pub use document::{Document, DocumentStore};
pub use dsl_embed::{
    DslDiagnosticHandle, HOSTED_DSL_CODE_MAX, HOSTED_DSL_CODE_MIN, current_handle,
    hosted_error_code, hosted_note_code, hosted_warn_code, install_router_handle,
    interleave_by_span, with_current_handle,
};
pub use incremental::IncrementalEngine;
pub use server::{Backend, capabilities};
pub use workspace::{ManifestError, SigningConfig, WorkspaceConfig, WorkspaceManifest};
