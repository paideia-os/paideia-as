//! `@packed_struct` struct-level attribute parser
//! (paideia-as#1373, v0.28-M1-004, Wave 0 Batch 2).
//!
//! # Grammar
//!
//! ```text
//! PackedStructDecl :=
//!     "@packed_struct" ( "(" "align" "=" IntLit ")" )?
//!     "struct" Ident GenericParams? "{" Field* "}"
//! ```
//!
//! # Semantics
//!
//! - **Layout:** dense — the elaborator emits each field at its
//!   no-padding offset (running sum of preceding field sizes,
//!   honouring per-field endianness).
//! - **Alignment:** forced to `align` when the argument is present,
//!   else 1. When present, `align` is a positive power of two — the
//!   parser rejects zero and non-powers-of-two in place (P0293) so
//!   downstream phases can trust the invariant.
//! - **Placement:** the attribute may only prefix a `struct` decl.
//!   Any other keyword after the (optional) argument list is rejected
//!   (P0295) — this is the "reject on non-struct decls" rule from the
//!   issue spec.
//!
//! # Composition with `@endian` (paideia-as#1372, b2-05)
//!
//! `@packed_struct` is a struct-level attribute; `@endian(be|le)` is a
//! field-level attribute (see [`paideia_as_ast::FieldAttr`]). Composition
//! is orthogonal: packing fixes each field's byte offset, and the
//! endian annotation fixes each field's byte order at load/store sites.
//! The parser accepts the two on the same declaration without ordering
//! ambiguity — `@packed_struct` is prefixed to the whole decl, `@endian`
//! is prefixed to individual fields — and the resulting struct-attr and
//! field-attr side-tables are populated independently.
//!
//! # AST landing
//!
//! On success, the returned [`NodeId`] is the ordinary struct-decl node
//! produced by `Parser::parse_struct_decl`, and a
//! [`StructAttr::Packed { align }`] entry is pushed onto that node's
//! slot in [`paideia_as_ast::StructAttrTable`] (accessed via
//! `AstArena::struct_attr_mut`). Phase-1 landing is parser + AST-only;
//! the elaborator layout pass that consumes the attribute lands in a
//! subsequent Wave-0 primitive.
//!
//! # Diagnostics
//!
//! All three codes live in the P0292-P0295 free block above the
//! `@abi` P0290-P0291 range, and are reserved for `@packed_struct`
//! (b2-05 `@endian` uses its own P0301-P0304 block).
//!
//! - **P0292** — malformed `@packed_struct(align=N)` syntax: the
//!   attribute name is not `packed_struct`, or the argument list is
//!   missing `align`, `=`, or `)`.
//! - **P0293** — `align` value is zero, or is not a power of two.
//! - **P0295** — `@packed_struct` is followed by a decl other than
//!   `struct` (e.g. `enum`, `trait`, `let`, `impl`, …).

use paideia_as_ast::{NodeId, StructAttr};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

