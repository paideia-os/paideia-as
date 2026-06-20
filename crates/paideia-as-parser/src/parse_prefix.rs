//! Prefix expression parsing: unary operators like !, -, &, *, and linear consume marker.
//!
//! Prefix operators bind tighter than all infix operators and consume a single
//! operand to their right. This module handles all prefix operator variations
//! uniformly: bump, allocate op node (Placeholder), recurse via parse_expr_bp,
//! wrap in ExprPrefix.

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::Span;

use crate::parser::{ParseError, Parser};
use crate::precedence::prefix_bp;

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a prefix operator and its operand.
    ///
    /// Handles all prefix operators uniformly: bump the operator token,
    /// allocate an operator node (Placeholder), recurse via parse_expr_bp
    /// with the prefix binding power, then wrap in ExprPrefix.
    ///
    /// Special handling:
    /// - `&` (Amp) in expression position: creates Borrow { expr, mutable: false }
    /// - `&mut` (Amp followed by KwMut) in expression position: creates Borrow { expr, mutable: true }
    /// - `*` (Star) in expression position: creates Deref { expr }
    /// - Other prefix operators: `!`, `-`, `$` → ExprPrefix as before.
    ///
    /// Returns appropriately typed prefix expression node.
    pub(crate) fn parse_prefix(&mut self) -> Result<NodeId, ParseError> {
        let op_tok = self
            .bump()
            .expect("parse_prefix called only when prefix_bp succeeded, so peek is Some");
        let op_span = op_tok.span;

        // Look up the binding power for this prefix operator
        let op_bp =
            prefix_bp(op_tok.kind).expect("parse_prefix called only when prefix_bp succeeded");

        // Special handling for & (borrow) and * (deref) in expression position
        match op_tok.kind {
            paideia_as_lexer::TokenKind::Amp => {
                // Check for &mut
                let mutable = if self.at(paideia_as_lexer::TokenKind::KwMut) {
                    self.bump();
                    true
                } else {
                    false
                };

                // Recursively parse the operand at the prefix binding power
                let operand = self.parse_expr_bp(op_bp)?;

                // Get the span of the operand from the arena
                let operand_span = self
                    .arena()
                    .get(operand)
                    .map(|nd| nd.span)
                    .unwrap_or(op_span);

                // Compute span from op start to operand end
                let borrow_span = Span::new(
                    op_span.file(),
                    op_span.byte_start(),
                    operand_span.byte_start() + operand_span.byte_len() - op_span.byte_start(),
                );

                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprBorrow,
                    borrow_span,
                    ExprData::Borrow {
                        expr: operand,
                        mutable,
                    },
                ))
            }
            paideia_as_lexer::TokenKind::Star => {
                // Recursively parse the operand at the prefix binding power
                let operand = self.parse_expr_bp(op_bp)?;

                // Get the span of the operand from the arena
                let operand_span = self
                    .arena()
                    .get(operand)
                    .map(|nd| nd.span)
                    .unwrap_or(op_span);

                // Compute span from op start to operand end
                let deref_span = Span::new(
                    op_span.file(),
                    op_span.byte_start(),
                    operand_span.byte_start() + operand_span.byte_len() - op_span.byte_start(),
                );

                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprDeref,
                    deref_span,
                    ExprData::Deref { expr: operand },
                ))
            }
            _ => {
                // For other prefix operators (!,  -, $): use generic ExprPrefix

                // Allocate operator node as Placeholder
                let op_node = self.arena_mut().alloc(NodeKind::Placeholder, op_span);

                // Recursively parse the operand at the prefix binding power
                let operand = self.parse_expr_bp(op_bp)?;

                // Get the span of the operand from the arena
                let operand_span = self
                    .arena()
                    .get(operand)
                    .map(|nd| nd.span)
                    .unwrap_or(op_span);

                // Compute span from op start to operand end
                let prefix_span = Span::new(
                    op_span.file(),
                    op_span.byte_start(),
                    operand_span.byte_start() + operand_span.byte_len() - op_span.byte_start(),
                );

                Ok(self.arena_mut().alloc_expr(
                    NodeKind::ExprPrefix,
                    prefix_span,
                    ExprData::Prefix {
                        op: op_node,
                        expr: operand,
                    },
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::{Token, TokenKind};

    /// Helper: create a token at a given byte offset with length 1.
    fn tok(kind: TokenKind, byte_start: u32) -> Token {
        Token::new(kind, Span::new(FileId::new(1).unwrap(), byte_start, 1))
    }

    /// Helper: parse a token stream and return (arena, root, diagnostics).
    fn parse(
        tokens: Vec<Token>,
    ) -> (
        AstArena,
        Result<NodeId, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let root = {
            let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
            p.parse_expr()
        };
        let diags = sink.diagnostics().to_vec();
        (arena, root, diags)
    }

    #[test]
    fn prefix_bang() {
        // !a
        let tokens = vec![
            tok(TokenKind::Bang, 0),  // !
            tok(TokenKind::Ident, 1), // a
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPrefix);
    }

    #[test]
    fn prefix_minus() {
        // -a
        let tokens = vec![
            tok(TokenKind::Minus, 0), // -
            tok(TokenKind::Ident, 1), // a
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPrefix);
    }

    #[test]
    fn prefix_linear_consume_dollar() {
        // $cap (LinearMark as prefix)
        let tokens = vec![
            tok(TokenKind::LinearMark, 0), // $
            tok(TokenKind::Ident, 1),      // cap
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprPrefix,
            "LinearMark is a recognized prefix operator"
        );
    }

    #[test]
    fn parse_borrow_immutable() {
        // &x (immutable borrow)
        let tokens = vec![
            tok(TokenKind::Amp, 0),   // &
            tok(TokenKind::Ident, 1), // x
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprBorrow,
            "&x should parse to ExprBorrow"
        );

        // Verify it's immutable
        if let Some(ExprData::Borrow { mutable, .. }) = arena.expr_data(root) {
            assert!(!mutable, "immutable borrow should have mutable=false");
        } else {
            panic!("Expected Borrow variant");
        }
    }

    #[test]
    fn parse_borrow_mutable() {
        // &mut x (mutable borrow)
        let tokens = vec![
            tok(TokenKind::Amp, 0),   // &
            tok(TokenKind::KwMut, 1), // mut
            tok(TokenKind::Ident, 2), // x
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprBorrow,
            "&mut x should parse to ExprBorrow"
        );

        // Verify it's mutable
        if let Some(ExprData::Borrow { mutable, .. }) = arena.expr_data(root) {
            assert!(mutable, "mutable borrow should have mutable=true");
        } else {
            panic!("Expected Borrow variant");
        }
    }

    #[test]
    fn parse_deref() {
        // *r (dereference)
        let tokens = vec![
            tok(TokenKind::Star, 0),  // *
            tok(TokenKind::Ident, 1), // r
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprDeref,
            "*r should parse to ExprDeref"
        );
    }

    #[test]
    fn parse_reborrow_chain() {
        // &*r (reborrow pattern: dereference then re-borrow)
        let tokens = vec![
            tok(TokenKind::Amp, 0),   // &
            tok(TokenKind::Star, 1),  // *
            tok(TokenKind::Ident, 2), // r
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprBorrow,
            "outer & should create ExprBorrow"
        );

        // Verify the inner deref
        if let Some(ExprData::Borrow {
            expr: inner_id,
            mutable: false,
        }) = arena.expr_data(root)
        {
            let inner_node = arena.get(*inner_id).unwrap();
            assert_eq!(
                inner_node.kind,
                NodeKind::ExprDeref,
                "inner * should create ExprDeref"
            );
        } else {
            panic!("Expected outer Borrow wrapping Deref");
        }
    }

    #[test]
    fn nested_prefix() {
        // !-a => outer ! wraps inner -
        let tokens = vec![
            tok(TokenKind::Bang, 0),  // !
            tok(TokenKind::Minus, 1), // -
            tok(TokenKind::Ident, 2), // a
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPrefix, "outer prefix operator");
    }
}
