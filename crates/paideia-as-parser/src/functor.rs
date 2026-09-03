//! Session-typed functor signature parser (v0.25-M1-001, #1355).
//!
//! **Surface syntax:**
//!
//! ```text
//! functor F(In : SigIn) -> SigOut with S : session { ... }
//! ```
//!
//! This module is the first pass of the v0.25 session-functors round. It
//! elevates the *session type* `S` to a first-class binder on the functor
//! signature, alongside the parameter signature `SigIn` and the result
//! signature `SigOut`. M1-002/003/004 will then subtype-check, elaborate,
//! and wire it into the R29 driver framework; those follow-ups build on
//! [`FunctorDecl`] by shape, so the field layout here is deliberately
//! stable.
//!
//! # Positioning within the parser
//!
//! [`parse_functor`] is a **standalone entry point** — it constructs its
//! own [`TokenCursor`] over the supplied token slice and does not route
//! through `parse_primary` / `parse_item`. That mirrors the pattern
//! established by `modules::parse_functor_app` / `parse_pack_expr`, which
//! also expose per-form parsers ahead of full grammar integration to
//! avoid regressing the item parser while the surface is still moving.
//! Wiring into `parse_item` is the follow-up patch's responsibility.
//!
//! # Body
//!
//! The `{ ... }` body is captured as an opaque span — this milestone is
//! about the *signature*, and the body will be re-parsed with the full
//! item parser once M1-004 lands. The body span records where the body
//! starts and ends so later phases can splice their own parser in
//! without re-tokenising.
//!
//! # Diagnostics
//!
//! Errors are emitted in the parser's `P02xx` block:
//!
//! | Code   | Cause                                                       |
//! |--------|-------------------------------------------------------------|
//! | P0230  | Missing / malformed `functor` header (name, `(`, `:`, `)`). |
//! | P0231  | Missing `->` or return signature.                           |
//! | P0232  | Malformed `with S : session` clause.                        |
//! | P0233  | Unbalanced `{...}` body.                                    |
//!
//! P0100 (generic "unexpected token") is *not* re-used — the header is
//! narrow enough that a targeted message reads better than the generic.

use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, FileId, Severity, Span,
};
use paideia_as_lexer::{Token, TokenKind};

use crate::cursor::TokenCursor;
use crate::parser::ParseError;

/// A functor declaration with a first-class session-type binder.
///
/// See the module-level docs for the grammar. The struct is designed so
/// that M1-002+ can populate elaborated types alongside these fields
/// without churning M1-001 call sites — added fields go at the end and
/// default to `None`/empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctorDecl {
    /// Functor name (`F` in `functor F(...)`).
    pub name: String,
    /// Formal parameter name (`In` in `(In : SigIn)`).
    pub param_name: String,
    /// Parameter signature identifier (`SigIn`). Stored as a plain path
    /// component — signature resolution happens in the elaborator.
    pub param_sig: String,
    /// Result signature identifier (`SigOut`).
    pub return_sig: String,
    /// Optional `with S : session` binder introducing a first-class
    /// session variable on the signature. Absent iff the `with` clause
    /// was omitted, in which case the functor has no protocol constraint.
    pub session_binding: Option<SessionBinding>,
    /// Span covering the body `{ ... }`. The bytes inside are opaque to
    /// M1-001; M1-004 re-parses them with the full item parser.
    pub body_span: Span,
    /// Span covering the entire declaration, from `functor` to `}`.
    pub span: Span,
}

/// The `with S : session` clause on a [`FunctorDecl`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBinding {
    /// Name of the session variable (`S`).
    pub var: String,
    /// Span of the whole `with S : session` clause.
    pub span: Span,
}

