//! Context-tracking state machine over `Utf8Decoder`.
//!
//! The lexer walks the source once, classifying each byte / code point
//! into a token, and maintaining a [`ContextStack`] that pushes on `{`
//! (with the `datalog` latch discriminating Datalog vs. Lambda) and
//! pops on `}`.
//!
//! # Why a hand-rolled state machine (not `logos`)
//!
//! `paideia-as-lexer` uses `logos` for the assembly-language surface,
//! but the shell lexer needs *context-carrying* tokens, and `logos`
//! only emits token kinds. Wrapping `logos` here would still leave us
//! writing the context stack by hand, plus an awkward post-pass to
//! re-stamp every token with its context — the state machine we would
//! write anyway. Hand-rolling keeps the state machine and the context
//! stack coincident in one loop.
//!
//! # Error recovery
//!
//! The lexer emits `Err(LexError)` for a malformed byte / unmatched
//! `}` / unterminated string, and then advances past the offending
//! byte. Downstream consumers (R229 REPL, R227 script loader) collect
//! errors while still receiving subsequent tokens, so a bad byte in the
//! middle of a script does not blind the parser to everything after it.

use crate::context::{Context, ContextStack};
use crate::token::{Span, Token, TokenKind};
use paideia_as_unicode::Utf8Decoder;

/// A lexical error carrying the offending byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexError {
    /// Byte offset at which the offending construct began.
    pub offset: usize,
    /// The specific failure mode.
    pub kind: LexErrorKind,
}

/// Discriminated shell-lexer failure modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexErrorKind {
    /// A byte / sequence that `Utf8Decoder` rejected. The specific
    /// UTF-8 error kind is not re-exported here; consumers that need
    /// it should decode `input.as_bytes()` themselves.
    InvalidUtf8,
    /// A `}` at the outermost `Pipeline` scope with no matching `{`.
    /// The lexer resumes past it so a REPL user's next character is
    /// still tokenized.
    UnmatchedRBrace,
    /// A string literal whose closing quote never arrived before EOF.
    UnterminatedString,
    /// A character that no token rule accepted (a stray `#` or `@`,
    /// say — R221.M4 does not yet handle attribute macros).
    UnexpectedChar,
    /// A `$` or `?` sigil not immediately followed by a well-formed
    /// identifier.
    EmptySigilName,
}

/// The context-tracking shell lexer.
///
/// Iterator semantics: each `next()` returns
/// `Some(Ok(Token))` for a successfully-classified token,
/// `Some(Err(LexError))` for a recoverable failure (the lexer has
/// already advanced past the offending bytes), or `None` at
/// end-of-input.
pub struct Lexer<'a> {
    /// The full source as a `&str`; we index into it for span slicing.
    src: &'a str,
    /// Absolute byte position of the next byte to consume.
    pos: usize,
    /// The context stack. Bottom is always `Pipeline`.
    stack: ContextStack,
    /// True immediately after emitting a `DatalogKw` token: gates the
    /// next `{` to push `Datalog` instead of `Lambda`. Cleared by
    /// anything other than trailing whitespace / newlines.
    pending_datalog: bool,
}

impl<'a> Lexer<'a> {
    /// Wrap a `&str` for tokenization. The `src` reference is borrowed
    /// for the life of the iterator; token payloads own their strings,
    /// so the returned tokens outlive `src`.
    pub fn new(src: &'a str) -> Self {
        // Validate UTF-8 up front is unnecessary: `&str` is already
        // valid by Rust's type invariant. The `Utf8Decoder` reference
        // in the design plan is used indirectly for consistency with
        // R221.M6's provenance layer — see `decode_at` below.
        Self {
            src,
            pos: 0,
            stack: ContextStack::new(),
            pending_datalog: false,
        }
    }

    /// Decode the next char at `self.pos`. Uses `Utf8Decoder` for
    /// symmetry with R221.M6 span-tracking, but `&str`'s bytes are
    /// already validated so the error branch is unreachable in practice
    /// — we surface it as `LexError::InvalidUtf8` defensively.
    fn decode_at(&self) -> Option<(char, usize)> {
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() {
            return None;
        }
        let mut dec = Utf8Decoder::new(&bytes[self.pos..]);
        match dec.next()? {
            Ok(c) => Some((c, dec.position())),
            Err(_) => None,
        }
    }

    /// Peek one char without consuming.
    fn peek(&self) -> Option<char> {
        self.decode_at().map(|(c, _)| c)
    }

    /// Peek the second char (one code point past the current one).
    /// Used for two-char lookahead (`=>`, `->`, `<=`, `>=`, `==`, `!=`,
    /// `\r\n`).
    fn peek2(&self) -> Option<char> {
        // BUG-FIX: `decode_at` returns `dec.position()`, which is relative
        // to the *sub-slice* it was constructed over (that slice starts
        // at `self.pos`). We must translate back to an absolute source
        // offset before indexing `self.src` for the second decoder.
        let (_, first_end_rel) = self.decode_at()?;
        let abs = self.pos + first_end_rel;
        let bytes = self.src.as_bytes();
        if abs >= bytes.len() {
            return None;
        }
        let mut dec = Utf8Decoder::new(&bytes[abs..]);
        match dec.next()? {
            Ok(c) => Some(c),
            Err(_) => None,
        }
    }

