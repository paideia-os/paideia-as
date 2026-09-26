//! Tests for `trait_args` extraction on trait-impl blocks
//! (paideia-as#1497, PAS-DEBT-B2-003).
//!
//! Pre-fix, `parse_impl_decl` stored `trait_args = Vec::new()` even when the
//! trait position carried type arguments — `impl Trait(u32) for MyStruct`
//! was AST-indistinguishable from `impl Trait for MyStruct`. Post-fix, the
//! parser mirrors the trait TypeName's `args` into `ImplDecl::trait_args`.
//!
//! Fixtures:
//! - `impl Trait(u32) for MyStruct { }`     — one arg.
//! - `impl Trait(T, U) for MyStruct { }`    — two args.
//! - `impl Trait for MyStruct { }`          — no args (regression).
//! - `impl MyStruct { }`                    — inherent impl (regression).

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind, TypeData};
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::Parser;

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

fn parse_item(
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
        p.parse_item().ok()
    };
    let diags = sink.diagnostics().to_vec();
    (arena, root, diags)
}

/// `impl Trait(u32) for MyStruct { }` — trait carries a single type arg.
#[test]
fn trait_impl_extracts_single_trait_arg() {
    // impl Trait(u32) for MyStruct { }
    let tokens = vec![
        tok(TokenKind::KwImpl, 0, 4),
        tok(TokenKind::Ident, 5, 5),   // Trait
        tok(TokenKind::LParen, 10, 1),
        tok(TokenKind::Ident, 11, 3),  // u32
        tok(TokenKind::RParen, 14, 1),
        tok(TokenKind::KwFor, 16, 3),
        tok(TokenKind::Ident, 20, 8),  // MyStruct
        tok(TokenKind::LBrace, 29, 1),
        tok(TokenKind::RBrace, 30, 1),
        tok(TokenKind::Eof, 31, 0),
    ];
    let (arena, root_opt, diags) = parse_item(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");
    assert_eq!(arena.get(root).unwrap().kind, NodeKind::Impl);

    match arena.item_data(root) {
        Some(ItemData::Impl(decl)) => {
            assert!(decl.trait_name.is_some(), "trait impl must set trait_name");
            assert_eq!(decl.trait_args.len(), 1, "one trait arg");
            let arg = decl.trait_args[0];
            assert_eq!(
                arena.get(arg).unwrap().kind,
                NodeKind::TypeName,
                "trait arg should be a TypeName node"
            );
        }
        other => panic!("expected ItemData::Impl, got {:?}", other),
    }
}

/// `impl Trait(T, U) for MyStruct { }` — trait carries two type args.
#[test]
fn trait_impl_extracts_multiple_trait_args() {
    // impl Trait(T, U) for MyStruct { }
    let tokens = vec![
        tok(TokenKind::KwImpl, 0, 4),
        tok(TokenKind::Ident, 5, 5),   // Trait
        tok(TokenKind::LParen, 10, 1),
        tok(TokenKind::Ident, 11, 1),  // T
        tok(TokenKind::Comma, 12, 1),
        tok(TokenKind::Ident, 14, 1),  // U
        tok(TokenKind::RParen, 15, 1),
        tok(TokenKind::KwFor, 17, 3),
        tok(TokenKind::Ident, 21, 8),  // MyStruct
        tok(TokenKind::LBrace, 30, 1),
        tok(TokenKind::RBrace, 31, 1),
        tok(TokenKind::Eof, 32, 0),
    ];
    let (arena, root_opt, diags) = parse_item(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");

    match arena.item_data(root) {
        Some(ItemData::Impl(decl)) => {
            assert_eq!(decl.trait_args.len(), 2, "two trait args");
            for arg in &decl.trait_args {
                assert_eq!(
                    arena.get(*arg).unwrap().kind,
                    NodeKind::TypeName,
                    "each trait arg should be a TypeName node"
                );
            }
            // Mirror invariant: trait_args must equal the TypeName's own args.
            let trait_name = decl.trait_name.expect("trait_name present");
            match arena.type_data(trait_name) {
                Some(TypeData::Name { args, .. }) => {
                    assert_eq!(
                        args, &decl.trait_args,
                        "trait_args mirror TypeName args"
                    );
                }
                other => panic!("expected trait_name TypeData::Name, got {:?}", other),
            }
        }
        other => panic!("expected ItemData::Impl, got {:?}", other),
    }
}

/// `impl Trait for MyStruct { }` — no trait args (regression).
#[test]
fn trait_impl_no_trait_args_stays_empty() {
    // impl Trait for MyStruct { }
    let tokens = vec![
        tok(TokenKind::KwImpl, 0, 4),
        tok(TokenKind::Ident, 5, 5),   // Trait
        tok(TokenKind::KwFor, 11, 3),
        tok(TokenKind::Ident, 15, 8),  // MyStruct
        tok(TokenKind::LBrace, 24, 1),
        tok(TokenKind::RBrace, 25, 1),
        tok(TokenKind::Eof, 26, 0),
    ];
    let (arena, root_opt, diags) = parse_item(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");

    match arena.item_data(root) {
        Some(ItemData::Impl(decl)) => {
            assert!(decl.trait_name.is_some(), "trait impl must set trait_name");
            assert!(
                decl.trait_args.is_empty(),
                "trait with no args must leave trait_args empty"
            );
        }
        other => panic!("expected ItemData::Impl, got {:?}", other),
    }
}

/// `impl MyStruct { }` — inherent impl carries no trait, no trait_args.
#[test]
fn inherent_impl_has_no_trait_args() {
    // impl MyStruct { }
    let tokens = vec![
        tok(TokenKind::KwImpl, 0, 4),
        tok(TokenKind::Ident, 5, 8),   // MyStruct
        tok(TokenKind::LBrace, 14, 1),
        tok(TokenKind::RBrace, 15, 1),
        tok(TokenKind::Eof, 16, 0),
    ];
    let (arena, root_opt, diags) = parse_item(tokens);
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
    let root = root_opt.expect("parse should succeed");

    match arena.item_data(root) {
        Some(ItemData::Impl(decl)) => {
            assert!(decl.trait_name.is_none(), "inherent impl has no trait");
            assert!(decl.trait_args.is_empty(), "inherent impl has no trait_args");
        }
        other => panic!("expected ItemData::Impl, got {:?}", other),
    }
}
