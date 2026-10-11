//! UAX#15 Normalization Form C shim.
//!
//! The real NFC implementation (and its NFD / NFKC / NFKD siblings)
//! lives in [`crate::norm`] after the UNI-1 generalization. This module
//! stays as a thin re-export surface so the shell-lex / shell-ast /
//! shell-completion consumers that `use paideia_as_unicode::nfc::...`
//! keep compiling without a cascade edit.
//!
//! New callers should import from `paideia_as_unicode` (the crate root)
//! or from `paideia_as_unicode::norm` directly. This shim is kept for
//! source compatibility and will be marked `#[deprecated]` once every
//! consumer migrates (tracked by UNI-16).

pub use crate::norm::{is_ascii_fast_path_eligible, is_nfc, nfc_normalize};
