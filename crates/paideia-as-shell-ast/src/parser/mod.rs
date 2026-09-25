//! Recursive-descent parser producing [`crate::SyntaxNode`] from the
//! R221.M4 context-tagged token stream.
//!
//! # Dispatch model
//!
//! Every token carries a [`paideia_as_shell_lex::Context`] from the
//! lexer. The parser dispatches on it at expression boundaries:
//!
//! * `Context::Pipeline` → [`pipeline`] sub-parser (commands, `|`,
//!   `;`, `&`, redirects).
//! * `Context::Datalog`  → [`datalog`] sub-parser (atoms, rules,
//!   `?var`, `$var`, `not`).
//! * `Context::Lambda`   → [`lambda`] sub-parser (variables,
//!   application, `let`, `match`, arithmetic).
//!
//! # Error recovery
//!
//! The parser stops at the first error and returns a `ParseError`
//! with the offending span (in both pre-NFC and post-NFC coords) —
//! per Q-A5, LSP consumers want the *original* span for their
//! squigglies, not the NFC one. Recovery to a synchronization point is
//! deferred to R229 (REPL incremental parse).
//!
//! # Never-panic invariant (R221.M7 fuzz target)
//!
//! Every parser method returns `Result<T, ParseError>`; no `.unwrap()`,
//! no `panic!()`, no `unreachable!()` on parser-observable input. The
//! only `unreachable!` in this crate guards impossible enum branches
//! the type system already proved, and would fire only on a bug in
//! the parser itself — not on any input the fuzz corpus can produce.

pub mod datalog;
pub mod lambda;
pub mod pipeline;

use paideia_as_shell_lex::{Context, Lexer, Token, TokenKind};

use crate::ast::SyntaxNode;
use crate::nfc_map::NfcMap;
use crate::span::NodeSpan;

/// Discriminated parser failure modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Lexer surfaced a `LexError` before the parser could consume the
    /// next token. The offending byte offset is inside the containing
    /// `ParseError`.
    LexerError(String),
    /// The parser expected one of a set of `TokenKind`s but saw
    /// something else — or ran off the end of the token stream.
    UnexpectedToken {
        /// Human-readable description of what was expected.
        expected: String,
        /// The token actually observed, or a marker for EOF.
        got: String,
    },
    /// The parser saw a token in a context that its sub-parser cannot
    /// handle (e.g. `?` outside a Datalog block, `&` inside a lambda).
    ContextMismatch {
        /// The sub-language that owns this token kind.
        expected_context: Context,
        /// The context the token was actually stamped with.
        actual_context: Context,
    },
    /// A numeric literal exceeded `i64::MAX`.
    NumericOverflow(String),
    /// A `not not atom` or otherwise doubly-negated Datalog goal.
    DoubleNegation,
    /// Nesting depth exceeded the parser's soft cap (default 256).
    /// The cap keeps the R221.M7 adversarial-nesting fuzz case from
    /// blowing the stack.
    NestingTooDeep {
        /// The reached depth at rejection.
        depth: usize,
    },
    /// End-of-input encountered mid-expression.
    UnexpectedEof {
        /// Human-readable description of what would have completed
        /// the pending construct.
        expected: String,
    },
}

/// A parser failure with source-span provenance for LSP diagnostics.
///
/// Both `original_span` and `nfc_span` are carried per Q-A5 / R221.M6:
/// LSP squigglies underline `original_span`; the R226/R229 renderer
/// uses `nfc_span` to align its cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The specific failure mode.
    pub kind: ParseErrorKind,
    /// Byte range in the pre-NFC (user-typed) source.
    pub original_span: (usize, usize),
    /// Byte range in the post-NFC source the parser walked.
    pub nfc_span: (usize, usize),
}

/// Maximum recursive-descent depth. Cap chosen so a well-formed script
/// (nesting `{ |x| { |y| ... } }` up to ~50 deep, which no real script
/// approaches) parses without hitting the cap while a fuzz input of
/// `{{{{{{{{...}}}}}}}}` bails cleanly instead of blowing the stack.
pub const MAX_NESTING_DEPTH: usize = 256;

/// Top-level parse entry: NFC-normalize `src`, tokenize the result,
/// and drive the recursive-descent parser to a `SyntaxNode` root
/// wrapped in [`SyntaxNode::Seq`] (empty seq for empty input).
pub fn parse(src: &str) -> Result<SyntaxNode, ParseError> {
    let (nfc, map) = NfcMap::build(src);
    parse_with_map(&nfc, &map)
}

/// Parse a pre-normalized source string with an explicit NFC map. Used
/// by the test corpus to construct malformed-NFC inputs whose maps
/// would not normally exist.
pub fn parse_with_map(nfc_src: &str, map: &NfcMap) -> Result<SyntaxNode, ParseError> {
    // Sanity: the caller must have handed us NFC-normalized text (the
    // parser's grammar is defined over NFC). We check in debug to
    // catch caller bugs but not in release — the check is O(n) on the
    // input length.
    debug_assert!(
        nfc_src == paideia_as_unicode::nfc_normalize(nfc_src),
        "parse_with_map called with non-NFC source; call parse() to normalize."
    );

    let tokens = collect_tokens(nfc_src, map)?;
    let mut p = Parser::new(&tokens, map);
    let node = p.parse_program()?;
    Ok(node)
}

