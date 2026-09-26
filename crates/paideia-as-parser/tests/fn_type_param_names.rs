//! Tests for function-type named-parameter preservation (paideia-as#1502,
//! PAS-DEBT-B2-009).
//!
//! Pre-fix, `parse_type_or_named_param` consumed `Ident Colon` and dropped
//! the name; `(bar: MmioRegion) -> u32` was AST-indistinguishable from
//! `(MmioRegion) -> u32`. Post-fix, `TypeData::FnPtr::param_names` carries
//! `Some(<Ident>)` for named slots and `None` for unnamed ones (or is empty
//! when no name was given at all — the backwards-compat shorthand).
//!
//! Fixtures:
//! - `(x: u64) -> u64`         — one named param.
//! - `(u64) -> u64`            — one unnamed param (backwards-compat: names empty).
//! - `(x: u64, y: u64) -> u64` — two named params.
//! - `(x: u64, u64) -> u64`    — mixed named + unnamed.
//! - `() -> u64`               — zero params.
//! - Nested: `(x: (y: u64) -> u64) -> u64` guards `parse_type` recursion.

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

/// `(x: u64) -> u64` — one named param.
#[test]
fn fn_ptr_single_named_param() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),   // x
        tok(TokenKind::Colon, 2, 1),
        tok(TokenKind::Ident, 4, 3),   // u64
        tok(TokenKind::RParen, 7, 1),
        tok(TokenKind::Arrow, 9, 2),
        tok(TokenKind::Ident, 12, 3),  // u64
        tok(TokenKind::Eof, 15, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::TypeFnPtr);
    match arena.type_data(root) {
        Some(TypeData::FnPtr {
            params,
            param_names,
            ..
        }) => {
            assert_eq!(params.len(), 1);
            assert_eq!(param_names.len(), 1, "names must parallel params when any is Some");
            let name = param_names[0].expect("`x` should be preserved");
            assert_eq!(
                arena.get(name).unwrap().kind,
                NodeKind::Ident,
                "param name must be an Ident node"
            );
        }
        other => panic!("expected TypeData::FnPtr, got {:?}", other),
    }
}

/// `(u64) -> u64` — one unnamed param; backwards-compat: `param_names` is
/// the empty shorthand (no allocation).
#[test]
fn fn_ptr_single_unnamed_param_backwards_compat() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 3),   // u64
        tok(TokenKind::RParen, 4, 1),
        tok(TokenKind::Arrow, 6, 2),
        tok(TokenKind::Ident, 9, 3),   // u64
        tok(TokenKind::Eof, 12, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::TypeFnPtr);
    match arena.type_data(root) {
        Some(TypeData::FnPtr {
            params,
            param_names,
            ..
        }) => {
            assert_eq!(params.len(), 1);
            assert!(
                param_names.is_empty(),
                "no name → empty shorthand, not [None]; got {:?}",
                param_names
            );
        }
        other => panic!("expected TypeData::FnPtr, got {:?}", other),
    }
}

/// `(x: u64, y: u64) -> u64` — two named params.
#[test]
fn fn_ptr_two_named_params() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),   // x
        tok(TokenKind::Colon, 2, 1),
        tok(TokenKind::Ident, 4, 3),   // u64
        tok(TokenKind::Comma, 7, 1),
        tok(TokenKind::Ident, 9, 1),   // y
        tok(TokenKind::Colon, 10, 1),
        tok(TokenKind::Ident, 12, 3),  // u64
        tok(TokenKind::RParen, 15, 1),
        tok(TokenKind::Arrow, 17, 2),
        tok(TokenKind::Ident, 20, 3),  // u64
        tok(TokenKind::Eof, 23, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    match arena.type_data(root) {
        Some(TypeData::FnPtr {
            params,
            param_names,
            ..
        }) => {
            assert_eq!(params.len(), 2);
            assert_eq!(param_names.len(), 2);
            assert!(param_names[0].is_some(), "x preserved");
            assert!(param_names[1].is_some(), "y preserved");
            let x_span = arena.get(param_names[0].unwrap()).unwrap().span;
            let y_span = arena.get(param_names[1].unwrap()).unwrap().span;
            assert_eq!(x_span.byte_start(), 1, "x span points at token position");
            assert_eq!(y_span.byte_start(), 9, "y span points at token position");
        }
        other => panic!("expected TypeData::FnPtr, got {:?}", other),
    }
}

/// `(x: u64, u64) -> u64` — mixed named + unnamed. The second slot is None.
#[test]
fn fn_ptr_mixed_named_and_unnamed() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),   // x
        tok(TokenKind::Colon, 2, 1),
        tok(TokenKind::Ident, 4, 3),   // u64
        tok(TokenKind::Comma, 7, 1),
        tok(TokenKind::Ident, 9, 3),   // u64 (no name)
        tok(TokenKind::RParen, 12, 1),
        tok(TokenKind::Arrow, 14, 2),
        tok(TokenKind::Ident, 17, 3),  // u64
        tok(TokenKind::Eof, 20, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    match arena.type_data(root) {
        Some(TypeData::FnPtr {
            params,
            param_names,
            ..
        }) => {
            assert_eq!(params.len(), 2);
            assert_eq!(
                param_names.len(),
                2,
                "when any name is Some, the vec is fully sized"
            );
            assert!(param_names[0].is_some(), "x named");
            assert!(param_names[1].is_none(), "second param unnamed");
        }
        other => panic!("expected TypeData::FnPtr, got {:?}", other),
    }
}

