//! Top-level `@macro_expand(...)` directive parser
//! (paideia-as#1556, PAS-DEBT-B7-001-b, v0.36.74).
//!
//! # Surface syntax
//!
//! ```text
//! MacroExpandDirective := "@" "macro_expand" "(" "[" Ident "," StringLit "]" ")" ";"?
//! ```
//!
//! # Semantics
//!
//! A compile-time driver hook: the elaborator's `macro_expand_pass`
//! looks up the referenced macro by name, feeds the string-literal's
//! interior text through `match_structured` + `expand_macro`, and
//! emits any resulting diagnostics
//! (M0308/M0309/M0310/M0311/M0314/M0315) through the normal
//! DiagnosticSink → SARIF route.
//!
//! Chosen shape for the argument list:
//! - `[macro_name, "input string"]` — a bracket-enclosed 2-tuple.
//!   Reads naturally to a paideia-as author and gives the parser a
//!   fixed shape to key the future R221.M4 grammar work against.
//!   The trailing `,` inside the brackets is not accepted (single
//!   element separator only).
//!
//! # Diagnostics
//!
//! Parser diagnostics live in the `P0100..0299` range per
//! `diagnostics.md` §2. This directive reuses P0100 for the generic
//! "malformed directive" cases rather than minting a fresh code —
//! the annotation is INTERNAL-ONLY and the shape is fixed.
//!
//! # AST landing
//!
//! On success the parser pushes a
//! [`paideia_as_ast::MacroExpandDirective`] entry into the arena's
//! side-table (see [`paideia_as_ast::MacroExpandDirectiveTable`]) and
//! returns a `NodeKind::Placeholder` NodeId so the source-file loop's
//! per-iteration item invariant is preserved. The placeholder is
//! semantically invisible to lowering: no ItemData is attached and
//! downstream walkers skip Placeholder-kind nodes.

