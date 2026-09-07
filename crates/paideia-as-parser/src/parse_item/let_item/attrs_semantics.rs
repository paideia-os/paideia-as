//! Execution-semantics trailing attribute parsers for let bindings:
//! `@abi("ms"|"sysv")`, `@interrupt("vec")` / `@interrupt_error("vec")`,
//! `@atomic(Ordering)`.
//!
//! Split out of `let_item.rs` (paideia-as#1408, 2026-09-07). Also hosts the
//! private canonical-vector table + resolver used exclusively by
//! [`Parser::parse_interrupt_attr`]. Each parser here is called from
//! [`crate::parse_item::let_item::Parser::parse_optional_symbol_attributes`]
//! after the leading `@name` has been consumed; the parsers below own the
//! `( ... )` argument list and its diagnostics only.

use paideia_as_ast::{AtomicOrdering, CallingConvention, InterruptAttr};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

/// Canonical x86_64 exception / IRQ names → vector number (paideia-as#1278).
///
/// Covers the exception vectors 0..=31 (Intel SDM Vol. 3A §6.15) plus the
/// LAPIC-timer / spurious / TLB-shootdown IPI vectors paideia-os wires today
/// (see `src/kernel/core/int/isr_trampoline.pdx`). A caller who needs a
/// vector outside this table can still pass the decimal spelling — e.g.
/// `@interrupt("42")` — which the numeric branch of [`resolve_vector_name`]
/// accepts unconditionally in `0..=255`.
const CANONICAL_VECTORS: &[(&str, u8)] = &[
    // Faults / traps / aborts (0..=31), Intel SDM Vol. 3A §6.15.
    ("divide_error",         0),
    ("div_by_zero",          0),
    ("debug",                1),
    ("nmi",                  2),
    ("breakpoint",           3),
    ("int3",                 3),
    ("overflow",             4),
    ("bound_range",          5),
    ("invalid_opcode",       6),
    ("device_not_available", 7),
    ("double_fault",         8),   // error-code vector
    ("invalid_tss",         10),   // error-code vector
    ("segment_not_present", 11),   // error-code vector
    ("stack_segment_fault", 12),   // error-code vector
    ("general_protection",  13),   // error-code vector
    ("page_fault",          14),   // error-code vector
    ("x87_fp",              16),
    ("alignment_check",     17),   // error-code vector
    ("machine_check",       18),
    ("simd_fp",             19),
    ("virtualization",      20),
    ("control_protection",  21),   // error-code vector
    ("hypervisor_injection",28),
    ("vmm_communication",   29),   // error-code vector
    ("security",            30),   // error-code vector
    // IRQ vectors paideia-os wires today (see isr_trampoline.pdx entry stubs).
    ("timer",               32),
    ("keyboard",            33),
    ("cascade",             34),
    ("com1",                36),
    ("apic_timer",         240),
    ("apic_spurious",      241),
    ("tlb_shootdown_ipi",  242),
];

