//! paideia-lsp: Language Server Protocol implementation for paideia-as.
//!
//! LSP server wrapping the elaborator.
//! Design: design/toolchain/editor-support.md in the PaideiaOS monorepo.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod diagnostics;
pub mod document;
pub mod server;
pub mod workspace;

pub use document::{Document, DocumentStore};
pub use server::{Backend, capabilities};
pub use workspace::{ManifestError, SigningConfig, WorkspaceConfig, WorkspaceManifest};
