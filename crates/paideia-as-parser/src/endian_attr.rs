//! `@endian(be|le)` field-attribute parsing (paideia-as#1372, v0.28-M1-003).
//!
//! Grammar (inside a `struct` body, before each field):
//!
//! ```text
//! @endian ( <be | le> ) <field_name> : <type>
//! ```
//!
//! The attribute is only meaningful on integral scalar field types
//! (`u8` / `u16` / `u32` / `u64` / `usize` / `i8` / `i16` / `i32` /
//! `i64` / `isize`) — the elaborator will insert a byte-swap on load /
//! store for `Be` on little-endian hosts. This module rejects non-scalar
//! field types (records, enums, tuples, arrays, pointers, refs) with a
//! `P0301`-range diagnostic; the byte-swap insertion itself is deferred
//! to a later elaborator milestone.
//!
//! Diagnostic codes (P-category, free block above the `@atomic` P0286-P0289
//! range from paideia-as#1296):
//! - `P0301` — `@endian` applied to a non-integral scalar type.
//! - `P0302` — unknown field-position attribute (not `endian`).
//! - `P0303` — malformed `@endian(...)` syntax (missing `(` or `)`).
//! - `P0304` — `@endian` argument is not one of `be` or `le`.
//!
//! Composes with `@packed_struct` (paideia-as#1373, wave-0 batch-2 issue
//! b2-06) — the packed-struct attribute lives at struct scope, so an
//! `@endian(be)` field is unaffected by its enclosing struct's layout
//! attribute. If b2-06 later grows per-field packing directives, the
//! `FieldAttr` enum in `paideia-as-ast::field_attr` is the append point.

pub use paideia_as_ast::Endianness;

use paideia_as_ast::{AstArena, FieldAttr, NodeId, TypeData};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_lexer::TokenKind;

use crate::parser::{ParseError, Parser};