/// Parse a session-typed functor declaration from `tokens`.
///
/// The token slice is expected to start at the `functor` keyword and to
/// contain at least the trailing `}` of the body. Any trailing tokens
/// past the body's closing brace are left in the slice — callers can
/// feed those to further parsers if needed. In this M1-001 pass we do
/// not expose the cursor position because the item parser is not wired
/// yet; M1-004 will refactor to a `parse_functor(&mut Parser)` method
/// that participates in top-level dispatch.
///
/// Diagnostics are emitted through `sink` per the P02xx table in the
/// module-level docs.
pub fn parse_functor(
    tokens: &[Token],
    source: &str,
    file: FileId,
    sink: &mut dyn DiagnosticSink,
) -> Result<FunctorDecl, ParseError> {
    let mut cursor = TokenCursor::new(tokens, file);

    let span_start = cursor.current_span();

    // `functor`
    if !cursor.at(TokenKind::KwFunctor) {
        emit(sink, 230, "expected `functor` keyword", cursor.current_span());
        return Err(ParseError);
    }
    cursor.bump();

    // Functor name (identifier)
    let name = expect_ident(&mut cursor, source, sink, 230, "functor name")?;

    // `(`
    if !cursor.at(TokenKind::LParen) {
        emit(
            sink,
            230,
            "malformed functor header: expected `(` after functor name",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    cursor.bump();

    // Parameter name : parameter signature
    let param_name = expect_ident(&mut cursor, source, sink, 230, "parameter name")?;

    if !cursor.at(TokenKind::Colon) {
        emit(
            sink,
            230,
            "malformed functor parameter: expected `:` after parameter name",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    cursor.bump();

    let param_sig = expect_ident(&mut cursor, source, sink, 230, "parameter signature")?;

    // `)`
    if !cursor.at(TokenKind::RParen) {
        emit(
            sink,
            230,
            "malformed functor header: expected `)` closing the parameter list",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    cursor.bump();

    // `->` return_sig
    if !cursor.at(TokenKind::Arrow) {
        emit(
            sink,
            231,
            "expected `->` before the result signature",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    cursor.bump();

    let return_sig = expect_ident(&mut cursor, source, sink, 231, "result signature")?;

    // Optional `with S : session`
    let session_binding = if cursor.at(TokenKind::KwWith) {
        Some(parse_with_session_clause(&mut cursor, source, sink)?)
    } else {
        None
    };

    // Body `{ ... }` — record span; contents opaque in M1-001.
    if !cursor.at(TokenKind::LBrace) {
        emit(
            sink,
            233,
            "expected `{` to open the functor body",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    let body_open_span = cursor.current_span();
    cursor.bump();

    let body_close_span = match skip_balanced_body(&mut cursor) {
        Some(span) => span,
        None => {
            emit(
                sink,
                233,
                "unbalanced functor body: reached end of input before `}`",
                cursor.previous_span(),
            );
            return Err(ParseError);
        }
    };

    let body_span = merge_spans(body_open_span, body_close_span);
    let full_span = merge_spans(span_start, body_close_span);

    Ok(FunctorDecl {
        name,
        param_name,
        param_sig,
        return_sig,
        session_binding,
        body_span,
        span: full_span,
    })
}

/// Parse a `with S : session` clause. The `with` token is consumed here.
fn parse_with_session_clause(
    cursor: &mut TokenCursor<'_>,
    source: &str,
    sink: &mut dyn DiagnosticSink,
) -> Result<SessionBinding, ParseError> {
    let start = cursor.current_span();
    // `with` — caller guaranteed via `at(KwWith)`.
    cursor.bump();

    let var = expect_ident(cursor, source, sink, 232, "session variable name")?;

    if !cursor.at(TokenKind::Colon) {
        emit(
            sink,
            232,
            "malformed `with` clause: expected `:` after session variable",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    cursor.bump();

    // Contextual keyword `session`.
    if !is_contextual_keyword(cursor, source, "session") {
        emit(
            sink,
            232,
            "malformed `with` clause: expected contextual keyword `session`",
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    let end = cursor.current_span();
    cursor.bump();

    Ok(SessionBinding {
        var,
        span: merge_spans(start, end),
    })
}

/// Consume tokens up to and including the matching `}` for a `{` that
/// has already been consumed. Returns the span of the matching `}` on
/// success, or `None` if EOF is reached first.
fn skip_balanced_body(cursor: &mut TokenCursor<'_>) -> Option<Span> {
    let mut depth: u32 = 1;
    loop {
        if cursor.is_at_end() {
            return None;
        }
        let kind = cursor.current_kind();
        let span = cursor.current_span();
        match kind {
            TokenKind::LBrace => {
                depth += 1;
                cursor.bump();
            }
            TokenKind::RBrace => {
                depth -= 1;
                cursor.bump();
                if depth == 0 {
                    return Some(span);
                }
            }
            _ => {
                cursor.bump();
            }
        }
    }
}

/// Expect the current token to be an `Ident` and return its lexeme.
fn expect_ident(
    cursor: &mut TokenCursor<'_>,
    source: &str,
    sink: &mut dyn DiagnosticSink,
    code: u16,
    slot: &str,
) -> Result<String, ParseError> {
    if !cursor.at(TokenKind::Ident) {
        emit(
            sink,
            code,
            &format!("expected {} (identifier)", slot),
            cursor.current_span(),
        );
        return Err(ParseError);
    }
    let tok = cursor
        .bump()
        .expect("at(Ident) implies bump returns Some");
    let start = tok.span.byte_start() as usize;
    let end = start + tok.span.byte_len() as usize;
    Ok(extract_lexeme(source, start, end))
}

/// True iff the current token is `TokenKind::Ident` whose source text
/// equals `text`. Does not advance.
fn is_contextual_keyword(cursor: &TokenCursor<'_>, source: &str, text: &str) -> bool {
    if !cursor.at(TokenKind::Ident) {
        return false;
    }
    let Some(tok) = cursor.peek() else {
        return false;
    };
    let start = tok.span.byte_start() as usize;
    let end = start + tok.span.byte_len() as usize;
    if start <= source.len() && end <= source.len() {
        &source[start..end] == text
    } else {
        false
    }
}

fn extract_lexeme(source: &str, start: usize, end: usize) -> String {
    if start <= source.len() && end <= source.len() && start <= end {
        source[start..end].to_string()
    } else {
        format!("__{start}_{end}__")
    }
}

fn merge_spans(a: Span, b: Span) -> Span {
    let start = a.byte_start().min(b.byte_start());
    let end = (a.byte_start() + a.byte_len()).max(b.byte_start() + b.byte_len());
    Span::new(a.file(), start, end - start)
}

fn emit(sink: &mut dyn DiagnosticSink, code: u16, msg: &str, span: Span) {
    let d = Diagnostic::error(p_code(code))
        .message(msg.to_string())
        .with_span(span)
        .finish();
    let _ = sink.emit(d);
}

fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span, VecSink};

    fn file() -> FileId {
        FileId::new(1).unwrap()
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, Span::new(file(), byte_start, byte_len))
    }

    // Byte layout for "functor F(In : SigIn) -> SigOut with S : session { }":
    //   0: functor(7)  8: F(1)  9: ((1)  10: In(2)  13: :(1)  15: SigIn(5)
    //   20: )(1)  22: ->(2)  25: SigOut(6)  32: with(4)  37: S(1)  39: :(1)
    //   41: session(7)  49: {(1)  51: }(1)
    fn tokens_full() -> (String, Vec<Token>) {
        let src = "functor F(In : SigIn) -> SigOut with S : session { }".to_string();
        let toks = vec![
            tok(TokenKind::KwFunctor, 0, 7),
            tok(TokenKind::Ident, 8, 1),     // F
            tok(TokenKind::LParen, 9, 1),    // (
            tok(TokenKind::Ident, 10, 2),    // In
            tok(TokenKind::Colon, 13, 1),    // :
            tok(TokenKind::Ident, 15, 5),    // SigIn
            tok(TokenKind::RParen, 20, 1),   // )
            tok(TokenKind::Arrow, 22, 2),    // ->
            tok(TokenKind::Ident, 25, 6),    // SigOut
            tok(TokenKind::KwWith, 32, 4),   // with
            tok(TokenKind::Ident, 37, 1),    // S
            tok(TokenKind::Colon, 39, 1),    // :
            tok(TokenKind::Ident, 41, 7),    // session
            tok(TokenKind::LBrace, 49, 1),   // {
            tok(TokenKind::RBrace, 51, 1),   // }
            tok(TokenKind::Eof, 52, 0),
        ];
        (src, toks)
    }

    #[test]
    fn parses_full_functor_with_session_binding() {
        let (src, toks) = tokens_full();
        let mut sink = VecSink::new();
        let result = parse_functor(&toks, &src, file(), &mut sink);
        assert!(result.is_ok(), "diagnostics: {:?}", sink.diagnostics());
        let d = result.unwrap();
        assert_eq!(d.name, "F");
        assert_eq!(d.param_name, "In");
        assert_eq!(d.param_sig, "SigIn");
        assert_eq!(d.return_sig, "SigOut");
        let sb = d.session_binding.expect("session binding present");
        assert_eq!(sb.var, "S");
    }

    #[test]
    fn parses_functor_without_with_clause() {
        // "functor F(In : SigIn) -> SigOut { }"
        let src = "functor F(In : SigIn) -> SigOut { }".to_string();
        let toks = vec![
            tok(TokenKind::KwFunctor, 0, 7),
            tok(TokenKind::Ident, 8, 1),    // F
            tok(TokenKind::LParen, 9, 1),
            tok(TokenKind::Ident, 10, 2),   // In
            tok(TokenKind::Colon, 13, 1),
            tok(TokenKind::Ident, 15, 5),   // SigIn
            tok(TokenKind::RParen, 20, 1),
            tok(TokenKind::Arrow, 22, 2),
            tok(TokenKind::Ident, 25, 6),   // SigOut
            tok(TokenKind::LBrace, 32, 1),
            tok(TokenKind::RBrace, 34, 1),
            tok(TokenKind::Eof, 35, 0),
        ];
        let mut sink = VecSink::new();
        let result = parse_functor(&toks, &src, file(), &mut sink);
        assert!(result.is_ok(), "diagnostics: {:?}", sink.diagnostics());
        let d = result.unwrap();
        assert!(d.session_binding.is_none());
    }

    #[test]
    fn body_span_covers_braces_and_survives_nesting() {
        // "functor F(In : SigIn) -> SigOut { { } }"
        let src = "functor F(In : SigIn) -> SigOut { { } }".to_string();
        let toks = vec![
            tok(TokenKind::KwFunctor, 0, 7),
            tok(TokenKind::Ident, 8, 1),
            tok(TokenKind::LParen, 9, 1),
            tok(TokenKind::Ident, 10, 2),
            tok(TokenKind::Colon, 13, 1),
            tok(TokenKind::Ident, 15, 5),
            tok(TokenKind::RParen, 20, 1),
            tok(TokenKind::Arrow, 22, 2),
            tok(TokenKind::Ident, 25, 6),
            tok(TokenKind::LBrace, 32, 1),   // outer {
            tok(TokenKind::LBrace, 34, 1),   // inner {
            tok(TokenKind::RBrace, 36, 1),   // inner }
            tok(TokenKind::RBrace, 38, 1),   // outer }
            tok(TokenKind::Eof, 39, 0),
        ];
        let mut sink = VecSink::new();
        let d = parse_functor(&toks, &src, file(), &mut sink).unwrap();
        assert_eq!(d.body_span.byte_start(), 32);
        // Body span ends at the outer `}` (byte 38, length 1).
        assert_eq!(
            d.body_span.byte_start() + d.body_span.byte_len(),
            39
        );
    }

    #[test]
    fn rejects_missing_functor_keyword_p0230() {
        // Just an identifier where `functor` is expected.
        let src = "Foo".to_string();
        let toks = vec![tok(TokenKind::Ident, 0, 3), tok(TokenKind::Eof, 3, 0)];
        let mut sink = VecSink::new();
        let result = parse_functor(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        let diags = sink.diagnostics();
        assert!(!diags.is_empty());
        assert!(format!("{}", diags[0].code()).contains("P0230"));
    }

    #[test]
    fn rejects_missing_arrow_p0231() {
        // "functor F(In : SigIn) SigOut { }" — no `->`.
        let src = "functor F(In : SigIn) SigOut { }".to_string();
        let toks = vec![
            tok(TokenKind::KwFunctor, 0, 7),
            tok(TokenKind::Ident, 8, 1),
            tok(TokenKind::LParen, 9, 1),
            tok(TokenKind::Ident, 10, 2),
            tok(TokenKind::Colon, 13, 1),
            tok(TokenKind::Ident, 15, 5),
            tok(TokenKind::RParen, 20, 1),
            tok(TokenKind::Ident, 22, 6),
            tok(TokenKind::LBrace, 29, 1),
            tok(TokenKind::RBrace, 31, 1),
            tok(TokenKind::Eof, 32, 0),
        ];
        let mut sink = VecSink::new();
        assert!(parse_functor(&toks, &src, file(), &mut sink).is_err());
        assert!(
            sink.diagnostics()
                .iter()
                .any(|d| format!("{}", d.code()).contains("P0231"))
        );
    }

    #[test]
    fn rejects_bad_session_keyword_p0232() {
        // "functor F(In : SigIn) -> SigOut with S : notsession { }"
        let src = "functor F(In : SigIn) -> SigOut with S : notsession { }".to_string();
        let toks = vec![
            tok(TokenKind::KwFunctor, 0, 7),
            tok(TokenKind::Ident, 8, 1),
            tok(TokenKind::LParen, 9, 1),
            tok(TokenKind::Ident, 10, 2),
            tok(TokenKind::Colon, 13, 1),
            tok(TokenKind::Ident, 15, 5),
            tok(TokenKind::RParen, 20, 1),
            tok(TokenKind::Arrow, 22, 2),
            tok(TokenKind::Ident, 25, 6),
            tok(TokenKind::KwWith, 32, 4),
            tok(TokenKind::Ident, 37, 1),
            tok(TokenKind::Colon, 39, 1),
            tok(TokenKind::Ident, 41, 10),   // "notsession"
            tok(TokenKind::LBrace, 52, 1),
            tok(TokenKind::RBrace, 54, 1),
            tok(TokenKind::Eof, 55, 0),
        ];
        let mut sink = VecSink::new();
        assert!(parse_functor(&toks, &src, file(), &mut sink).is_err());
        assert!(
            sink.diagnostics()
                .iter()
                .any(|d| format!("{}", d.code()).contains("P0232"))
        );
    }

    #[test]
    fn rejects_unbalanced_body_p0233() {
        // "functor F(In : SigIn) -> SigOut {" — no closing `}`.
        let src = "functor F(In : SigIn) -> SigOut {".to_string();
        let toks = vec![
            tok(TokenKind::KwFunctor, 0, 7),
            tok(TokenKind::Ident, 8, 1),
            tok(TokenKind::LParen, 9, 1),
            tok(TokenKind::Ident, 10, 2),
            tok(TokenKind::Colon, 13, 1),
            tok(TokenKind::Ident, 15, 5),
            tok(TokenKind::RParen, 20, 1),
            tok(TokenKind::Arrow, 22, 2),
            tok(TokenKind::Ident, 25, 6),
            tok(TokenKind::LBrace, 32, 1),
            tok(TokenKind::Eof, 33, 0),
        ];
        let mut sink = VecSink::new();
        assert!(parse_functor(&toks, &src, file(), &mut sink).is_err());
        assert!(
            sink.diagnostics()
                .iter()
                .any(|d| format!("{}", d.code()).contains("P0233"))
        );
    }
}
