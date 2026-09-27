//! Macro declaration parsing for phase-1 pattern-based macros.
//!
//! Implements parsing of macro declarations in the form:
//! - Single-rule: `macro Name(pattern) => template`
//! - Multi-rule: `macro Name { (pattern) => template ; (pattern) => template }`
//!
//! Slice A (PAS-DEBT-B2-010, #1503, v0.36.52) upgraded the pattern side to a
//! real fragment-kind grammar: the pattern arena node is now
//! [`NodeKind::MacroPattern`] and [`MacroRule::pattern_elems`] carries the
//! ordered fragment / literal sequence.
//!
//! Slice B (PAS-DEBT-B2-010b, #1541, v0.36.65) lifts templates to the
//! same shape: the template arena node is now
//! [`NodeKind::MacroTemplate`] and [`MacroRule::template_elems`]
//! carries the ordered fragment-reference / literal sequence. The
//! structured expander lives in
//! `paideia-as-elaborator::macro_expand::expand_macro`.
//!
//! Slice C (PAS-DEBT-B2-010c, #1542, v0.36.66) caps the grammar with
//! repetition groups: `$( ... )*` and `$( ... )+`, optionally
//! separated (`$( $x:expr ),*`). Both extractors below recognise the
//! `$(` opener, recurse into the inner element sequence, and read the
//! optional single-char separator followed by the required `*` / `+`
//! terminator. Nested repetition groups are supported by the recursion.

