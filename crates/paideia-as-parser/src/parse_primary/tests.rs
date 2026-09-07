//! Test suite for `parse_primary`. Extracted from `parse_primary/mod.rs`
//! (issue #1407 God-file split). All tests are relocated verbatim — the
//! `mod tests { ... }` wrapper is replaced by `#[cfg(test)] mod tests;`
//! in `mod.rs`, so `super` still resolves to `parse_primary`.

use super::embed::{GuidError, parse_guid};
use crate::Parser;
use paideia_as_ast::{AstArena, ExprData, NodeKind};
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

#[test]
fn parses_int_literal() {
    let tokens = vec![tok(TokenKind::IntLit, 0, 2), tok(TokenKind::Eof, 2, 0)];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    // Verify it's an ExprLiteral
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprLiteral);
}

#[test]
fn parses_string_literal() {
    let source = "\"hello\"";
    let tokens = vec![tok(TokenKind::StringLit, 0, 7), tok(TokenKind::Eof, 7, 0)];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprString);

    // Verify the string content was parsed
    if let Some(expr_data) = arena.expr_data(expr_id) {
        match expr_data {
            ExprData::StringLiteral(bytes) => assert_eq!(bytes.as_slice(), b"hello"),
            _ => panic!("expected StringLiteral"),
        }
    }
}

#[test]
fn parses_char_literal() {
    let tokens = vec![tok(TokenKind::CharLit, 0, 3), tok(TokenKind::Eof, 3, 0)];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprLiteral);
}

#[test]
fn parses_bool_true() {
    let tokens = vec![tok(TokenKind::KwTrue, 0, 4), tok(TokenKind::Eof, 4, 0)];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprLiteral);
}

#[test]
fn parses_bool_false() {
    let tokens = vec![tok(TokenKind::KwFalse, 0, 5), tok(TokenKind::Eof, 5, 0)];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprLiteral);
}

#[test]
fn parses_simple_identifier() {
    let tokens = vec![tok(TokenKind::Ident, 0, 4), tok(TokenKind::Eof, 4, 0)];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprPath);
}

#[test]
fn parses_path_of_three_segments() {
    let tokens = vec![
        tok(TokenKind::Ident, 0, 2),      // "a"
        tok(TokenKind::ColonColon, 2, 2), // "::"
        tok(TokenKind::Ident, 4, 2),      // "b"
        tok(TokenKind::ColonColon, 6, 2), // "::"
        tok(TokenKind::Ident, 8, 2),      // "c"
        tok(TokenKind::Eof, 10, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprPath);
}

#[test]
fn parses_parenthesized_expression() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::IntLit, 1, 2),
        tok(TokenKind::RParen, 3, 1),
        tok(TokenKind::Eof, 4, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprLiteral);
}

#[test]
fn parses_unit_literal() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::RParen, 1, 1),
        tok(TokenKind::Eof, 2, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprLiteral);
}

#[test]
fn parses_tuple_three_elements() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::IntLit, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::IntLit, 3, 1),
        tok(TokenKind::Comma, 4, 1),
        tok(TokenKind::IntLit, 5, 1),
        tok(TokenKind::RParen, 6, 1),
        tok(TokenKind::Eof, 7, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::Placeholder);
}

#[test]
fn mismatched_paren_emits_p0101() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::IntLit, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::IntLit, 3, 1),
        // Missing RParen
        tok(TokenKind::Eof, 4, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_err());
    assert_eq!(sink.diagnostics().len(), 1);
    let diag = &sink.diagnostics()[0];
    assert_eq!(diag.code().number(), 101);
}

