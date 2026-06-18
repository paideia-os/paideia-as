//! Memory reference parsing for assembly operands.
//!
//! Implements parsing of `[ addr_expr ]` memory references as specified in
//! §8 MemoryRef grammar.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a memory reference operand: `[ addr_expr ]`.
    ///
    /// Expects the current token to be `LBracket`. Parses one expression
    /// as the address, then expects `RBracket`. Allocates an
    /// `OperandMemoryRef` node wrapping the address expression.
    ///
    /// Returns the `NodeId` of the allocated operand on success.
    pub(crate) fn parse_memref(&mut self) -> Result<NodeId, ParseError> {
        let lbracket_tok = self.expect(TokenKind::LBracket)?;
        let span_start = lbracket_tok.span;

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
            ExprData::OperandMemoryRef { addr },
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
}