use paideia_as_ast::{
    ItemData, MacroDeclData, MacroFragment, MacroFragmentKind, MacroPatternElem, MacroRule,
    MacroTemplateElem, NodeId, NodeKind, RepMin,
};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, FileId, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse a macro declaration: `macro Name(pattern) => template` or
    /// `macro Name { (pattern) => template ; (pattern) => template }`.
    ///
    /// **Algorithm:**
    /// 1. Verify we're at the contextual "macro" keyword (Ident with source text "macro").
    /// 2. Consume the `macro` Ident.
    /// 3. Expect and consume the macro name Ident.
    /// 4. Check if next is `(` or `{`:
    ///    - If `(`: single-rule form
    ///    - If `{`: multi-rule form
    /// 5. For each rule: scan pattern, expect `=>`, scan template, consume `;` or `}`.
    /// 6. Allocate MacroDecl item with rules vector.
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

        // Determine which form: single-rule `(` or multi-rule `{`
        let is_multi_rule = self.at(TokenKind::LBrace);

        let mut rules = Vec::new();

        if is_multi_rule {
            // Multi-rule form: `macro Name { (pattern) => template ; ... }`
            self.expect(TokenKind::LBrace)?;

            while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                // Parse one rule: (pattern) => template
                let rule = self.parse_macro_rule()?;
                rules.push(rule);

                // After template, expect `;` if not at `}`
                if self.at(TokenKind::RBrace) {
                    break;
                } else {
                    self.eat(TokenKind::Semicolon);
                }
            }

            self.expect(TokenKind::RBrace)?;
        } else {
            // Single-rule form: `macro Name (pattern) => template`
            self.expect(TokenKind::LParen)?;
            let rule = self.parse_macro_rule_inner()?;
            rules.push(rule);

            // Consume trailing `;` if present
            self.eat(TokenKind::Semicolon);
        }

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

        let decl_data = MacroDeclData {
            name: name_id,
            rules,
            doc: None,
        };

        let item = self.arena_mut().alloc_item(
            NodeKind::MacroDecl,
            full_span,
            ItemData::MacroDecl(decl_data),
        );
        Ok(item)
    }

    /// Parse a single macro rule within a multi-rule form.
    /// Expects current position to be at `(` and will consume through `=>`
    /// and template.
    fn parse_macro_rule(&mut self) -> Result<MacroRule, ParseError> {
        self.expect(TokenKind::LParen)?;
        self.parse_macro_rule_inner()
    }

    /// Parse the pattern and template of a macro rule, assuming we just
    /// consumed `(` and are at the pattern content.
    fn parse_macro_rule_inner(&mut self) -> Result<MacroRule, ParseError> {
        let pattern_start_span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));

        // Scan to matching `)`
        let pattern_end_span = self.skip_to_closing_paren()?;
        let pattern_span = Span::new(
            pattern_start_span.file(),
            pattern_start_span.byte_start(),
            pattern_end_span.byte_start() + pattern_end_span.byte_len()
                - pattern_start_span.byte_start(),
        );

        // Slice A: allocate a real MacroPattern node (was Placeholder) and
        // populate a structured element list. #1503.
        let pattern_id = self.arena_mut().alloc(NodeKind::MacroPattern, pattern_span);

        // Extract structured pattern elements + fragment-only projection.
        let (pattern_elems, fragments) = self.extract_macro_pattern(pattern_span)?;

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

        // Slice B: allocate a real MacroTemplate node (was Placeholder)
        // and populate a structured element list. #1541.
        let template_id = self.arena_mut().alloc(NodeKind::MacroTemplate, template_span);

        // Extract structured template elements (fragment refs + literal
        // spans). Unknown fragment references remain permissive here —
        // the expander in `paideia-as-elaborator::macro_expand` emits
        // M0309 when a `$name` reference has no binding in the matched
        // rule; there is no template-side kind selector to reject.
        let template_elems = self.extract_macro_template(template_span);

        Ok(MacroRule {
            pattern: pattern_id,
            template: template_id,
            pattern_elems,
            fragments,
            template_elems,
        })
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
    /// The template ends at `;`, `}`, or at the start of the next item/EOF.
    /// Tracks brace/paren depth to avoid stopping inside nested structures.
    fn skip_to_template_end(&mut self) -> Result<Span, ParseError> {
        let mut last_span = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));
        let mut depth = 0;

        while !self.at(TokenKind::Eof) && self.peek().is_some() {
            match self.peek().map(|t| t.kind) {
                Some(TokenKind::LBrace) => {
                    depth += 1;
                    last_span = self.peek().map(|t| t.span).unwrap_or(last_span);
                    self.bump();
                }
                Some(TokenKind::RBrace) => {
                    if depth == 0 {
                        // Top-level `}` ends the template (in multi-rule form)
                        break;
                    } else {
                        depth -= 1;
                        last_span = self.peek().map(|t| t.span).unwrap_or(last_span);
                        self.bump();
                    }
                }
                Some(TokenKind::Semicolon) if depth == 0 => {
                    // Top-level `;` ends the template
                    break;
                }
                Some(TokenKind::KwModule)
                | Some(TokenKind::KwSignature)
                | Some(TokenKind::KwLet)
                | Some(TokenKind::KwEffect)
                | Some(TokenKind::KwCapability)
                | Some(TokenKind::KwStruct)
                | Some(TokenKind::KwEnum)
                | Some(TokenKind::KwUnsafe)
                    if depth == 0 =>
                {
                    // Top-level keyword ends the template (single-rule form)
                    break;
                }
                Some(TokenKind::Eof) => break,
                _ => {
                    last_span = self.peek().map(|t| t.span).unwrap_or(last_span);
                    self.bump();
                }
            }
        }

        Ok(last_span)
    }

    /// Extract the structured pattern element list from `pattern_span` by
    /// scanning source bytes for `$name:kind` fragment sites and
    /// `$( ... )SEP? {*|+}` repetition groups; every span in between
    /// (or an unrecognised `$...` sequence) becomes a
    /// `MacroPatternElem::Literal`.
    ///
    /// Returns `(pattern_elems, fragments)` where `fragments` is the
    /// fragment-only projection kept for downstream matcher lookup.
    /// The projection is a *flat* view of every fragment reachable
    /// inside the pattern — including fragments nested inside
    /// repetition groups — so the matcher's per-name lookup stays
    /// unified with the shape it saw before Slice C.
    ///
    /// Emits P0110 for unknown fragment kinds (the offending
    /// `$name:kind` still collapses to a `Literal` element so the
    /// pattern stays structurally complete) and P0111 for a malformed
    /// repetition group (missing `)` or missing `*`/`+` terminator).
    fn extract_macro_pattern(
        &mut self,
        pattern_span: Span,
    ) -> Result<(Vec<MacroPatternElem>, Vec<MacroFragment>), ParseError> {
        let start = pattern_span.byte_start() as usize;
        let end = (pattern_span.byte_start() + pattern_span.byte_len()) as usize;

        // Clone the pattern text to own it — the second pass needs
        // `&mut self` for arena.alloc / emit_diagnostic, and an in-place
        // borrow of self.source() would conflict.
        let pattern_text: String = {
            let source = self.source();
            if start >= source.len() || end > source.len() {
                return Ok((vec![], vec![]));
            }
            source[start..end].to_string()
        };

        // First pass: recursively scan into an intermediate raw tree.
        let mut cursor = 0usize;
        let raw = scan_raw_pattern_elems(&pattern_text, &mut cursor, pattern_text.len(), false);

        // Second pass: convert raw → structured, allocating Ident
        // arena nodes and emitting P0110 for unknown fragment kinds.
        let mut fragments: Vec<MacroFragment> = Vec::new();
        let elems = self.materialize_pattern_elems(
            pattern_span.file(),
            start,
            &pattern_text,
            &raw,
            &mut fragments,
        );
        Ok((elems, fragments))
    }

    /// Convert a slice of raw pattern elements (bytes-only intermediate
    /// from `scan_raw_pattern_elems`) into the structured AST form,
    /// allocating Ident nodes and emitting P0110 for unknown fragment
    /// kinds along the way. Recurses into `Repetition` groups so nested
    /// `$( )*` groups round-trip.
    fn materialize_pattern_elems(
        &mut self,
        file: FileId,
        start: usize,
        pattern_text: &str,
        raw: &[RawPatternElem],
        fragments: &mut Vec<MacroFragment>,
    ) -> Vec<MacroPatternElem> {
        let mut out: Vec<MacroPatternElem> = Vec::with_capacity(raw.len());
        for r in raw {
            match r {
                RawPatternElem::Literal { start_off, end_off } => {
                    let lit_pos = (start + *start_off) as u32;
                    let lit_len = (*end_off - *start_off) as u32;
                    out.push(MacroPatternElem::Literal {
                        span: Span::new(file, lit_pos, lit_len),
                    });
                }
                RawPatternElem::Fragment { site } => {
                    if let Some(kind) = MacroFragmentKind::parse(&site.kind_str) {
                        let name_byte_pos = (start + site.name_start) as u32;
                        let name_byte_len = (site.name_end - site.name_start) as u32;
                        let name_span = Span::new(file, name_byte_pos, name_byte_len);
                        let name_id = self.arena_mut().alloc(NodeKind::Ident, name_span);

                        let site_pos = (start + site.site_start) as u32;
                        let site_len = (site.site_end - site.site_start) as u32;
                        let frag_span = Span::new(file, site_pos, site_len);
                        out.push(MacroPatternElem::Fragment {
                            name: name_id,
                            kind,
                            span: frag_span,
                        });
                        fragments.push(MacroFragment { name: name_id, kind });
                    } else {
                        // Unknown kind → P0110, collapse to Literal.
                        let kind_byte_pos = (start + site.kind_start) as u32;
                        let kind_byte_len = (site.kind_end - site.kind_start) as u32;
                        let kind_span = Span::new(file, kind_byte_pos, kind_byte_len);
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 110)
                            .expect("valid P0110 code");
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "unknown macro fragment kind: '{}'",
                                site.kind_str
                            ))
                            .with_span(kind_span)
                            .finish();
                        self.emit_diagnostic(diag);

                        let site_pos = (start + site.site_start) as u32;
                        let site_len = (site.site_end - site.site_start) as u32;
                        out.push(MacroPatternElem::Literal {
                            span: Span::new(file, site_pos, site_len),
                        });
                    }
                }
                RawPatternElem::Repetition {
                    inner,
                    sep,
                    min,
                    start_off,
                    end_off,
                } => {
                    // Recurse to flatten fragments inside the group into
                    // the fragment projection — downstream matchers key
                    // by name only and expect a flat lookup shape.
                    let inner_elems =
                        self.materialize_pattern_elems(file, start, pattern_text, inner, fragments);
                    let separator = sep.map(|(s, e)| {
                        Span::new(file, (start + s) as u32, (e - s) as u32)
                    });
                    let group_span = Span::new(
                        file,
                        (start + *start_off) as u32,
                        (*end_off - *start_off) as u32,
                    );
                    out.push(MacroPatternElem::Repetition {
                        inner: inner_elems,
                        separator,
                        min: *min,
                        span: group_span,
                    });
                }
            }
        }
        out
    }

    /// Extract the structured template element list from `template_span`
    /// by scanning source bytes for `$name` fragment references and
    /// `$( ... )SEP? *` repetition groups; every span in between becomes
    /// a `MacroTemplateElem::Literal`.
    ///
    /// Distinguishing detail vs. the pattern extractor:
    /// - There is no `:kind` suffix; templates carry references only.
    ///   A lone `$` (not followed by an ident-start or `(`) collapses
    ///   into the surrounding literal — the phase-1 string expander
    ///   (`expand_template`) has always accepted stray `$` bytes, so
    ///   the structured form matches that shape rather than reject.
    /// - No diagnostic is emitted here for unbound references; the
    ///   expander (`expand_macro`) will emit M0309 when a `$name`
    ///   fails to resolve in the matched rule's bindings.
    /// - Template repetition groups never carry a `+` terminator —
    ///   templates only *emit*, so multiplicity is inferred from the
    ///   matched pattern side. Both `$( ... )*` and (for grammatical
    ///   symmetry with the pattern) `$( ... )+` are accepted at parse
    ///   time; both produce a `MacroTemplateElem::Repetition`.
    fn extract_macro_template(&mut self, template_span: Span) -> Vec<MacroTemplateElem> {
        let start = template_span.byte_start() as usize;
        let end = (template_span.byte_start() + template_span.byte_len()) as usize;

        // Clone the template text so the arena borrow does not conflict
        // with a live source-string borrow in the second pass.
        let template_text: String = {
            let source = self.source();
            if start >= source.len() || end > source.len() {
                return vec![];
            }
            source[start..end].to_string()
        };

        let mut cursor = 0usize;
        let raw = scan_raw_template_elems(&template_text, &mut cursor, template_text.len(), false);
        self.materialize_template_elems(template_span.file(), start, &template_text, &raw)
    }

    /// Convert a slice of raw template elements into the AST form,
    /// allocating Ident nodes for fragment refs. Recurses into
    /// `Repetition` groups.
    fn materialize_template_elems(
        &mut self,
        file: FileId,
        start: usize,
        template_text: &str,
        raw: &[RawTemplateElem],
    ) -> Vec<MacroTemplateElem> {
        let mut out: Vec<MacroTemplateElem> = Vec::with_capacity(raw.len());
        for r in raw {
            match r {
                RawTemplateElem::Literal { start_off, end_off } => {
                    out.push(MacroTemplateElem::Literal {
                        span: Span::new(
                            file,
                            (start + *start_off) as u32,
                            (*end_off - *start_off) as u32,
                        ),
                    });
                }
                RawTemplateElem::Fragment {
                    site_start,
                    site_end,
                    name_start,
                    name_end,
                } => {
                    let name_span = Span::new(
                        file,
                        (start + *name_start) as u32,
                        (*name_end - *name_start) as u32,
                    );
                    let name_id = self.arena_mut().alloc(NodeKind::Ident, name_span);
                    let ref_span = Span::new(
                        file,
                        (start + *site_start) as u32,
                        (*site_end - *site_start) as u32,
                    );
                    out.push(MacroTemplateElem::Fragment {
                        name: name_id,
                        span: ref_span,
                    });
                }
                RawTemplateElem::Repetition {
                    inner,
                    sep,
                    start_off,
                    end_off,
                } => {
                    let inner_elems =
                        self.materialize_template_elems(file, start, template_text, inner);
                    let separator = sep.map(|(s, e)| {
                        Span::new(file, (start + s) as u32, (e - s) as u32)
                    });
                    out.push(MacroTemplateElem::Repetition {
                        inner: inner_elems,
                        separator,
                        span: Span::new(
                            file,
                            (start + *start_off) as u32,
                            (*end_off - *start_off) as u32,
                        ),
                    });
                }
            }
        }
        out
    }
}

