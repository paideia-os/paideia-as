//! Postfix expression parsing: function calls, indexing, field access, method calls.
//!
//! Postfix operators bind tightest of all operators and are applied to an
//! existing lhs expression. This module handles:
//! - Function call: `f(args)` → ExprCall
//! - Indexing: `arr[i]` → ExprCall (phase-1 modeling)
//! - Field access: `obj.field` → ExprPostfix
//! - Method call: `obj.foo(args)` → ExprCall wrapping ExprPostfix
//! - Question postfix: `a?` → ExprPostfix

use paideia_as_ast::{ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Apply one postfix operation to `lhs` and return the new expression id.
    ///
    /// Dispatches on the current token kind:
    /// - **LParen**: parse function call `f(args)`, return ExprCall.
    /// - **LBracket**: parse indexing `arr[i]`, return ExprCall (phase-1 modeling).
    /// - **Dot**: parse field access `obj.field`, or method call `obj.foo(args)`.
    /// - **LBrace**: if lhs is a bare Ident-path, parse record constructor `Type { ... }`.
    /// - **Question**: parse postfix `?`, return ExprPostfix.
    ///
    /// For all cases, the resulting node's span covers from `lhs.span.start`
    /// to the closing token's end.
    pub(crate) fn parse_postfix(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        match self.peek() {
            None => Err(ParseError), // Shouldn't happen if called from Pratt loop
            Some(tok) => match tok.kind {
                TokenKind::LParen => self.parse_call(lhs),
                TokenKind::LBracket => self.parse_index(lhs),
                TokenKind::Dot => self.parse_field_or_method(lhs),
                TokenKind::LBrace => self.parse_record_cons_or_err(lhs),
                TokenKind::Question => self.parse_question_postfix(lhs),
                _ => Err(ParseError), // Shouldn't happen if called from postfix_bp
            },
        }
    }

    /// Try to parse a record constructor `Type { ... }` if lhs is a bare Ident path.
    /// Otherwise, return an error (since `{` is not a valid postfix operator for
    /// other expression types in phase-1).
    fn parse_record_cons_or_err(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        // Check if lhs is an ExprPath with a single segment (bare Ident).
        if let Some(lhs_data) = self.arena().expr_data(lhs) {
            if let ExprData::Path { segments } = lhs_data {
                if segments.len() == 1 {
                    // This is a bare Ident; try to parse as record constructor.
                    let type_name_id = segments[0];
                    let lhs_span = self.arena().get(lhs).map(|nd| nd.span).unwrap_or_else(|| {
                        self.peek()
                            .map(|t| t.span)
                            .unwrap_or_else(|| Span::new(self.file(), 0, 0))
                    });
                    return self.parse_record_cons_fields(type_name_id, lhs_span);
                }
            }
        }

        // Not a bare Ident; this is a parse error.
        Err(ParseError)
    }

    /// Parse the fields of a record constructor: `{ field1: expr1, ... }`.
    /// Called after confirming lhs is a bare Ident.
    pub(crate) fn parse_record_cons_fields(
        &mut self,
        type_name: NodeId,
        span_start: Span,
    ) -> Result<NodeId, ParseError> {
        // Expect opening brace
        if !self.at(TokenKind::LBrace) {
            return Err(ParseError);
        }
        self.bump(); // consume {

        let mut fields = Vec::new();

        // Parse fields: name : expr, name : expr, ...
        loop {
            // Check for closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }

            // Expect field name (Ident)
            let field_name_tok = self.expect(TokenKind::Ident)?;
            let field_name_id = self.arena_mut().alloc(NodeKind::Ident, field_name_tok.span);

            // Expect colon
            if !self.at(TokenKind::Colon) {
                return Err(ParseError);
            }
            self.bump(); // consume :

            // Parse field value expression (full expression with operators)
            let field_value = self.parse_expr()?;

            fields.push((field_name_id, field_value));

            // Check for comma or closing brace
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump(); // consume comma

            // Allow trailing comma before closing brace
            if self.at(TokenKind::RBrace) {
                break;
            }
        }

        // Expect closing brace
        if !self.at(TokenKind::RBrace) {
            return Err(ParseError);
        }
        let rbrace_tok = self.bump().unwrap();

        // Compute span
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            rbrace_tok.span.byte_start() + rbrace_tok.span.byte_len() - span_start.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprRecordCons,
            span,
            ExprData::RecordCons { type_name, fields },
        ))
    }

    /// Parse a function call: `f(args)` → ExprCall.
    ///
    /// Consumes LParen, parses comma-separated args, expects RParen.
    /// Returns ExprCall with callee=lhs and args=parsed_args.
    fn parse_call(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        let lparen_tok = self.bump().expect("LParen already peeked");
        let lparen_span = lparen_tok.span;

        // Parse comma-separated arguments
        let mut args = Vec::new();

        // Check for empty argument list
        if !self.at(TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);

                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump(); // consume comma

                // Trailing comma is OK — check for close paren
                if self.at(TokenKind::RParen) {
                    break;
                }
            }
        }

        // Expect closing paren
        if !self.at(TokenKind::RParen) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };

            let diag = Diagnostic::error(p_code(101))
                .message("mismatched delimiter: expected `)`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);

            return Err(ParseError);
        }

        let rparen_tok = self.bump().expect("RParen already checked");
        let rparen_span = rparen_tok.span;

        // Compute span from lhs start to rparen end
        let lhs_span = self
            .arena()
            .get(lhs)
            .map(|nd| nd.span)
            .unwrap_or(lparen_span);
        let call_span = Span::new(
            lhs_span.file(),
            lhs_span.byte_start(),
            rparen_span.byte_start() + rparen_span.byte_len() - lhs_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprCall,
            call_span,
            ExprData::Call { callee: lhs, args },
        ))
    }

    /// Parse indexing: `arr[i]` → ExprCall.
    ///
    /// Phase-1 modeling: indexing is represented as a unary call to distinguish
    /// from function calls in the elaborator. This is structurally identical to
    /// how a real parser would lower it after desugaring.
    ///
    /// Consumes LBracket, parses one expression, expects RBracket.
    /// Returns ExprCall with callee=lhs and args=vec![index_expr].
    fn parse_index(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        let lbracket_tok = self.bump().expect("LBracket already peeked");
        let lbracket_span = lbracket_tok.span;

        // Parse the index expression
        let index_expr = self.parse_expr()?;

        // Expect closing bracket
        if !self.at(TokenKind::RBracket) {
            let span = if let Some(tok) = self.peek() {
                tok.span
            } else {
                Span::new(self.file(), 0, 0)
            };

            let diag = Diagnostic::error(p_code(101))
                .message("mismatched delimiter: expected `]`".to_string())
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);

            return Err(ParseError);
        }

        let rbracket_tok = self.bump().expect("RBracket already checked");
        let rbracket_span = rbracket_tok.span;

        // Compute span from lhs start to rbracket end
        let lhs_span = self
            .arena()
            .get(lhs)
            .map(|nd| nd.span)
            .unwrap_or(lbracket_span);
        let index_span = Span::new(
            lhs_span.file(),
            lhs_span.byte_start(),
            rbracket_span.byte_start() + rbracket_span.byte_len() - lhs_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprCall,
            index_span,
            ExprData::Call {
                callee: lhs,
                args: vec![index_expr],
            },
        ))
    }

    /// Parse field access or method call: `obj.field` or `obj.foo(args)`.
    ///
    /// Consumes Dot, expects an Ident. If the next token after the Ident is
    /// LParen, this is a method call: synthesize a field-access ExprPostfix,
    /// then parse the call with that as callee. Otherwise just allocate a
    /// field-access ExprPostfix.
    fn parse_field_or_method(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        let dot_tok = self.bump().expect("Dot already peeked");
        let dot_span = dot_tok.span;

        // Expect identifier
        let ident_tok = self.expect(TokenKind::Ident)?;
        let ident_span = ident_tok.span;
        let field_ident = self.arena_mut().alloc(NodeKind::Ident, ident_span);

        // Compute span for field-access node
        let lhs_span = self.arena().get(lhs).map(|nd| nd.span).unwrap_or(dot_span);
        let field_access_span = Span::new(
            lhs_span.file(),
            lhs_span.byte_start(),
            ident_span.byte_start() + ident_span.byte_len() - lhs_span.byte_start(),
        );

        // Allocate the field-access ExprFieldAccess
        let field_access = self.arena_mut().alloc_expr(
            NodeKind::ExprFieldAccess,
            field_access_span,
            ExprData::FieldAccess {
                receiver: lhs,
                field: field_ident,
            },
        );

        // Check if this is a method call (LParen follows)
        if self.at(TokenKind::LParen) {
            // This is a method call: parse_call with field_access as callee
            self.parse_call(field_access)
        } else {
            // Just field access
            Ok(field_access)
        }
    }

    /// Parse question postfix: `a?` → ExprPostfix.
    ///
    /// Consumes Question, allocates op node as Placeholder, wraps in ExprPostfix.
    fn parse_question_postfix(&mut self, lhs: NodeId) -> Result<NodeId, ParseError> {
        let question_tok = self.bump().expect("Question already peeked");
        let question_span = question_tok.span;
        let op_node = self.arena_mut().alloc(NodeKind::Placeholder, question_span);

        // Compute span from lhs start to question end
        let lhs_span = self
            .arena()
            .get(lhs)
            .map(|nd| nd.span)
            .unwrap_or(question_span);
        let postfix_span = Span::new(
            lhs_span.file(),
            lhs_span.byte_start(),
            question_span.byte_start() + question_span.byte_len() - lhs_span.byte_start(),
        );

        Ok(self.arena_mut().alloc_expr(
            NodeKind::ExprPostfix,
            postfix_span,
            ExprData::Postfix {
                expr: lhs,
                op: op_node,
            },
        ))
    }
}

