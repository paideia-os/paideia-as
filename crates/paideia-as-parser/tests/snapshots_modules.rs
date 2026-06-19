//! Snapshot tests for functor application parsing.
//!
//! Tests the AST shapes produced by the functor application parser.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::Token;
use paideia_as_lexer::TokenKind;
use paideia_as_parser::Parser;

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

#[test]
fn snapshot_shape_a_single_argument() {
    // Shape A: F(M) — one functor path, one argument, zero sharing.
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),  // F
        tok(TokenKind::LParen, 1, 1), // (
        tok(TokenKind::Ident, 2, 1),  // M
        tok(TokenKind::RParen, 3, 1), // )
        tok(TokenKind::Eof, 4, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        "F(M)",
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let expr_id = parser.parse_functor_app().expect("parse succeeded");
    let expr_data = arena.expr_data(expr_id).expect("expr data exists").clone();
    let snapshot = format!("{:#?}", expr_data);
    insta::assert_snapshot!("shape_a_single_argument", snapshot);
}

#[test]
fn snapshot_shape_b_curried_arguments() {
    // Shape B: F(M)(N) — two arguments, zero sharing.
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),  // F
        tok(TokenKind::LParen, 1, 1), // (
        tok(TokenKind::Ident, 2, 1),  // M
        tok(TokenKind::RParen, 3, 1), // )
        tok(TokenKind::LParen, 4, 1), // (
        tok(TokenKind::Ident, 5, 1),  // N
        tok(TokenKind::RParen, 6, 1), // )
        tok(TokenKind::Eof, 7, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        "F(M)(N)",
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let expr_id = parser.parse_functor_app().expect("parse succeeded");
    let expr_data = arena.expr_data(expr_id).expect("expr data exists").clone();
    let snapshot = format!("{:#?}", expr_data);
    insta::assert_snapshot!("shape_b_curried_arguments", snapshot);
}

#[test]
fn snapshot_shape_c_with_sharing() {
    // Shape C: F(M)(N) sharing (M::t = N::t) — two args, one constraint.
    let tokens = vec![
        tok(TokenKind::Ident, 0, 1),       // F
        tok(TokenKind::LParen, 1, 1),      // (
        tok(TokenKind::Ident, 2, 1),       // M
        tok(TokenKind::RParen, 3, 1),      // )
        tok(TokenKind::LParen, 4, 1),      // (
        tok(TokenKind::Ident, 5, 1),       // N
        tok(TokenKind::RParen, 6, 1),      // )
        tok(TokenKind::Ident, 8, 7),       // sharing
        tok(TokenKind::LParen, 15, 1),     // (
        tok(TokenKind::Ident, 16, 1),      // M
        tok(TokenKind::ColonColon, 17, 2), // ::
        tok(TokenKind::Ident, 19, 1),      // t
        tok(TokenKind::Eq, 21, 1),         // =
        tok(TokenKind::Ident, 23, 1),      // N
        tok(TokenKind::ColonColon, 24, 2), // ::
        tok(TokenKind::Ident, 26, 1),      // t
        tok(TokenKind::RParen, 27, 1),     // )
        tok(TokenKind::Eof, 28, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        "F(M)(N) sharing (M::t = N::t)",
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let expr_id = parser.parse_functor_app().expect("parse succeeded");
    let expr_data = arena.expr_data(expr_id).expect("expr data exists").clone();
    let snapshot = format!("{:#?}", expr_data);
    insta::assert_snapshot!("shape_c_with_sharing", snapshot);
}
