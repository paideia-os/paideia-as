//! Memory reference parsing for assembly operands.
//!
//! Implements parsing of `[ addr_expr ]` memory references as specified in
//! §8 MemoryRef grammar. Also supports segment prefix syntax `[fs:...]` and
//! `[gs:...]` (PA-R13-002).

use paideia_as_ast::{ExprData, NodeId, NodeKind, SegPrefix};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a memory reference operand: `[ addr_expr ]` or `[ fs:addr_expr ]` or `[ gs:addr_expr ]`.
    ///
    /// Expects the current token to be `LBracket`. Optionally parses a segment prefix
    /// (fs/gs followed by colon), then parses the address expression, and expects
    /// `RBracket`. Allocates an `OperandMemoryRef` node wrapping the address expression.
    ///
    /// Returns the `NodeId` of the allocated operand on success.
    pub(crate) fn parse_memref(&mut self) -> Result<NodeId, ParseError> {
        let lbracket_tok = self.expect(TokenKind::LBracket)?;
        let span_start = lbracket_tok.span;

        // Peek for segment override: only `fs:` and `gs:` are accepted.
        let segment = if self.at(TokenKind::Ident) {
            let ident_text = self.peek_ident_text().unwrap_or("");
            if (ident_text == "fs" || ident_text == "gs") && self.peek_at(1).map(|t| t.kind) == Some(TokenKind::Colon) {
                let seg = if ident_text == "fs" { SegPrefix::Fs } else { SegPrefix::Gs };
                self.bump(); // consume ident
                self.bump(); // consume colon
                Some(seg)
            } else {
                None
            }
        } else {
            None
        };

        // Parse the address expression
        let addr = self.parse_expr()?;

        // Expect closing bracket
        let rbracket_tok = self.expect(TokenKind::RBracket)?;
        let span_end = rbracket_tok.span;

        let span = paideia_as_diagnostics::Span::new(
            span_start.file(),
            span_start.byte_start(),
            span_end.byte_start() + span_end.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::OperandMemoryRef,
            span,
            ExprData::OperandMemoryRef { segment, addr },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, span(byte_start, byte_len))
    }

    #[test]
    fn simple_memref() {
        // `[ x ]`
        let toks = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::Ident, 2, 1), // "x"
            tok(TokenKind::RBracket, 4, 1),
        ];
        let source = "[ x ]";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_memref();
        assert!(result.is_ok());
        let memref_id = result.unwrap();

        // Verify it's an OperandMemoryRef node
        let node = arena.get(memref_id).unwrap();
        assert_eq!(node.kind, NodeKind::OperandMemoryRef);
    }

    #[test]
    fn arith_memref() {
        // `[ rbp - 8 ]`
        let toks = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::Ident, 2, 3), // "rbp"
            tok(TokenKind::Minus, 6, 1),
            tok(TokenKind::IntLit, 8, 1), // "8"
            tok(TokenKind::RBracket, 10, 1),
        ];
        let source = "[ rbp - 8 ]";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_memref();
        assert!(result.is_ok());
        let memref_id = result.unwrap();

        // Verify it's an OperandMemoryRef node
        let node = arena.get(memref_id).unwrap();
        assert_eq!(node.kind, NodeKind::OperandMemoryRef);
    }

    #[test]
    fn symbol_only_memref() {
        // `[ symbol ]` - bare symbol (not a register)
        let toks = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::Ident, 2, 6), // "symbol"
            tok(TokenKind::RBracket, 9, 1),
        ];
        let source = "[ symbol ]";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_memref();
        assert!(result.is_ok());
        let memref_id = result.unwrap();

        // Verify it's an OperandMemoryRef node with either Ident or ExprPath inside
        let node = arena.get(memref_id).unwrap();
        assert_eq!(node.kind, NodeKind::OperandMemoryRef);

        if let Some(ExprData::OperandMemoryRef { segment: _, addr }) = arena.expr_data(memref_id) {
            let addr_node = arena.get(*addr).unwrap();
            // Parser can resolve bare identifiers as either Ident or ExprPath
            // both are valid for symbol references
            assert!(
                matches!(addr_node.kind, NodeKind::Ident | NodeKind::ExprPath),
                "expected Ident or ExprPath, got {:?}",
                addr_node.kind
            );
        } else {
            panic!("Expected OperandMemoryRef data");
        }
    }

    #[test]
    fn symbol_plus_intlit_memref() {
        // `[ symbol + 8 ]`
        let toks = vec![
            tok(TokenKind::LBracket, 0, 1),
            tok(TokenKind::Ident, 2, 6), // "symbol"
            tok(TokenKind::Plus, 9, 1),
            tok(TokenKind::IntLit, 11, 1), // "8"
            tok(TokenKind::RBracket, 13, 1),
        ];
        let source = "[ symbol + 8 ]";
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(
            &toks,
            source,
            FileId::new(1).unwrap(),
            &mut arena,
            &mut sink,
        );

        let result = p.parse_memref();
        assert!(result.is_ok());
        let memref_id = result.unwrap();

        // Verify it's an OperandMemoryRef node with ExprInfix inside
        let node = arena.get(memref_id).unwrap();
        assert_eq!(node.kind, NodeKind::OperandMemoryRef);

        if let Some(ExprData::OperandMemoryRef { segment: _, addr }) = arena.expr_data(memref_id) {
            let addr_node = arena.get(*addr).unwrap();
            assert_eq!(addr_node.kind, NodeKind::ExprInfix);
        } else {
            panic!("Expected OperandMemoryRef data");
        }
    }
}