#[test]
fn parses_empty_block_rejected() {
    // Empty blocks are not allowed; must have a tail expression.
    // Per #156 requirement: emits P0157 and returns Err.
    let tokens = vec![
        tok(TokenKind::LBrace, 0, 1),
        tok(TokenKind::RBrace, 1, 1),
        tok(TokenKind::Eof, 2, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    // Note: blocks are now parsed in parse_expr_bp Step 0, not in parse_primary
    let result = parser.parse_expr();
    assert!(result.is_err(), "empty block should parse error");

    let diags = sink.diagnostics();
    assert!(
        diags.iter().any(|d| d.code().number() == 157),
        "expected P0157 diagnostic (empty block)"
    );
}

#[test]
fn parses_perform_basic() {
    // perform Io::port_read(0x60)
    let tokens = vec![
        tok(TokenKind::KwPerform, 0, 7),
        tok(TokenKind::Ident, 8, 2),
        tok(TokenKind::ColonColon, 10, 2),
        tok(TokenKind::Ident, 12, 9),
        tok(TokenKind::LParen, 21, 1),
        tok(TokenKind::IntLit, 22, 3),
        tok(TokenKind::RParen, 25, 1),
        tok(TokenKind::Eof, 26, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprPerform);
}

#[test]
fn parses_perform_zero_args() {
    // perform Io::flush()
    let tokens = vec![
        tok(TokenKind::KwPerform, 0, 7),
        tok(TokenKind::Ident, 8, 2),
        tok(TokenKind::ColonColon, 10, 2),
        tok(TokenKind::Ident, 12, 5),
        tok(TokenKind::LParen, 17, 1),
        tok(TokenKind::RParen, 18, 1),
        tok(TokenKind::Eof, 19, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprPerform);
    if let Some(ExprData::Perform { args, .. }) = arena.expr_data(expr_id) {
        assert_eq!(args.len(), 0);
    } else {
        panic!("expected Perform variant");
    }
}

#[test]
fn parses_perform_multi_args() {
    // perform Io::port_write(0x64, 0xED)
    let tokens = vec![
        tok(TokenKind::KwPerform, 0, 7),
        tok(TokenKind::Ident, 8, 2),
        tok(TokenKind::ColonColon, 10, 2),
        tok(TokenKind::Ident, 12, 10),
        tok(TokenKind::LParen, 22, 1),
        tok(TokenKind::IntLit, 23, 3),
        tok(TokenKind::Comma, 26, 1),
        tok(TokenKind::IntLit, 28, 3),
        tok(TokenKind::RParen, 31, 1),
        tok(TokenKind::Eof, 32, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    if let Some(ExprData::Perform { args, .. }) = arena.expr_data(expr_id) {
        assert_eq!(args.len(), 2);
    } else {
        panic!("expected Perform variant");
    }
}

#[test]
fn parses_perform_path_three_segments() {
    // perform Mod::Io::read(addr)
    let tokens = vec![
        tok(TokenKind::KwPerform, 0, 7),
        tok(TokenKind::Ident, 8, 3),
        tok(TokenKind::ColonColon, 11, 2),
        tok(TokenKind::Ident, 13, 2),
        tok(TokenKind::ColonColon, 15, 2),
        tok(TokenKind::Ident, 17, 4),
        tok(TokenKind::LParen, 21, 1),
        tok(TokenKind::Ident, 22, 4),
        tok(TokenKind::RParen, 26, 1),
        tok(TokenKind::Eof, 27, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    if let Some(ExprData::Perform { op_path, .. }) = arena.expr_data(expr_id) {
        if let Some(ExprData::Path { segments }) = arena.expr_data(*op_path) {
            assert_eq!(segments.len(), 3);
        } else {
            panic!("expected Path for op_path");
        }
    } else {
        panic!("expected Perform variant");
    }
}

#[test]
fn perform_missing_paren_emits_p0161() {
    // perform Io::flush ... missing (
    let tokens = vec![
        tok(TokenKind::KwPerform, 0, 7),
        tok(TokenKind::Ident, 8, 2),
        tok(TokenKind::ColonColon, 10, 2),
        tok(TokenKind::Ident, 12, 5),
        tok(TokenKind::Eof, 17, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_err());
    assert_eq!(sink.diagnostics().len(), 1);
    let diag = &sink.diagnostics()[0];
    assert_eq!(diag.code().number(), 161);
}

#[test]
fn parses_resume_value() {
    // resume v
    let tokens = vec![
        tok(TokenKind::KwResume, 0, 6),
        tok(TokenKind::Ident, 7, 1),
        tok(TokenKind::Eof, 8, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprResume);
    if let Some(ExprData::Resume { value }) = arena.expr_data(expr_id) {
        let value_node = arena.get(*value).unwrap();
        assert_eq!(value_node.kind, NodeKind::ExprPath);
    } else {
        panic!("expected Resume variant");
    }
}

#[test]
fn parses_resume_unit() {
    // resume ()
    let tokens = vec![
        tok(TokenKind::KwResume, 0, 6),
        tok(TokenKind::LParen, 7, 1),
        tok(TokenKind::RParen, 8, 1),
        tok(TokenKind::Eof, 9, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprResume);
}

#[test]
fn parses_array_lit_single_element() {
    // [1]
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),
        tok(TokenKind::IntLit, 1, 1),
        tok(TokenKind::RBracket, 2, 1),
        tok(TokenKind::Eof, 3, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprArrayLit);
    if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
        assert_eq!(elements.len(), 1);
    } else {
        panic!("expected ArrayLit variant");
    }
}

#[test]
fn parses_array_lit_three_elements() {
    // [1, 2, 3]
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),
        tok(TokenKind::IntLit, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::IntLit, 3, 1),
        tok(TokenKind::Comma, 4, 1),
        tok(TokenKind::IntLit, 5, 1),
        tok(TokenKind::RBracket, 6, 1),
        tok(TokenKind::Eof, 7, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprArrayLit);
    if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
        assert_eq!(elements.len(), 3);
    } else {
        panic!("expected ArrayLit variant");
    }
}

#[test]
fn parses_array_lit_trailing_comma() {
    // [1, 2, 3,]
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),
        tok(TokenKind::IntLit, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::IntLit, 3, 1),
        tok(TokenKind::Comma, 4, 1),
        tok(TokenKind::IntLit, 5, 1),
        tok(TokenKind::Comma, 6, 1),
        tok(TokenKind::RBracket, 7, 1),
        tok(TokenKind::Eof, 8, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprArrayLit);
    if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
        assert_eq!(elements.len(), 3);
    } else {
        panic!("expected ArrayLit variant");
    }
}

#[test]
fn parses_array_lit_with_byte_literals() {
    // [0xCF, 0x9A, 0x00, 0x00, 0xFF]
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),
        tok(TokenKind::IntLit, 1, 4), // 0xCF
        tok(TokenKind::Comma, 5, 1),
        tok(TokenKind::IntLit, 6, 4), // 0x9A
        tok(TokenKind::Comma, 10, 1),
        tok(TokenKind::IntLit, 11, 5), // 0x00
        tok(TokenKind::Comma, 16, 1),
        tok(TokenKind::IntLit, 17, 5), // 0x00
        tok(TokenKind::Comma, 22, 1),
        tok(TokenKind::IntLit, 23, 4), // 0xFF
        tok(TokenKind::RBracket, 27, 1),
        tok(TokenKind::Eof, 28, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_ok());
    let expr_id = result.unwrap();
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprArrayLit);
    if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(expr_id) {
        assert_eq!(elements.len(), 5);
    } else {
        panic!("expected ArrayLit variant");
    }
}

#[test]
fn empty_array_lit_emits_p0210() {
    // []
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),
        tok(TokenKind::RBracket, 1, 1),
        tok(TokenKind::Eof, 2, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_err(), "empty array literal should fail");
    assert_eq!(sink.diagnostics().len(), 1);
    let diag = &sink.diagnostics()[0];
    assert_eq!(diag.code().number(), 210);
}

#[test]
fn array_lit_missing_close_emits_p0101() {
    // [1, 2 (EOF) - missing ]
    let tokens = vec![
        tok(TokenKind::LBracket, 0, 1),
        tok(TokenKind::IntLit, 1, 1),
        tok(TokenKind::Comma, 2, 1),
        tok(TokenKind::IntLit, 3, 1),
        tok(TokenKind::Eof, 4, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);

    let result = parser.parse_primary();
    assert!(result.is_err(), "missing closing bracket should fail");
    assert!(!sink.diagnostics().is_empty());
    let diag = &sink.diagnostics()[sink.diagnostics().len() - 1];
    assert_eq!(diag.code().number(), 101);
}

/// Phase 6 m5-001: Test — `uninit` contextual keyword parses as ExprUninit.
#[test]
fn parses_uninit_expr() {
    let source = "uninit";
    let tokens = vec![
        tok(TokenKind::Ident, 0, 6), // "uninit"
        tok(TokenKind::Eof, 6, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "uninit should parse as a primary expression"
    );
    let expr_id = result.unwrap();

    // Verify it's an ExprUninit node
    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprUninit);

    // Verify the ExprData is Uninit
    if let Some(ExprData::Uninit) = arena.expr_data(expr_id) {
        // Success
    } else {
        panic!("expected Uninit variant");
    }
}

#[test]
fn parse_guid_valid_lowercase() {
    let guid = "12345678-1234-1234-1234-123456789abc";
    let result = parse_guid(guid);
    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(
        bytes,
        [0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x12, 0x34, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
    );
}

#[test]
fn parse_guid_uppercase() {
    let guid = "ABCDEF01-2345-6789-ABCD-EF0123456789";
    let result = parse_guid(guid);
    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(
        bytes,
        [0x01, 0xef, 0xcd, 0xab, 0x45, 0x23, 0x89, 0x67, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89]
    );
}

#[test]
fn parse_guid_mixed_case() {
    let guid = "AaBbCcDd-EeFf-0011-2233-445566778899";
    let result = parse_guid(guid);
    assert!(result.is_ok());
    let bytes = result.unwrap();
    // Data1: AaBbCcDd → dd, cc, bb, aa (little-endian)
    // Data2: EeFf → ff, ee (little-endian)
    // Data3: 0011 → 11, 00 (little-endian)
    // Data4: 2233, 445566778899 → 22, 33, 44, 55, 66, 77, 88, 99
    assert_eq!(
        bytes,
        [0xdd, 0xcc, 0xbb, 0xaa, 0xff, 0xee, 0x11, 0x00, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99]
    );
}

#[test]
fn parse_guid_all_zeros() {
    let guid = "00000000-0000-0000-0000-000000000000";
    let result = parse_guid(guid);
    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(bytes, [0u8; 16]);
}

#[test]
fn parse_guid_all_fs() {
    let guid = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    let result = parse_guid(guid);
    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(bytes, [0xffu8; 16]);
}

#[test]
fn parse_guid_dashes_in_wrong_places() {
    // GUID with dash at position 7 instead of 8
    // A dash at a position where hex is expected is treated as NonHex, not MalformedDashes
    let guid = "1234567-12345-1234-1234-123456789abc";
    let result = parse_guid(guid);
    assert!(matches!(result, Err(GuidError::NonHex)));
}

#[test]
fn parse_guid_wrong_length() {
    let guid = "12345678-1234-1234-1234-123456789abc-extra";
    let result = parse_guid(guid);
    assert!(matches!(result, Err(GuidError::WrongLength)));
}

#[test]
fn parse_guid_too_short() {
    let guid = "12345678-1234-1234-1234-123456789ab";
    let result = parse_guid(guid);
    assert!(matches!(result, Err(GuidError::WrongLength)));
}

#[test]
fn parse_guid_non_hex() {
    let guid = "gggggggg-1234-1234-1234-123456789abc";
    let result = parse_guid(guid);
    assert!(matches!(result, Err(GuidError::NonHex)));
}

#[test]
fn parse_guid_empty() {
    let guid = "";
    let result = parse_guid(guid);
    assert!(matches!(result, Err(GuidError::WrongLength)));
}

// === @include_bytes directive tests ===

#[test]
fn happy_path_relative_directory_read() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("data.bin");
    std::fs::write(&data_file, b"hello").unwrap();

    let source = r#"@include_bytes("data.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 10),
        tok(TokenKind::RParen, 25, 1),
        tok(TokenKind::Eof, 26, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "expected parse to succeed, got diagnostics: {:?}",
        sink.diagnostics()
    );
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprInlineBytes);

    if let Some(ExprData::InlineBytes(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, b"hello");
    } else {
        panic!("expected InlineBytes variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn rejects_missing_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let source = r#"@include_bytes("does_not_exist.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 20),
        tok(TokenKind::RParen, 35, 1),
        tok(TokenKind::Eof, 36, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for missing file");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for missing file");
}

#[test]
fn rejects_absolute_path() {
    let source = r#"@include_bytes("/etc/passwd")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 13),
        tok(TokenKind::RParen, 28, 1),
        tok(TokenKind::Eof, 29, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for absolute path");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for absolute path");
}

#[test]
fn rejects_empty_path() {
    let source = r#"@include_bytes("")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 2),
        tok(TokenKind::RParen, 17, 1),
        tok(TokenKind::Eof, 18, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for empty path");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for empty path");
}

#[test]
fn rejects_directory_target() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let source = r#"@include_bytes(".")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 3),
        tok(TokenKind::RParen, 18, 1),
        tok(TokenKind::Eof, 19, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for directory target");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(
        diag.code().number(),
        279,
        "expected P0279 for directory target"
    );
}

#[test]
fn accepts_empty_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let empty_file = temp_dir.path().join("empty.bin");
    std::fs::write(&empty_file, b"").unwrap();

    let source = r#"@include_bytes("empty.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 11),
        tok(TokenKind::RParen, 26, 1),
        tok(TokenKind::Eof, 27, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "expected parse to succeed for empty file, got diagnostics: {:?}",
        sink.diagnostics()
    );
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprInlineBytes);

    if let Some(ExprData::InlineBytes(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes.len(), 0, "expected empty byte vector");
    } else {
        panic!("expected InlineBytes variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn accepts_dotdot_traversal() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let data_file = data_dir.join("foo.bin");
    std::fs::write(&data_file, b"foo content").unwrap();

    // From a subdirectory, traverse up and into data/ using ".."
    let source = r#"@include_bytes("../data/foo.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 17),
        tok(TokenKind::RParen, 32, 1),
        tok(TokenKind::Eof, 33, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();

    // Set source_dir to a subdirectory of temp_dir
    let source_subdir = temp_dir.path().join("src");
    std::fs::create_dir(&source_subdir).unwrap();

    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(source_subdir));

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "expected parse to succeed for dotdot traversal, got diagnostics: {:?}",
        sink.diagnostics()
    );
    let expr_id = result.unwrap();

    if let Some(ExprData::InlineBytes(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, b"foo content");
    } else {
        panic!("expected InlineBytes variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn missing_paren_after_directive() {
    let source = r#"@include_bytes "path""#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::StringLit, 15, 6),
        tok(TokenKind::Eof, 21, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for missing paren");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for missing paren");
}

#[test]
fn missing_string_literal() {
    let source = r#"@include_bytes(42)"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::IntLit, 15, 2),
        tok(TokenKind::RParen, 17, 1),
        tok(TokenKind::Eof, 18, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for missing string literal");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for missing string literal");
}

#[test]
fn rejects_oversized_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let large_file = temp_dir.path().join("large.bin");
    // Create a file with 256 bytes (larger than our test limit of 128)
    std::fs::write(&large_file, vec![0u8; 256]).unwrap();

    let source = r#"@include_bytes("large.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 13),
        tok(TokenKind::LParen, 14, 1),
        tok(TokenKind::StringLit, 15, 11),
        tok(TokenKind::RParen, 26, 1),
        tok(TokenKind::Eof, 27, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()))
    .with_test_max_embed_bytes(Some(128));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for oversized file");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 280, "expected P0280 for oversized file");
}

// === @include_str directive tests ===

#[test]
fn include_str_happy_path() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("data.txt");
    std::fs::write(&data_file, b"hello").unwrap();

    let source = r#"@include_str("data.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 10),
        tok(TokenKind::RParen, 23, 1),
        tok(TokenKind::Eof, 24, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "expected parse to succeed, got diagnostics: {:?}",
        sink.diagnostics()
    );
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprInlineStr);

    if let Some(ExprData::InlineStr(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, b"hello");
    } else {
        panic!("expected InlineStr variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn include_str_multibyte_utf8_ok() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("utf8.txt");
    // "héllo" = [0x68, 0xc3, 0xa9, 0x6c, 0x6c, 0x6f]
    std::fs::write(&data_file, b"h\xc3\xa9llo").unwrap();

    let source = r#"@include_str("utf8.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 10),
        tok(TokenKind::RParen, 23, 1),
        tok(TokenKind::Eof, 24, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_ok(), "expected parse to succeed for valid UTF-8");
    let expr_id = result.unwrap();

    if let Some(ExprData::InlineStr(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, b"h\xc3\xa9llo");
    } else {
        panic!("expected InlineStr variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn include_str_rejects_invalid_utf8() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("invalid.txt");
    // [0x48, 0xFF, 0x69] - 0xFF is invalid in UTF-8
    std::fs::write(&data_file, &[0x48u8, 0xFF, 0x69]).unwrap();

    let source = r#"@include_str("invalid.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 13),
        tok(TokenKind::RParen, 26, 1),
        tok(TokenKind::Eof, 27, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for invalid UTF-8");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 281, "expected P0281 for invalid UTF-8");
    assert!(
        diag.message().contains("byte 1"),
        "expected byte-offset 1 in message, got: {}",
        diag.message()
    );
}

#[test]
fn include_str_rejects_lone_continuation_byte() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("lone.txt");
    // [0x80] - lone continuation byte
    std::fs::write(&data_file, &[0x80u8]).unwrap();

    let source = r#"@include_str("lone.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 10),
        tok(TokenKind::RParen, 23, 1),
        tok(TokenKind::Eof, 24, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for lone continuation byte");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 281, "expected P0281 for lone continuation byte");
    assert!(
        diag.message().contains("byte 0"),
        "expected byte-offset 0 in message, got: {}",
        diag.message()
    );
}

#[test]
fn include_str_accepts_empty_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let empty_file = temp_dir.path().join("empty.txt");
    std::fs::write(&empty_file, b"").unwrap();

    let source = r#"@include_str("empty.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 11),
        tok(TokenKind::RParen, 24, 1),
        tok(TokenKind::Eof, 25, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_ok(), "expected parse to succeed for empty file");
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprInlineStr);

    if let Some(ExprData::InlineStr(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes.len(), 0, "expected empty byte vector");
    } else {
        panic!("expected InlineStr variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn include_str_rejects_missing_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let source = r#"@include_str("missing.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 12),
        tok(TokenKind::RParen, 25, 1),
        tok(TokenKind::Eof, 26, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for missing file");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for missing file");
}

#[test]
fn include_str_rejects_absolute_path() {
    let source = r#"@include_str("/etc/passwd")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 13),
        tok(TokenKind::RParen, 26, 1),
        tok(TokenKind::Eof, 27, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for absolute path");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for absolute path");
}

#[test]
fn include_str_bom_included_verbatim() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("bom.txt");
    // UTF-8 BOM + "hi" = [0xEF, 0xBB, 0xBF, 0x68, 0x69]
    std::fs::write(&data_file, &[0xEF, 0xBB, 0xBF, 0x68, 0x69]).unwrap();

    let source = r#"@include_str("bom.txt")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::StringLit, 13, 9),
        tok(TokenKind::RParen, 22, 1),
        tok(TokenKind::Eof, 23, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_ok(), "expected parse to succeed for BOM file");
    let expr_id = result.unwrap();

    if let Some(ExprData::InlineStr(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, &[0xEF, 0xBB, 0xBF, 0x68, 0x69], "expected BOM included verbatim");
    } else {
        panic!("expected InlineStr variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

// === @include_bytes_as_str directive tests ===

#[test]
fn include_bytes_as_str_happy_path() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("data.bin");
    std::fs::write(&data_file, b"hello").unwrap();

    let source = r#"@include_bytes_as_str("data.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 20),
        tok(TokenKind::LParen, 21, 1),
        tok(TokenKind::StringLit, 22, 10),
        tok(TokenKind::RParen, 32, 1),
        tok(TokenKind::Eof, 33, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "expected parse to succeed, got diagnostics: {:?}",
        sink.diagnostics()
    );
    let expr_id = result.unwrap();

    let node = arena.get(expr_id).unwrap();
    assert_eq!(node.kind, NodeKind::ExprInlineStr);

    if let Some(ExprData::InlineStr(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, b"hello");
    } else {
        panic!("expected InlineStr variant");
    }

    assert_eq!(sink.diagnostics().len(), 0);
}

#[test]
fn include_bytes_as_str_accepts_invalid_utf8() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("invalid.bin");
    // [0x48, 0xFF, 0x69] - 0xFF is invalid in UTF-8, but should be accepted by _as_str
    std::fs::write(&data_file, &[0x48u8, 0xFF, 0x69]).unwrap();

    let source = r#"@include_bytes_as_str("invalid.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 20),
        tok(TokenKind::LParen, 21, 1),
        tok(TokenKind::StringLit, 22, 13),
        tok(TokenKind::RParen, 35, 1),
        tok(TokenKind::Eof, 36, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(
        result.is_ok(),
        "expected parse to succeed for invalid UTF-8 (unchecked), got diagnostics: {:?}",
        sink.diagnostics()
    );
    let expr_id = result.unwrap();

    if let Some(ExprData::InlineStr(bytes)) = arena.expr_data(expr_id) {
        assert_eq!(bytes, &[0x48u8, 0xFF, 0x69], "expected exact invalid bytes preserved");
    } else {
        panic!("expected InlineStr variant");
    }

    assert_eq!(sink.diagnostics().len(), 0, "expected zero diagnostics for unchecked _as_str");
}

#[test]
fn include_bytes_as_str_rejects_missing_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let source = r#"@include_bytes_as_str("missing.bin")"#;
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 20),
        tok(TokenKind::LParen, 21, 1),
        tok(TokenKind::StringLit, 22, 12),
        tok(TokenKind::RParen, 34, 1),
        tok(TokenKind::Eof, 35, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        source,
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    )
    .with_source_dir(Some(temp_dir.path().to_path_buf()));

    let result = parser.parse_primary();
    assert!(result.is_err(), "expected parse to fail for missing file");

    let diags = sink.diagnostics();
    assert!(diags.len() > 0, "expected at least one diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 279, "expected P0279 for missing file");
}
