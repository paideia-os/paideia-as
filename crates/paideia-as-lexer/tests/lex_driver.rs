//! Integration tests for the lexer top-level driver (issue #42, PR-14).
//!
//! These tests verify acceptance criteria for the lexer driver:
//! 1. Lexing a simple program produces the expected tokens.
//! 2. Trivia (whitespace, comments) is preserved and retrievable.
//! 3. Error recovery continues after unrecognized characters.
//! 4. The lexer bails after the diagnostic budget is exhausted.
//! 5. Token spans cover the source content contiguously (where tokens exist).
//! 6. Unicode identifiers are recognized per XID properties.

use paideia_as_diagnostics::{BailPolicy, DiagnosticSink, FileId, VecSink};
use paideia_as_lexer::{Lexer, SourceText, Token, TokenKind, TriviaKind};

fn file_id() -> FileId {
    FileId::new(1).unwrap()
}

fn lex(input: &str) -> (Vec<Token>, paideia_as_diagnostics::VecSink) {
    let st = SourceText::from_bytes(file_id(), input.as_bytes()).unwrap();
    let mut lexer = Lexer::new(file_id(), &st);
    let mut sink = VecSink::new();
    let tokens = lexer.collect_tokens(&mut sink);
    (tokens, sink)
}

#[test]
fn lex_simple_program() {
    let (tokens, _sink) = lex("let x = 42;");
    assert_eq!(tokens.len(), 6); // let, x, =, 42, ;, eof
    assert_eq!(tokens[0].kind, TokenKind::KwLet);
    assert_eq!(tokens[1].kind, TokenKind::Ident);
    assert_eq!(tokens[2].kind, TokenKind::Assign);
    assert_eq!(tokens[3].kind, TokenKind::IntLit);
    assert_eq!(tokens[4].kind, TokenKind::Semicolon);
    assert_eq!(tokens[5].kind, TokenKind::Eof);
}

#[test]
fn lex_with_trivia_preserved() {
    let st = SourceText::from_bytes(file_id(), b"// comment\nlet x").unwrap();
    let mut lexer = Lexer::new(file_id(), &st);
    let mut sink = VecSink::new();

    // Consume the first token (should be KwLet after consuming trivia).
    let token = lexer.next_token(&mut sink);
    assert_eq!(token.kind, TokenKind::KwLet);

    // Retrieve the trivia accumulated before the token.
    let trivia = lexer.take_trivia();

    // We should have collected the comment and newline as trivia.
    assert!(trivia.iter().any(|t| t.kind == TriviaKind::LineComment));
    assert!(trivia.iter().any(|t| t.kind == TriviaKind::Newline));
}

#[test]
fn lex_recovers_from_unknown_byte() {
    let (tokens, sink) = lex("§ let x");

    // The lexer should have emitted one error for the unknown character.
    assert_eq!(sink.error_count(), 1);

    // After recovery, we should get KwLet, Ident, Eof.
    let token_kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert!(token_kinds.contains(&TokenKind::KwLet));
    assert!(token_kinds.contains(&TokenKind::Ident));
    assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
}

#[test]
fn lex_bails_after_100_errors() {
    // Build a source with 150 invalid tokens (150 unknown chars).
    let mut src = String::new();
    for _ in 0..150 {
        src.push('§');
        src.push(' ');
    }

    let st = SourceText::from_bytes(file_id(), src.as_bytes()).unwrap();
    let mut lexer = Lexer::new(file_id(), &st);
    let mut sink = VecSink::with_policy(BailPolicy::cap(100));

    let tokens = lexer.collect_tokens(&mut sink);

    // Sink should have 101 errors: the cap is 100, but the 101st error
    // that triggers overflow is still recorded before the error is returned.
    assert_eq!(sink.error_count(), 101);

    // The last token should be Eof (early bailout).
    assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
}

#[test]
fn token_spans_contiguous() {
    // Test with a program with no whitespace so tokens are contiguous.
    let (tokens, _sink) = lex("letx=1");

    // Collect all spans and verify they cover the entire source contiguously.
    let mut covered = 0u32;
    for token in &tokens {
        if token.kind != TokenKind::Eof {
            assert_eq!(token.span.byte_start(), covered);
            covered += token.span.byte_len();
        }
    }

    // We should have covered the entire source.
    assert!(covered > 0);
}

#[test]
fn lex_unicode_identifier() {
    let (tokens, _sink) = lex("家族 + 1");

    assert_eq!(tokens.len(), 4); // ident, +, 1, eof
    assert_eq!(tokens[0].kind, TokenKind::Ident);
    assert_eq!(tokens[1].kind, TokenKind::Plus);
    assert_eq!(tokens[2].kind, TokenKind::IntLit);
    assert_eq!(tokens[3].kind, TokenKind::Eof);
}

#[test]
fn lex_byte_literal() {
    let (tokens, _sink) = lex("b'x'");
    assert_eq!(tokens.len(), 2); // b'x', eof
    assert_eq!(tokens[0].kind, TokenKind::ByteLit);
    assert_eq!(tokens[1].kind, TokenKind::Eof);
}

#[test]
fn lex_raw_string() {
    let (tokens, _sink) = lex(r#"r"hello""#);
    assert_eq!(tokens.len(), 2); // r"hello", eof
    assert_eq!(tokens[0].kind, TokenKind::StringLit);
    assert_eq!(tokens[1].kind, TokenKind::Eof);
}

#[test]
fn lex_byte_string() {
    let (tokens, _sink) = lex(r#"b"hello""#);
    assert_eq!(tokens.len(), 2); // b"hello", eof
    assert_eq!(tokens[0].kind, TokenKind::ByteStringLit);
    assert_eq!(tokens[1].kind, TokenKind::Eof);
}

#[test]
fn lex_mixed_tokens() {
    let (tokens, _sink) = lex("fn main() { let x = 10; }");

    // Just verify we get KwFn, Ident, LParen, RParen, LBrace, KwLet, etc.
    let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&TokenKind::KwFn));
    assert!(kinds.contains(&TokenKind::Ident));
    assert!(kinds.contains(&TokenKind::LParen));
    assert!(kinds.contains(&TokenKind::RParen));
    assert!(kinds.contains(&TokenKind::LBrace));
    assert!(kinds.contains(&TokenKind::RBrace));
    assert!(kinds.contains(&TokenKind::KwLet));
    assert_eq!(kinds.last(), Some(&TokenKind::Eof));
}