/// `() -> u64` — zero-param fn type, no names.
#[test]
fn fn_ptr_zero_params_no_names() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::RParen, 1, 1),
        tok(TokenKind::Arrow, 3, 2),
        tok(TokenKind::Ident, 6, 3),   // u64
        tok(TokenKind::Eof, 9, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    match arena.type_data(root) {
        Some(TypeData::FnPtr {
            params,
            param_names,
            ..
        }) => {
            assert!(params.is_empty());
            assert!(param_names.is_empty());
        }
        other => panic!("expected TypeData::FnPtr, got {:?}", other),
    }
}

/// Round-trip pretty-print check: `param_names` appears in the printed dump
/// when a named param is present. Guards `pretty::print_type_internal`
/// against the wildcard-arm regression the Wave-3/4 debugger warned about.
#[test]
fn fn_ptr_pretty_print_includes_param_names() {
    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),   // x
        tok(TokenKind::Colon, 2, 1),
        tok(TokenKind::Ident, 4, 3),   // u64
        tok(TokenKind::RParen, 7, 1),
        tok(TokenKind::Arrow, 9, 2),
        tok(TokenKind::Ident, 12, 3),  // u64
        tok(TokenKind::Eof, 15, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);
    assert!(diags.is_empty());
    let root = root_opt.expect("parse should succeed");
    let printed = paideia_as_ast::pretty::print_type(&arena, root);
    assert!(printed.contains("FnPtr"), "print_type output: {}", printed);
    assert!(
        printed.contains("param_names"),
        "printer must surface param_names: {}",
        printed
    );
}

/// `reflect::children` emits Ident-name Terms for named params so downstream
/// walkers reach every allocated node (Wave-3 defensive-coverage lesson).
#[test]
fn fn_ptr_reflect_children_include_names() {
    use paideia_as_ast::reflect::Term;

    let tokens = vec![
        tok(TokenKind::LParen, 0, 1),
        tok(TokenKind::Ident, 1, 1),   // x
        tok(TokenKind::Colon, 2, 1),
        tok(TokenKind::Ident, 4, 3),   // u64
        tok(TokenKind::Comma, 7, 1),
        tok(TokenKind::Ident, 9, 3),   // u64 (no name)
        tok(TokenKind::RParen, 12, 1),
        tok(TokenKind::Arrow, 14, 2),
        tok(TokenKind::Ident, 17, 3),  // u64 ret
        tok(TokenKind::Eof, 20, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);
    assert!(diags.is_empty());
    let root = root_opt.expect("parse should succeed");

    let term = Term::new(&arena, root);
    let children = term.children();
    // 1 name (x only) + 2 params + 1 ret = 4.
    assert_eq!(
        children.len(),
        4,
        "children: 1 name + 2 params + 1 ret, got {}",
        children.len()
    );
    // First child is the `x` Ident, second is the first param type, then second param type, then ret.
    assert_eq!(arena.get(children[0].id()).unwrap().kind, NodeKind::Ident);
}

/// Nested function type in a named-param position: `(f: (u64) -> u64) -> u64`.
/// Guards that `parse_type_or_named_param` uses `parse_type` recursion
/// rather than a hand-rolled shortcut (Wave-7 discipline).
#[test]
fn fn_ptr_nested_named_param_type() {
    let tokens = vec![
        // outer `(`
        tok(TokenKind::LParen, 0, 1),
        // `f`
        tok(TokenKind::Ident, 1, 1),
        // `:`
        tok(TokenKind::Colon, 2, 1),
        // inner `(u64) -> u64`
        tok(TokenKind::LParen, 4, 1),
        tok(TokenKind::Ident, 5, 3),
        tok(TokenKind::RParen, 8, 1),
        tok(TokenKind::Arrow, 10, 2),
        tok(TokenKind::Ident, 13, 3),
        // outer `)` ` -> u64`
        tok(TokenKind::RParen, 16, 1),
        tok(TokenKind::Arrow, 18, 2),
        tok(TokenKind::Ident, 21, 3),
        tok(TokenKind::Eof, 24, 0),
    ];
    let (arena, root_opt, diags) = parse_ty(tokens);

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    match arena.type_data(root) {
        Some(TypeData::FnPtr {
            params,
            param_names,
            ..
        }) => {
            assert_eq!(params.len(), 1, "one outer param");
            assert_eq!(param_names.len(), 1, "one named slot");
            let f_name = param_names[0].expect("`f` preserved");
            assert_eq!(arena.get(f_name).unwrap().kind, NodeKind::Ident);
            // The outer param's type is itself a FnPtr.
            let inner_ty = params[0];
            assert_eq!(
                arena.get(inner_ty).unwrap().kind,
                NodeKind::TypeFnPtr,
                "inner type must be a FnPtr (parse_type recursion)"
            );
        }
        other => panic!("expected outer TypeData::FnPtr, got {:?}", other),
    }
}