// ─── Raw (byte-offset) intermediate for the pattern/template scanners ─
//
// Kept file-private: the arena-materialising phase in
// `parse_macro.rs::extract_macro_pattern` / `extract_macro_template`
// converts these to `MacroPatternElem` / `MacroTemplateElem`. The two
// scanners recurse into `$( ... )` groups without touching `self`, so
// they operate on the cloned pattern / template text alone and the
// two-pass structure keeps the arena borrow scoped to materialisation.

#[derive(Clone, Debug)]
struct RawFragmentSite {
    site_start: usize,
    site_end: usize,
    name_start: usize,
    name_end: usize,
    kind_str: String,
    kind_start: usize,
    kind_end: usize,
}

#[derive(Clone, Debug)]
enum RawPatternElem {
    Literal {
        start_off: usize,
        end_off: usize,
    },
    Fragment {
        site: RawFragmentSite,
    },
    Repetition {
        inner: Vec<RawPatternElem>,
        sep: Option<(usize, usize)>,
        min: RepMin,
        start_off: usize,
        end_off: usize,
    },
}

#[derive(Clone, Debug)]
enum RawTemplateElem {
    Literal {
        start_off: usize,
        end_off: usize,
    },
    Fragment {
        site_start: usize,
        site_end: usize,
        name_start: usize,
        name_end: usize,
    },
    Repetition {
        inner: Vec<RawTemplateElem>,
        sep: Option<(usize, usize)>,
        start_off: usize,
        end_off: usize,
    },
}

