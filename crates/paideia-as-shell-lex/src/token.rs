//! Token type + span for the R221.M4 shell lexer.
//!
//! Every emitted token is a triple `(kind, span, context)`. `kind`
//! discriminates the surface syntax; `span` is a byte range into the
//! original source (for the R221.M6 provenance layer + R229 REPL
//! syntax colorer); `context` is the sub-language grammar the parser
//! should invoke at R221.M5.
//!
//! # Design decisions
//!
//! * **Owned strings** for identifiers and literals rather than
//!   borrowing from the source. The R229 REPL edits its input buffer
//!   incrementally; a token that borrowed from a since-mutated buffer
//!   would dangle. R221.M6 will offer a `TokenRef<'a>` companion that
//!   borrows, for parse-once-and-discard workloads (script loading).
//!
//! * **`Dot` and `Pipe` as one variant each**, regardless of context.
//!   The parser at R221.M5 dispatches on `context` to decide whether a
//!   `Dot` in `Datalog` context ends a fact or (in `Pipeline`/`Lambda`)
//!   is field-access. This keeps the lexer shape-blind — see the
//!   crate-level doc's "What this crate does NOT decide" section.
//!
//! * **`DatalogKw` as a distinct variant**, separate from
//!   `Ident("datalog")`. The lexer needs to gate the next `{`'s context
//!   push on whether the preceding identifier was `datalog`; making
//!   the discrimination a `TokenKind` variant (rather than a string
//!   compare on `Ident`) both encodes the reserved-word status in the
//!   type system and lets a REPL colorer render `datalog` in the
//!   keyword face.
//!
//! * **`Op(String)` as an escape hatch**. `+`, `-`, `*`, `/`, `<`, `>`,
//!   `<=`, `>=`, `==`, `!=`, `and`, `or`, `not` are all `Op`. The full
//!   operator lexicon lives in R221.M5's grammar; the lexer glues
//!   contiguous punctuation runs and looks up the keyword operators
//!   (`and`/`or`/`not`) at the identifier stage.

use crate::context::Context;

/// Half-open byte range `[start, end)` into the source string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl Span {
    /// Construct a span from a start and end offset.
    #[inline]
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "empty or reversed span: {start}..{end}");
        Self { start, end }
    }

    /// Length in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span is empty (start == end).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Discriminated token variants. See the module doc for design notes on
/// why `Dot` / `Pipe` / `DatalogKw` are shaped as they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Bare identifier (a command name, argument, variable name, field
    /// name). Stored as an owned `String` so the token outlives an
    /// edited REPL buffer.
    Ident(String),
    /// The reserved word `datalog`. Distinct from `Ident("datalog")`
    /// so the state machine can gate the following `{`'s context push
    /// without a string compare (and so R229's colorer can render it
    /// in the keyword face).
    DatalogKw,
    /// A Datalog logic variable `?name`. The `?` sigil is not stored
    /// (the token *is* the variable reference).
    QVar(String),
    /// A Datalog pipeline-value interpolation `$name`. Only meaningful
    /// inside `Context::Datalog`; the lexer still emits it in other
    /// contexts so R229's colorer can flag the misuse.
    InterpVar(String),
    /// Numeric literal (integer or floating; the parser at R221.M5
    /// disambiguates). Stored as the raw source lexeme for lossless
    /// round-trip.
    Number(String),
    /// String literal with quotes stripped and escapes preserved
    /// unresolved. The parser evaluates escape sequences; the lexer
    /// keeps the raw inner text for source-round-trip.
    Str(String),
    /// `|` — pipeline joiner in `Pipeline`, parameter delimiter in
    /// `Lambda`.
    Pipe,
    /// `{` — Datalog-block opener if preceded by `DatalogKw`, else
    /// Lambda opener. Emitted with the *newly-pushed* context stamped.
    LBrace,
    /// `}` — closer for the enclosing block. Emitted with the *still-
    /// current* (about-to-be-popped) context; the pop happens after.
    RBrace,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `,`
    Comma,
    /// `.` — Datalog fact terminator or field-access; parser decides.
    Dot,
    /// `;` — pipeline expression separator.
    Semi,
    /// Newline (`\n` or `\r\n`). At `Pipeline` top level ends the
    /// current expression; inside `Datalog` / `Lambda` it is
    /// whitespace-equivalent.
    Newline,
    /// `=>` — Datalog rule head/body separator, or an in-expression
    /// arrow in `Lambda`.
    FatArrow,
    /// `->` — lambda arrow in the `\args -> body` form.
    ThinArrow,
    /// `\` — lambda introducer in the `\args -> body` form.
    Backslash,
    /// Operator / punctuation glyph the lexer glued but did not further
    /// classify: `+`, `-`, `*`, `/`, `<`, `>`, `<=`, `>=`, `==`, `!=`.
    /// The keyword operators `and`, `or`, `not` also arrive here (the
    /// identifier scanner recognizes them and rewrites).
    Op(String),
}

/// A lexed token: its shape (`kind`), its origin (`span`), and the
/// sub-language grammar the parser should apply (`context`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The token's discriminated shape.
    pub kind: TokenKind,
    /// Byte range into the original source string.
    pub span: Span,
    /// Sub-language context active at the moment this token was emitted.
    pub context: Context,
}

impl Token {
    /// Construct a token; convenience for the lexer.
    #[inline]
    pub fn new(kind: TokenKind, span: Span, context: Context) -> Self {
        Self { kind, span, context }
    }
}
