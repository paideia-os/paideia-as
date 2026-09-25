//! Lambda-context sub-parser.
//!
//! Grammar (informally):
//!
//! ```text
//! lambda_block ::= '{' '|' params '|' expr '}'
//!               | '{' expr '}'                    -- no-param form (thunk)
//! expr         ::= let_expr | match_expr | binop_expr
//! let_expr     ::= 'let' IDENT '=' expr 'in' expr
//! match_expr   ::= 'match' expr '{' arm (',' arm)* '}'
//! arm          ::= expr ('if' expr)? '=>' expr
//! binop_expr   ::= comparison
//! comparison   ::= additive (('==' | '!=' | '<' | '<=' | '>' | '>='
//!                            | 'and' | 'or') additive)*
//! additive     ::= multiplicative (('+' | '-') multiplicative)*
//! multiplicative ::= unary (('*' | '/') unary)*
//! unary        ::= ('-' | 'not') unary | postfix
//! postfix      ::= atom ('.' IDENT | '(' args? ')')*
//! atom         ::= IDENT | NUMBER | STR | 'true' | 'false'
//!               | '(' expr ')'
//! ```
//!
//! Called from Pipeline context when a bare `{` is seen as a stage
//! argument. The lexer stamps the opening `{` and its inner tokens with
//! `Context::Lambda`.

use paideia_as_shell_lex::{Context, TokenKind};

use crate::ast::SyntaxNode;
use crate::parser::{ParseError, Parser};

use super::datalog;

/// Consume `{ |p1 p2 …| body }` or `{ body }` and produce a
/// [`SyntaxNode::Lambda`].
pub fn parse_lambda_block(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let lbrace = match p.peek() {
        Some(t) if matches!(t.kind, TokenKind::LBrace) && matches!(t.context, Context::Lambda) => {
            p.bump().unwrap()
        }
        _ => return Err(p.err_expected("`{` opening lambda block")),
    };
    let lbrace_span = p.token_span(lbrace);
    p.enter_nesting(Some(lbrace))?;

    // Optional `|param1 param2 …|` header.
    let mut params = Vec::new();
    if let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::Pipe) {
            p.bump();
            loop {
                match p.peek() {
                    Some(t) if matches!(t.kind, TokenKind::Pipe) => {
                        p.bump();
                        break;
                    }
                    Some(t) => match &t.kind {
                        TokenKind::Ident(name) => {
                            params.push(name.clone());
                            p.bump();
                            // Allow optional `,` between params.
                            if let Some(tt) = p.peek() {
                                if matches!(tt.kind, TokenKind::Comma) {
                                    p.bump();
                                }
                            }
                        }
                        _ => {
                            p.exit_nesting();
                            return Err(p.err_expected("lambda parameter name"));
                        }
                    },
                    None => {
                        p.exit_nesting();
                        return Err(p.err_expected("`|` closing lambda parameters"));
                    }
                }
            }
        }
    }

    // Body — one expression. Skip leading newlines (whitespace).
    skip_lambda_newlines(p);
    let body = parse_expr(p)?;
    skip_lambda_newlines(p);

    let rbrace = match p.peek() {
        Some(t) if matches!(t.kind, TokenKind::RBrace) => p.bump().unwrap(),
        _ => {
            p.exit_nesting();
            return Err(p.err_expected("`}` closing lambda block"));
        }
    };
    p.exit_nesting();
    let span = lbrace_span.union(p.token_span(rbrace));
    Ok(SyntaxNode::Lambda {
        params,
        body: Box::new(body),
        span,
    })
}

/// Skip newline tokens (transparent inside a lambda body).
fn skip_lambda_newlines(p: &mut Parser<'_>) {
    while let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::Newline) {
            p.bump();
        } else {
            break;
        }
    }
}

/// Top-level lambda expression: dispatch to comparison-precedence
/// chain. `let` and `match` handled at the outer layer.
fn parse_expr(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    parse_comparison(p)
}