/// Byte-cursor scanner for the pattern side. Recurses into `$(` groups.
///
/// * `text` — the cloned pattern text.
/// * `cursor` — byte offset into `text`; advanced past each parsed
///   element or literal segment.
/// * `end` — byte offset at which scanning stops (either
///   `text.len()` at the top level, or the position of the `)`
///   terminating an enclosing group).
/// * `inside_group` — when true, scanning stops at the matching `)`;
///   at the top level (`inside_group = false`) any bare `)` is
///   accepted as literal text.
///
/// Emits `Literal { .. }` runs between structural markers (`$name:kind`
/// or `$( ... )`), and produces `Fragment { .. }` / `Repetition { .. }`
/// for each recognised site. Malformed sites collapse into the
/// surrounding literal run — the arena-materialising phase reports
/// P0110 for unknown kinds, and the matcher / expander surfaces
/// runtime shape errors.
fn scan_raw_pattern_elems(
    text: &str,
    cursor: &mut usize,
    end: usize,
    inside_group: bool,
) -> Vec<RawPatternElem> {
    let mut out: Vec<RawPatternElem> = Vec::new();
    let mut lit_start: Option<usize> = Some(*cursor);
    while *cursor < end {
        let rest = &text[*cursor..end];
        let ch = match rest.chars().next() {
            Some(c) => c,
            None => break,
        };
        let ch_len = ch.len_utf8();

        // `)` at group depth 0 (inside_group=true) closes the group.
        if inside_group && ch == ')' {
            if let Some(ls) = lit_start.take()
                && *cursor > ls
            {
                out.push(RawPatternElem::Literal { start_off: ls, end_off: *cursor });
            }
            return out;
        }

        // Try `$(` — open a repetition group.
        if ch == '$'
            && rest[ch_len..].chars().next() == Some('(')
        {
            let group_start = *cursor;
            if let Some(ls) = lit_start.take()
                && group_start > ls
            {
                out.push(RawPatternElem::Literal { start_off: ls, end_off: group_start });
            }
            // Advance past `$(`.
            *cursor += ch_len + '('.len_utf8();
            // Recurse into inner.
            let inner = scan_raw_pattern_elems(text, cursor, end, true);
            // Expect `)` at cursor.
            if text[*cursor..end].chars().next() == Some(')') {
                *cursor += ')'.len_utf8();
            }
            // Skip separator + terminator (`*` or `+`), tolerating whitespace.
            let (sep, min, term_end) = scan_rep_terminator(text, *cursor, end);
            *cursor = term_end;
            out.push(RawPatternElem::Repetition {
                inner,
                sep,
                min,
                start_off: group_start,
                end_off: *cursor,
            });
            lit_start = Some(*cursor);
            continue;
        }

        // Try `$name:kind` — a fragment site.
        if ch == '$'
            && let Some(next_ch) = rest[ch_len..].chars().next()
            && (next_ch.is_alphabetic() || next_ch == '_')
        {
            let site_start = *cursor;
            // Scan the name.
            let name_start = site_start + ch_len;
            let mut name_end = name_start;
            for (off, c) in text[name_start..end].char_indices() {
                if c.is_alphanumeric() || c == '_' {
                    name_end = name_start + off + c.len_utf8();
                } else {
                    break;
                }
            }
            // Require `:kind`.
            if text[name_end..end].chars().next() == Some(':') {
                let kind_start = name_end + ':'.len_utf8();
                let mut kind_end = kind_start;
                for (off, c) in text[kind_start..end].char_indices() {
                    if c.is_alphanumeric() || c == '_' {
                        kind_end = kind_start + off + c.len_utf8();
                    } else {
                        break;
                    }
                }
                if kind_end > kind_start {
                    if let Some(ls) = lit_start.take()
                        && site_start > ls
                    {
                        out.push(RawPatternElem::Literal { start_off: ls, end_off: site_start });
                    }
                    let site = RawFragmentSite {
                        site_start,
                        site_end: kind_end,
                        name_start,
                        name_end,
                        kind_str: text[kind_start..kind_end].to_string(),
                        kind_start,
                        kind_end,
                    };
                    out.push(RawPatternElem::Fragment { site });
                    *cursor = kind_end;
                    lit_start = Some(*cursor);
                    continue;
                }
            }
            // Malformed — fall through and treat as literal char.
        }

        // Regular char — extend the literal run.
        *cursor += ch_len;
    }

    if let Some(ls) = lit_start
        && *cursor > ls
    {
        out.push(RawPatternElem::Literal { start_off: ls, end_off: *cursor });
    }
    out
}

