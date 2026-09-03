//! Explicit-sync timeline primitives: `@timeline_wait(handle, value)` and
//! `@timeline_signal(handle, value)`.
//!
//! **v0.27-M1-002 (Wave 0 Batch 2, closes paideia-as#1366).** This module
//! parses the surface syntax only — lowering to timeline-fence primitives on
//! `KIND_GPU_TIMELINE` / `KIND_DISPLAY_TIMELINE` / `KIND_AUDIO_CLOCK`
//! capabilities is deferred to v0.27-M2. See
//! `design/kernel/linearity-and-tags.md` §KIND_HW_TIMELINE for the
//! capability contract these primitives will lower against.
//!
//! Unblocks driver work on R33 (display) and R37 (audio) once M2 lands
//! the lowering: driver code will write explicit fence points as
//! `@timeline_signal(display_cap, N)` on the producer side and
//! `@timeline_wait(display_cap, N)` on the consumer side, replacing
//! today's implicit-sync scaffolding.
//!
//! ## Grammar
//!
//! ```text
//! TimelineWait   ::= "@" "timeline_wait"   "(" HandleIdent "," IntLit ")"
//! TimelineSignal ::= "@" "timeline_signal" "(" HandleIdent "," IntLit ")"
//! HandleIdent    ::= Ident      // typed as `Cap<KIND_*_TIMELINE>` by
//!                               // elaboration; parser only enforces shape.
//! IntLit         ::= u64Lit     // parsed as u64, must be strictly positive.
//! ```
//!
//! ## Diagnostics
//!
//! Slot into the `P0300+` block reserved for v0.27-M1 explicit-sync
//! primitives. Parser diagnostics elsewhere in the crate stay in the
//! documented `P0100-P0299` range (see `parser.rs`); this small
//! extension keeps timeline errors clustered so future explicit-sync
//! forms (`@fence_signal`, `@fence_wait`, …) fall in the same
//! neighbourhood. Follow-on codes should stay contiguous with the block
//! opened here.
//!
//! - **P0300** — malformed shape: `@` not followed by the expected
//!   `timeline_wait` / `timeline_signal` identifier, or missing
//!   `(` / `)`.
//! - **P0301** — wrong arity: needs exactly two arguments
//!   `(handle, value)`. Missing arguments, missing separator, or a
//!   trailing extra comma all fail here.
//! - **P0302** — non-monotonic literal value: timeline values must be
//!   strictly positive. `0` is the reset sentinel and can never satisfy
//!   a wait or signal against a monotonic timeline counter, so it is
//!   rejected at parse time. Rejection of a *cross-call* decreasing
//!   sequence (e.g. two `signal(5)` then `signal(3)`) is elaborator
//!   work — that requires seeing the whole block.
//! - **P0303** — wrong handle shape: the first argument must be a bare
//!   capability identifier. The elaborator later checks it resolves to
//!   one of `Cap<KIND_GPU_TIMELINE>` / `Cap<KIND_DISPLAY_TIMELINE>` /
//!   `Cap<KIND_AUDIO_CLOCK>`; the parser only enforces the surface
//!   shape (an `Ident` token).

use paideia_as_ast::{NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

/// Discriminant for a parsed timeline primitive.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TimelineOpKind {
    /// `@timeline_wait(handle, value)` — block until `handle`'s counter
    /// reaches or exceeds `value`.
    Wait,
    /// `@timeline_signal(handle, value)` — advance `handle`'s counter to
    /// `value` (must be strictly greater than the previous signal on
    /// the same capability — enforced by elaboration).
    Signal,
}

impl TimelineOpKind {
    /// Source spelling of the primitive (without the leading `@`).
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            TimelineOpKind::Wait => "timeline_wait",
            TimelineOpKind::Signal => "timeline_signal",
        }
    }
}

/// Parsed surface form of a `@timeline_wait` / `@timeline_signal` call.
///
/// The parser produces this value with the capability handle
/// materialised as a `NodeKind::Ident` node in the AST arena so later
/// passes can attach type information. `value` is guaranteed non-zero;
/// cross-call monotonicity against the *previous* signal on the same
/// capability is elaborator work.
#[derive(Clone, Debug)]
pub struct TimelineOp {
    /// Which primitive this is (`Wait` or `Signal`).
    pub kind: TimelineOpKind,
    /// Arena `NodeKind::Ident` node for the capability handle. The
    /// handle's source text is recoverable through [`Self::handle_span`].
    pub handle: NodeId,
    /// Byte span of the handle identifier (for diagnostic anchoring).
    pub handle_span: Span,
    /// Monotonic u64 fence value. Guaranteed `> 0`.
    pub value: u64,
    /// Byte span of the value literal.
    pub value_span: Span,
    /// Full span from the leading `@` through the closing `)`.
    pub span: Span,
}