/// Collect all tokens up front. Streaming would be marginally more
/// memory-efficient but the parser needs 2-token lookahead in several
/// places (Datalog `pred(` vs. `pred .` for rule heads without args)
/// and a `Vec<Token>` keeps that trivial.
fn collect_tokens(nfc_src: &str, map: &NfcMap) -> Result<Vec<Token>, ParseError> {
    let mut out = Vec::new();
    for r in Lexer::new(nfc_src) {
        match r {
            Ok(t) => out.push(t),
            Err(e) => {
                // Translate the lexer's byte offset to pre-NFC coords.
                let orig = map.to_original((e.offset, e.offset + 1));
                return Err(ParseError {
                    kind: ParseErrorKind::LexerError(format!("{:?}", e.kind)),
                    original_span: orig,
                    nfc_span: (e.offset, e.offset + 1),
                });
            }
        }
    }
    Ok(out)
}

/// Shared parser state driven by the three sub-parsers via `pub(super)`
/// helper methods declared below.
pub struct Parser<'a> {
    /// Full token vector; indexed by `pos`.
    tokens: &'a [Token],
    /// Cursor into `tokens`. `pos == tokens.len()` means EOF.
    pos: usize,
    /// The NFC map for pre-NFC span reconstruction.
    map: &'a NfcMap,
    /// Current recursive-descent depth. Incremented at each `Group` /
    /// `Lambda` / `DatalogBlock` / `Match` push; decremented on return.
    /// Prevents adversarial `{{{…}}}` inputs from blowing the stack
    /// (R221.M7 fuzz target).
    depth: usize,
}

impl<'a> Parser<'a> {
    /// Wrap a token slice for parsing.
    pub fn new(tokens: &'a [Token], map: &'a NfcMap) -> Self {
        Self { tokens, pos: 0, map, depth: 0 }
    }

    /// Peek the current token without consuming.
    pub(crate) fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    /// Peek the token at offset `n` ahead. `n == 0` is `peek()`.
    pub(crate) fn peek_at(&self, n: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + n)
    }

    /// Consume and return the current token. Returns `None` at EOF.
    pub(crate) fn bump(&mut self) -> Option<&'a Token> {
        let t = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(t)
    }

    /// Consume newlines and semicolons in Pipeline context — the
    /// pipeline parser uses these as statement separators but they
    /// are transparent inside a lambda body or datalog rule.
    pub(crate) fn skip_pipeline_separators(&mut self) {
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::Newline | TokenKind::Semi => {
                    self.bump();
                }
                _ => break,
            }
        }
    }

    /// Turn a lexer span into a NodeSpan for the given context. Applies
    /// the R221.M6 NfcMap to reconstruct the pre-NFC range.
    pub(crate) fn span_of(&self, lex_span: paideia_as_shell_lex::Span, ctx: Context) -> NodeSpan {
        let nfc = (lex_span.start, lex_span.end);
        let original = self.map.to_original(nfc);
        NodeSpan::new(original, nfc, ctx)
    }

    /// Build a NodeSpan for a token being consumed.
    pub(crate) fn token_span(&self, t: &Token) -> NodeSpan {
        self.span_of(t.span, t.context)
    }

    /// Assert we haven't recursed past the soft depth cap. Sub-parsers
    /// call this at the top of each `{`/`(`/`match` entry and pair it
    /// with a manual decrement on return.
    pub(crate) fn enter_nesting(&mut self, at: Option<&Token>) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            let (orig, nfc) = match at {
                Some(t) => (
                    self.map.to_original((t.span.start, t.span.end)),
                    (t.span.start, t.span.end),
                ),
                None => ((0, 0), (0, 0)),
            };
            return Err(ParseError {
                kind: ParseErrorKind::NestingTooDeep { depth: self.depth },
                original_span: orig,
                nfc_span: nfc,
            });
        }
        Ok(())
    }

    /// Pair with [`Self::enter_nesting`] on return from a nested
    /// construct.
    pub(crate) fn exit_nesting(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Error constructor for the common "expected X, got Y" shape.
    pub(crate) fn err_expected<S: Into<String>>(
        &self,
        expected: S,
    ) -> ParseError {
        let expected = expected.into();
        match self.peek() {
            Some(t) => ParseError {
                kind: ParseErrorKind::UnexpectedToken {
                    expected,
                    got: format!("{:?}", t.kind),
                },
                original_span: self.map.to_original((t.span.start, t.span.end)),
                nfc_span: (t.span.start, t.span.end),
            },
            None => {
                let end = self.map.nfc_len();
                ParseError {
                    kind: ParseErrorKind::UnexpectedEof { expected },
                    original_span: self.map.to_original((end, end)),
                    nfc_span: (end, end),
                }
            }
        }
    }

    /// Top-level program: a sequence of pipeline expressions separated
    /// by newlines / `;`. The result is either a single expression
    /// unwrapped, or a `Seq` covering all of them. An empty program
    /// yields `Seq { items: [] }` with a zero-span at position 0.
    pub fn parse_program(&mut self) -> Result<SyntaxNode, ParseError> {
        self.skip_pipeline_separators();
        let mut items = Vec::new();
        while self.peek().is_some() {
            let expr = pipeline::parse_pipeline_expr(self)?;
            items.push(expr);
            self.skip_pipeline_separators();
        }
        match items.len() {
            0 => Ok(SyntaxNode::Seq {
                items: Vec::new(),
                span: NodeSpan::new((0, 0), (0, 0), Context::Pipeline),
            }),
            1 => Ok(items.into_iter().next().unwrap()),
            _ => {
                let first = items.first().unwrap().span();
                let last = items.last().unwrap().span();
                Ok(SyntaxNode::Seq {
                    items,
                    span: first.union(last),
                })
            }
        }
    }
}
