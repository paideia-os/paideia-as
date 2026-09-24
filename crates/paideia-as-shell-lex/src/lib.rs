//! paideia-as-shell-lex — R221.M4 context-tracking lexer for the
//! semantic shell's three sub-languages (Pipeline / Datalog / Lambda).
//!
//! # SH-D1 in one paragraph
//!
//! The shell composes three sub-languages *lexically*, not
//! semantically: the parser is in one of `Pipeline`, `Datalog`, or
//! `Lambda` at every point, and the current context is a stack
//! (`Pipeline` on the bottom by construction; `datalog { … }` pushes
//! `Datalog`; `{ |args| body }` pushes `Lambda`; the matching `}`
//! pops). This crate is the state machine that walks the source,
//! yields tokens, and stamps every token with the context that was
//! active at emission time. Consumers (R221.M5 unified AST parser,
//! R226 Datalog parser, R229 REPL syntax colorer) never re-derive the
//! context: they pattern-match on `token.context`.
//!
//! # Delimiter arithmetic
//!
//! * `datalog` — a plain [`TokenKind::DatalogKw`] token in whatever
//!   context is current (typically `Pipeline`; permitted inside
//!   `Lambda` per SH-D1 §2.3). Sets a one-shot "next `{` opens a
//!   Datalog block" latch that the following `LBrace` consumes.
//! * `{` — pushes `Datalog` if the latch is set, else `Lambda`. The
//!   `LBrace` token itself carries the *newly-pushed* context — so a
//!   `datalog { …` sequence renders as
//!   `(DatalogKw, Pipeline), (LBrace, Datalog), …`.
//! * `}` — is emitted with the *soon-to-be-popped* context still
//!   current, then the stack pops. So `datalog { p(?x) }` renders as
//!   `(DatalogKw, Pipeline), (LBrace, Datalog), (Ident("p"), Datalog),
//!    (LParen, Datalog), (QVar("x"), Datalog), (RParen, Datalog),
//!    (RBrace, Datalog)` — and any subsequent token is `Pipeline`
//!   again.
//! * An unmatched closing `}` at the outermost `Pipeline` scope is a
//!   diagnostic (`Err::UnmatchedRBrace`) but the lexer resumes past
//!   it so a REPL user can keep typing.
//!
//! # What this crate does NOT decide
//!
//! The lexer is deliberately *shape-blind*. It does not know that `.`
//! terminates a Datalog fact or that `|` in Lambda context separates
//! parameters from body — those are the grammar's job at R221.M5. The
//! same `TokenKind::Dot` may be a fact-terminator or a field-access;
//! the same `TokenKind::Pipe` may be a pipeline joiner or a lambda
//! parameter delimiter. Context alone tells the parser which.
//!
//! # ASCII fast path
//!
//! Well-formed ASCII input walks the state machine one byte at a time
//! without ever consulting the UAX#15 / UAX#29 tables — which is
//! important because the overwhelming majority of REPL input at
//! interactive latency is ASCII (`ls`, `cd`, `grep`) and R229 budgets
//! ≤5ms end-to-end for a keystroke's echo. The `Utf8Decoder` path
//! kicks in the moment we see a byte `>= 0x80`.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod context;
pub mod lexer;
pub mod token;

pub use context::{Context, ContextStack};
pub use lexer::{Lexer, LexError, LexErrorKind};
pub use token::{Span, Token, TokenKind};

/// Convenience: tokenize `src` end-to-end, returning all tokens (errors
/// and successes interleaved). Used by tests and the R229 REPL syntax
/// colorer; production parsers consume the `Lexer` iterator directly to
/// stop on the first hard error.
pub fn tokenize(src: &str) -> Vec<Result<Token, LexError>> {
    Lexer::new(src).collect()
}

/// Convenience: tokenize `src` and drop error results, panicking if any
/// occurred. Test-fixture-only helper — production callers want the
/// errors for diagnostics; this convenience just exists so a fixture
/// can spell an expected-happy-path sequence without unwrap noise on
/// every element. Kept unconditionally public (rather than under a
/// `cfg(test)`) because integration tests live in separate crates and
/// would not see a `cfg(test)`-gated helper.
pub fn tokenize_ok(src: &str) -> Vec<Token> {
    Lexer::new(src)
        .map(|r| r.expect("tokenize_ok called on input with lex errors"))
        .collect()
}
