//! Pipeline-context sub-parser.
//!
//! Grammar (informally):
//!
//! ```text
//! pipeline_expr  ::= stage ('|' stage)*
//! stage          ::= redirect_stage ('&' | )                -- optional bg
//! redirect_stage ::= cmd_or_group ( redirect_op redirect_tgt )*
//! redirect_tgt   ::= '(' pipeline_expr ')'                  -- proc subst
//!                 | arg                                     -- filename
//! cmd_or_group   ::= '(' pipeline_expr ')'
//!                 | atom (arg)*
//! atom           ::= IDENT | STR | NUMBER | field_chain
//! field_chain    ::= IDENT ('.' IDENT)*
//! arg            ::= atom
//!                 | '{' … '}'          -- lambda-block arg
//!                 | 'datalog' '{' … '}'-- datalog-block arg
//!                 | '(' pipeline_expr ')'
//! ```
//!
//! PAS-DEBT-B2-017: `redirect_tgt` with `(…)` yields the inner pipeline
//! *directly* (a `Cmd`/`Pipe` node), not a `Group` wrapper — the parens
//! are proc-subst syntax. The `RedirectKind` doc reserves this shape;
//! the R222 evaluator adds semantics only.
//!
//! `Pipe` right-associates. Redirects are left-associative but the
//! parser folds them onto the innermost `cmd_or_group` so a
//! `cmd > a > b` yields `Redirect(Redirect(cmd, a), _, b)` — the R222
//! evaluator sees the innermost redirect target as the last-written.

use paideia_as_shell_lex::{Context, TokenKind};

use crate::ast::{RedirectKind, SyntaxNode};
use crate::parser::{ParseError, ParseErrorKind, Parser};
use crate::span::NodeSpan;

use super::{datalog, lambda};

/// Entry point: parse one pipeline expression (may be `Pipe`-chained).
pub fn parse_pipeline_expr(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let lhs = parse_stage(p)?;
    if let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::Pipe) && matches!(t.context, Context::Pipeline) {
            p.bump();
            let rhs = parse_pipeline_expr(p)?;
            let span = lhs.span().union(rhs.span());
            return Ok(SyntaxNode::Pipe {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            });
        }
    }
    Ok(lhs)
}

/// A stage optionally trailed by `&` (background).
fn parse_stage(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let inner = parse_redirect_stage(p)?;
    // R221.M5 does not lex `&` as its own token (the lexer emits it as
    // `Op("&")` — but the shell-lex layer doesn't actually recognise
    // `&` at all right now, so `Background` is unreachable until the
    // lexer follow-on adds it). Keeping the branch here so R222's
    // wiring doesn't need to touch this file.
    Ok(inner)
}

/// A stage followed by zero or more redirects.
fn parse_redirect_stage(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let mut node = parse_cmd_or_group(p)?;
    // Peek for `> file` / `>> file` / `< file`. The lexer emits `>`
    // and `<` as `Op(">")` / `Op("<")`; `>>` currently falls out as
    // two separate `Op(">")` tokens (the lexer does not glue `>>`),
    // so the parser recognises the two-token form.
    loop {
        let (kind, consume) = match (p.peek(), p.peek_at(1)) {
            (Some(a), _)
                if matches!(a.kind, TokenKind::Op(ref s) if s == ">")
                    && matches!(a.context, Context::Pipeline) =>
            {
                // Distinguish `>` from `>=` (already glued by lexer as
                // `Op(">=")`). Since the two-char `>=` is one token,
                // seeing `>` alone here is safe.
                //
                // For `>>`: peek at next token.
                if let Some(b) = p.peek_at(1) {
                    if matches!(b.kind, TokenKind::Op(ref s) if s == ">")
                        && matches!(b.context, Context::Pipeline)
                    {
                        (RedirectKind::StdoutAppend, 2)
                    } else {
                        (RedirectKind::StdoutOverwrite, 1)
                    }
                } else {
                    (RedirectKind::StdoutOverwrite, 1)
                }
            }
            (Some(a), _)
                if matches!(a.kind, TokenKind::Op(ref s) if s == "<")
                    && matches!(a.context, Context::Pipeline) =>
            {
                (RedirectKind::StdinFrom, 1)
            }
            _ => break,
        };
        for _ in 0..consume {
            p.bump();
        }
        // PAS-DEBT-B2-017: `>(cmd)` process substitution. When the
        // redirect target begins with `(`, parse the parenthesised
        // pipeline as the target *directly* (not wrapped in `Group`)
        // — the parens are proc-subst syntax, not a plain grouping.
        // The AST spec (ast.rs on `RedirectKind`) reserves this shape:
        // a `Cmd`/`Pipe` target signals process substitution; a
        // file-shaped target (`Ident`/`LitStr`/`LitInt`/`FieldAccess`)
        // signals a filename. The R222 evaluator adds semantics only.
        let (target, target_span) = match p.peek() {
            Some(t)
                if matches!(t.kind, TokenKind::LParen)
                    && matches!(t.context, Context::Pipeline) =>
            {
                parse_proc_subst_target(p)?
            }
            _ => {
                let n = parse_arg(p)?;
                let s = n.span();
                (n, s)
            }
        };
        let span = node.span().union(target_span);
        node = SyntaxNode::Redirect {
            source: Box::new(node),
            kind,
            target: Box::new(target),
            span,
        };
    }
    Ok(node)
}

