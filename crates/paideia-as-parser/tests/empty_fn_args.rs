//! Tests for empty lambda argument lists.
//! Issue #743 (Phase 6 m2-001): `parser: fn () -> body empty-arg list accepted`
//! Fixes paideia-as #735 (P0100 'expected pattern' on `fn () -> ...`).

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::Parser;

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

fn parse(
    tokens: Vec<Token>,
) -> (
    AstArena,
    Option<paideia_as_ast::NodeId>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let root = {
        let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.parse_expr().ok()
    };
    let diags = sink.diagnostics().to_vec();
    (arena, root, diags)
}

/// Test 1: `fn () -> 42` parses; lambda's params is empty vec.
#[test]
fn empty_fn_args_simple_int() {
    let tokens = vec![
        tok(TokenKind::KwFn, 0, 2),
        tok(TokenKind::LParen, 3, 1),
        tok(TokenKind::RParen, 4, 1),
        tok(TokenKind::Arrow, 6, 2),
        tok(TokenKind::IntLit, 9, 2), // 42
        tok(TokenKind::Eof, 11, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert_eq!(
        diags.len(),
        0,
        "no diagnostics expected for `fn () -> 42`"
    );
    let root = root_opt.expect("parse should succeed");
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.kind,
        paideia_as_ast::NodeKind::ExprLambda,
        "should be ExprLambda"
    );
    if let Some(expr_data) = arena.expr_data(root) {
        if let paideia_as_ast::ExprData::Lambda {
            generic_params,
            params,
            pipe_form,
            ..
        } = expr_data
        {
            assert!(generic_params.is_empty(), "no generic params");
            assert_eq!(params.len(), 0, "params should be empty vec");
            assert!(!pipe_form, "should be fn form, not pipe form");
        } else {
            panic!("expected ExprLambda");
        }
    } else {
        panic!("expected expr data");
    }
}

/// Test 2: `fn () -> body` parses with various body expressions (validates empty params work).
#[test]
fn empty_fn_args_block_body() {
    let tokens = vec![
        tok(TokenKind::KwFn, 0, 2),
        tok(TokenKind::LParen, 3, 1),
        tok(TokenKind::RParen, 4, 1),
        tok(TokenKind::Arrow, 6, 2),
        tok(TokenKind::LBrace, 9, 1),
        tok(TokenKind::IntLit, 10, 1), // 7
        tok(TokenKind::RBrace, 11, 1),
        tok(TokenKind::Eof, 12, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    // Filter for P-class diagnostics (parser errors, not semantic errors)
    let p_class_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            let code = d.code();
            code.to_string().starts_with("P0")
        })
        .collect();

    assert_eq!(
        p_class_diags.len(),
        0,
        "no P-class parser diagnostics expected for `fn () -> {{ ... }}`"
    );
    let root = root_opt.expect("parse should succeed");
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.kind,
        paideia_as_ast::NodeKind::ExprLambda,
        "should be ExprLambda"
    );
    if let Some(expr_data) = arena.expr_data(root) {
        if let paideia_as_ast::ExprData::Lambda { params, .. } = expr_data {
            assert_eq!(params.len(), 0, "params should be empty vec");
        } else {
            panic!("expected ExprLambda");
        }
    } else {
        panic!("expected expr data");
    }
}

/// Test 3: `let _start : () -> () = fn () -> ()` parses without P-class diagnostics.
#[test]
fn empty_fn_args_unit_type() {
    let tokens = vec![
        tok(TokenKind::KwFn, 0, 2),
        tok(TokenKind::LParen, 3, 1),
        tok(TokenKind::RParen, 4, 1),
        tok(TokenKind::Arrow, 6, 2),
        tok(TokenKind::LParen, 9, 1),
        tok(TokenKind::RParen, 10, 1),
        tok(TokenKind::Eof, 11, 0),
    ];
    let (_arena, _root_opt, diags) = parse(tokens);

    // Filter for P-class diagnostics (error codes P0xxx)
    let p_class_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            let code = d.code();
            code.to_string().starts_with("P0")
        })
        .collect();

    assert_eq!(
        p_class_diags.len(),
        0,
        "no P-class diagnostics expected for `fn () -> ()`"
    );
}