/// Parse an optional `@endian(be|le)` field-attribute prefix.
///
/// Called immediately before a struct-body field's name. Returns
/// `Ok(None)` when the current token is not `@` — the common non-endian
/// case that must stay hot-path fast. Returns `Ok(Some(Endianness))` on a
/// well-formed attribute. Emits a diagnostic and returns `Err(ParseError)`
/// on malformed syntax; the caller is expected to recover to the next
/// field boundary (`,`, `}`) via
/// [`Parser::recover_to_one_of`](crate::parser::Parser::recover_to_one_of).
///
/// The type-shape rejection (`P0301`, non-integral scalar) is delegated
/// to the caller — after both the attribute AND the field type have been
/// parsed, call [`validate_endian_field_type`] with the resolved type
/// node. This split keeps the attribute parser one-shot and lets the
/// caller position the diagnostic on the type span rather than the
/// attribute span.
pub fn parse_endian_attr(
    parser: &mut Parser<'_, '_, '_>,
) -> Result<Option<Endianness>, ParseError> {
    if !parser.at(TokenKind::At) {
        return Ok(None);
    }

    let at_tok = parser.expect(TokenKind::At)?;
    let at_span = at_tok.span;

    // Attribute name — must be the identifier `endian`.
    let name_tok = parser.expect(TokenKind::Ident)?;
    let name_span = name_tok.span;
    let name_text = parser.source_text_for_span(name_span).to_string();
    if name_text != "endian" {
        let code = DiagnosticCode::new(Category::P, Severity::Error, 302)
            .expect("valid P0302 code");
        let diag = Diagnostic::error(code)
            .message(format!(
                "unknown field-position attribute '@{}' on struct field (only '@endian(be|le)' is recognised)",
                name_text
            ))
            .with_span(name_span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Expect `(`.
    if !parser.eat(TokenKind::LParen) {
        let span = parser.peek().map(|t| t.span).unwrap_or(at_span);
        let code = DiagnosticCode::new(Category::P, Severity::Error, 303)
            .expect("valid P0303 code");
        let diag = Diagnostic::error(code)
            .message("malformed @endian(be|le) syntax: expected '(' after 'endian'")
            .with_span(span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    // Endianness argument — case-sensitive identifier `be` or `le`.
    let arg_tok = parser.expect(TokenKind::Ident)?;
    let arg_span = arg_tok.span;
    let arg_text = parser.source_text_for_span(arg_span).to_string();
    let endianness = match arg_text.as_str() {
        "be" => Endianness::Be,
        "le" => Endianness::Le,
        other => {
            let code = DiagnosticCode::new(Category::P, Severity::Error, 304)
                .expect("valid P0304 code");
            let diag = Diagnostic::error(code)
                .message(format!(
                    "unknown byte-order '{}' in @endian(...) (expected one of: be, le)",
                    other
                ))
                .with_span(arg_span)
                .finish();
            parser.emit_diagnostic(diag);
            return Err(ParseError);
        }
    };

    // Expect `)`.
    if !parser.eat(TokenKind::RParen) {
        let span = parser.peek().map(|t| t.span).unwrap_or(arg_span);
        let code = DiagnosticCode::new(Category::P, Severity::Error, 303)
            .expect("valid P0303 code");
        let diag = Diagnostic::error(code)
            .message("malformed @endian(be|le) syntax: expected ')' after byte-order name")
            .with_span(span)
            .finish();
        parser.emit_diagnostic(diag);
        return Err(ParseError);
    }

    Ok(Some(endianness))
}

/// After parsing an `@endian(...)`-attributed field's type node, validate
/// that the type is an integral scalar. On rejection, emit `P0301`
/// anchored on the type span through `emit`.
///
/// Returns `true` when the type is acceptable and the caller should
/// insert the attribute into
/// [`AstArena::struct_field_attrs_mut`], `false` otherwise (attribute is
/// dropped, diagnostic already emitted).
///
/// **Recognised integral scalars.** Matches the paideia-as scalar-type
/// vocabulary: `u8`, `u16`, `u32`, `u64`, `usize`, `i8`, `i16`, `i32`,
/// `i64`, `isize`. Floating-point (`f32`, `f64`) and `bool` are rejected
/// — a byte-swap on those has no defensible semantics at the field
/// level. Anything with type-arguments (`Foo<T>`), or any non-`Name`
/// type-data variant (records, enums, tuples, arrays, pointers, refs,
/// linear-class wrappers, effect-rows, self-qualified paths, function
/// pointers, closures), is rejected as non-scalar.
pub fn validate_endian_field_type(
    ast: &AstArena,
    source: &str,
    type_id: NodeId,
    emit: &mut dyn FnMut(Diagnostic),
) -> bool {
    let ty = match ast.type_data(type_id) {
        Some(t) => t,
        None => {
            // No type-data recorded — either the parser failed upstream
            // or the caller passed a non-type node. Either way, silently
            // pass; the primary diagnostic lives upstream.
            return false;
        }
    };

    let span = ast.get(type_id).map(|n| n.span);

    match ty {
        TypeData::Name { name, args } if args.is_empty() => {
            let name_span = match ast.get(*name).map(|n| n.span) {
                Some(s) => s,
                None => return false,
            };
            let start = name_span.byte_start() as usize;
            let end = (name_span.byte_start() + name_span.byte_len()) as usize;
            let name_text = if start <= source.len() && end <= source.len() {
                &source[start..end]
            } else {
                ""
            };
            if is_integral_scalar_name(name_text) {
                return true;
            }
            let code = DiagnosticCode::new(Category::P, Severity::Error, 301)
                .expect("valid P0301 code");
            let mut b = Diagnostic::error(code).message(format!(
                "@endian(...) requires an integral scalar field type (u8/u16/u32/u64/usize/i8/i16/i32/i64/isize); \
                 found '{}'",
                name_text
            ));
            if let Some(s) = span {
                b = b.with_span(s);
            }
            emit(b.finish());
            false
        }
        _ => {
            let kind_word = describe_type_kind(ty);
            let code = DiagnosticCode::new(Category::P, Severity::Error, 301)
                .expect("valid P0301 code");
            let mut b = Diagnostic::error(code).message(format!(
                "@endian(...) requires an integral scalar field type; found {}",
                kind_word
            ));
            if let Some(s) = span {
                b = b.with_span(s);
            }
            emit(b.finish());
            false
        }
    }
}

/// `true` iff `name` is one of the recognised integral-scalar type
/// spellings (`u8`, `u16`, `u32`, `u64`, `usize`, `i8`, `i16`, `i32`,
/// `i64`, `isize`).
fn is_integral_scalar_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
    )
}

/// Short human word for a `TypeData` variant, used in diagnostics.
fn describe_type_kind(ty: &TypeData) -> &'static str {
    match ty {
        TypeData::Name { args, .. } if !args.is_empty() => "a generic type application",
        TypeData::Name { .. } => "a named type",
        TypeData::FnPtr { .. } => "a function-pointer type",
        TypeData::Closure { .. } => "a closure type",
        TypeData::Tuple { .. } => "a tuple type",
        TypeData::LinearClass { .. } => "a linearity-annotated type",
        TypeData::EffectRow { .. } => "an effect row",
        TypeData::Ptr { .. } => "a pointer type",
        TypeData::Ref { .. } => "a reference type",
        TypeData::Record { .. } => "a record type",
        TypeData::Enum { .. } => "an enum type",
        TypeData::SelfQualifiedPath { .. } => "a Self-qualified path",
        TypeData::Array { .. } => "an array type",
    }
}

/// Convenience: build a `FieldAttr::Endian(e)` — the sole variant currently
/// produced by this module.
#[must_use]
pub fn as_field_attr(e: Endianness) -> FieldAttr {
    FieldAttr::Endian(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::Token;

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(
            kind,
            Span::new(FileId::new(1).unwrap(), byte_start, byte_len),
        )
    }

    /// Drive `parse_endian_attr` over `source` given `tokens`, returning
    /// the outcome and any diagnostics.
    fn run(
        source: &str,
        tokens: Vec<Token>,
    ) -> (
        Result<Option<Endianness>, ParseError>,
        Vec<paideia_as_diagnostics::Diagnostic>,
    ) {
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let result = {
            let mut p = Parser::new(
                &tokens,
                source,
                FileId::new(1).unwrap(),
                &mut arena,
                &mut sink,
            );
            parse_endian_attr(&mut p)
        };
        (result, sink.into_diagnostics())
    }

    #[test]
    fn absent_returns_ok_none() {
        // Just a lone identifier — no `@`.
        let (r, diags) = run("magic", vec![tok(TokenKind::Ident, 0, 5), tok(TokenKind::Eof, 5, 0)]);
        assert_eq!(r.unwrap(), None);
        assert!(diags.is_empty());
    }

    #[test]
    fn well_formed_be_returns_be() {
        // `@endian(be)`
        let source = "@endian(be)";
        let (r, diags) = run(
            source,
            vec![
                tok(TokenKind::At, 0, 1),
                tok(TokenKind::Ident, 1, 6),
                tok(TokenKind::LParen, 7, 1),
                tok(TokenKind::Ident, 8, 2),
                tok(TokenKind::RParen, 10, 1),
                tok(TokenKind::Eof, 11, 0),
            ],
        );
        assert_eq!(r.unwrap(), Some(Endianness::Be));
        assert!(diags.is_empty());
    }

    #[test]
    fn well_formed_le_returns_le() {
        let source = "@endian(le)";
        let (r, diags) = run(
            source,
            vec![
                tok(TokenKind::At, 0, 1),
                tok(TokenKind::Ident, 1, 6),
                tok(TokenKind::LParen, 7, 1),
                tok(TokenKind::Ident, 8, 2),
                tok(TokenKind::RParen, 10, 1),
                tok(TokenKind::Eof, 11, 0),
            ],
        );
        assert_eq!(r.unwrap(), Some(Endianness::Le));
        assert!(diags.is_empty());
    }

    #[test]
    fn unknown_attribute_name_emits_p0302() {
        // `@packed(be)` — `packed` is not a per-field attribute.
        let source = "@packed(be)";
        let (r, diags) = run(
            source,
            vec![
                tok(TokenKind::At, 0, 1),
                tok(TokenKind::Ident, 1, 6),
                tok(TokenKind::LParen, 7, 1),
                tok(TokenKind::Ident, 8, 2),
                tok(TokenKind::RParen, 10, 1),
                tok(TokenKind::Eof, 11, 0),
            ],
        );
        assert!(r.is_err());
        assert!(diags
            .iter()
            .any(|d| d.code().category().letter() == 'P' && d.code().number() == 302));
    }

    #[test]
    fn missing_lparen_emits_p0303() {
        // `@endian be)`
        let source = "@endian be)";
        let (r, diags) = run(
            source,
            vec![
                tok(TokenKind::At, 0, 1),
                tok(TokenKind::Ident, 1, 6),
                tok(TokenKind::Ident, 8, 2),
                tok(TokenKind::RParen, 10, 1),
                tok(TokenKind::Eof, 11, 0),
            ],
        );
        assert!(r.is_err());
        assert!(diags
            .iter()
            .any(|d| d.code().category().letter() == 'P' && d.code().number() == 303));
    }

    #[test]
    fn unknown_byte_order_emits_p0304() {
        // `@endian(mid)` — not `be` or `le`.
        let source = "@endian(mid)";
        let (r, diags) = run(
            source,
            vec![
                tok(TokenKind::At, 0, 1),
                tok(TokenKind::Ident, 1, 6),
                tok(TokenKind::LParen, 7, 1),
                tok(TokenKind::Ident, 8, 3),
                tok(TokenKind::RParen, 11, 1),
                tok(TokenKind::Eof, 12, 0),
            ],
        );
        assert!(r.is_err());
        assert!(diags
            .iter()
            .any(|d| d.code().category().letter() == 'P' && d.code().number() == 304));
    }

    #[test]
    fn missing_rparen_emits_p0303() {
        // `@endian(be` — no closing paren.
        let source = "@endian(be";
        let (r, diags) = run(
            source,
            vec![
                tok(TokenKind::At, 0, 1),
                tok(TokenKind::Ident, 1, 6),
                tok(TokenKind::LParen, 7, 1),
                tok(TokenKind::Ident, 8, 2),
                tok(TokenKind::Eof, 10, 0),
            ],
        );
        assert!(r.is_err());
        assert!(diags
            .iter()
            .any(|d| d.code().category().letter() == 'P' && d.code().number() == 303));
    }

    #[test]
    fn as_field_attr_builds_endian_variant() {
        assert_eq!(as_field_attr(Endianness::Be), FieldAttr::Endian(Endianness::Be));
        assert_eq!(as_field_attr(Endianness::Le), FieldAttr::Endian(Endianness::Le));
    }
}
