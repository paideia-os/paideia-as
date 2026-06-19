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

#[test]
fn snapshot_shape_d_pack_simple() {
    // Shape D: pack M : S
    let tokens = vec![
        tok(TokenKind::Ident, 0, 4), // pack
        tok(TokenKind::Ident, 5, 1), // M
        tok(TokenKind::Colon, 7, 1), // :
        tok(TokenKind::Ident, 9, 1), // S
        tok(TokenKind::Eof, 10, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        "pack M : S",
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let expr_id = parser.parse_pack_expr().expect("parse succeeded");
    let expr_data = arena.expr_data(expr_id).expect("expr data exists").clone();
    let snapshot = format!("{:#?}", expr_data);
    insta::assert_snapshot!("shape_d_pack_simple", snapshot);
}

#[test]
fn snapshot_shape_e_let_module_simple() {
    // Shape E: let module N = unpack v in body
    let tokens = vec![
        tok(TokenKind::KwLet, 0, 3),  // let
        tok(TokenKind::Ident, 4, 6),  // module
        tok(TokenKind::Ident, 11, 1), // N
        tok(TokenKind::Eq, 13, 1),    // =
        tok(TokenKind::Ident, 15, 6), // unpack
        tok(TokenKind::Ident, 22, 1), // v
        tok(TokenKind::Ident, 24, 2), // in
        tok(TokenKind::Ident, 27, 4), // body
        tok(TokenKind::Eof, 31, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        "let module N = unpack v in body",
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let expr_id = parser.parse_let_module().expect("parse succeeded");
    let expr_data = arena.expr_data(expr_id).expect("expr data exists").clone();
    let snapshot = format!("{:#?}", expr_data);
    insta::assert_snapshot!("shape_e_let_module_simple", snapshot);
}

#[test]
fn snapshot_shape_f_nested_unpack_in_let_module() {
    // Shape F: let module N = unpack (v) in body
    // This tests that unpack can accept a parenthesized expression
    let tokens = vec![
        tok(TokenKind::KwLet, 0, 3),   // let
        tok(TokenKind::Ident, 4, 6),   // module
        tok(TokenKind::Ident, 11, 1),  // N
        tok(TokenKind::Eq, 13, 1),     // =
        tok(TokenKind::Ident, 15, 6),  // unpack
        tok(TokenKind::LParen, 22, 1), // (
        tok(TokenKind::Ident, 23, 1),  // v
        tok(TokenKind::RParen, 24, 1), // )
        tok(TokenKind::Ident, 26, 2),  // in
        tok(TokenKind::Ident, 29, 4),  // body
        tok(TokenKind::Eof, 33, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(
        &tokens,
        "let module N = unpack (v) in body",
        FileId::new(1).unwrap(),
        &mut arena,
        &mut sink,
    );

    let expr_id = parser.parse_let_module().expect("parse succeeded");
    let expr_data = arena.expr_data(expr_id).expect("expr data exists").clone();
    let snapshot = format!("{:#?}", expr_data);
    insta::assert_snapshot!("shape_f_nested_unpack_in_let_module", snapshot);
}
