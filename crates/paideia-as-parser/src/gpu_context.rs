//! `@gpu_context(engine) { stmts }` — implicit GPU-submission scope.
//!
//! paideia-as#1370 (v0.28-M1-001), Wave 0 Batch 2 primitive.
//!
//! # Grammar
//!
//! ```text
//! GpuContext ::= '@' 'gpu_context' '(' Expr ')' Block
//! Block      ::= '{' Stmt* '}'
//! ```
//!
//! `Expr` is any expression whose value elaborates to `Cap<KIND_GPU_ENGINE>`
//! at the v0.29-M1-001 wiring step. Statements inside the body elaborate
//! with an implicit GPU-submission effect row that the elaborator stamps
//! from the enclosing scope — this parser only lands the shape.
//!
//! # Rules
//!
//! * **Single-level.** Nested `@gpu_context` blocks are rejected with
//!   `P0293`. The implicit effect row is scoped to exactly one dynamic-extent
//!   frame; stacking either masks the outer engine's discipline or silently
//!   picks one, and both are surprising.
//! * **Parser only.** The effect-row wiring lives in v0.29-M1-001 (parallel
//!   batch mate). This module produces a [`GpuContextBlock`] payload that
//!   downstream passes consume via [`paideia_as_ast::BlockScope::GpuContext`].
//!
//! # Diagnostics
//!
//! * `P0292` — malformed prefix or missing punctuation (`@`, `gpu_context`,
//!   `(`, `)`).
//! * `P0293` — nested `@gpu_context` is not allowed (single-level rule).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_lexer::TokenKind;

use crate::parse_control::BlockKind;
use crate::parser::{ParseError, Parser};

pub use paideia_as_ast::GpuContextBlock;

/// Parse a `@gpu_context(engine) { stmts }` block.
///
/// Assumes the cursor is positioned at the leading `@` token. Returns the
/// structured payload rather than an arena `NodeId`: the Wave 0 landing
/// keeps the AST enum ([`paideia_as_ast::BlockScope`]) as the serialization
/// point, and the parser hands the payload to callers that fold it into a
/// scope-tagged block once effect-row wiring lands (v0.29-M1-001).
///
/// # Errors
///
/// * `P0292` on any syntactic malformation of the prefix.
/// * `P0293` if invoked while the parser is already inside another
///   `@gpu_context` block (single-level rule).
pub fn parse_gpu_context(
    p: &mut Parser<'_, '_, '_>,
) -> Result<GpuContextBlock, ParseError> {
    // `@` — anchor for span-hinting on downstream errors.
    let at_tok = p.expect(TokenKind::At)?;
    let at_span = at_tok.span;

    // Contextual keyword `gpu_context`. Using `expect(Ident)` + text-check
    // mirrors the `@atomic(...)` prefix (parse_stmt::parse_optional_atomic_prefix)
    // — we do not want to burn a keyword token slot for what is a *prefix
    // attribute*, and identifier-then-text keeps the lexer table small.
    let name_tok = p.expect(TokenKind::Ident)?;
    let name_text = p.source_text_for_span(name_tok.span);
    if name_text != "gpu_context" {
        let code = p_code(292);
        p.emit_diagnostic(
            Diagnostic::error(code)
                .message(format!(
                    "unknown prefix attribute '@{}' — expected '@gpu_context(engine) {{ … }}'",
                    name_text
                ))
                .with_span(name_tok.span)
                .finish(),
        );
        return Err(ParseError);
    }

    // Nested-rejection *before* consuming any more tokens: the diagnostic
    // then points at the redundant `@gpu_context` prefix rather than at a
    // downstream punctuation error we would otherwise surface first.
    if p.gpu_context_depth > 0 {
        let code = p_code(293);
        p.emit_diagnostic(
            Diagnostic::error(code)
                .message(
                    "nested `@gpu_context` is not allowed — the implicit GPU-submission scope \
                     is single-level; hoist the inner statements into the enclosing block or \
                     factor them into a separate action",
                )
                .with_span(name_tok.span)
                .finish(),
        );
        return Err(ParseError);
    }

    // `(` engine `)`.
    if !p.eat(TokenKind::LParen) {
        let span = p.peek().map(|t| t.span).unwrap_or(at_span);
        p.emit_diagnostic(
            Diagnostic::error(p_code(292))
                .message("expected '(' after '@gpu_context'")
                .with_span(span)
                .finish(),
        );
        return Err(ParseError);
    }

    let engine = p.parse_expr()?;

    if !p.eat(TokenKind::RParen) {
        let span = p.peek().map(|t| t.span).unwrap_or(at_span);
        p.emit_diagnostic(
            Diagnostic::error(p_code(292))
                .message("expected ')' after '@gpu_context' engine expression")
                .with_span(span)
                .finish(),
        );
        return Err(ParseError);
    }

    // Body — a statement-position block so a trailing `;` synthesises a unit
    // tail (matches every other statement-position block in the grammar).
    //
    // The nested-guard bracket wraps only the body parse: an inner
    // `@gpu_context` seen inside the body will observe `depth > 0` and
    // reject; on exit the counter returns to its prior value even if the
    // body parse errored out (we still decrement before propagating).
    p.gpu_context_depth = p
        .gpu_context_depth
        .checked_add(1)
        .expect("gpu_context_depth overflow (u32::MAX nested blocks — programmer error)");
    let body_result = p.parse_block_kind(BlockKind::Statement);
    p.gpu_context_depth -= 1;
    let body = body_result?;

    Ok(GpuContextBlock { engine, body })
}

