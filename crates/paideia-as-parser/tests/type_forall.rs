//! Tests for `forall` type-quantifier parsing (paideia-as#1501, PAS-DEBT-B2-008).
//!
//! Pre-fix, `parse_type` consumed the bound variable via `expect(Ident)`
//! and dropped it on the floor, so the returned `NodeId` pointed straight
//! at the unquantified body — `forall a. T` was AST-indistinguishable
//! from `T`. Post-fix, the parser allocates a
//! `TypeData::Forall { bound: Vec<NodeId>, body }` node and returns it.
//!
//! Fixtures:
//! - `forall a. a`               — single bound, body is TypeName.
//! - `forall a. (a) -> a`        — single bound, body is TypeFnPtr.
//! - `forall a b. (a, b) -> b`   — two bound vars, body is TypeFnPtr.
//! - `forall a. forall b. (a) -> b` — nested foralls compose right.
//! - `forall .`                  — zero binders rejected (P0100 on `.`).

use paideia_as_ast::{AstArena, NodeId, NodeKind, TypeData};
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::Parser;

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

fn parse_ty(
    tokens: Vec<Token>,
) -> (
    AstArena,
    Option<NodeId>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let root = {
        let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.parse_type().ok()
    };
    let diags = sink.diagnostics().to_vec();
    (arena, root, diags)
}