/// Byte-cursor scanner for the template side. Same recursion shape as
/// [`scan_raw_pattern_elems`] but with no `:kind` suffix on fragment
/// references and no `+` / `*` distinction on repetition groups
/// (multiplicity is a matcher concern, not the emitter's — templates
/// just replay the matched count).
fn scan_raw_template_elems(
    text: &str,
    cursor: &mut usize,
    end: usize,
    inside_group: bool,
) -> Vec<RawTemplateElem> {
    let mut out: Vec<RawTemplateElem> = Vec::new();
    let mut lit_start: Option<usize> = Some(*cursor);
    while *cursor < end {
        let rest = &text[*cursor..end];
        let ch = match rest.chars().next() {
            Some(c) => c,
            None => break,
        };
        let ch_len = ch.len_utf8();

        if inside_group && ch == ')' {
            if let Some(ls) = lit_start.take()
                && *cursor > ls
            {
                out.push(RawTemplateElem::Literal { start_off: ls, end_off: *cursor });
            }
            return out;
        }

        if ch == '$'
            && rest[ch_len..].chars().next() == Some('(')
        {
            let group_start = *cursor;
            if let Some(ls) = lit_start.take()
                && group_start > ls
            {
                out.push(RawTemplateElem::Literal { start_off: ls, end_off: group_start });
            }
            *cursor += ch_len + '('.len_utf8();
            let inner = scan_raw_template_elems(text, cursor, end, true);
            if text[*cursor..end].chars().next() == Some(')') {
                *cursor += ')'.len_utf8();
            }
            let (sep, _min, term_end) = scan_rep_terminator(text, *cursor, end);
            *cursor = term_end;
            out.push(RawTemplateElem::Repetition {
                inner,
                sep,
                start_off: group_start,
                end_off: *cursor,
            });
            lit_start = Some(*cursor);
            continue;
        }

        if ch == '$'
            && let Some(next_ch) = rest[ch_len..].chars().next()
            && (next_ch.is_alphabetic() || next_ch == '_')
        {
            let site_start = *cursor;
            let name_start = site_start + ch_len;
            let mut name_end = name_start;
            for (off, c) in text[name_start..end].char_indices() {
                if c.is_alphanumeric() || c == '_' {
                    name_end = name_start + off + c.len_utf8();
                } else {
                    break;
                }
            }
            if name_end > name_start {
                if let Some(ls) = lit_start.take()
                    && site_start > ls
                {
                    out.push(RawTemplateElem::Literal { start_off: ls, end_off: site_start });
                }
                out.push(RawTemplateElem::Fragment {
                    site_start,
                    site_end: name_end,
                    name_start,
                    name_end,
                });
                *cursor = name_end;
                lit_start = Some(*cursor);
                continue;
            }
        }

        *cursor += ch_len;
    }

    if let Some(ls) = lit_start
        && *cursor > ls
    {
        out.push(RawTemplateElem::Literal { start_off: ls, end_off: *cursor });
    }
    out
}

