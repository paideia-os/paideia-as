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
pub mod hover;
pub mod incremental;
pub mod inlay_hints;
pub mod navigation;
pub mod semantic_tokens;
pub mod server;
pub mod workspace;

pub use cache::{CacheEntry, ParseCache, content_hash};
pub use document::{Document, DocumentStore};
pub use incremental::IncrementalEngine;
pub use server::{Backend, capabilities};
pub use workspace::{ManifestError, SigningConfig, WorkspaceConfig, WorkspaceManifest};
