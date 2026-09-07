//! Let-declaration parsing plus `@align` / `@ring` / `@link_section` / `@abi` symbol attributes.
//! Split out of `parse_item.rs` (2026-07-08).
//!
//! **Module layout (2026-09-07 split from 1,621-LOC single file — paideia-as#1408):**
//! - This `mod.rs` — the [`LetSymbolAttrs`] struct, the top-level
//!   [`Parser::parse_let_decl_with_visibility`] / [`Parser::parse_let_decl`] entry
//!   points, and the trailing-attribute dispatcher
//!   [`Parser::parse_optional_symbol_attributes`] that walks the `@...` run and
//!   calls each per-attribute parser in turn.
//! - `attrs_layout` — memory-layout attribute parsers: `@align(N)`, `@ring(...)`,
//!   `@link_section("name")`.
//! - `attrs_semantics` — execution-semantics attribute parsers: `@abi("...")`,
//!   `@interrupt(...)` / `@interrupt_error(...)` (with the canonical vector
//!   table), `@atomic(Ordering)`.
//! - `tests` — the full unit-test suite for the above.
//!
//! Every public path — `paideia_as_parser::parse_item::let_item::…` — is
//! preserved across the split; the sub-files keep methods on `impl Parser`
//! and remain `pub(super)`-visible to this `mod.rs` (which is the parent
//! `parse_item::let_item` module).

use std::collections::HashSet;

