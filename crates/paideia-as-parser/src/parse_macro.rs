//! Macro declaration parsing for phase-1 pattern-based macros.
//!
//! Implements parsing of macro declarations in the form:
//! - Single-rule: `macro Name(pattern) => template`
//!
//! Pattern and template token streams are stored as `Placeholder` nodes whose
//! spans cover the byte ranges of the tokens. The actual pattern matching and
//! expansion are deferred to PR-47+.

use paideia_as_ast::{
    ItemData, MacroDeclData, MacroFragment, MacroFragmentKind, MacroRule, NodeId, NodeKind,
};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a macro declaration: `macro Name(pattern) => template`.
    ///
    /// **Algorithm:**
    /// 1. Verify we're at the contextual "macro" keyword (Ident with source text "macro").
    /// 2. Consume the `macro` Ident.
    /// 3. Expect and consume the macro name Ident.
    /// 4. Expect `(` for the pattern.
    /// 5. Scan to `)` to capture pattern tokens, then extract MacroFragments.
    /// 6. Expect `=>` (FatArrow).
    /// 7. Scan template tokens until end of statement.
    /// 8. Allocate MacroDecl item.
    ///
    /// Emits `P0110` for unknown fragment kinds.
    pub(crate) fn parse_macro_decl(&mut self) -> Result<NodeId, ParseError> {
        let macro_tok = self.expect(TokenKind::Ident)?;
        let span_start = macro_tok.span;

        // Verify the token is contextually "macro" by checking source text
        let source = self.source();
        let start = macro_tok.span.byte_start() as usize;
        let end = (macro_tok.span.byte_start() + macro_tok.span.byte_len()) as usize;
        let macro_lexeme = if start <= source.len() && end <= source.len() {
            &source[start..end]
        } else {
            ""
        };

        if macro_lexeme != "macro" {
            let code =
                DiagnosticCode::new(Category::P, Severity::Error, 100).expect("valid P0100 code");
            let diag = Diagnostic::error(code)
                .message("expected contextual keyword 'macro'")
                .with_span(macro_tok.span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Parse macro name
        let name_tok = self.expect(TokenKind::Ident)?;
        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_tok.span);

        // Expect `(` for pattern
        self.expect(TokenKind::LParen)?;
        let pattern_start_span = name_tok.span;

        // Scan to matching `)`
        let pattern_end_span = self.skip_to_closing_paren()?;
        let pattern_span = Span::new(
            pattern_start_span.file(),
            pattern_start_span.byte_start(),
            pattern_end_span.byte_start() + pattern_end_span.byte_len()
                - pattern_start_span.byte_start(),
        );

        // Allocate pattern placeholder
        let pattern_id = self.arena_mut().alloc(NodeKind::Placeholder, pattern_span);

        // Extract fragments from pattern
        let fragments = self.extract_macro_fragments(pattern_span)?;

        // Expect `=>`
        self.expect(TokenKind::FatArrow)?;

        // Scan template tokens
        let template_start = self.peek().map(|t| t.span).unwrap_or(pattern_span);
        let template_end_span = self.skip_to_template_end()?;
        let template_span = Span::new(
            template_start.file(),
            template_start.byte_start(),
            template_end_span.byte_start() + template_end_span.byte_len()
                - template_start.byte_start(),
        );

        // Allocate template placeholder
        let template_id = self.arena_mut().alloc(NodeKind::Placeholder, template_span);

        // Consume trailing `;` if present
        self.eat(TokenKind::Semicolon);

        // Compute full span for the macro declaration
        let end_span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));
        let full_span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            end_span.byte_start() + end_span.byte_len() - span_start.byte_start(),
        );

        // Allocate MacroDecl item with one rule
        let rule = MacroRule {
            pattern: pattern_id,
            template: template_id,
            fragments,
        };

        let decl_data = MacroDeclData {
            name: name_id,
            rules: vec![rule],
            doc: None,
        };

        let item = self.arena_mut().alloc_item(
            NodeKind::MacroDecl,
            full_span,
            ItemData::MacroDecl(decl_data),
        );
        Ok(item)
    }

    /// Skip from current position to the matching closing paren `)`.
    ///
    /// Assumes we just consumed `(` and are now inside the pattern.
    /// Returns the span of the closing paren.
    fn skip_to_closing_paren(&mut self) -> Result<Span, ParseError> {
        let mut depth = 1;
        while depth > 0 && self.peek().is_some() && !self.at(TokenKind::Eof) {
            match self.peek().map(|t| t.kind) {
                Some(TokenKind::LParen) => depth += 1,
                Some(TokenKind::RParen) => depth -= 1,
                _ => {}
            }

            if depth > 0 {
                self.bump();
            } else {
                // depth == 0; consume the final `)`
                let rparen = self.bump().expect("at(RParen) implies peek() is Some");
                return Ok(rparen.span);
            }
        }

        // EOF before closing paren
        let span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));
        let code =
            DiagnosticCode::new(Category::P, Severity::Error, 100).expect("valid P0100 code");
        let diag = Diagnostic::error(code)
            .message("unexpected EOF in macro pattern; expected ')'")
            .with_span(span)
            .finish();
        self.emit_diagnostic(diag);
        Err(ParseError)
    }

    /// Skip from current position to the end of the template.
    ///
    /// The template ends at `;` or at the start of the next item/EOF.
    fn skip_to_template_end(&mut self) -> Result<Span, ParseError> {
        let mut last_span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));

        while !self.at(TokenKind::Eof) && self.peek().is_some() {
            match self.peek().map(|t| t.kind) {
                Some(TokenKind::Semicolon)
                | Some(TokenKind::KwModule)
                | Some(TokenKind::KwSignature)
                | Some(TokenKind::KwLet)
                | Some(TokenKind::KwEffect)
                | Some(TokenKind::KwCapability)
                | Some(TokenKind::KwStruct)
                | Some(TokenKind::KwEnum)
                | Some(TokenKind::KwUnsafe)
                | Some(TokenKind::Eof) => break,
                _ => {
                    last_span = self.peek().map(|t| t.span).unwrap_or(last_span);
                    self.bump();
                }
            }
        }

        Ok(last_span)
    }

    /// Extract MacroFragments from a pattern span by scanning the source text
    /// for `$name:kind` occurrences.
    ///
    /// Emits P0110 for unknown fragment kinds.
    fn extract_macro_fragments(
        &mut self,
        pattern_span: Span,
    ) -> Result<Vec<MacroFragment>, ParseError> {
        // First pass: collect fragment metadata without mutating the arena
        let source = self.source();
        let start = pattern_span.byte_start() as usize;
        let end = (pattern_span.byte_start() + pattern_span.byte_len()) as usize;

        if start >= source.len() || end > source.len() {
            return Ok(vec![]);
        }

        let pattern_text = &source[start..end];

        #[derive(Clone)]
        struct FragmentMetadata {
            name_start: usize,
            name_end: usize,
            kind_str: String,
            kind_start: usize,
            kind_end: usize,
        }

        let mut metadata = vec![];
        let mut chars = pattern_text.char_indices().peekable();

        while let Some((i, ch)) = chars.next() {
            if ch == '$'
                && let Some((_, name_ch)) = chars.peek()
                && (name_ch.is_alphabetic() || *name_ch == '_')
            {
                let name_start = i + 1;
                let mut name_end = name_start;
                while let Some((j, c)) = chars.peek() {
                    if c.is_alphanumeric() || *c == '_' {
                        name_end = j + 1;
                        chars.next();
                    } else {
                        break;
                    }
                }

                if let Some((_, ':')) = chars.peek() {
                    chars.next();
                    let kind_start = chars.peek().map(|(j, _)| *j).unwrap_or(pattern_text.len());
                    let mut kind_end = kind_start;
                    while let Some((j, c)) = chars.peek() {
                        if c.is_alphanumeric() || *c == '_' {
                            kind_end = j + 1;
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    if kind_end > kind_start {
                        let kind_str = pattern_text[kind_start..kind_end].to_string();
                        metadata.push(FragmentMetadata {
                            name_start,
                            name_end,
                            kind_str,
                            kind_start,
                            kind_end,
                        });
                    }
                }
            }
        }

        // Second pass: allocate nodes and fragments
        let mut fragments = vec![];
        for meta in metadata {
            if let Some(kind) = MacroFragmentKind::parse(&meta.kind_str) {
                let name_byte_pos = (start + meta.name_start) as u32;
                let name_byte_len = (meta.name_end - meta.name_start) as u32;
                let name_span = Span::new(pattern_span.file(), name_byte_pos, name_byte_len);
                let name_id = self.arena_mut().alloc(NodeKind::Ident, name_span);

                fragments.push(MacroFragment {
                    name: name_id,
                    kind,
                });
            } else {
                let kind_byte_pos = (start + meta.kind_start) as u32;
                let kind_byte_len = (meta.kind_end - meta.kind_start) as u32;
                let kind_span = Span::new(pattern_span.file(), kind_byte_pos, kind_byte_len);

                let code = DiagnosticCode::new(Category::P, Severity::Error, 110)
                    .expect("valid P0110 code");
                let diag = Diagnostic::error(code)
                    .message(format!("unknown macro fragment kind: '{}'", meta.kind_str))
                    .with_span(kind_span)
                    .finish();
                self.emit_diagnostic(diag);
            }
        }

        Ok(fragments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, Severity, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};

    fn parse_source_str(
        source: &str,
    ) -> (
        paideia_as_ast::AstArena,
        Result<NodeId, ParseError>,
        Vec<Diagnostic>,
    ) {
        let mut source_map = paideia_as_diagnostics::SourceMap::new();
        let file = source_map.add_file(std::path::PathBuf::from("test.pdx"), source.to_string());
        let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
        let mut arena = paideia_as_ast::AstArena::new();
        let mut sink = VecSink::new();
        let mut lex = Lexer::new(file, &source_text);
        let mut collector = VecSink::new();
        let tokens = lex.collect_tokens(&mut collector);
        for d in collector.into_diagnostics() {
            let _ = sink.emit(d);
        }
        let result = {
            let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
            p.parse_source_file()
        };
        (arena, result, sink.into_diagnostics())
    }

    #[test]
    fn single_rule_macro_parses() {
        let (_arena, result, diags) = parse_source_str("macro foo($x:expr) => { simple_form($x) }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn unknown_fragment_kind_emits_p0110() {
        let (_arena, result, diags) = parse_source_str("macro foo($x:wat) => { x }");
        assert!(result.is_ok(), "should parse despite fragment kind error");
        let p0110_diags: Vec<_> = diags.iter().filter(|d| d.code().number() == 110).collect();
        assert_eq!(
            p0110_diags.len(),
            1,
            "should emit exactly one P0110 diagnostic"
        );
    }

    #[test]
    fn fragment_kinds_expr_recognized() {
        let (_arena, result, diags) = parse_source_str("macro test($a:expr) => { a }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn empty_template_block_ok() {
        let (_arena, result, diags) = parse_source_str("macro foo($x:expr) => { }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }
}