fn parse_comparison(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let mut lhs = parse_additive(p)?;
    while let Some(t) = p.peek() {
        let op_str = match &t.kind {
            TokenKind::Op(s)
                if matches!(
                    s.as_str(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or"
                ) =>
            {
                s.clone()
            }
            _ => break,
        };
        p.bump();
        let rhs = parse_additive(p)?;
        let span = lhs.span().union(rhs.span());
        lhs = SyntaxNode::BinOp {
            op: op_str,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        };
    }
    Ok(lhs)
}

fn parse_additive(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let mut lhs = parse_multiplicative(p)?;
    while let Some(t) = p.peek() {
        let op_str = match &t.kind {
            TokenKind::Op(s) if s == "+" || s == "-" => s.clone(),
            _ => break,
        };
        p.bump();
        let rhs = parse_multiplicative(p)?;
        let span = lhs.span().union(rhs.span());
        lhs = SyntaxNode::BinOp {
            op: op_str,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        };
    }
    Ok(lhs)
}

fn parse_multiplicative(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let mut lhs = parse_unary(p)?;
    while let Some(t) = p.peek() {
        let op_str = match &t.kind {
            TokenKind::Op(s) if s == "*" || s == "/" => s.clone(),
            _ => break,
        };
        p.bump();
        let rhs = parse_unary(p)?;
        let span = lhs.span().union(rhs.span());
        lhs = SyntaxNode::BinOp {
            op: op_str,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        };
    }
    Ok(lhs)
}

fn parse_unary(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    if let Some(t) = p.peek() {
        if let TokenKind::Op(s) = &t.kind {
            if s == "-" || s == "not" {
                let op = s.clone();
                let op_tok = p.bump().unwrap();
                let op_span = p.token_span(op_tok);
                let inner = parse_unary(p)?;
                let span = op_span.union(inner.span());
                return Ok(SyntaxNode::UnaryOp {
                    op,
                    inner: Box::new(inner),
                    span,
                });
            }
        }
    }
    parse_postfix(p)
}

fn parse_postfix(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let mut acc = parse_atom(p)?;
    loop {
        let Some(t) = p.peek() else { break };
        match &t.kind {
            TokenKind::Dot => {
                // Field access: `.ident`.
                let Some(next) = p.peek_at(1) else { break };
                let TokenKind::Ident(field) = &next.kind else {
                    break;
                };
                let field = field.clone();
                let dot = p.bump().unwrap();
                let dot_span = p.token_span(dot);
                let ident = p.bump().unwrap();
                let ident_span = p.token_span(ident);
                let span = acc.span().union(dot_span).union(ident_span);
                acc = SyntaxNode::FieldAccess {
                    base: Box::new(acc),
                    field,
                    span,
                };
            }
            // Shell-uniform application: `ident { |args| body }` inside
            // a lambda body means "call ident with the lambda block as
            // an arg". Only fires when the accumulator is a Var (bare
            // identifier); accepts a trailing `{...}` or `datalog {...}`
            // as the applied arg. Wraps into `Cmd` so downstream layers
            // see the same shape as pipeline-level command invocation.
            TokenKind::LBrace
                if matches!(t.context, Context::Lambda)
                    && matches!(acc, SyntaxNode::Var { .. }) =>
            {
                let arg = parse_lambda_block(p)?;
                let span = acc.span().union(arg.span());
                acc = SyntaxNode::Cmd {
                    name: Box::new(acc),
                    args: vec![arg],
                    span,
                };
            }
            TokenKind::DatalogKw if matches!(acc, SyntaxNode::Var { .. }) => {
                let arg = datalog::parse_datalog_block(p)?;
                let span = acc.span().union(arg.span());
                acc = SyntaxNode::Cmd {
                    name: Box::new(acc),
                    args: vec![arg],
                    span,
                };
            }
            _ => break,
        }
    }
    Ok(acc)
}

fn parse_atom(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let Some(t) = p.peek() else {
        return Err(p.err_expected("expression atom"));
    };
    let span = p.token_span(t);
    match &t.kind {
        TokenKind::Ident(name) => {
            let name_clone = name.clone();
            p.bump();
            match name_clone.as_str() {
                "true" => Ok(SyntaxNode::LitBool { value: true, span }),
                "false" => Ok(SyntaxNode::LitBool { value: false, span }),
                _ => Ok(SyntaxNode::Var { name: name_clone, span }),
            }
        }
        TokenKind::Number(s) => {
            let raw = s.clone();
            p.bump();
            let cleaned: String = raw.chars().filter(|&c| c != '_').collect();
            let value = cleaned.parse::<i64>().map_err(|_| {
                crate::parser::ParseError {
                    kind: crate::parser::ParseErrorKind::NumericOverflow(raw.clone()),
                    original_span: span.original,
                    nfc_span: span.nfc,
                }
            })?;
            Ok(SyntaxNode::LitInt { value, span })
        }
        TokenKind::Str(s) => {
            let value = s.clone();
            p.bump();
            Ok(SyntaxNode::LitStr { value, span })
        }
        TokenKind::LParen => {
            let lparen = p.bump().unwrap();
            let lparen_span = p.token_span(lparen);
            p.enter_nesting(Some(lparen))?;
            let inner = parse_expr(p)?;
            let rparen = match p.peek() {
                Some(t) if matches!(t.kind, TokenKind::RParen) => p.bump().unwrap(),
                _ => {
                    p.exit_nesting();
                    return Err(p.err_expected("`)` closing group"));
                }
            };
            p.exit_nesting();
            let span = lparen_span.union(p.token_span(rparen));
            Ok(SyntaxNode::Group {
                inner: Box::new(inner),
                span,
            })
        }
        // A `datalog { ... }` block used as a lambda-body expression —
        // supports test 03 shape `{ |x| datalog { pred($it, ?y) } }`.
        TokenKind::DatalogKw => datalog::parse_datalog_block(p),
        _ => Err(p.err_expected("expression atom")),
    }
}