// Helper to get mutable sink access (requires Parser to expose it)
// For now, we'll add a method to Parser

/// Construct a P-category diagnostic code at the given number.
fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    /// Helper: create a token at a given byte offset with length 1.
    fn tok(kind: TokenKind, byte_start: u32) -> Token {
        Token::new(kind, Span::new(FileId::new(1).unwrap(), byte_start, 1))
    }

    /// Helper: parse a token stream and return (arena, root, diagnostics).
    fn parse(tokens: Vec<Token>) -> (AstArena, Result<NodeId, ParseError>, Vec<Diagnostic>) {
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
    fn function_call_no_args() {
        // f()
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // f
            tok(TokenKind::LParen, 1), // (
            tok(TokenKind::RParen, 2), // )
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "no diagnostics expected");
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprCall);
    }

    #[test]
    fn function_call_three_args() {
        // f(a, b, c)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // f
            tok(TokenKind::LParen, 1), // (
            tok(TokenKind::Ident, 2),  // a
            tok(TokenKind::Comma, 3),  // ,
            tok(TokenKind::Ident, 4),  // b
            tok(TokenKind::Comma, 5),  // ,
            tok(TokenKind::Ident, 6),  // c
            tok(TokenKind::RParen, 7), // )
            tok(TokenKind::Eof, 8),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprCall);
    }

    #[test]
    fn function_call_trailing_comma() {
        // f(a, b,)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // f
            tok(TokenKind::LParen, 1), // (
            tok(TokenKind::Ident, 2),  // a
            tok(TokenKind::Comma, 3),  // ,
            tok(TokenKind::Ident, 4),  // b
            tok(TokenKind::Comma, 5),  // ,
            tok(TokenKind::RParen, 6), // )
            tok(TokenKind::Eof, 7),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0, "trailing comma should be accepted");
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprCall);
    }

    #[test]
    fn function_call_missing_close_emits_p0101() {
        // f(a, b (EOF)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // f
            tok(TokenKind::LParen, 1), // (
            tok(TokenKind::Ident, 2),  // a
            tok(TokenKind::Comma, 3),  // ,
            tok(TokenKind::Ident, 4),  // b
            tok(TokenKind::Eof, 5),
        ];
        let (_arena, result, diags) = parse(tokens);

        assert!(result.is_err());
        assert!(!diags.is_empty());
        let diag = &diags[diags.len() - 1]; // Get last diagnostic
        assert_eq!(diag.code().number(), 101);
    }

    #[test]
    fn indexing() {
        // arr[i]
        let tokens = vec![
            tok(TokenKind::Ident, 0),    // arr
            tok(TokenKind::LBracket, 1), // [
            tok(TokenKind::Ident, 2),    // i
            tok(TokenKind::RBracket, 3), // ]
            tok(TokenKind::Eof, 4),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprCall,
            "indexing modeled as ExprCall"
        );
    }

    #[test]
    fn chained_index_field() {
        // arr[i].field
        let tokens = vec![
            tok(TokenKind::Ident, 0),    // arr
            tok(TokenKind::LBracket, 1), // [
            tok(TokenKind::Ident, 2),    // i
            tok(TokenKind::RBracket, 3), // ]
            tok(TokenKind::Dot, 4),      // .
            tok(TokenKind::Ident, 5),    // field
            tok(TokenKind::Eof, 6),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.kind,
            NodeKind::ExprFieldAccess,
            "outer is field access (Dot)"
        );
    }

    #[test]
    fn field_access() {
        // obj.field
        let tokens = vec![
            tok(TokenKind::Ident, 0), // obj
            tok(TokenKind::Dot, 1),   // .
            tok(TokenKind::Ident, 2), // field
            tok(TokenKind::Eof, 3),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprFieldAccess);
    }

    #[test]
    fn method_call() {
        // obj.foo(a, b)
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // obj
            tok(TokenKind::Dot, 1),    // .
            tok(TokenKind::Ident, 2),  // foo
            tok(TokenKind::LParen, 3), // (
            tok(TokenKind::Ident, 4),  // a
            tok(TokenKind::Comma, 5),  // ,
            tok(TokenKind::Ident, 6),  // b
            tok(TokenKind::RParen, 7), // )
            tok(TokenKind::Eof, 8),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprCall, "method call is ExprCall");
    }

    #[test]
    fn question_postfix() {
        // a?
        let tokens = vec![
            tok(TokenKind::Ident, 0),    // a
            tok(TokenKind::Question, 1), // ?
            tok(TokenKind::Eof, 2),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprPostfix);
    }

    #[test]
    fn chained_calls() {
        // f()()
        let tokens = vec![
            tok(TokenKind::Ident, 0),  // f
            tok(TokenKind::LParen, 1), // (
            tok(TokenKind::RParen, 2), // )
            tok(TokenKind::LParen, 3), // (
            tok(TokenKind::RParen, 4), // )
            tok(TokenKind::Eof, 5),
        ];
        let (arena, result, diags) = parse(tokens);

        assert_eq!(diags.len(), 0);
        assert!(result.is_ok());
        let root = result.unwrap();
        let node = arena.get(root).unwrap();
        assert_eq!(node.kind, NodeKind::ExprCall, "outer call is ExprCall");
    }
}