/// Parse the bytes immediately after a `$( ... )`'s closing paren:
/// an optional single-char separator followed by the required `*` /
/// `+` terminator. Tolerates ASCII whitespace between the pieces.
///
/// Returns `(sep, min, end_cursor)`:
/// * `sep` — `Some((start, end))` when a separator char was consumed
///   (byte range in `text`), else `None`.
/// * `min` — `RepMin::One` for `+`, `RepMin::Zero` for `*` OR when
///   the terminator was absent (permissive fallback so the parser
///   does not blow up on a malformed group).
/// * `end_cursor` — position past the terminator.
///
/// The separator recogniser accepts one character that is neither
/// `*`, `+`, `)`, `$`, alphanumeric, nor `_` — i.e., a
/// punctuation-only separator such as `,` or `;`. If no separator is
/// present the terminator is expected immediately.
fn scan_rep_terminator(
    text: &str,
    start: usize,
    end: usize,
) -> (Option<(usize, usize)>, RepMin, usize) {
    let mut cursor = start;
    // Skip whitespace.
    while cursor < end {
        let c = match text[cursor..end].chars().next() {
            Some(c) => c,
            None => break,
        };
        if c.is_whitespace() {
            cursor += c.len_utf8();
        } else {
            break;
        }
    }
    // Try separator.
    let mut sep: Option<(usize, usize)> = None;
    if let Some(c) = text[cursor..end].chars().next()
        && c != '*'
        && c != '+'
        && c != ')'
        && c != '$'
        && !c.is_alphanumeric()
        && c != '_'
    {
        let s = cursor;
        cursor += c.len_utf8();
        sep = Some((s, cursor));
        // Skip whitespace between sep and terminator.
        while cursor < end {
            let ch = match text[cursor..end].chars().next() {
                Some(c) => c,
                None => break,
            };
            if ch.is_whitespace() {
                cursor += ch.len_utf8();
            } else {
                break;
            }
        }
    }
    // Terminator.
    let (min, term_len) = match text[cursor..end].chars().next() {
        Some('*') => (RepMin::Zero, '*'.len_utf8()),
        Some('+') => (RepMin::One, '+'.len_utf8()),
        _ => (RepMin::Zero, 0),
    };
    (sep, min, cursor + term_len)
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

    #[test]
    fn multi_rule_macro_two_rules_parses() {
        let (arena, result, diags) = parse_source_str(
            "macro twice { ($x:expr) => { x + x } ; ($x:expr, $y:expr) => { x + y } }",
        );
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Extract the macro from the root structure
        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                if let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) {
                    assert_eq!(
                        decl.rules.len(),
                        2,
                        "should have exactly two rules in the macro"
                    );
                } else {
                    panic!("expected MacroDecl item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    #[test]
    fn multi_rule_macro_three_rules_parses() {
        let (arena, result, diags) = parse_source_str(
            "macro choose { ($x:expr) => x ; ($x:expr, $y:expr) => { x + y } ; ($x:expr, $y:expr, $z:expr) => { x + y + z } }",
        );
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                if let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) {
                    assert_eq!(
                        decl.rules.len(),
                        3,
                        "should have exactly three rules in the macro"
                    );
                } else {
                    panic!("expected MacroDecl item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    #[test]
    fn single_rule_form_still_parses() {
        let (arena, result, diags) = parse_source_str("macro foo($x:expr) => x + 1");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        // Verify single rule
        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                if let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) {
                    assert_eq!(
                        decl.rules.len(),
                        1,
                        "single-rule form should have exactly one rule"
                    );
                } else {
                    panic!("expected MacroDecl item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    #[test]
    fn multi_rule_trailing_semi_ok() {
        let (_arena, result, diags) = parse_source_str("macro foo { ($x:expr) => { x } ; }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");
    }

    #[test]
    fn multi_rule_with_nested_braces_ok() {
        let (arena, result, diags) =
            parse_source_str("macro nested { ($x:expr) => { { inner } } ; ($y:expr) => { y } }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                if let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) {
                    assert_eq!(decl.rules.len(), 2, "should have two rules");
                } else {
                    panic!("expected MacroDecl item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    #[test]
    fn macro_in_structure_body_parses() {
        let (arena, result, diags) =
            parse_source_str("module M = structure { macro foo($x:expr) => $x + 1 }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                // The structure contains a Module item
                if let Some(ItemData::Module { body, .. }) = arena.item_data(items[0]) {
                    if let Some(ItemData::Structure {
                        items: body_items, ..
                    }) = arena.item_data(*body)
                    {
                        assert!(
                            !body_items.is_empty(),
                            "module structure should have at least one item"
                        );
                        if let Some(ItemData::MacroDecl(_decl)) = arena.item_data(body_items[0]) {
                            // Success: macro was parsed inside the structure body
                        } else {
                            panic!("expected MacroDecl in structure body");
                        }
                    } else {
                        panic!("expected Structure as module body");
                    }
                } else {
                    panic!("expected Module item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    #[test]
    fn macro_in_nested_structure_parses() {
        let (arena, result, diags) = parse_source_str(
            "module M = structure { module Inner = structure { macro f($x:expr) => $x } }",
        );
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                // First item is outer Module
                if let Some(ItemData::Module {
                    body: outer_body, ..
                }) = arena.item_data(items[0])
                {
                    if let Some(ItemData::Structure {
                        items: outer_items, ..
                    }) = arena.item_data(*outer_body)
                    {
                        assert!(!outer_items.is_empty(), "outer structure should have items");
                        // First item in outer structure is inner Module
                        if let Some(ItemData::Module {
                            body: inner_body, ..
                        }) = arena.item_data(outer_items[0])
                        {
                            if let Some(ItemData::Structure {
                                items: inner_items, ..
                            }) = arena.item_data(*inner_body)
                            {
                                assert!(
                                    !inner_items.is_empty(),
                                    "inner structure should have items"
                                );
                                if let Some(ItemData::MacroDecl(_decl)) =
                                    arena.item_data(inner_items[0])
                                {
                                    // Success: macro was parsed in nested structure
                                } else {
                                    panic!("expected MacroDecl in inner structure");
                                }
                            } else {
                                panic!("expected Structure as inner module body");
                            }
                        } else {
                            panic!("expected Module as first item in outer structure");
                        }
                    } else {
                        panic!("expected Structure as outer module body");
                    }
                } else {
                    panic!("expected Module item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    #[test]
    fn macro_then_let_in_structure_parses() {
        let (arena, result, diags) =
            parse_source_str("module M = structure { macro foo($x:expr) => $x + 1 ; let y = 42 }");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                if let Some(ItemData::Module { body, .. }) = arena.item_data(items[0]) {
                    if let Some(ItemData::Structure {
                        items: body_items, ..
                    }) = arena.item_data(*body)
                    {
                        assert_eq!(body_items.len(), 2, "structure should have exactly 2 items");
                        if let Some(ItemData::MacroDecl(_decl)) = arena.item_data(body_items[0]) {
                            if let Some(ItemData::Let { .. }) = arena.item_data(body_items[1]) {
                                // Success: both macro and let parsed correctly
                            } else {
                                panic!("expected Let as second item");
                            }
                        } else {
                            panic!("expected MacroDecl as first item");
                        }
                    } else {
                        panic!("expected Structure as module body");
                    }
                } else {
                    panic!("expected Module item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }

    // ─── Slice C (PAS-DEBT-B2-010c, #1542) repetition parsing ─────────

    /// Extract the first macro rule from a parse result, panicking with
    /// context if the source did not produce a `MacroDecl`.
    fn first_rule<'a>(arena: &'a paideia_as_ast::AstArena, root: NodeId) -> &'a MacroRule {
        let Some(ItemData::Structure { items, .. }) = arena.item_data(root) else {
            panic!("expected Structure root");
        };
        let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) else {
            panic!("expected MacroDecl");
        };
        &decl.rules[0]
    }

    #[test]
    fn slice_c_pattern_star_repetition_parses() {
        let (arena, result, diags) =
            parse_source_str("macro list($($x:expr),*) => { [ $($x),* ] }");
        let root = result.expect("should parse");
        assert!(
            diags.iter().all(|d| d.code().severity() != Severity::Error),
            "unexpected parse errors: {diags:?}",
        );
        let rule = first_rule(&arena, root);
        let has_rep = rule.pattern_elems.iter().any(|e| {
            matches!(e, MacroPatternElem::Repetition { min: RepMin::Zero, .. })
        });
        assert!(has_rep, "expected a `*` repetition group in pattern_elems");
        // fragment projection flattens the interior fragment.
        assert!(
            rule.fragments.iter().any(|f| f.kind == MacroFragmentKind::Expr),
            "flat fragment projection should include the inner $x:expr",
        );
    }

    #[test]
    fn slice_c_pattern_plus_repetition_parses() {
        let (arena, result, diags) =
            parse_source_str("macro nonempty($($x:expr),+) => { first($($x),+) }");
        let root = result.expect("should parse");
        assert!(
            diags.iter().all(|d| d.code().severity() != Severity::Error),
            "unexpected parse errors: {diags:?}",
        );
        let rule = first_rule(&arena, root);
        let has_plus = rule.pattern_elems.iter().any(|e| {
            matches!(e, MacroPatternElem::Repetition { min: RepMin::One, .. })
        });
        assert!(has_plus, "expected a `+` repetition group in pattern_elems");
    }

    #[test]
    fn slice_c_pattern_repetition_without_separator() {
        let (arena, result, diags) = parse_source_str("macro tokens($($x:tt)*) => { $($x)* }");
        let root = result.expect("should parse");
        assert!(
            diags.iter().all(|d| d.code().severity() != Severity::Error),
            "unexpected parse errors: {diags:?}",
        );
        let rule = first_rule(&arena, root);
        let sep_is_none = rule.pattern_elems.iter().any(|e| {
            matches!(e, MacroPatternElem::Repetition { separator: None, .. })
        });
        assert!(sep_is_none, "unseparated `*` repetition should carry separator = None");
    }

    #[test]
    fn slice_c_template_repetition_parses() {
        let (arena, result, diags) =
            parse_source_str("macro list($($x:expr),*) => { [ $($x),* ] }");
        let root = result.expect("should parse");
        assert!(
            diags.iter().all(|d| d.code().severity() != Severity::Error),
            "unexpected parse errors: {diags:?}",
        );
        let rule = first_rule(&arena, root);
        let has_rep = rule.template_elems.iter().any(|e| {
            matches!(e, MacroTemplateElem::Repetition { .. })
        });
        assert!(has_rep, "expected a repetition group in template_elems");
    }

    #[test]
    fn top_level_macro_still_parses() {
        let (arena, result, diags) = parse_source_str("macro foo($x:expr) => $x + 1");
        assert!(result.is_ok(), "should parse successfully");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.code().severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "should have no parse errors");

        if let Ok(root_id) = result {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(root_id) {
                assert!(!items.is_empty(), "should have at least one item");
                if let Some(ItemData::MacroDecl(decl)) = arena.item_data(items[0]) {
                    assert_eq!(decl.rules.len(), 1, "should have one rule");
                } else {
                    panic!("expected MacroDecl item");
                }
            } else {
                panic!("expected Structure root");
            }
        }
    }
}