/// Resolve a `@interrupt("...")` argument to a vector number.
///
/// Accepts a canonical name from [`CANONICAL_VECTORS`] (case-sensitive) OR
/// the decimal spelling of any number in `0..=255`. Returns `None` for both
/// unknown names and out-of-range numeric input.
fn resolve_vector_name(name: &str) -> Option<u8> {
    if let Some((_, v)) = CANONICAL_VECTORS.iter().find(|(k, _)| *k == name) {
        return Some(*v);
    }
    name.parse::<u16>().ok().and_then(|n| u8::try_from(n).ok())
}

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Parse `@abi("ms"|"sysv")` where the argument is a calling convention string.
    /// Validates: only "ms" or "sysv" (lowercase, case-sensitive).
    /// Emits P0285 for invalid strings, non-strings, empty strings, or missing parens.
    pub(super) fn parse_abi_attr(&mut self) -> Result<CallingConvention, ParseError> {
        // Expect `(`
        if !self.eat(TokenKind::LParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("malformed @abi(...) syntax: expected '(' after 'abi'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Check for string literal; emit P0285 if it's not a string
        if !self.at(TokenKind::StringLit) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("@abi value must be a string literal, expected \"ms\" or \"sysv\"")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            // Skip the invalid token
            self.bump();
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let str_span = str_tok.span;
        let str_text = self.source_text_for_span(str_span);

        // Extract string content (without quotes)
        let value = if str_text.starts_with('"') && str_text.ends_with('"') && str_text.len() >= 2 {
            str_text[1..str_text.len() - 1].to_string()
        } else {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("@abi value must be a valid string literal")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        // Validate: non-empty
        if value.is_empty() {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("@abi value must be non-empty; expected \"ms\" or \"sysv\"")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Validate: only "ms" or "sysv" (lowercase, case-sensitive)
        let cc = match value.as_str() {
            "ms" => CallingConvention::Ms,
            "sysv" => CallingConvention::Sysv,
            _ => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                    .expect("valid P0285 code");
                let diag = Diagnostic::error(code)
                    .message(format!("invalid @abi value \"{}\"; expected \"ms\" or \"sysv\"", value))
                    .with_span(str_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Expect `)`
        if !self.eat(TokenKind::RParen) {
            let span = self
                .peek()
                .map(|t| t.span)
                .unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 285)
                .expect("valid P0285 code");
            let diag = Diagnostic::error(code)
                .message("malformed @abi(...) syntax: expected ')' after value")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(cc)
    }

    /// Parse `@interrupt("vec")` or `@interrupt_error("vec")` where `vec` is
    /// either a canonical x86_64 exception name or the decimal spelling of a
    /// vector number in `0..=255` (paideia-as#1278, v0.21-002).
    ///
    /// **`has_error_code`** — `true` when the caller matched `@interrupt_error`,
    /// `false` for the plain `@interrupt` form; propagated onto the returned
    /// [`InterruptAttr`] so the phase-2 elaborator emit can insert the
    /// CPU-error-code skip before `iretq`.
    ///
    /// **Diagnostics (P-category, sharing the free P0290-P0294 block above `@abi`):**
    /// - P0290 — malformed `(...)` syntax (missing `(`, `)` or non-string arg).
    /// - P0291 — the string does not resolve to a canonical name and is not a
    ///   numeric literal in `0..=255`.
    pub(super) fn parse_interrupt_attr(&mut self, has_error_code: bool) -> Result<InterruptAttr, ParseError> {
        // Expect `(`.
        if !self.eat(TokenKind::LParen) {
            let span = self.peek().map(|t| t.span).unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 290)
                .expect("valid P0290 code");
            let attr = if has_error_code { "interrupt_error" } else { "interrupt" };
            let diag = Diagnostic::error(code)
                .message(format!("malformed @{}(\"vec\") syntax: expected '(' after '{}'", attr, attr))
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        // Argument must be a string literal.
        if !self.at(TokenKind::StringLit) {
            let span = self.peek().map(|t| t.span).unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 290)
                .expect("valid P0290 code");
            let diag = Diagnostic::error(code)
                .message("@interrupt / @interrupt_error argument must be a string literal (canonical name or decimal vector number)")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            self.bump();
            return Err(ParseError);
        }

        let str_tok = self.expect(TokenKind::StringLit)?;
        let str_span = str_tok.span;
        let raw = self.source_text_for_span(str_span);
        let name = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
            raw[1..raw.len() - 1].to_string()
        } else {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 290)
                .expect("valid P0290 code");
            let diag = Diagnostic::error(code)
                .message("@interrupt / @interrupt_error argument must be a valid string literal")
                .with_span(str_span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        };

        // Resolve to a vector number.
        let vector = match resolve_vector_name(&name) {
            Some(v) => v,
            None => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 291)
                    .expect("valid P0291 code");
                let diag = Diagnostic::error(code)
                    .message(format!(
                        "unknown interrupt vector '{}' — expected a canonical name (e.g. \"page_fault\", \"general_protection\", \"breakpoint\") or a decimal number in 0..=255",
                        name
                    ))
                    .with_span(str_span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        // Expect `)`.
        if !self.eat(TokenKind::RParen) {
            let span = self.peek().map(|t| t.span).unwrap_or_else(|| Span::new(self.file(), 0, 0));
            let code = DiagnosticCode::new(Category::P, Severity::Error, 290)
                .expect("valid P0290 code");
            let diag = Diagnostic::error(code)
                .message("malformed @interrupt / @interrupt_error syntax: expected ')' after vector name")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(InterruptAttr { has_error_code, vector, name })
    }

    /// Parse `@atomic(Ordering)` — the item-position mirror of
    /// `parse_stmt::parse_optional_atomic_prefix`. Called from
    /// `parse_optional_symbol_attributes` after the `@atomic` keyword has been
    /// recognised. Consumes `( <Ordering> )` where `<Ordering>` is one of
    /// `Relaxed`, `Acquire`, `Release`, `SeqCst` (case-sensitive). Reuses the
    /// P0287 / P0288 / P0289 codes assigned to `@atomic` at statement position
    /// so the diagnostic surface stays uniform across the two entry points.
    pub(super) fn parse_atomic_attr(&mut self, attr_span: Span) -> Result<AtomicOrdering, ParseError> {
        if !self.eat(TokenKind::LParen) {
            let span = self.peek().map(|t| t.span).unwrap_or(attr_span);
            let code = DiagnosticCode::new(Category::P, Severity::Error, 287)
                .expect("valid P0287 code");
            let diag = Diagnostic::error(code)
                .message("malformed @atomic(Ordering) syntax: expected '(' after 'atomic'")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        let ord_tok = self.expect(TokenKind::Ident)?;
        let ord_text = self.source_text_for_span(ord_tok.span);
        let ordering = match ord_text {
            "Relaxed" => AtomicOrdering::Relaxed,
            "Acquire" => AtomicOrdering::Acquire,
            "Release" => AtomicOrdering::Release,
            "SeqCst" => AtomicOrdering::SeqCst,
            other => {
                let code = DiagnosticCode::new(Category::P, Severity::Error, 288)
                    .expect("valid P0288 code");
                let diag = Diagnostic::error(code)
                    .message(format!(
                        "unknown atomic ordering '{}' (expected one of: Relaxed, Acquire, Release, SeqCst)",
                        other
                    ))
                    .with_span(ord_tok.span)
                    .finish();
                self.emit_diagnostic(diag);
                return Err(ParseError);
            }
        };

        if !self.eat(TokenKind::RParen) {
            let span = self.peek().map(|t| t.span).unwrap_or(ord_tok.span);
            let code = DiagnosticCode::new(Category::P, Severity::Error, 289)
                .expect("valid P0289 code");
            let diag = Diagnostic::error(code)
                .message("malformed @atomic(Ordering) syntax: expected ')' after ordering name")
                .with_span(span)
                .finish();
            self.emit_diagnostic(diag);
            return Err(ParseError);
        }

        Ok(ordering)
    }
}
