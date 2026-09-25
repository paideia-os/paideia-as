//! Datalog-context sub-parser.
//!
//! Grammar (informally):
//!
//! ```text
//! datalog_block ::= 'datalog' '{' item* '}'
//! item          ::= rule | fact | negated_fact
//! fact          ::= atom '.'
//! negated_fact  ::= 'not' atom '.'
//! rule          ::= atom '=>' atom_list '.'
//! atom_list     ::= atom (',' atom)*
//! atom          ::= IDENT '(' term (',' term)* ')'
//!                 | IDENT                              -- 0-arg atom
//! term          ::= QVAR | INTERP | IDENT | STR | NUMBER
//! ```
//!
//! Called from Pipeline context (the enclosing block is a stage). The
//! opening `datalog {` is emitted by the lexer as `DatalogKw` (in
//! Pipeline context) followed by `LBrace` (in Datalog context) — this
//! function consumes both.

use paideia_as_shell_lex::{Context, TokenKind};

use crate::ast::SyntaxNode;
use crate::parser::{ParseError, ParseErrorKind, Parser};

/// Consume `datalog { … }` and produce a [`SyntaxNode::DatalogBlock`].
pub fn parse_datalog_block(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    // `datalog` keyword (Pipeline context).
    let kw = match p.peek() {
        Some(t) if matches!(t.kind, TokenKind::DatalogKw) => p.bump().unwrap(),
        _ => return Err(p.err_expected("`datalog` keyword")),
    };
    let kw_span = p.token_span(kw);
    // Opening `{` (already stamped Datalog by the lexer).
    let lbrace = match p.peek() {
        Some(t) if matches!(t.kind, TokenKind::LBrace) && matches!(t.context, Context::Datalog) => {
            p.bump().unwrap()
        }
        _ => return Err(p.err_expected("`{` opening datalog block")),
    };
    p.enter_nesting(Some(lbrace))?;
    let mut items = Vec::new();
    loop {
        // Datalog is whitespace/newline-transparent inside the block;
        // skip stray newlines.
        while let Some(t) = p.peek() {
            if matches!(t.kind, TokenKind::Newline) {
                p.bump();
            } else {
                break;
            }
        }
        match p.peek() {
            Some(t) if matches!(t.kind, TokenKind::RBrace) => break,
            None => {
                p.exit_nesting();
                return Err(p.err_expected("`}` to close datalog block"));
            }
            _ => {}
        }
        let item = parse_item(p)?;
        items.push(item);
    }
    let rbrace = p.bump().unwrap();
    p.exit_nesting();
    // Force the block's context to Datalog (kw was Pipeline; the
    // block-as-a-whole is a Datalog subtree). Rationale: R226 keys off
    // context to skip pipeline-only handling.
    let block_span = kw_span
        .union_with_context(p.token_span(rbrace), Context::Datalog);
    Ok(SyntaxNode::DatalogBlock {
        items,
        span: block_span,
    })
}

/// Parse one item: rule or fact (possibly negated). Terminates at `.`.
fn parse_item(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    // Negated fact: `not atom .`
    if let Some(t) = p.peek() {
        if let TokenKind::Op(s) = &t.kind {
            if s == "not" {
                let neg = p.bump().unwrap();
                let neg_span = p.token_span(neg);
                let inner = parse_atom(p)?;
                // Optional `.` terminator.
                if let Some(t) = p.peek() {
                    if matches!(t.kind, TokenKind::Dot) {
                        p.bump();
                    }
                }
                let span = neg_span.union(inner.span());
                return Ok(SyntaxNode::NotAtom {
                    inner: Box::new(inner),
                    span,
                });
            }
        }
    }

    let head = parse_atom(p)?;
    // Rule? `atom => body_atom (, body_atom)* .`
    if let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::FatArrow) {
            p.bump();
            let mut body = Vec::new();
            loop {
                let a = parse_atom(p)?;
                body.push(a);
                match p.peek() {
                    Some(t) if matches!(t.kind, TokenKind::Comma) => {
                        p.bump();
                    }
                    _ => break,
                }
            }
            // Optional trailing `.`.
            let end_span = if let Some(t) = p.peek() {
                if matches!(t.kind, TokenKind::Dot) {
                    let d = p.bump().unwrap();
                    p.token_span(d)
                } else {
                    body.last().unwrap().span()
                }
            } else {
                body.last().unwrap().span()
            };
            let span = head.span().union(end_span);
            return Ok(SyntaxNode::Rule {
                head: Box::new(head),
                body,
                span,
            });
        }
    }
    // Fact: `atom .` (dot optional at end-of-block).
    if let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::Dot) {
            p.bump();
        }
    }
    Ok(head)
}

/// Parse one atom: `pred(term, term, …)` or bare `pred`.
fn parse_atom(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let head = match p.peek() {
        Some(t) if matches!(t.kind, TokenKind::Ident(_)) => p.bump().unwrap(),
        _ => return Err(p.err_expected("Datalog predicate name")),
    };
    let TokenKind::Ident(pred) = &head.kind else {
        return Err(p.err_expected("Datalog predicate name"));
    };
    let pred = pred.clone();
    let head_span = p.token_span(head);
    let mut args = Vec::new();
    // Optional argument list.
    if let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::LParen) {
            p.bump();
            loop {
                if let Some(t) = p.peek() {
                    if matches!(t.kind, TokenKind::RParen) {
                        break;
                    }
                }
                let term = parse_term(p)?;
                args.push(term);
                match p.peek() {
                    Some(t) if matches!(t.kind, TokenKind::Comma) => {
                        p.bump();
                    }
                    Some(t) if matches!(t.kind, TokenKind::RParen) => {}
                    _ => return Err(p.err_expected("`,` or `)` in Datalog atom")),
                }
            }
            let rparen = match p.peek() {
                Some(t) if matches!(t.kind, TokenKind::RParen) => p.bump().unwrap(),
                _ => return Err(p.err_expected("`)` closing Datalog atom")),
            };
            let span = head_span.union(p.token_span(rparen));
            return Ok(SyntaxNode::Atom {
                pred,
                args,
                span,
            });
        }
    }
    Ok(SyntaxNode::Atom {
        pred,
        args,
        span: head_span,
    })
}

/// Parse one Datalog term: `?var`, `$var`, ident, string, number.
fn parse_term(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let Some(t) = p.peek() else {
        return Err(p.err_expected("Datalog term"));
    };
    let span = p.token_span(t);
    match &t.kind {
        TokenKind::QVar(name) => {
            let name = name.clone();
            p.bump();
            Ok(SyntaxNode::QVar { name, span })
        }
        TokenKind::InterpVar(name) => {
            let name = name.clone();
            p.bump();
            Ok(SyntaxNode::InterpVar { name, span })
        }
        TokenKind::Ident(name) => {
            let name = name.clone();
            p.bump();
            Ok(SyntaxNode::Ident { name, span })
        }
        TokenKind::Str(s) => {
            let value = s.clone();
            p.bump();
            Ok(SyntaxNode::LitStr { value, span })
        }
        TokenKind::Number(s) => {
            let raw = s.clone();
            p.bump();
            let cleaned: String = raw.chars().filter(|&c| c != '_').collect();
            let value = cleaned.parse::<i64>().map_err(|_| ParseError {
                kind: ParseErrorKind::NumericOverflow(raw.clone()),
                original_span: span.original,
                nfc_span: span.nfc,
            })?;
            Ok(SyntaxNode::LitInt { value, span })
        }
        _ => Err(p.err_expected("Datalog term (`?var`, `$var`, ident, string, or number)")),
    }
}