/// Parse `@timeline_wait(handle, value)`.
///
/// The cursor must currently be positioned on the leading `@`; on
/// success it advances past the closing `)`. On failure a `P0300..P0303`
/// diagnostic has already been emitted through the parser's sink.
pub fn parse_timeline_wait(p: &mut Parser<'_, '_, '_>) -> Result<TimelineOp, ParseError> {
    parse_timeline_op(p, TimelineOpKind::Wait)
}

/// Parse `@timeline_signal(handle, value)`.
///
/// The cursor must currently be positioned on the leading `@`; on
/// success it advances past the closing `)`. On failure a `P0300..P0303`
/// diagnostic has already been emitted through the parser's sink.
pub fn parse_timeline_signal(p: &mut Parser<'_, '_, '_>) -> Result<TimelineOp, ParseError> {
    parse_timeline_op(p, TimelineOpKind::Signal)
}

fn parse_timeline_op(
    p: &mut Parser<'_, '_, '_>,
    kind: TimelineOpKind,
) -> Result<TimelineOp, ParseError> {
    let keyword = kind.keyword();

    // `@`
    let at_tok = p.expect(TokenKind::At)?;
    let start_span = at_tok.span;

    // primitive name
    let name_tok = p.expect(TokenKind::Ident)?;
    let name_text = span_text(p, name_tok.span).to_string();
    if name_text != keyword {
        emit(
            p,
            300,
            name_tok.span,
            format!(
                "expected `@{}` after `@`, found `@{}`",
                keyword, name_text
            ),
        );
        return Err(ParseError);
    }

    // `(`
    if !p.eat(TokenKind::LParen) {
        let sp = current_span_or(p, name_tok.span);
        emit(
            p,
            300,
            sp,
            format!(
                "malformed @{}(handle, value) syntax: expected `(` after `{}`",
                keyword, keyword
            ),
        );
        return Err(ParseError);
    }

    // handle argument: must be a bare identifier.
    let handle_span = match p.peek() {
        Some(tok) if tok.kind == TokenKind::Ident => tok.span,
        Some(tok) if tok.kind == TokenKind::RParen => {
            emit(
                p,
                301,
                tok.span,
                format!(
                    "@{}(handle, value) needs exactly 2 arguments; found `)`",
                    keyword
                ),
            );
            return Err(ParseError);
        }
        Some(tok) => {
            let tok_span = tok.span;
            emit(
                p,
                303,
                tok_span,
                format!(
                    "@{} handle position must be a capability identifier \
                     (`Cap<KIND_GPU_TIMELINE>` / `Cap<KIND_DISPLAY_TIMELINE>` / \
                     `Cap<KIND_AUDIO_CLOCK>`)",
                    keyword
                ),
            );
            return Err(ParseError);
        }
        None => {
            let sp = current_span_or(p, name_tok.span);
            emit(
                p,
                301,
                sp,
                format!(
                    "@{}(handle, value) needs exactly 2 arguments; found end of input",
                    keyword
                ),
            );
            return Err(ParseError);
        }
    };
    let _ = p.bump();
    let handle_id = p.arena_mut().alloc(NodeKind::Ident, handle_span);

    // `,`
    if !p.eat(TokenKind::Comma) {
        let sp = current_span_or(p, handle_span);
        emit(
            p,
            301,
            sp,
            format!(
                "@{}(handle, value) needs exactly 2 arguments; expected `,` after handle",
                keyword
            ),
        );
        return Err(ParseError);
    }

    // value argument: integer literal, non-zero.
    let value_span;
    let value = match p.peek() {
        Some(tok) if tok.kind == TokenKind::IntLit => {
            let v_span = tok.span;
            let v_text = span_text(p, v_span).to_string();
            let _ = p.bump();
            value_span = v_span;
            match parse_u64_literal(&v_text) {
                Some(v) => v,
                None => {
                    emit(
                        p,
                        302,
                        v_span,
                        format!(
                            "@{} value `{}` is not a valid u64 literal",
                            keyword, v_text
                        ),
                    );
                    return Err(ParseError);
                }
            }
        }
        Some(tok) if tok.kind == TokenKind::RParen => {
            emit(
                p,
                301,
                tok.span,
                format!(
                    "@{}(handle, value) needs a `value` argument after the handle",
                    keyword
                ),
            );
            return Err(ParseError);
        }
        Some(tok) => {
            let tok_span = tok.span;
            emit(
                p,
                302,
                tok_span,
                format!(
                    "@{} value must be a u64 integer literal",
                    keyword
                ),
            );
            return Err(ParseError);
        }
        None => {
            let sp = current_span_or(p, handle_span);
            emit(
                p,
                301,
                sp,
                format!(
                    "@{}(handle, value) needs exactly 2 arguments; found end of input",
                    keyword
                ),
            );
            return Err(ParseError);
        }
    };

    if value == 0 {
        emit(
            p,
            302,
            value_span,
            format!(
                "@{} value must be non-zero — timeline counters are monotonic \
                 (`0` is the reset sentinel and can never satisfy a wait/signal)",
                keyword
            ),
        );
        return Err(ParseError);
    }

    // Reject a stray extra argument.
    if p.at(TokenKind::Comma) {
        let sp = current_span_or(p, value_span);
        emit(
            p,
            301,
            sp,
            format!(
                "@{}(handle, value) takes exactly 2 arguments; found a trailing `,`",
                keyword
            ),
        );
        return Err(ParseError);
    }

    // `)`
    let rparen_span = if p.at(TokenKind::RParen) {
        p.bump()
            .expect("at(RParen) implies peek() is Some")
            .span
    } else {
        let sp = current_span_or(p, value_span);
        emit(
            p,
            300,
            sp,
            format!(
                "malformed @{}(handle, value) syntax: expected `)` after value",
                keyword
            ),
        );
        return Err(ParseError);
    };

    let span = Span::new(
        start_span.file(),
        start_span.byte_start(),
        rparen_span.byte_start() + rparen_span.byte_len() - start_span.byte_start(),
    );

    Ok(TimelineOp {
        kind,
        handle: handle_id,
        handle_span,
        value,
        value_span,
        span,
    })
}