/// Construct a parser diagnostic code in the `P0100–P0299` band.
#[inline]
fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P-code")
}

#[cfg(test)]
mod tests {
    //! Unit tests that exercise the depth-guard directly.
    //!
    //! The natural surface-source path to nested `@gpu_context` needs the
    //! v0.29-M1-001 block-parser dispatch to fire — until then, source like
    //! `@gpu_context(a) { @gpu_context(b) { … } }` errors on the inner `@`
    //! at the block parser's `parse_expr` level, before ever reaching the
    //! guard. So we test the guard through its in-crate lever
    //! (`Parser::gpu_context_depth`, which is `pub(crate)`), which is the
    //! same lever the recursive call would exercise once dispatch lands.
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::{Token, TokenKind};

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    /// A `parse_gpu_context` call entered while the enclosing parser is
    /// already inside a `@gpu_context` block (i.e. `gpu_context_depth > 0`)
    /// must reject with `P0293` and emit exactly one diagnostic before
    /// consuming the body.
    #[test]
    fn nested_gpu_context_rejects_with_p0293() {
        let src = "@gpu_context(a) { x }";
        let tokens = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 11),
            tok(TokenKind::LParen, 12, 1),
            tok(TokenKind::Ident, 13, 1),
            tok(TokenKind::RParen, 14, 1),
            tok(TokenKind::LBrace, 16, 1),
            tok(TokenKind::Ident, 18, 1),
            tok(TokenKind::RBrace, 20, 1),
            tok(TokenKind::Eof, 21, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let (result, final_depth) = {
            let mut parser =
                Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena, &mut sink);
            // Simulate already being inside another @gpu_context block.
            parser.gpu_context_depth = 1;
            let r = parse_gpu_context(&mut parser);
            (r, parser.gpu_context_depth)
        };
        assert!(result.is_err(), "nested @gpu_context must not parse");

        let codes: Vec<String> = sink
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(
            codes,
            vec!["P0293".to_string()],
            "expected exactly one P0293 diagnostic, got {:?}",
            codes
        );

        // Depth must not have been mutated by a rejected entry.
        assert_eq!(
            final_depth, 1,
            "rejected entry must not touch the depth counter"
        );
    }

    /// A well-formed `parse_gpu_context` returns the depth counter to its
    /// starting value on exit. Guards against a decrement-on-panic hole that
    /// would let a sibling `parse_gpu_context` at the same textual level
    /// spuriously see `depth > 0` and reject.
    #[test]
    fn well_formed_parse_leaves_depth_zero() {
        let src = "@gpu_context(a) { x }";
        let tokens = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 11),
            tok(TokenKind::LParen, 12, 1),
            tok(TokenKind::Ident, 13, 1),
            tok(TokenKind::RParen, 14, 1),
            tok(TokenKind::LBrace, 16, 1),
            tok(TokenKind::Ident, 18, 1),
            tok(TokenKind::RBrace, 20, 1),
            tok(TokenKind::Eof, 21, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let final_depth = {
            let mut parser =
                Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena, &mut sink);
            assert_eq!(parser.gpu_context_depth, 0);
            parse_gpu_context(&mut parser).expect("parse succeeded");
            parser.gpu_context_depth
        };
        assert_eq!(
            final_depth, 0,
            "depth counter must return to zero on well-formed exit"
        );
    }

    /// The wrong prefix identifier (`@wrong(a) { … }`) is rejected with
    /// P0292 before the depth guard runs.
    #[test]
    fn unknown_prefix_ident_rejects_with_p0292() {
        // @wrong(a) { x }
        let src = "@wrong(a) { x }";
        let tokens = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 5),
            tok(TokenKind::LParen, 6, 1),
            tok(TokenKind::Ident, 7, 1),
            tok(TokenKind::RParen, 8, 1),
            tok(TokenKind::LBrace, 10, 1),
            tok(TokenKind::Ident, 12, 1),
            tok(TokenKind::RBrace, 14, 1),
            tok(TokenKind::Eof, 15, 0),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut parser =
                Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena, &mut sink);
            parse_gpu_context(&mut parser)
        };
        assert!(result.is_err());
        let codes: Vec<String> = sink
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, vec!["P0292".to_string()]);
    }
}