/// Test 4: P0100 fires on `fn (,) -> 42` (leading comma).
#[test]
fn empty_fn_args_leading_comma_error() {
    let tokens = vec![
        tok(TokenKind::KwFn, 0, 2),
        tok(TokenKind::LParen, 3, 1),
        tok(TokenKind::Comma, 4, 1), // leading comma - error
        tok(TokenKind::RParen, 5, 1),
        tok(TokenKind::Arrow, 7, 2),
        tok(TokenKind::IntLit, 10, 2), // 42
        tok(TokenKind::Eof, 12, 0),
    ];
    let (_arena, _root_opt, diags) = parse(tokens);

    // Should have at least one P0100 diagnostic
    let p0100_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            let code = d.code();
            code.to_string() == "P0100"
        })
        .collect();

    assert!(
        !p0100_diags.is_empty(),
        "expected P0100 diagnostic for leading comma in `fn (,) -> 42`"
    );
}

/// Test 5: P0100 fires on `fn (x,,y) -> 42` (double comma).
#[test]
fn empty_fn_args_double_comma_error() {
    let tokens = vec![
        tok(TokenKind::KwFn, 0, 2),
        tok(TokenKind::LParen, 3, 1),
        tok(TokenKind::Ident, 4, 1), // x
        tok(TokenKind::Colon, 5, 1),
        tok(TokenKind::Ident, 6, 3), // u64
        tok(TokenKind::RParen, 9, 1),
        tok(TokenKind::LParen, 11, 1),
        tok(TokenKind::Comma, 12, 1), // double comma - error (no identifier before it)
        tok(TokenKind::Ident, 13, 1), // y
        tok(TokenKind::Colon, 14, 1),
        tok(TokenKind::Ident, 15, 3), // u64
        tok(TokenKind::RParen, 18, 1),
        tok(TokenKind::Arrow, 20, 2),
        tok(TokenKind::IntLit, 23, 2), // 42
        tok(TokenKind::Eof, 25, 0),
    ];
    let (_arena, _root_opt, diags) = parse(tokens);

    // Should have at least one P0100 diagnostic
    let p0100_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            let code = d.code();
            code.to_string() == "P0100"
        })
        .collect();

    assert!(
        !p0100_diags.is_empty(),
        "expected P0100 diagnostic for malformed params in `fn (x,,y) -> 42`"
    );
}

/// Test 6: `|| body` parses (empty pipe form lambda).
#[test]
fn empty_pipe_form_lambda() {
    let tokens = vec![
        tok(TokenKind::Pipe, 0, 1),
        tok(TokenKind::Pipe, 1, 1),
        tok(TokenKind::IntLit, 3, 1), // 5
        tok(TokenKind::Eof, 4, 0),
    ];
    let (arena, root_opt, diags) = parse(tokens);

    assert_eq!(
        diags.len(),
        0,
        "no diagnostics expected for `|| 5`"
    );
    let root = root_opt.expect("parse should succeed");
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.kind,
        paideia_as_ast::NodeKind::ExprLambda,
        "should be ExprLambda"
    );
    if let Some(expr_data) = arena.expr_data(root) {
        if let paideia_as_ast::ExprData::Lambda {
            generic_params,
            params,
            pipe_form,
            ..
        } = expr_data
        {
            assert!(generic_params.is_empty(), "pipe form has no generic params");
            assert_eq!(params.len(), 0, "params should be empty vec");
            assert!(pipe_form, "should be pipe form");
        } else {
            panic!("expected ExprLambda");
        }
    } else {
        panic!("expected expr data");
    }
}