use paideia_as_ast::{MacroExpandDirective, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a top-level `@macro_expand([<macro>, "<input>"])`
    /// directive.
    ///
    /// The cursor points at the leading `@`. Consumes through the
    /// closing `)` and an optional trailing `;`. On success, pushes
    /// one [`MacroExpandDirective`] into the arena's side-table and
    /// returns a `NodeKind::Placeholder` NodeId spanning the whole
    /// directive; on failure, emits a P0100 diagnostic and returns
    /// `Err(ParseError)`.
    ///
    /// Only `macro_expand` is accepted as the attribute name. Any
    /// other identifier after the `@` emits P0100 — top-level `@`
    /// directives are otherwise reserved and unknown names are a
    /// hard error rather than a silent skip.
    pub(crate) fn parse_top_level_at_directive(&mut self) -> Result<NodeId, ParseError> {
        let at_tok = self.expect(TokenKind::At)?;
        let span_start = at_tok.span;

        // Name — must be `macro_expand`.
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_text = self.source_text_for_span(name_tok.span);
        if name_text != "macro_expand" {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message(format!(
                    "unknown top-level attribute '@{name_text}'; only \
                     '@macro_expand(...)' is recognised at file scope"
                ))
                .with_span(name_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // `(`
        if !self.eat(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message(
                    "malformed @macro_expand([<macro>, \"<input>\"]) directive: expected '(' after 'macro_expand'",
                )
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // `[`
        if !self.eat(TokenKind::LBracket) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message(
                    "malformed @macro_expand(...) directive: expected '[' inside '(' \
                     (shape is '@macro_expand([macro_name, \"input\"])')",
                )
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Macro name — one identifier token.
        if !self.at(TokenKind::Ident) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message(
                    "malformed @macro_expand([<macro>, \"<input>\"]) directive: expected macro-name identifier as first element",
                )
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let macro_name_tok = self.expect(TokenKind::Ident)?;
        let macro_name = self.source_text_for_span(macro_name_tok.span).to_string();

        // `,` separator.
        if !self.eat(TokenKind::Comma) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message(
                    "malformed @macro_expand([<macro>, \"<input>\"]) directive: expected ',' between macro name and input",
                )
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Input string literal.
        if !self.at(TokenKind::StringLit) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message(
                    "malformed @macro_expand([<macro>, \"<input>\"]) directive: expected string literal as second element",
                )
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }
        let str_tok = self.expect(TokenKind::StringLit)?;
        let raw = self.source_text_for_span(str_tok.span);
        // Strip surrounding quotes; leave any escapes verbatim — the
        // matcher / re-lexer sees the raw byte range that appeared
        // between the quotes.
        let input = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
            raw[1..raw.len() - 1].to_string()
        } else {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message("@macro_expand input must be a valid string literal")
                .with_span(str_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        // `]`
        if !self.eat(TokenKind::RBracket) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message("malformed @macro_expand(...) directive: expected ']' after input string")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // `)`
        if !self.eat(TokenKind::RParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 100)
                .expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message("malformed @macro_expand(...) directive: expected ')' after ']'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Optional trailing `;`.
        let _ = self.eat(TokenKind::Semicolon);

        // Compute full span for the directive.
        let end_span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));
        let full_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            end_span
                .byte_start()
                .saturating_sub(span_start.byte_start()),
        );

        // Push the directive into the arena side-table.
        self.arena_mut().macro_expand_directives_mut().push(MacroExpandDirective {
            macro_name,
            input,
            span: full_span,
        });

        // Return a Placeholder NodeId so the source-file loop's
        // per-iteration "one item" invariant is preserved. Downstream
        // walkers ignore Placeholder-kind top-level entries.
        let placeholder = self.arena_mut().alloc(NodeKind::Placeholder, full_span);
        Ok(placeholder)
    }
}

#[cfg(test)]
mod tests {
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, SourceMap, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};

    use crate::Parser;

    fn parse_all(source: &str) -> (AstArena, Vec<paideia_as_diagnostics::Diagnostic>) {
        let mut source_map = SourceMap::new();
        let file: FileId =
            source_map.add_file(std::path::PathBuf::from("t.pdx"), source.to_string());
        let src = SourceText::from_bytes(file, source.as_bytes()).unwrap();
        let mut lex_sink = VecSink::new();
        let mut lexer = Lexer::new(file, &src);
        let tokens = lexer.collect_tokens(&mut lex_sink);
        let mut arena = AstArena::new();
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(&tokens, src.content(), file, &mut arena, &mut parser_sink);
        let _ = p.parse_source_file();
        (arena, parser_sink.into_diagnostics())
    }

    #[test]
    fn parses_macro_expand_directive() {
        let src = r#"module M = structure {
  macro id($x:expr) => { $x }
}
@macro_expand([id, "42"])
"#;
        let (arena, diags) = parse_all(src);
        assert!(
            diags.iter().all(|d| d.code().category().letter() != 'P'),
            "unexpected parser diagnostics: {:?}",
            diags
        );
        let dirs = arena.macro_expand_directives();
        assert_eq!(dirs.len(), 1, "expected one directive, got {}", dirs.len());
        assert_eq!(dirs.entries()[0].macro_name, "id");
        assert_eq!(dirs.entries()[0].input, "42");
    }

    #[test]
    fn parses_two_directives_in_order() {
        let src = r#"module M = structure {
  macro id($x:expr) => { $x }
  macro id2($y:expr) => { $y }
}
@macro_expand([id, "42"]);
@macro_expand([id2, "hello"]);
"#;
        let (arena, diags) = parse_all(src);
        assert!(
            diags.iter().all(|d| d.code().category().letter() != 'P'),
            "unexpected parser diagnostics: {:?}",
            diags
        );
        let dirs = arena.macro_expand_directives();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs.entries()[0].macro_name, "id");
        assert_eq!(dirs.entries()[0].input, "42");
        assert_eq!(dirs.entries()[1].macro_name, "id2");
        assert_eq!(dirs.entries()[1].input, "hello");
    }

    #[test]
    fn rejects_unknown_top_level_attribute() {
        let src = "@bogus([id, \"42\"])\n";
        let (_arena, diags) = parse_all(src);
        assert!(
            diags.iter().any(|d| d.code().category().letter() == 'P' && d.code().number() == 100),
            "expected P0100 for unknown '@bogus'; got {:?}",
            diags
        );
    }

    #[test]
    fn rejects_missing_bracket() {
        let src = "@macro_expand(id, \"42\")\n";
        let (_arena, diags) = parse_all(src);
        assert!(
            diags.iter().any(|d| d.code().category().letter() == 'P' && d.code().number() == 100),
            "expected P0100 for missing '['; got {:?}",
            diags
        );
    }
}