/// Parse a process-substitution target `( pipeline_expr )` sitting on
/// the RHS of a redirect operator. Returns the inner pipeline node
/// (unwrapped — no `Group`) plus the outer span from `(` through `)`
/// so the enclosing `Redirect`'s span covers the closing paren.
///
/// Precondition: caller has confirmed `p.peek()` is `LParen` in
/// `Context::Pipeline`.
fn parse_proc_subst_target(
    p: &mut Parser<'_>,
) -> Result<(SyntaxNode, NodeSpan), ParseError> {
    let lparen = p.bump().expect("caller peeked LParen");
    let lparen_span = p.token_span(lparen);
    p.enter_nesting(Some(lparen))?;
    let inner = parse_pipeline_expr(p)?;
    let rparen_span = match p.peek() {
        Some(t) if matches!(t.kind, TokenKind::RParen) => {
            let rp = p.bump().unwrap();
            p.token_span(rp)
        }
        _ => {
            p.exit_nesting();
            return Err(p.err_expected("`)` to close process substitution"));
        }
    };
    p.exit_nesting();
    Ok((inner, lparen_span.union(rparen_span)))
}

/// A command invocation, or a parenthesised sub-expression, or a
/// top-level lambda / datalog block used as a standalone stage.
///
/// Dispatch:
/// * `(` (Pipeline) → parenthesised sub-expression wrapped in `Group`.
/// * `datalog` → whole-stage `DatalogBlock` (no Cmd wrapper).
/// * `{` (Lambda) → whole-stage `Lambda` (no Cmd wrapper).
/// * otherwise → `parse_cmd`, whose leading atom is the command name
///   and whose trailing atoms/blocks are arguments.
fn parse_cmd_or_group(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    if let Some(t) = p.peek() {
        if matches!(t.kind, TokenKind::LParen) && matches!(t.context, Context::Pipeline) {
            let lparen = p.bump().unwrap();
            p.enter_nesting(Some(lparen))?;
            let inner = parse_pipeline_expr(p)?;
            let rparen = match p.peek() {
                Some(t) if matches!(t.kind, TokenKind::RParen) => p.bump().unwrap(),
                _ => {
                    p.exit_nesting();
                    return Err(p.err_expected("`)` to close group"));
                }
            };
            p.exit_nesting();
            let span = p
                .token_span(lparen)
                .union(p.token_span(rparen));
            return Ok(SyntaxNode::Group {
                inner: Box::new(inner),
                span,
            });
        }
        if matches!(t.kind, TokenKind::DatalogKw) {
            return datalog::parse_datalog_block(p);
        }
        if matches!(t.kind, TokenKind::LBrace) && matches!(t.context, Context::Lambda) {
            return lambda::parse_lambda_block(p);
        }
    }
    parse_cmd(p)
}