use paideia_as_ast::{AtomicOrdering, CallingConvention, InterruptAttr, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

// --- Split sub-modules (2026-09-07 refactor, paideia-as#1408) ---
mod attrs_layout;
mod attrs_semantics;

#[cfg(test)]
mod tests;

/// Structured container for trailing symbol attributes on Let bindings.
/// Refactored in PA19-r19-001 to eliminate growing tuple.
pub(super) struct LetSymbolAttrs {
    pub align: Option<u32>,
    pub ring: Option<(u32, u32)>,
    pub link_section: Option<String>,
    pub abi: Option<CallingConvention>,
    /// `@no_frame` opt-out for the default SysV frame prologue/epilogue
    /// (paideia-as#1276, unblocks paideia-os#716). Bare flag — no arguments.
    ///
    /// Phase-1 landing (parser + AST/IR plumbing): the flag is captured here and
    /// propagated to [`paideia_as_ast::ItemData::Let::no_frame`] / IR `LetInfo::no_frame`,
    /// but the elaborator emit pass still ignores it. Non-function placement is not
    /// yet diagnosed (deferred to the phase that wires the emit change).
    pub no_frame: bool,
    /// `@interrupt("vec_name")` or `@interrupt_error("vec_name")` ISR sugar
    /// (paideia-as#1278, v0.21-002). Phase-1 landing captures the parsed
    /// [`InterruptAttr`] here and propagates it into
    /// [`paideia_as_ast::ItemData::Let::interrupt`]; the elaborator emit-side
    /// synthesis (spill/iretq/error-code skip) lands in a phase-2 follow-up.
    pub interrupt: Option<InterruptAttr>,
    /// `@atomic(Ordering)` binding-position discipline
    /// (paideia-as#1301, v0.21-003c). When `Some(o)`, the caller stamps the
    /// resulting let node into `AstArena::item_atomic` so the elaborator's
    /// `populate_let_meta` propagates it into IR `LetInfo::atomic`, and the
    /// emit walker's load/store sites bracket accesses to this binding with
    /// the ordering-appropriate fences (mfence for SeqCst; nothing for
    /// Relaxed/Acquire/Release under x86_64 TSO).
    pub atomic: Option<AtomicOrdering>,
}

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Walk the trailing `@...` run after a let value expression, dispatching
    /// each attribute name to its per-attribute parser in `attrs_layout` /
    /// `attrs_semantics`. Enforces at-most-once semantics (P0250 / P0283 on
    /// duplicates) and rejects unknown attribute names with P0250.
    pub(super) fn parse_optional_symbol_attributes(&mut self) -> Result<LetSymbolAttrs, ParseError> {
        let mut align: Option<u32> = None;
        let mut ring: Option<(u32, u32)> = None;
        let mut link_section: Option<String> = None;
        let mut abi: Option<CallingConvention> = None;
        let mut no_frame: bool = false;
        let mut interrupt: Option<InterruptAttr> = None;
        let mut atomic: Option<AtomicOrdering> = None;
        let mut seen_attrs = HashSet::new();

        // Loop to accept attributes in any order
        loop {
            if !self.at(TokenKind::At) {
                break;
            }

            self.bump(); // consume `@`

            // Expect an identifier for the attribute name
            let attr_name_tok = self.expect(TokenKind::Ident)?;
            let attr_name = self.source_text_for_span(attr_name_tok.span);

            // Check for duplicate
            if seen_attrs.contains(attr_name) {
                match attr_name {
                    "link_section" => {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 283)
                            .expect("valid P0283 code");
                        let diag = Diagnostic::error(code)
                            .message("duplicate @link_section directive")
                            .with_span(attr_name_tok.span)
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }
                    _ => {
                        let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                            .expect("valid P0250 code");
                        let diag = Diagnostic::error(code)
                            .message(format!("duplicate @{} attribute", attr_name))
                            .with_span(attr_name_tok.span)
                            .finish();
                        self.emit_diagnostic(diag);
                        return Err(ParseError);
                    }
                }
            }
            seen_attrs.insert(attr_name.to_string());

            match attr_name {
                "align" => {
                    align = Some(self.parse_align_attr()?);
                }
                "ring" => {
                    ring = Some(self.parse_ring_attr(attr_name_tok.span)?);
                }
                "link_section" => {
                    link_section = Some(self.parse_link_section_attr()?);
                }
                "abi" => {
                    abi = Some(self.parse_abi_attr()?);
                }
                "no_frame" => {
                    // paideia-as#1276 (phase 1): bare flag, no arguments.
                    // Recognized here so the elaborator can consult it at emit time
                    // in a later phase. Placement validation (must-be-function) lands
                    // with the emit change; phase 1 keeps the attribute inert.
                    no_frame = true;
                }
                "interrupt" | "interrupt_error" => {
                    // paideia-as#1278 (phase 1): `@interrupt("vec_name")` /
                    // `@interrupt_error("vec_name")` — ISR-entry sugar over
                    // `@no_frame`. Phase-1 landing captures the parsed
                    // InterruptAttr for later emit-side synthesis; the
                    // `_error` variant marks the vector as having a
                    // CPU-pushed error code (skipped before iretq).
                    let has_error_code = attr_name == "interrupt_error";
                    interrupt = Some(self.parse_interrupt_attr(has_error_code)?);
                }
                "atomic" => {
                    // paideia-as#1301 (v0.21-003c, phase-2): `@atomic(Ordering)`
                    // trailing symbol attribute on item-position lets. Enables the
                    // fixture `pub let mut counter : u64 = 0 @atomic(SeqCst);`.
                    // Ordering is one of Relaxed / Acquire / Release / SeqCst
                    // (case-sensitive), matching the C11 / Rust vocabulary and
                    // the statement-position spelling already accepted by
                    // parse_stmt.rs::parse_optional_atomic_prefix. Parser diagnoses
                    // malformed syntax in place; the elaborator's populate_let_meta
                    // reads the value from AstArena::item_atomic and stamps it into
                    // IR `LetInfo::atomic`. Emit changes at load/store sites gate
                    // fence emission on that IR field.
                    atomic = Some(self.parse_atomic_attr(attr_name_tok.span)?);
                }
                _ => {
                    // P0250: unknown symbol attribute
                    let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                        .expect("valid P0250 code");
                    let diag = Diagnostic::error(code)
                        .message(format!("unknown symbol attribute '@{}' (only 'align', 'ring', 'link_section', 'abi', 'no_frame', 'interrupt', 'interrupt_error', and 'atomic' supported)", attr_name))
                        .with_span(attr_name_tok.span)
                        .finish();
                    self.emit_diagnostic(diag);
                    return Err(ParseError);
                }
            }
        }

        Ok(LetSymbolAttrs { align, ring, link_section, abi, no_frame, interrupt, atomic })
    }

    /// Parse a top-level let declaration with optional visibility: `[pub] let [mut] <Ident> <GenericParams>? (: Type)? = Expr @align(N)? @ring(...)? @link_section("name")? @abi("ms"|"sysv")?`
    pub(super) fn parse_let_decl_with_visibility(&mut self, public: bool) -> Result<NodeId, ParseError> {
        let let_tok = self.expect(TokenKind::KwLet)?;
        let span_start = let_tok.span;

        // `pub` is consumed by the caller (parse_item dispatcher) and passed in.
        // Do NOT re-check for KwPub here.

        // Check for optional `mut` keyword
        let mutable = if self.at(TokenKind::KwMut) {
            self.bump();
            true
        } else {
            false
        };

        // Try to parse a pattern first (could be a tuple, struct, enum variant, etc.)
        // If that fails, fall back to parsing a simple identifier.
        // Peek ahead to see if we have a pattern or just a name.
        let mut pattern_or_name = None;

        // Check if the next token looks like a pattern start
        if let Some(tok) = self.peek() {
            match tok.kind {
                // These are pattern starters
                TokenKind::LParen => {
                    // This is a pattern
                    pattern_or_name = Some(self.parse_pattern()?);
                }
                TokenKind::Ident => {
                    // Could be a pattern or just an identifier name.
                    // Peek at the next token to disambiguate.
                    if let Some(next_tok) = self.peek_at(1) {
                        match next_tok.kind {
                            // These indicate a pattern
                            TokenKind::ColonColon | TokenKind::LBrace => {
                                pattern_or_name = Some(self.parse_pattern()?);
                            }
                            // Otherwise just an identifier
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // If we didn't parse a pattern, just parse a simple identifier
        let name_id = if let Some(pat) = pattern_or_name {
            pat
        } else {
            let name_tok = self.expect(TokenKind::Ident)?;
            self.arena_mut().alloc(NodeKind::Ident, name_tok.span)
        };

        // Optional generic parameters: `< T, U: Trait >`
        let generic_params = if self.at(TokenKind::Lt) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // Optional type annotation
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect `=`
        self.expect(TokenKind::Assign)?;

        // Parse value expression
        let value = self.parse_expr()?;

        // Parse optional symbol attributes (@align, @ring, @link_section, @abi, @no_frame,
        // @interrupt, @interrupt_error, or @atomic).
        let LetSymbolAttrs { align, ring, link_section, abi, no_frame, interrupt, atomic } = self.parse_optional_symbol_attributes()?;

        // paideia-as#1276 phase 3: `@no_frame` is a function-only attribute — it
        // toggles the SysV frame-pointer prologue/epilogue that the elaborator
        // emits around `fn (...)` lambdas. Applying it to a non-function
        // binding (`pub let CONST : u64 = 42 @no_frame`) is a category error:
        // there is no lambda / no ret / no prologue to suppress. Reject at
        // parse time via P0250 so downstream phases can trust that any
        // `LetInfo::no_frame == true` corresponds to a Lambda RHS.
        //
        // paideia-as#1278 phase 1: `@interrupt(...)` and `@interrupt_error(...)`
        // share the same category constraint — an ISR entry stub only makes
        // sense wrapping a lambda body. Reject non-lambda placement here so
        // phase-2 emit synthesis can trust the shape.
        if no_frame || interrupt.is_some() {
            let value_kind = self
                .arena()
                .get(value)
                .map(|n| n.kind);
            if value_kind != Some(NodeKind::ExprLambda) {
                let value_span = self
                    .arena()
                    .get(value)
                    .map(|n| n.span)
                    .unwrap_or(span_start);
                let code = DiagnosticCode::new(Category::P, Severity::Error, 250)
                    .expect("valid P0250 code");
                let msg = if interrupt.is_some() {
                    "@interrupt / @interrupt_error is a function-only attribute; ISR entry sugar can only wrap a lambda body"
                } else {
                    "@no_frame is a function-only attribute; it toggles the SysV frame-pointer prologue/epilogue and cannot be applied to a non-function binding"
                };
                let diag = Diagnostic::error(code)
                    .message(msg)
                    .with_span(value_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        }

        // Consume optional `;`
        self.eat(TokenKind::Semicolon);

        // Compute span
        let value_span = self
            .arena()
            .get(value)
            .map(|n| n.span)
            .unwrap_or(span_start);
        let span = Span::new(
            span_start.file(),
            span_start.byte_start(),
            value_span.byte_start() + value_span.byte_len() - span_start.byte_start(),
        );

        let item = self.arena_mut().alloc_item(
            NodeKind::Let,
            span,
            ItemData::Let {
                public,
                mutable,
                name: name_id,
                generic_params,
                ty,
                value,
                align,
                ring,
                link_section,
                abi,
                no_frame,
                interrupt,
                doc: None,
            },
        );

        // paideia-as#1301 (v0.21-003c): stamp the item-level `@atomic(Ordering)`
        // ordering onto the AST arena's item_atomic side-table, keyed by the
        // freshly allocated Let node. Kept off ItemData::Let to avoid touching
        // every construction / destructuring site of the enum. Elaborator
        // `populate_let_meta` reads back the ordering here and forwards it into
        // IR `LetInfo::atomic`.
        if let Some(ord) = atomic {
            self.arena_mut().item_atomic_mut().insert(item, ord);
        }

        Ok(item)
    }

    /// Wrapper for backward compatibility and simplicity when public visibility is not needed.
    pub(super) fn parse_let_decl(&mut self) -> Result<NodeId, ParseError> {
        self.parse_let_decl_with_visibility(false)
    }
}