/// Parse `@packed_struct[(align=<n>)] struct <Name> { <fields> }` at
/// item position and return the resulting struct-decl `NodeId`.
///
/// Free-function entry point mirroring [`crate::parse_endian_attr`] and
/// [`crate::parse_functor`] so a call-site can dispatch here directly
/// after peeking `@packed_struct` without needing an `impl Parser`
/// method in scope.
///
/// The struct body is parsed by the regular `Parser::parse_struct_decl`
/// after the attribute prefix is consumed, so all existing struct
/// diagnostics (P0277 malformed field, …) still fire inside the body.
/// The parsed `align` (or `None` for the bare `@packed_struct` form)
/// is recorded as [`StructAttr::Packed { align }`] on the struct's
/// entry in [`paideia_as_ast::StructAttrTable`].
///
/// See the module-level docs for the full grammar, semantics, and
/// diagnostic taxonomy.
pub fn parse_packed_struct(parser: &mut Parser<'_, '_, '_>) -> Result<NodeId, ParseError> {
    // Consume the leading `@`.
    let at_tok = parser.expect(TokenKind::At)?;
    let at_span = at_tok.span;

    // The attribute name identifier must be exactly `packed_struct`.
    // The top-level item dispatcher normally verifies this before
    // routing here, but recheck defensively so a direct call still
    // enforces the shape.
    let name_tok = parser.expect(TokenKind::Ident)?;
    let name_span = name_tok.span;
    let name_text = parser.source_text_for_span(name_span).to_string();
    if name_text != "packed_struct" {
        let code = DiagnosticCode::new(Category::P, Severity::Error, 292)
            .expect("valid P0292 code");
        let diag = Diagnostic::error(code)
            .message(format!(
                "expected '@packed_struct', got '@{}'",
                name_text
            ))
            .with_span(name_span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Optional `(align = <int>)` argument list.
    let align = if parser.at(TokenKind::LParen) {
        Some(parse_packed_align_arg(parser, name_span)?)
    } else {
        None
    };

    // The next token MUST be `struct` — the attribute is struct-only.
    // Any other item kind here (`enum`, `trait`, `let`, `impl`, …)
    // rejects with P0295, satisfying the "reject on non-struct decls"
    // semantic rule.
    if !parser.at(TokenKind::KwStruct) {
        let span = parser.peek().map(|t| t.span).unwrap_or(at_span);
        let found = parser
            .peek()
            .map(|t| describe_non_struct_kind(t.kind))
            .unwrap_or("end of input");
        let code = DiagnosticCode::new(Category::P, Severity::Error, 295)
            .expect("valid P0295 code");
        let diag = Diagnostic::error(code)
            .message(format!(
                "@packed_struct may only be applied to a struct declaration, found {}",
                found
            ))
            .with_span(span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Delegate the body parse to the regular struct-decl parser so all
    // existing field diagnostics (P0277 malformed field, P0301 `@endian`
    // on non-scalar, …) still apply inside the body. That parser also
    // runs `parse_attributes` first, but no `#[...]` sits between
    // `@packed_struct(...)` and `struct` in this grammar, so that inner
    // call is a cheap no-op.
    let struct_id = parser.parse_struct_decl()?;

    // Attach the attribute to the struct's side-table entry. Sparse by
    // design — only structs that carry a struct-level attribute occupy
    // space here.
    parser
        .arena_mut()
        .struct_attr_mut()
        .push(struct_id, StructAttr::Packed { align });

    Ok(struct_id)
}

/// Parse the optional `(align = <int>)` argument list of
/// `@packed_struct`. Consumes from `(` through `)` inclusive and
/// returns the validated power-of-two alignment.
///
/// - P0292 — missing `align` keyword, `=`, or closing `)`.
/// - P0293 — value is zero, does not parse as a `u32`, or is not
///   a power of two.
fn parse_packed_align_arg(
    parser: &mut Parser<'_, '_, '_>,
    attr_name_span: Span,
) -> Result<u32, ParseError> {
    // Consume `(` — the caller has already peeked LParen.
    parser.expect(TokenKind::LParen)?;

    // Expect the `align` keyword identifier.
    let key_tok = parser.expect(TokenKind::Ident)?;
    let key_text = parser.source_text_for_span(key_tok.span).to_string();
    if key_text != "align" {
        let code = DiagnosticCode::new(Category::P, Severity::Error, 292)
            .expect("valid P0292 code");
        let diag = Diagnostic::error(code)
            .message(format!(
                "malformed @packed_struct(...) syntax: expected 'align', got '{}'",
                key_text
            ))
            .with_span(key_tok.span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Expect `=`.
    if !parser.eat(TokenKind::Assign) {
        let span = parser.peek().map(|t| t.span).unwrap_or(key_tok.span);
        let code = DiagnosticCode::new(Category::P, Severity::Error, 292)
            .expect("valid P0292 code");
        let diag = Diagnostic::error(code)
            .message(
                "malformed @packed_struct(align=N) syntax: expected '=' after 'align'",
            )
            .with_span(span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Parse the integer literal for the alignment value.
    let lit_tok = parser.expect(TokenKind::IntLit)?;
    let lit_text = parser.source_text_for_span(lit_tok.span).to_string();
    let value: u32 = lit_text.parse().map_err(|_| {
        let code = DiagnosticCode::new(Category::P, Severity::Error, 293)
            .expect("valid P0293 code");
        let diag = Diagnostic::error(code)
            .message(
                "@packed_struct align value must be a valid u32 integer literal",
            )
            .with_span(lit_tok.span)
            .finish();
        parser.emit_diagnostic(diag);
        ParseError
    })?;

    // Power-of-two check: zero is rejected, and the classic
    // `v & (v-1) == 0` check catches every non-power-of-two.
    if value == 0 || (value & (value - 1)) != 0 {
        let code = DiagnosticCode::new(Category::P, Severity::Error, 293)
            .expect("valid P0293 code");
        let diag = Diagnostic::error(code)
            .message(format!(
                "@packed_struct align value must be a positive power of two, got {}",
                value
            ))
            .with_span(lit_tok.span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Expect `)`.
    if !parser.eat(TokenKind::RParen) {
        let span = parser.peek().map(|t| t.span).unwrap_or(attr_name_span);
        let code = DiagnosticCode::new(Category::P, Severity::Error, 292)
            .expect("valid P0292 code");
        let diag = Diagnostic::error(code)
            .message(
                "malformed @packed_struct(align=N) syntax: expected ')' after value",
            )
            .with_span(span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    Ok(value)
}

/// Short human-readable label for the "found …" clause of the P0295
/// diagnostic. Kept close to the parser rather than in the shared
/// keyword table so the exact wording — "an `enum` declaration",
/// "a `let` declaration" — stays local to this primitive and does
/// not perturb other diagnostics.
fn describe_non_struct_kind(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::KwEnum => "an `enum` declaration",
        TokenKind::KwLet => "a `let` declaration",
        TokenKind::KwTrait => "a `trait` declaration",
        TokenKind::KwImpl => "an `impl` block",
        TokenKind::KwModule => "a `module` declaration",
        TokenKind::KwSignature => "a `signature` declaration",
        TokenKind::KwEffect => "an `effect` declaration",
        TokenKind::KwCapability => "a `capability` declaration",
        TokenKind::KwUnsafe => "an `unsafe` block",
        _ => "a non-`struct` token",
    }
}
