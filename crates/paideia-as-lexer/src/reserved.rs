//! Reserved-word table per `syntax-reference.md` §3.4.
//!
//! The canonical lookup table is implemented as a `match` in
//! [`crate::token::keyword_kind`] for now; a `phf` perfect-hash table is
//! a future optimization (the AC text mentions `phf`, deferred for the
//! same workspace-allowlist reason as PR-4's catalog).
//!
//! This module exists so callers and downstream tooling can `use
//! paideia_as_lexer::reserved::{is_reserved, RESERVED_WORDS};` without
//! reaching into the token module.

use crate::token::{TokenKind, keyword_kind};

pub use crate::token::RESERVED_WORDS;

/// Returns `true` if `text` is one of the 71 reserved words from §3.4.
///
/// Equivalent to `crate::token::keyword_kind(text).is_some()`, exposed as
/// a predicate for callers that don't need the `TokenKind`.
#[must_use]
pub fn is_reserved(text: &str) -> bool {
    keyword_kind(text).is_some()
}

/// Returns the [`TokenKind`] for `text` if it is a reserved word, else
/// `None`. Re-exported from [`crate::token::keyword_kind`].
#[must_use]
pub fn lookup(text: &str) -> Option<TokenKind> {
    keyword_kind(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_reserved_words_are_reserved() {
        for kw in RESERVED_WORDS {
            assert!(is_reserved(kw), "expected {kw:?} to be reserved");
        }
    }

    #[test]
    fn ordinary_words_are_not_reserved() {
        for w in ["foo", "bar", "Foo", "_x", "x42", "家族"] {
            assert!(!is_reserved(w), "did not expect {w:?} to be reserved");
        }
    }

    #[test]
    fn lookup_matches_keyword_kind() {
        assert_eq!(lookup("let"), Some(TokenKind::KwLet));
        assert_eq!(lookup("not_a_keyword"), None);
    }
}