/// `forall a. a` — one bound var, body is a bare TypeName.
#[test]
fn forall_single_bound_type_name_body() {
    let tokens = vec![
        tok(TokenKind::KwForall, 0, 6),
        tok(TokenKind::Ident, 7, 1),   // a
        tok(TokenKind::Dot, 8, 1),
        tok(TokenKind::Ident, 10, 1),  // a
        tok(TokenKind::Eof, 11, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::TypeForall);
    match arena.type_data(root) {
        Some(TypeData::Forall { bound, body }) => {
            assert_eq!(bound.len(), 1, "one bound var");
            assert_eq!(
                arena.get(bound[0]).unwrap().kind,
                NodeKind::Ident,
                "bound var must be an Ident node"
            );
            assert_eq!(
                arena.get(*body).unwrap().kind,
                NodeKind::TypeName,
                "body must be a TypeName"
            );
        }
        other => panic!("expected TypeData::Forall, got {:?}", other),
    }
}

/// `forall a. (a) -> a` — one bound var, body is a function-pointer type.
#[test]
fn forall_single_bound_fn_ptr_body() {
    let tokens = vec![
        tok(TokenKind::KwForall, 0, 6),
        tok(TokenKind::Ident, 7, 1),    // a
        tok(TokenKind::Dot, 8, 1),
        tok(TokenKind::LParen, 10, 1),
        tok(TokenKind::Ident, 11, 1),   // a
        tok(TokenKind::RParen, 12, 1),
        tok(TokenKind::Arrow, 14, 2),
        tok(TokenKind::Ident, 17, 1),   // a
        tok(TokenKind::Eof, 18, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::TypeForall);
    match arena.type_data(root) {
        Some(TypeData::Forall { bound, body }) => {
            assert_eq!(bound.len(), 1);
            assert_eq!(arena.get(*body).unwrap().kind, NodeKind::TypeFnPtr);
        }
        other => panic!("expected TypeData::Forall, got {:?}", other),
    }
}

/// `forall a b. (a, b) -> b` — two bound vars, body is a function-pointer.
///
/// Multi-bound is the `forall a b. T` shape (space-separated names, no
/// commas). The parser's collector loops on Ident until it sees Dot.
#[test]
fn forall_two_bound_fn_ptr_body() {
    let tokens = vec![
        tok(TokenKind::KwForall, 0, 6),
        tok(TokenKind::Ident, 7, 1),    // a
        tok(TokenKind::Ident, 9, 1),    // b
        tok(TokenKind::Dot, 10, 1),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::Ident, 13, 1),   // a
        tok(TokenKind::Comma, 14, 1),
        tok(TokenKind::Ident, 16, 1),   // b
        tok(TokenKind::RParen, 17, 1),
        tok(TokenKind::Arrow, 19, 2),
        tok(TokenKind::Ident, 22, 1),   // b
        tok(TokenKind::Eof, 23, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::TypeForall);
    match arena.type_data(root) {
        Some(TypeData::Forall { bound, body }) => {
            assert_eq!(bound.len(), 2, "two bound vars: a, b");
            assert_eq!(arena.get(bound[0]).unwrap().kind, NodeKind::Ident);
            assert_eq!(arena.get(bound[1]).unwrap().kind, NodeKind::Ident);
            assert_eq!(arena.get(*body).unwrap().kind, NodeKind::TypeFnPtr);
        }
        other => panic!("expected TypeData::Forall, got {:?}", other),
    }
}

/// `forall a. forall b. (a) -> b` — nested foralls compose right-associatively.
///
/// The outer TypeForall wraps another TypeForall wraps the arrow. This
/// exercises the recursive `parse_type` call inside the forall production.
#[test]
fn forall_nested() {
    let tokens = vec![
        tok(TokenKind::KwForall, 0, 6),
        tok(TokenKind::Ident, 7, 1),    // a
        tok(TokenKind::Dot, 8, 1),
        tok(TokenKind::KwForall, 10, 6),
        tok(TokenKind::Ident, 17, 1),   // b
        tok(TokenKind::Dot, 18, 1),
        tok(TokenKind::LParen, 20, 1),
        tok(TokenKind::Ident, 21, 1),   // a
        tok(TokenKind::RParen, 22, 1),
        tok(TokenKind::Arrow, 24, 2),
        tok(TokenKind::Ident, 27, 1),   // b
        tok(TokenKind::Eof, 28, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::TypeForall);
    match arena.type_data(root) {
        Some(TypeData::Forall { bound, body }) => {
            assert_eq!(bound.len(), 1, "outer binds `a`");
            let inner = *body;
            assert_eq!(
                arena.get(inner).unwrap().kind,
                NodeKind::TypeForall,
                "body must itself be a TypeForall"
            );
            match arena.type_data(inner) {
                Some(TypeData::Forall {
                    bound: b2,
                    body: b2_body,
                }) => {
                    assert_eq!(b2.len(), 1, "inner binds `b`");
                    assert_eq!(
                        arena.get(*b2_body).unwrap().kind,
                        NodeKind::TypeFnPtr,
                        "innermost body is the arrow"
                    );
                }
                other => panic!("expected inner TypeData::Forall, got {:?}", other),
            }
        }
        other => panic!("expected outer TypeData::Forall, got {:?}", other),
    }
}

/// `forall .` — zero binders rejected. `expect(Ident)` fires P0100 on `.`.
#[test]
fn forall_zero_binders_rejected() {
    let tokens = vec![
        tok(TokenKind::KwForall, 0, 6),
        tok(TokenKind::Dot, 7, 1),
        tok(TokenKind::Ident, 9, 1), // T (never reached)
        tok(TokenKind::Eof, 10, 0),
    ];
    let (_arena, root_opt, diags) = parse_ty(tokens);

    assert!(root_opt.is_none(), "parse should fail");
    assert!(
        diags.iter().any(|d| d.code().number() == 100),
        "expected P0100 (unexpected token), got: {:?}",
        diags
    );
}

/// Round-trip pretty-print check: `Forall` appears in the printed dump.
///
/// Guards `pretty::print_type_internal` against the wildcard shape the
/// Wave-3/4 debugger warned about — silent misses on new TypeData
/// variants show up as an empty printer arm.
#[test]
fn forall_pretty_print_includes_forall() {
    let tokens = vec![
        tok(TokenKind::KwForall, 0, 6),
        tok(TokenKind::Ident, 7, 1),  // a
        tok(TokenKind::Dot, 8, 1),
        tok(TokenKind::Ident, 10, 1), // a
        tok(TokenKind::Eof, 11, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);
    assert!(diags.is_empty());
    let root = root_opt.expect("parse should succeed");
    let printed = paideia_as_ast::pretty::print_type(&arena, root);
    assert!(printed.contains("Forall"), "print_type output: {}", printed);
    assert!(printed.contains("bound"), "print_type output: {}", printed);
}