// ---------- helpers -----------------------------------------------------

fn span_text<'src>(p: &'src Parser<'_, '_, '_>, span: Span) -> &'src str {
    let src = p.source();
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if start <= src.len() && end <= src.len() {
        &src[start..end]
    } else {
        ""
    }
}

fn current_span_or(p: &Parser<'_, '_, '_>, fallback: Span) -> Span {
    p.peek().map(|t| t.span).unwrap_or(fallback)
}

fn emit(p: &mut Parser<'_, '_, '_>, number: u16, span: Span, msg: String) {
    let code = DiagnosticCode::new(Category::P, Severity::Error, number)
        .expect("valid P code");
    let diag = Diagnostic::error(code)
        .message(msg)
        .with_span(span)
        .finish();
    p.emit_diagnostic(diag);
}

/// Parse a paideia-as integer literal lexeme as `u64`.
///
/// Handles the same base prefixes and suffixes the lexer accepts —
/// `0x`, `0b`, `0o`, `_` separators, and a trailing width/sign suffix
/// (`u8`..`u128`, `usize`, `i8`..`i128`, `isize`). Returns `None` on
/// overflow or malformed body; the lexer's overflow diagnostic will
/// already have fired for the overflow case, so we do not double-emit.
fn parse_u64_literal(text: &str) -> Option<u64> {
    let body = strip_int_suffix(text);
    let cleaned: String = body.chars().filter(|c| *c != '_').collect();
    let (radix, digits): (u32, &str) = if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
        (16, &cleaned[2..])
    } else if cleaned.starts_with("0b") || cleaned.starts_with("0B") {
        (2, &cleaned[2..])
    } else if cleaned.starts_with("0o") || cleaned.starts_with("0O") {
        (8, &cleaned[2..])
    } else {
        (10, cleaned.as_str())
    };
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(digits, radix).ok()
}