    /// Advance `self.pos` past one code point. Returns the char if a
    /// valid one was consumed.
    fn bump(&mut self) -> Option<char> {
        let (c, delta) = self.decode_at()?;
        self.pos += delta;
        Some(c)
    }

    /// Skip inline whitespace (spaces + tabs) but NOT newlines.
    /// Newlines are their own `TokenKind::Newline` variant so the
    /// R221.M5 parser can end pipeline expressions on them.
    fn skip_inline_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.bump();
            } else {
                break;
            }
        }
    }

    /// Consume an identifier `[A-Za-z_][A-Za-z0-9_]*`. Non-ASCII
    /// identifier characters (Unicode ID_Start / ID_Continue per
    /// UAX#31) are permitted; the shell surface intentionally admits
    /// them so identifiers spelled in the user's own script work
    /// (`ökotag`, `ファイル`).
    fn consume_ident(&mut self, start: usize) -> String {
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                self.bump();
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_owned()
    }

    /// Consume a numeric literal (integers and floats; suffix-free
    /// per R221.M4 — R225 will admit `1.MB` and friends). Deliberately
    /// permissive: `123`, `1.5`, `0.001`, `42_000` all lex; further
    /// validation is R221.M5's job.
    fn consume_number(&mut self, start: usize) -> String {
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        // Optional fractional part — but only if the next byte is a
        // digit, so `f.size` (field access on a numeric-looking name)
        // does not swallow the dot.
        if self.peek() == Some('.') {
            if let Some(next) = self.peek2() {
                if next.is_ascii_digit() {
                    self.bump(); // consume '.'
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() || c == '_' {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        self.src[start..self.pos].to_owned()
    }

    /// Consume a `"…"` string literal, returning the inner text with
    /// escape sequences left unresolved (the parser evaluates them).
    /// Assumes the opening quote at `self.pos` has NOT yet been
    /// consumed.
    fn consume_string(
        &mut self,
        start: usize,
    ) -> Result<(String, usize), LexError> {
        self.bump(); // opening "
        let inner_start = self.pos;
        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    let inner_end = self.pos;
                    self.bump(); // closing "
                    return Ok((
                        self.src[inner_start..inner_end].to_owned(),
                        self.pos,
                    ));
                }
                '\\' => {
                    self.bump();
                    // Swallow the next char blindly; the parser
                    // validates the escape.
                    if self.peek().is_some() {
                        self.bump();
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
        Err(LexError {
            offset: start,
            kind: LexErrorKind::UnterminatedString,
        })
    }
}

/// UAX#31 ID_Start (approximated): ASCII letter or `_`, or any
/// non-ASCII alphabetic char. Precise UAX#31 tables are `unicode-ident`
/// crate territory; we defer to `char::is_alphabetic` here to keep the
/// dep list small and rely on R220's `unicode-ident` for the strict
/// check at R221.M5.
#[inline]
fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic() || (!c.is_ascii() && c.is_alphabetic())
}

/// UAX#31 ID_Continue (approximated): `is_ident_start` plus digits.
#[inline]
fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || (!c.is_ascii() && c.is_alphanumeric())
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.skip_inline_whitespace();
            if self.pos >= self.src.len() {
                return None;
            }
            let start = self.pos;
            let Some(c) = self.peek() else {
                // `peek` returned `None` mid-stream ⇒ malformed UTF-8.
                // `&str` guarantees this never fires, but we handle it
                // defensively so a caller that constructs a `Lexer`
                // over an unsafe-transmuted `&str` gets a diagnostic
                // instead of an infinite loop.
                self.pos += 1;
                return Some(Err(LexError {
                    offset: start,
                    kind: LexErrorKind::InvalidUtf8,
                }));
            };

            // ---- Newline -----------------------------------------
            if c == '\n' {
                self.bump();
                let tok = Token::new(
                    TokenKind::Newline,
                    Span::new(start, self.pos),
                    self.stack.current(),
                );
                // Newline does NOT clear pending_datalog: `datalog\n{`
                // is a legal (if unidiomatic) way to open a block.
                return Some(Ok(tok));
            }
            if c == '\r' {
                // Treat CR or CR-LF as a single Newline token.
                self.bump();
                if self.peek() == Some('\n') {
                    self.bump();
                }
                let tok = Token::new(
                    TokenKind::Newline,
                    Span::new(start, self.pos),
                    self.stack.current(),
                );
                return Some(Ok(tok));
            }

            // ---- Comment (# to end-of-line) ---------------------
            if c == '#' {
                while let Some(cc) = self.peek() {
                    if cc == '\n' {
                        break;
                    }
                    self.bump();
                }
                // Comments do not emit tokens; loop for the next.
                continue;
            }

            // ---- Sigil-prefixed identifiers ---------------------
            if c == '?' || c == '$' {
                let sigil = c;
                self.bump();
                let name_start = self.pos;
                if let Some(nc) = self.peek() {
                    if is_ident_start(nc) {
                        while let Some(cc) = self.peek() {
                            if is_ident_continue(cc) {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                        let name = self.src[name_start..self.pos].to_owned();
                        let kind = if sigil == '?' {
                            TokenKind::QVar(name)
                        } else {
                            TokenKind::InterpVar(name)
                        };
                        self.pending_datalog = false;
                        return Some(Ok(Token::new(
                            kind,
                            Span::new(start, self.pos),
                            self.stack.current(),
                        )));
                    }
                }
                self.pending_datalog = false;
                return Some(Err(LexError {
                    offset: start,
                    kind: LexErrorKind::EmptySigilName,
                }));
            }

            // ---- Identifier / keyword ---------------------------
            if is_ident_start(c) {
                self.bump();
                let text = self.consume_ident(start);
                let (kind, sets_pending) = classify_ident(&text);
                self.pending_datalog = sets_pending;
                return Some(Ok(Token::new(
                    kind,
                    Span::new(start, self.pos),
                    self.stack.current(),
                )));
            }

            // ---- Numeric literal --------------------------------
            if c.is_ascii_digit() {
                self.bump();
                let text = self.consume_number(start);
                self.pending_datalog = false;
                return Some(Ok(Token::new(
                    TokenKind::Number(text),
                    Span::new(start, self.pos),
                    self.stack.current(),
                )));
            }

            // ---- String literal ---------------------------------
            if c == '"' {
                match self.consume_string(start) {
                    Ok((s, end)) => {
                        self.pending_datalog = false;
                        return Some(Ok(Token::new(
                            TokenKind::Str(s),
                            Span::new(start, end),
                            self.stack.current(),
                        )));
                    }
                    Err(e) => {
                        self.pending_datalog = false;
                        return Some(Err(e));
                    }
                }
            }

            // ---- Punctuation + operators -----------------------
            // Two-char operators first (longest-match).
            if let Some(next) = self.peek2() {
                let two = [c, next];
                let two_str: String = two.iter().collect();
                let matched = matches!(
                    two_str.as_str(),
                    "=>" | "->" | "<=" | ">=" | "==" | "!="
                );
                if matched {
                    self.bump();
                    self.bump();
                    let kind = match two_str.as_str() {
                        "=>" => TokenKind::FatArrow,
                        "->" => TokenKind::ThinArrow,
                        _ => TokenKind::Op(two_str),
                    };
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        kind,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
            }

            // Single-char punctuation.
            match c {
                '|' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::Pipe,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                '{' => {
                    // Push the appropriate context BEFORE emitting the
                    // token so the token carries the new context.
                    let new_ctx = if self.pending_datalog {
                        Context::Datalog
                    } else {
                        Context::Lambda
                    };
                    self.pending_datalog = false;
                    self.stack.push(new_ctx);
                    self.bump();
                    return Some(Ok(Token::new(
                        TokenKind::LBrace,
                        Span::new(start, self.pos),
                        new_ctx,
                    )));
                }
                '}' => {
                    // Emit with the *current* (about-to-be-popped)
                    // context, then pop. An unmatched `}` at the base
                    // Pipeline still emits an RBrace token with
                    // context=Pipeline plus an error result — but since
                    // one `next()` call yields one item, we emit the
                    // token now and defer the error to a subsequent
                    // call. Simpler: on unmatched, emit the error only
                    // (no token) so downstream consumers do not see a
                    // spurious RBrace.
                    let current = self.stack.current();
                    self.bump();
                    match self.stack.pop() {
                        Some(_) => {
                            self.pending_datalog = false;
                            return Some(Ok(Token::new(
                                TokenKind::RBrace,
                                Span::new(start, self.pos),
                                current,
                            )));
                        }
                        None => {
                            self.pending_datalog = false;
                            return Some(Err(LexError {
                                offset: start,
                                kind: LexErrorKind::UnmatchedRBrace,
                            }));
                        }
                    }
                }
                '(' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::LParen,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                ')' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::RParen,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                '[' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::LBracket,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                ']' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::RBracket,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                ',' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::Comma,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                '.' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::Dot,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                ';' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::Semi,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                '\\' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::Backslash,
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                '+' | '-' | '*' | '/' | '<' | '>' => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Ok(Token::new(
                        TokenKind::Op(c.to_string()),
                        Span::new(start, self.pos),
                        self.stack.current(),
                    )));
                }
                _ => {
                    self.bump();
                    self.pending_datalog = false;
                    return Some(Err(LexError {
                        offset: start,
                        kind: LexErrorKind::UnexpectedChar,
                    }));
                }
            }
        }
    }
}

/// Classify a lexed identifier text. Returns `(kind, sets_pending_datalog)`.
/// The one identifier that gates the following `{`'s context push is
/// `datalog`; keyword-operators `and`/`or`/`not` become `Op` tokens.
fn classify_ident(text: &str) -> (TokenKind, bool) {
    match text {
        "datalog" => (TokenKind::DatalogKw, true),
        "and" | "or" | "not" => (TokenKind::Op(text.to_owned()), false),
        _ => (TokenKind::Ident(text.to_owned()), false),
    }
}
