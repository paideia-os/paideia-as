//! §9.2 multi-line expressions + §9.3 block-level newline-as-separator
//! — pinning tests for paideia-as#1499 (PAS-DEBT-B2-006, v0.36.46).
//!
//! Together with the module doc-comment rewrite in `parse_stmt.rs`, these
//! tests pin the two capabilities that the original deferral note claimed
//! were missing but which the audit of 2026-09-25 showed are already
//! reachable through the current lexer + parser surface:
//!
//! - §9.2 works trivially: newlines are trivia, so any expression can span
//!   any number of lines with or without bracketing.
//! - §9.3 works at block level: `parse_block_kind` and `parse_handler_body`
//!   already wrap a non-`;`-terminated expression as a statement whenever
//!   the next token isn't the block closer, achieving newline-separated
//!   statement semantics without a lexer-context refactor.
//!
//! The residual (ambiguous-token pairs on adjacent lines — see the module
//! doc-comment note about B2-006-b) is deliberately *not* covered here —
//! covering it would require a lexer terminal for `Newline` in statement
//! context that we explicitly deferred.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{DiagnosticSink, Severity, SourceMap, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

/// End-to-end source-file parse via the real lexer, real parser.
/// Returns the diagnostics list only — every test here asserts no errors.
fn parse_source(source: &str) -> Vec<paideia_as_diagnostics::Diagnostic> {
    let mut source_map = SourceMap::new();
    let file = source_map.add_file(PathBuf::from("stmt_multiline.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let mut lex_sink = VecSink::new();
    let tokens = lex.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }
    {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        let _ = p.parse_source_file();
    }
    sink.into_diagnostics()
}

fn assert_clean(source: &str, tag: &str) {
    let diags = parse_source(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code().severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{tag}: expected zero error diagnostics, got {errors:#?}"
    );
}

// ---------------------------------------------------------------------------
// §9.2 — multi-line expressions
// ---------------------------------------------------------------------------

/// Arithmetic expression broken across three lines at binary-op boundaries.
/// Newlines are lexer trivia, so this parses identically to `1 + 2 + 3`.
#[test]
fn multiline_arith_at_binop_boundary() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = 1 +\n\
                         2 +\n\
                         3\n\
         }\n",
        "multiline_arith_at_binop_boundary",
    );
}

/// Function-call argument list spanning multiple lines. Parens bracket
/// the arg list, so any line break inside is trivia to the parser.
#[test]
fn multiline_call_args_across_lines() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = foo(\n\
             1,\n\
             2,\n\
             3\n\
           )\n\
         }\n",
        "multiline_call_args_across_lines",
    );
}

/// Tuple literal spanning multiple lines inside a `let`. Parens bracket
/// the tuple; the pattern side of the `let` and the expression side are
/// each on their own line.
#[test]
fn multiline_tuple_literal() {
    assert_clean(
        "module M = structure {\n\
           let pair : (u64, u64) = (\n\
             10,\n\
             20\n\
           )\n\
         }\n",
        "multiline_tuple_literal",
    );
}

/// Multi-line expression inside a block value (`= { ... }`). Freezes that
/// the `let` value expression itself can span lines when its body is a
/// block expression.
#[test]
fn multiline_block_value_expression() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = {\n\
             let y : u64 = 1;\n\
             let z : u64 = 2;\n\
             y + z\n\
           }\n\
         }\n",
        "multiline_block_value_expression",
    );
}

/// Multi-line expression without bracketing: a `let` where the RHS is a
/// binary expression that continues on the next line. The `+` on the
/// first line tells the pratt parser to keep pulling operands, so the
/// second line's `2` binds as the right operand.
#[test]
fn multiline_binop_no_bracket() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = 1 +\n\
             2\n\
         }\n",
        "multiline_binop_no_bracket",
    );
}

// ---------------------------------------------------------------------------
// §9.3 — block-level newline as statement separator (structural)
// ---------------------------------------------------------------------------

/// Three expression statements in a value block, separated only by
/// newlines. `parse_block_kind` sees no `;` after `1` but also no `}`,
/// so it wraps `1` as `StmtExpr` and loops. Same for `2`. The final `3`
/// is followed by `}` and becomes the block's tail expression.
#[test]
fn block_newline_separated_expr_statements() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = {\n\
             1\n\
             2\n\
             3\n\
           }\n\
         }\n",
        "block_newline_separated_expr_statements",
    );
}

/// Newline separates a `let` binding from a following tail expression
/// inside a block. The `let` statement's trailing `;` is optional
/// (`self.eat(TokenKind::Semicolon)` in `parse_let_stmt`), so the block
/// parser sees the next line's expression cleanly.
#[test]
fn block_newline_separates_let_from_tail() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = {\n\
             let y : u64 = 41\n\
             y + 1\n\
           }\n\
         }\n",
        "block_newline_separates_let_from_tail",
    );
}

/// Two `let` bindings in a value block, separated by a newline only.
/// Followed by a tail expression — also on its own line.
#[test]
fn block_newline_separates_two_lets_and_tail() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = {\n\
             let a : u64 = 1\n\
             let b : u64 = 2\n\
             a + b\n\
           }\n\
         }\n",
        "block_newline_separates_two_lets_and_tail",
    );
}

/// Let-in-block followed by a tail expression, both newline-terminated.
/// `parse_let_stmt` treats the trailing `;` as optional (`self.eat(...)`),
/// so a newline before the tail suffices.
#[test]
fn block_newline_after_typed_let_then_tail() {
    assert_clean(
        "module M = structure {\n\
           let z : u64 = {\n\
             let y : u64 = 41\n\
             y\n\
           }\n\
         }\n",
        "block_newline_after_typed_let_then_tail",
    );
}

// ---------------------------------------------------------------------------
// Mixed: newline separator + multi-line expression on the same line
// ---------------------------------------------------------------------------

/// A statement whose expression already spans multiple lines is followed
/// by a newline-only-terminated statement, then a tail. Exercises the
/// interaction between §9.2 and §9.3 at the block level.
#[test]
fn block_multiline_expr_then_newline_separator() {
    assert_clean(
        "module M = structure {\n\
           let x : u64 = {\n\
             let a : u64 = 1 +\n\
                           2 +\n\
                           3\n\
             let b : u64 = a * 2\n\
             a + b\n\
           }\n\
         }\n",
        "block_multiline_expr_then_newline_separator",
    );
}
