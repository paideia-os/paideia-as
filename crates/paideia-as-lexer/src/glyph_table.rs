//! Unicode-glyph / ASCII-fallback table for paideia-as operators and
//! punctuation per `syntax-reference.md` §4 (decision SY-D2).
//!
//! `paideia-as` accepts either form. `paideia-fmt` normalizes to the
//! Unicode form; `paideia-fmt --ascii` produces ASCII-only output.
//!
//! This table is the canonical mapping for the operator scanner. It does
//! NOT include §3.4 reserved words; those are handled by
//! [`crate::reserved::lookup`] (e.g., `forall`, `in`, `fn`).

use crate::token::TokenKind;

/// One entry in the glyph table.
#[derive(Copy, Clone, Debug)]
pub struct GlyphEntry {
    /// The Unicode glyph (or `None` if the form is ASCII-only).
    pub unicode: Option<&'static str>,
    /// The ASCII spelling. Always present.
    pub ascii: &'static str,
    /// The [`TokenKind`] this glyph produces.
    pub kind: TokenKind,
}

/// Table of operator/punctuation glyphs that have both Unicode and ASCII
/// spellings, listed longest-ASCII-first for the longest-match scanner.
///
/// Entries with only a Unicode form (e.g., `λ`, `∀`) and entries that
/// are reserved words in ASCII (`forall`, `in`, `fn`) are not in this
/// table — those are handled by [`crate::reserved::lookup`] which extends
/// with Unicode synonyms.
pub const GLYPHS: &[GlyphEntry] = &[
    // Effect/capability brackets (ASCII-only)
    GlyphEntry {
        unicode: None,
        ascii: "!{",
        kind: TokenKind::EffectOpen,
    },
    GlyphEntry {
        unicode: None,
        ascii: "@{",
        kind: TokenKind::CapOpen,
    },
    // Unicode-primary forms with ASCII fallback
    GlyphEntry {
        unicode: Some("→"),
        ascii: "->",
        kind: TokenKind::Arrow,
    },
    GlyphEntry {
        unicode: Some("↦"),
        ascii: "=>",
        kind: TokenKind::FatArrow,
    },
    GlyphEntry {
        unicode: Some("∷"),
        ascii: "::",
        kind: TokenKind::ColonColon,
    },
    GlyphEntry {
        unicode: Some("↓"),
        ascii: "$",
        kind: TokenKind::LinearMark,
    },
    // `~` is ASCII-canonical per §4 (AffineMark).
    GlyphEntry {
        unicode: None,
        ascii: "~",
        kind: TokenKind::AffineMark,
    },
];

/// Returns the entry whose ASCII spelling matches the supplied text exactly.
#[must_use]
pub fn by_ascii(text: &str) -> Option<&'static GlyphEntry> {
    GLYPHS.iter().find(|e| e.ascii == text)
}

/// Returns the entry whose Unicode spelling matches the supplied text exactly.
#[must_use]
pub fn by_unicode(text: &str) -> Option<&'static GlyphEntry> {
    GLYPHS.iter().find(|e| e.unicode == Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_arrow_resolves() {
        assert_eq!(by_ascii("->").unwrap().kind, TokenKind::Arrow);
    }

    #[test]
    fn unicode_arrow_resolves() {
        assert_eq!(by_unicode("→").unwrap().kind, TokenKind::Arrow);
    }

    #[test]
    fn ascii_and_unicode_agree_on_kind() {
        for e in GLYPHS {
            if let Some(uni) = e.unicode {
                assert_eq!(by_unicode(uni).unwrap().kind, e.kind);
            }
            assert_eq!(by_ascii(e.ascii).unwrap().kind, e.kind);
        }
    }

    #[test]
    fn unknown_returns_none() {
        assert!(by_ascii("zzz").is_none());
        assert!(by_unicode("∞").is_none());
    }
}