/// Strip a trailing integer width/sign suffix from a literal lexeme.
fn strip_int_suffix(text: &str) -> &str {
    // Order matters: check the longest suffixes first so `usize` is not
    // shortened to `u` + `size`, etc.
    const SUFFIXES: &[&str] = &[
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ];
    for suffix in SUFFIXES {
        if let Some(prefix) = text.strip_suffix(*suffix) {
            return prefix;
        }
    }
    text
}

// ---------- tests -------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};

    /// Compact snapshot capture: outcome + shape + diagnostic codes.
    ///
    /// Deliberately excludes full spans / node IDs so a downstream
    /// allocation-order change does not thrash the golden output.
    #[derive(Debug)]
    struct ParseSummary {
        outcome: Outcome,
        handle_text: Option<String>,
        value: Option<u64>,
        diags: Vec<DiagLine>,
    }

    #[derive(Debug)]
    enum Outcome {
        Ok(&'static str),
        Err,
    }

    #[derive(Debug)]
    struct DiagLine {
        code: String,
        message: String,
    }

    fn drive(
        source: &str,
        which: TimelineOpKind,
    ) -> ParseSummary {
        let mut source_map = paideia_as_diagnostics::SourceMap::new();
        let file =
            source_map.add_file(std::path::PathBuf::from("timeline.pdx"), source.to_string());
        let source_text =
            SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
        let mut arena = paideia_as_ast::AstArena::new();
        let mut sink = VecSink::new();

        let mut lex = Lexer::new(file, &source_text);
        let mut lex_sink = VecSink::new();
        let tokens = lex.collect_tokens(&mut lex_sink);
        for d in lex_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }

        let (result, handle_span_opt) = {
            let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
            let r = match which {
                TimelineOpKind::Wait => parse_timeline_wait(&mut p),
                TimelineOpKind::Signal => parse_timeline_signal(&mut p),
            };
            let hs = r.as_ref().ok().map(|op| op.handle_span);
            (r, hs)
        };

        let (outcome, handle_text, value) = match result {
            Ok(op) => {
                let kw = match op.kind {
                    TimelineOpKind::Wait => "wait",
                    TimelineOpKind::Signal => "signal",
                };
                let hs = handle_span_opt.expect("Ok implies handle_span present");
                let start = hs.byte_start() as usize;
                let end = start + hs.byte_len() as usize;
                let handle = source[start..end].to_string();
                (Outcome::Ok(kw), Some(handle), Some(op.value))
            }
            Err(_) => (Outcome::Err, None, None),
        };

        let diags: Vec<DiagLine> = sink
            .into_diagnostics()
            .into_iter()
            .map(|d| DiagLine {
                code: format!("{}{:04}", d.code().category().letter(), d.code().number()),
                message: d.message().to_string(),
            })
            .collect();

        ParseSummary {
            outcome,
            handle_text,
            value,
            diags,
        }
    }

    #[test]
    fn timeline_wait_happy_path() {
        let s = drive("@timeline_wait(gpu_cap, 42)", TimelineOpKind::Wait);
        insta::assert_debug_snapshot!("timeline_wait_happy_path", s);
    }

    #[test]
    fn timeline_signal_happy_path() {
        let s = drive("@timeline_signal(display_cap, 100)", TimelineOpKind::Signal);
        insta::assert_debug_snapshot!("timeline_signal_happy_path", s);
    }

    #[test]
    fn timeline_wait_wrong_arity_missing_value() {
        let s = drive("@timeline_wait(gpu_cap)", TimelineOpKind::Wait);
        insta::assert_debug_snapshot!("timeline_wait_wrong_arity_missing_value", s);
    }

    #[test]
    fn timeline_wait_non_monotonic_zero() {
        let s = drive("@timeline_wait(gpu_cap, 0)", TimelineOpKind::Wait);
        insta::assert_debug_snapshot!("timeline_wait_non_monotonic_zero", s);
    }

    #[test]
    fn timeline_wait_wrong_handle_shape() {
        // Integer literal in handle position — the elaborator would later
        // reject this as `Cap<KIND_*_TIMELINE>`-wrong, but the parser
        // already knows the surface shape is bad and fires P0303.
        let s = drive("@timeline_wait(42, 5)", TimelineOpKind::Wait);
        insta::assert_debug_snapshot!("timeline_wait_wrong_handle_shape", s);
    }
}