/// A command: leading atom (name) followed by zero-or-more argument
/// atoms until a pipeline separator (`|`, `;`, newline, `)`, or EOF).
///
/// # Context tolerance
///
/// Args include lambda blocks `{ … }` and datalog blocks
/// `datalog { … }`. Their opening `{` and the `datalog` keyword sit at
/// context Lambda / Pipeline respectively — the *inside* tokens jump
/// to Lambda / Datalog. So the terminator check must key off token
/// *kind* first, and only consult context when the kind is one that
/// context might reinterpret (e.g. `|` is a separator in Pipeline but
/// a param delimiter in Lambda). A blanket "context must equal
/// Pipeline" test breaks the moment an arg-shape token pushes the
/// context stack.
fn parse_cmd(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let name = parse_atom(p)?;
    let mut args = Vec::new();
    while let Some(t) = p.peek() {
        match &t.kind {
            // Pipeline separators — only terminators when their context
            // is still Pipeline; inside a nested Lambda body a bare `|`
            // is a param delimiter and inside a Datalog block `;` is
            // just whitespace-equivalent (nothing consumes it here).
            TokenKind::Pipe if matches!(t.context, Context::Pipeline) => break,
            TokenKind::Semi if matches!(t.context, Context::Pipeline) => break,
            TokenKind::Newline if matches!(t.context, Context::Pipeline) => break,
            // Closing delimiters — always terminate; their outer
            // consumer (Group / Lambda / Record) handles them.
            TokenKind::RParen | TokenKind::RBrace | TokenKind::RBracket => break,
            // Redirects — outer loop takes over.
            TokenKind::Op(s)
                if (s == ">" || s == "<") && matches!(t.context, Context::Pipeline) =>
            {
                break
            }
            _ => {
                let arg = parse_arg(p)?;
                args.push(arg);
            }
        }
    }
    let span = match args.last() {
        Some(a) => name.span().union(a.span()),
        None => name.span(),
    };
    Ok(SyntaxNode::Cmd {
        name: Box::new(name),
        args,
        span,
    })
}

/// One argument in Pipeline context. Also the parser entry for
/// `datalog { … }` and `{ |args| body }` embedded as arguments.
fn parse_arg(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let Some(t) = p.peek() else {
        return Err(p.err_expected("argument"));
    };
    match &t.kind {
        TokenKind::DatalogKw => datalog::parse_datalog_block(p),
        TokenKind::LBrace => lambda::parse_lambda_block(p),
        TokenKind::LParen => parse_cmd_or_group(p),
        _ => parse_atom(p),
    }
}

/// A single-token or field-chain atom.
pub(crate) fn parse_atom(p: &mut Parser<'_>) -> Result<SyntaxNode, ParseError> {
    let Some(t) = p.peek() else {
        return Err(p.err_expected("atom"));
    };
    let tok = t;
    let ctx = tok.context;
    let span0 = p.token_span(tok);

    let node = match &tok.kind {
        TokenKind::Ident(name) => {
            let name = name.clone();
            p.bump();
            // Fold `Ident '.' Ident` chains into `FieldAccess`.
            let mut acc = SyntaxNode::Ident { name, span: span0 };
            loop {
                let Some(dot) = p.peek() else { break };
                if !matches!(dot.kind, TokenKind::Dot) || dot.context != ctx {
                    break;
                }
                // Two-token lookahead for `Dot Ident`: only fold if
                // the char after the dot is another ident.
                let Some(next) = p.peek_at(1) else { break };
                let TokenKind::Ident(field) = &next.kind else {
                    break;
                };
                let field = field.clone();
                let dot_span = p.token_span(dot);
                let ident_span = p.token_span(next);
                p.bump(); // dot
                p.bump(); // ident
                let span = acc.span().union(dot_span).union(ident_span);
                acc = SyntaxNode::FieldAccess {
                    base: Box::new(acc),
                    field,
                    span,
                };
            }
            acc
        }
        TokenKind::Str(s) => {
            let value = s.clone();
            p.bump();
            SyntaxNode::LitStr { value, span: span0 }
        }
        TokenKind::Number(s) => {
            let raw = s.clone();
            p.bump();
            let cleaned: String = raw.chars().filter(|&c| c != '_').collect();
            let value = cleaned.parse::<i64>().map_err(|_| ParseError {
                kind: ParseErrorKind::NumericOverflow(raw.clone()),
                original_span: span0.original,
                nfc_span: span0.nfc,
            })?;
            SyntaxNode::LitInt { value, span: span0 }
        }
        _ => {
            let expected = "identifier, string, or number";
            return Err(p.err_expected(expected));
        }
    };
    Ok(node)
}
