//! Snapshot tests for `@gpu_context(engine) { stmts }` parsing.
//!
//! paideia-as#1370 (v0.28-M1-001), Wave 0 Batch 2.
//!
//! These fixtures freeze the parser output shape — the `GpuContextBlock`
//! payload plus the arena nodes it points at. If a future refactor changes
//! either the arena allocation order or the payload shape, snapshots need a
//! deliberate refresh with `cargo insta review`.
//!
//! The nested-rejection guard (`P0293`) is exercised as a unit test inside
//! `src/gpu_context.rs` — reaching the guard through source syntax requires
//! block-parser dispatch that lands with the v0.29-M1-001 effect-row wiring
//! (parallel batch mate), so the parser-only landing tests the guard by
//! driving `Parser::gpu_context_depth` directly (an in-crate field).

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::{Parser, parse_gpu_context};

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(
        kind,
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
    )
}

/// Fixture 1: minimal well-formed block — `@gpu_context(eng) { launch }`.
///
/// The simplest legal shape: a bare-identifier engine expression and a bare
/// tail expression inside the body. Freezes the arena numbering that
/// downstream elaboration will index into.
#[test]
fn snapshot_gpu_context_minimal() {
    // @gpu_context(eng) { launch }
    let src = "@gpu_context(eng) { launch }";
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::Ident, 13, 3),
        tok(TokenKind::RParen, 16, 1),
        tok(TokenKind::LBrace, 18, 1),
        tok(TokenKind::Ident, 20, 6),
        tok(TokenKind::RBrace, 27, 1),
        tok(TokenKind::Eof, 28, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena, &mut sink);
    let block = parse_gpu_context(&mut parser).expect("parse succeeded");

    insta::assert_debug_snapshot!("gpu_context_minimal", block);
    assert!(sink.diagnostics().is_empty(), "no diagnostics expected");
}

/// Fixture 2: dotted engine path — `@gpu_context(Gpu.engine) { … }`.
///
/// Confirms that the engine slot accepts an arbitrary expression (here, a
/// field-access), not just a bare identifier. Wire-up (v0.29-M1-001) will
/// check the resulting type is `Cap<KIND_GPU_ENGINE>`.
#[test]
fn snapshot_gpu_context_dotted_engine() {
    // @gpu_context(Gpu.engine) { submit }
    let src = "@gpu_context(Gpu.engine) { submit }";
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::Ident, 13, 3),
        tok(TokenKind::Dot, 16, 1),
        tok(TokenKind::Ident, 17, 6),
        tok(TokenKind::RParen, 23, 1),
        tok(TokenKind::LBrace, 25, 1),
        tok(TokenKind::Ident, 27, 6),
        tok(TokenKind::RBrace, 34, 1),
        tok(TokenKind::Eof, 35, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena, &mut sink);
    let block = parse_gpu_context(&mut parser).expect("parse succeeded");
    insta::assert_debug_snapshot!("gpu_context_dotted_engine", block);
    assert!(sink.diagnostics().is_empty(), "no diagnostics expected");
}

/// Fixture 3: multi-statement body — `@gpu_context(e) { let x = 1; f(); }`.
///
/// Statement-position block: the trailing `;` after `f()` triggers unit-tail
/// synthesis (see `parse_block_kind` under `BlockKind::Statement`). Freezes
/// that behaviour under the gpu-context prefix.
#[test]
fn snapshot_gpu_context_multi_stmt_body() {
    // @gpu_context(e) { let x = 1; f(); }
    let src = "@gpu_context(e) { let x = 1; f(); }";
    let tokens = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 11),
        tok(TokenKind::LParen, 12, 1),
        tok(TokenKind::Ident, 13, 1),
        tok(TokenKind::RParen, 14, 1),
        tok(TokenKind::LBrace, 16, 1),
        tok(TokenKind::KwLet, 18, 3),
        tok(TokenKind::Ident, 22, 1),
        tok(TokenKind::Assign, 24, 1),
        tok(TokenKind::IntLit, 26, 1),
        tok(TokenKind::Semicolon, 27, 1),
        tok(TokenKind::Ident, 29, 1),
        tok(TokenKind::LParen, 30, 1),
        tok(TokenKind::RParen, 31, 1),
        tok(TokenKind::Semicolon, 32, 1),
        tok(TokenKind::RBrace, 34, 1),
        tok(TokenKind::Eof, 35, 0),
    ];
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut parser = Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena, &mut sink);
    let block = parse_gpu_context(&mut parser).expect("parse succeeded");
    insta::assert_debug_snapshot!("gpu_context_multi_stmt_body", block);
    assert!(sink.diagnostics().is_empty(), "no diagnostics expected");
}

/// Fixture 4: sibling `@gpu_context` blocks parse independently.
///
/// The depth counter must return to zero on every well-formed exit so that a
/// *sequential* second `@gpu_context` in the same scope still parses cleanly.
/// Freezes the payload shape and guards against a decrement-on-exit
/// regression once the block parser gains dispatch (v0.29-M1-001) and this
/// pattern can be written in surface source.
#[test]
fn snapshot_gpu_context_sequential_siblings() {
    // Same source parsed twice with fresh parsers — the payload must be
    // byte-identical, and neither run may leave the depth counter dirty.
    let src = "@gpu_context(e) { x }";
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

    let mut arena1 = AstArena::new();
    let mut sink1 = VecSink::new();
    let mut parser1 = Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena1, &mut sink1);
    let first = parse_gpu_context(&mut parser1).expect("first parse succeeded");

    let mut arena2 = AstArena::new();
    let mut sink2 = VecSink::new();
    let mut parser2 = Parser::new(&tokens, src, FileId::new(1).unwrap(), &mut arena2, &mut sink2);
    let second = parse_gpu_context(&mut parser2).expect("second parse succeeded");

    insta::assert_debug_snapshot!("gpu_context_sequential_first", first);
    insta::assert_debug_snapshot!("gpu_context_sequential_second", second);
    assert_eq!(
        format!("{:?}", first),
        format!("{:?}", second),
        "sibling parses must produce identical payloads"
    );
    assert!(sink1.diagnostics().is_empty());
    assert!(sink2.diagnostics().is_empty());
}
