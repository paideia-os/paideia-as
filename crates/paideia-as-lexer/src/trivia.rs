//! Trivia — lexical content discarded from the main token stream.
//!
//! Whitespace (§2.1) and comments (§2.2) are not tokens for the parser
//! but the formatter, documentation generator, and LSP attach to them.
//! The lexer emits `Trivia` separately from `Token`; downstream tools
//! can re-associate trivia to adjacent tokens via the byte ranges
//! carried in [`Span`].

use paideia_as_diagnostics::Span;

/// A lexical class of trivia.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum TriviaKind {
    /// Run of whitespace per `syntax-reference.md` §2.1 (TAB, SPACE).
    Whitespace,
    /// A newline (LF or CRLF). Distinct from generic whitespace because
    /// it acts as a statement separator within a block.
    Newline,
    /// `// …` line comment.
    LineComment,
    /// `/* … */` block comment (may span lines).
    BlockComment,
    /// `/// …` line doc comment.
    DocLineComment,
    /// `/** … */` block doc comment.
    DocBlockComment,
}

/// A piece of trivia with its source-position range.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Trivia {
    /// The lexical class.
    pub kind: TriviaKind,
    /// Source-position range covered by this trivia run.
    pub span: Span,
}

impl Trivia {
    /// Construct a `Trivia`.
    pub fn new(kind: TriviaKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns `true` iff this trivia is a doc comment (line or block).
    /// Doc comments attach to the next declaration per §2.2.
    pub fn is_doc(&self) -> bool {
        matches!(
            self.kind,
            TriviaKind::DocLineComment | TriviaKind::DocBlockComment
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn trivia_constructs_and_compares() {
        let a = Trivia::new(TriviaKind::Whitespace, span());
        let b = Trivia::new(TriviaKind::Whitespace, span());
        assert_eq!(a, b);
    }

    #[test]
    fn doc_comments_classified_as_doc() {
        assert!(Trivia::new(TriviaKind::DocLineComment, span()).is_doc());
        assert!(Trivia::new(TriviaKind::DocBlockComment, span()).is_doc());
    }

    #[test]
    fn non_doc_trivia_not_classified_as_doc() {
        for kind in [
            TriviaKind::Whitespace,
            TriviaKind::Newline,
            TriviaKind::LineComment,
            TriviaKind::BlockComment,
        ] {
            assert!(!Trivia::new(kind, span()).is_doc());
        }
    }
}
