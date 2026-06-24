//! Tests for lexing the #![...] inner attribute sequence
//!
//! Validates that the lexer correctly tokenizes the Hash + Bang + LBracket
//! sequence used in inner attributes, without requiring new token types.

use paideia_as_diagnostics::{FileId, VecSink};
use paideia_as_lexer::{Lexer, SourceText, TokenKind};

fn lex(input: &str) -> (Vec<paideia_as_lexer::Token>, VecSink) {
    let file = FileId::new(1).unwrap();
    let st = SourceText::from_bytes(file, input.as_bytes()).unwrap();
    let mut lexer = Lexer::new(file, &st);
    let mut sink = VecSink::new();
    let tokens = lexer.collect_tokens(&mut sink);
    (tokens, sink)
}

#[test]
fn hash_bang_lbracket_sequence() {
    // Test: #![
    // Should lex as Hash, Bang, LBracket (existing tokens)
    let (tokens, _sink) = lex("#![");

    assert!(tokens.len() >= 3, "should have at least Hash, Bang, LBracket");
    assert_eq!(
        tokens[0].kind,
        TokenKind::Hash,
        "first token should be Hash"
    );
    assert_eq!(tokens[1].kind, TokenKind::Bang, "second token should be Bang");
    assert_eq!(
        tokens[2].kind,
        TokenKind::LBracket,
        "third token should be LBracket"
    );
}

#[test]
fn inner_attr_full_lex() {
    // Test: #![bits = 32]
    // Should lex as: Hash, Bang, LBracket, Ident("bits"), Assign, IntLit("32"), RBracket
    let (tokens, _sink) = lex("#![bits = 32]");

    assert!(
        tokens.len() >= 7,
        "should have at least 7 tokens (Hash, Bang, LBracket, Ident, Assign, IntLit, RBracket)"
    );

    assert_eq!(
        tokens[0].kind,
        TokenKind::Hash,
        "first token should be Hash"
    );
    assert_eq!(tokens[1].kind, TokenKind::Bang, "second token should be Bang");
    assert_eq!(
        tokens[2].kind,
        TokenKind::LBracket,
        "third token should be LBracket"
    );
    assert_eq!(
        tokens[3].kind,
        TokenKind::Ident,
        "fourth token should be Ident"
    );
    assert_eq!(
        tokens[4].kind,
        TokenKind::Assign,
        "fifth token should be Assign"
    );
    assert_eq!(
        tokens[5].kind,
        TokenKind::IntLit,
        "sixth token should be IntLit"
    );
    assert_eq!(
        tokens[6].kind,
        TokenKind::RBracket,
        "seventh token should be RBracket"
    );
}
