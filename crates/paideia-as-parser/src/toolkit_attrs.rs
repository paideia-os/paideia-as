//! Toolkit attribute prefixes on functor declarations
//! (paideia-as#1389, v0.32-M1-003, Toolkit Batch 3).
//!
//! # Surface syntax
//!
//! ```text
//! FunctorAttr := "@retain" | "@immediate"
//! FunctorDecl := FunctorAttr? "functor" Ident "(" Ident ":" Ident ")" ...
//! ```
//!
//! At most one of the two attributes may appear on a given functor
//! declaration — they name opposite ends of a capability-flow discipline
//! on the functor's input module and are therefore mutually exclusive at
//! the declaration site.
//!
//! # Semantics
//!
//! - **`@retain`** — the functor may hold a reference to its input
//!   capability across invocations. The elaborator does not insert a
//!   consume-on-return barrier at the functor boundary. Used when the
//!   functor needs to close over the input to satisfy long-lived driver
//!   contracts (a bus scan that revisits its bus handle across
//!   probe-response cycles, a filesystem functor that keeps its device
//!   handle live for the lifetime of the mount).
//!
//! - **`@immediate`** — the functor MUST consume its input capability by
//!   the time it returns; either by threading it into the result module
//!   or by dropping it explicitly. The elaborator inserts the
//!   corresponding return-path check. Used when the input is a
//!   single-shot capability that the caller does not want to see back
//!   (a one-shot buffer donation, an ISR context handoff).
//!
//! - **Composition.** `@retain` and `@immediate` are semantic opposites,
//!   so co-declaration is a category error, not a scoping question. The
//!   parser rejects it in place with **P0330** so that no downstream
//!   phase (session-type check, elaborator emit) has to re-check the
//!   invariant, and so the diagnostic anchors on the source location
//!   rather than a synthetic elaborator span.
//!
//! # Positioning within the parser
//!
//! [`parse_functor_with_attrs`] is a **standalone entry point** — it
//! constructs its own [`TokenCursor`] over the supplied token slice and
//! does not route through `parse_primary` / `parse_item`. That mirrors
//! [`crate::parse_functor`] (v0.25-M1-001), which is itself standalone
//! ahead of the M1-004 item-parser wiring, and keeps the M1-003 landing
//! from having to churn the top-level dispatcher before the M1-004
//! integration lands.
//!
//! # AST landing
//!
//! On success, the returned `(Vec<FunctorAttr>, FunctorDecl)` pair
//! carries the parsed attribute list alongside the ordinary
//! [`FunctorDecl`] produced by [`crate::parse_functor`]. The vector is
//! empty when neither attribute is present — the common case — and holds
//! exactly one entry otherwise.
//!
//! The M1-003 primitive does **not** push into
//! [`paideia_as_ast::FunctorAttrTable`] itself: [`FunctorDecl`] currently
//! carries no `NodeId` (the functor decl is not yet an item-parser node
//! kind), so there is no key to push against. The AST table ships with
//! this milestone so the arena surface stays symmetric with
//! [`paideia_as_ast::StructAttrTable`] and so the M1-004 item-parser
//! hookup can insert into it without a further AST churn. Callers that
//! elaborate the returned pair are expected to `push` into the table
//! once they mint the functor's node id.
//!
//! # Diagnostics
//!
//! Codes live under [`paideia_as_diagnostics::Category::M`] — "Module
//! system, imports, functors" (0300-0499) — the category that names
//! functors explicitly. The task-writer's shorthand "P0330" is retained
//! in discussion text, but the P range only extends to 0299
//! (`code.rs::Category::range`), so the canonical wire code lands under
//! `M`. The M0330-M0332 block is reserved for functor-level toolkit
//! attributes.
//!
//! - **M0330** — `@retain` and `@immediate` co-declared on the same
//!   functor (semantic opposition). Anchored on the second attribute's
//!   name span so the reader sees "the annotation that breaks the pair".
//! - **M0331** — `@retain` or `@immediate` applied to a non-functor
//!   item (`fn`, `let`, `struct`, `enum`, `trait`, `impl`, …).
//! - **M0332** — unknown functor-position attribute (not `retain` /
//!   `immediate`). Also fires when a `@retain` / `@immediate` prefix
//!   appears twice — the second occurrence is a redundant declaration
//!   that provides no new information.

use paideia_as_ast::FunctorAttr;
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, FileId, Severity, Span,
};
use paideia_as_lexer::{Token, TokenKind};

use crate::cursor::TokenCursor;
use crate::functor::{FunctorDecl, parse_functor};
use crate::parser::ParseError;

/// Parse an optional `@retain` / `@immediate` prefix followed by a full
/// functor declaration.
///
/// The token slice is expected to start at the leading `@`, or directly
/// at the `functor` keyword when no toolkit prefix is present. The
/// returned `Vec<FunctorAttr>` holds zero or one entry — never more:
/// the two attributes are mutually exclusive (see P0330) and the same
/// attribute repeating is likewise rejected (see P0332).
///
/// Any trailing tokens past the body's closing brace are left in the
/// slice — same convention as [`parse_functor`].
///
/// Diagnostics are emitted through `sink` per the M033x table in the
/// module-level docs.
pub fn parse_functor_with_attrs(
    tokens: &[Token],
    source: &str,
    file: FileId,
    sink: &mut dyn DiagnosticSink,
) -> Result<(Vec<FunctorAttr>, FunctorDecl), ParseError> {
    let mut cursor = TokenCursor::new(tokens, file);

    // Attribute prefix loop — consumes zero or more `@name` prefixes.
    //
    // On a well-formed input this executes 0..=1 times: either no `@`,
    // or exactly one of `@retain` / `@immediate`. A second `@` after a
    // successful first attribute triggers a P0330 (mixed pair) or a
    // P0332 (repeated same attribute), depending on the second name.
    let mut attrs: Vec<FunctorAttr> = Vec::new();
    while cursor.at(TokenKind::At) {
        let (attr, name_span) = parse_one_functor_attr(&mut cursor, source, sink)?;

        if let Some(&prior) = attrs.first() {
            if prior != attr {
                // Mixed pair — `@retain @immediate` or the reverse.
                emit(
                    sink,
                    330,
                    "'@retain' and '@immediate' are mutually exclusive on a functor \
                     declaration; drop one to keep the intended capability-flow discipline",
                    name_span,
                );
            } else {
                // Same attribute twice — redundant, and no honest reader
                // would write it. Handled under the generic "unknown /
                // malformed toolkit attribute" code so we do not spend
                // a scarce diagnostic code on a rare typo.
                emit(
                    sink,
                    332,
                    "duplicate functor-position toolkit attribute; a single \
                     '@retain' or '@immediate' is sufficient",
                    name_span,
                );
            }
            return Err(ParseError);
        }

        attrs.push(attr);
    }

    // The remaining tokens must open a functor declaration. Anything
    // else is a placement error: the toolkit attributes are functor-only
    // (P0331 when a prefix is present), or the caller handed us
    // something that is not a functor at all (also P0331 to keep the
    // "reject on non-functor items" contract honest).
    if !cursor.at(TokenKind::KwFunctor) {
        let found_span = cursor.current_span();
        let found = describe_non_functor_kind(cursor.current_kind());
        let msg = if attrs.is_empty() {
            format!(
                "expected 'functor' declaration at toolkit-attr entry point, found {}",
                found
            )
        } else {
            format!(
                "'@retain' / '@immediate' may only be applied to a 'functor' declaration, \
                 found {}",
                found
            )
        };
        emit(sink, 331, &msg, found_span);
        return Err(ParseError);
    }

    // Hand off the remaining tokens to the standalone functor parser.
    // parse_functor drives its own TokenCursor from index 0 of its own
    // slice, so we splice at the current cursor position.
    let pos = cursor.position();
    let decl = parse_functor(&tokens[pos..], source, file, sink)?;
    Ok((attrs, decl))
}

/// Parse a single `@retain` or `@immediate` attribute at the current
/// cursor position. On entry the cursor points at `@`. On success the
/// cursor is advanced past the attribute name. Emits and returns
/// `Err(ParseError)` when the name is not one of the two recognised
/// functor-position attributes.
fn parse_one_functor_attr(
    cursor: &mut TokenCursor<'_>,
    source: &str,
    sink: &mut dyn DiagnosticSink,
) -> Result<(FunctorAttr, Span), ParseError> {
    // Consume the leading `@`.
    let at_span = cursor.current_span();
    let _ = cursor.bump();

    // The next token must be an identifier — the attribute name.
    if !cursor.at(TokenKind::Ident) {
        let span = cursor.current_span();
        emit(
            sink,
            332,
            "expected a functor-position toolkit-attribute name after '@' (one of \
             'retain' or 'immediate')",
            merge_spans(at_span, span),
        );
        return Err(ParseError);
    }

    let name_tok = cursor.bump().expect("at(Ident) implies bump returns Some");
    let name_span = name_tok.span;
    let name_text = lexeme(source, name_span);

    let attr = match name_text.as_str() {
        "retain" => FunctorAttr::Retain,
        "immediate" => FunctorAttr::Immediate,
        other => {
            emit(
                sink,
                332,
                &format!(
                    "unknown functor-position toolkit attribute '@{}' (expected \
                     '@retain' or '@immediate')",
                    other
                ),
                name_span,
            );
            return Err(ParseError);
        }
    };

    Ok((attr, name_span))
}

/// Short human-readable label for the "found …" clause of the P0331
/// diagnostic. Kept close to the parser rather than in the shared
/// keyword table so the exact wording stays local to this primitive
/// and does not perturb other diagnostics.
fn describe_non_functor_kind(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::KwFn => "an 'fn' declaration",
        TokenKind::KwLet => "a 'let' declaration",
        TokenKind::KwStruct => "a 'struct' declaration",
        TokenKind::KwEnum => "an 'enum' declaration",
        TokenKind::KwTrait => "a 'trait' declaration",
        TokenKind::KwImpl => "an 'impl' block",
        TokenKind::KwModule => "a 'module' declaration",
        TokenKind::KwSignature => "a 'signature' declaration",
        TokenKind::KwStructure => "a 'structure' declaration",
        TokenKind::Eof => "end of input",
        _ => "a non-'functor' token",
    }
}

fn lexeme(source: &str, span: Span) -> String {
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if start <= source.len() && end <= source.len() && start <= end {
        source[start..end].to_string()
    } else {
        String::new()
    }
}

fn merge_spans(a: Span, b: Span) -> Span {
    let start = a.byte_start().min(b.byte_start());
    let end = (a.byte_start() + a.byte_len()).max(b.byte_start() + b.byte_len());
    Span::new(a.file(), start, end - start)
}

fn emit(sink: &mut dyn DiagnosticSink, code: u16, msg: &str, span: Span) {
    let d = Diagnostic::error(m_code(code))
        .message(msg.to_string())
        .with_span(span)
        .finish();
    let _ = sink.emit(d);
}

fn m_code(n: u16) -> DiagnosticCode {
    // Category::M — "Module system, imports, functors" (range 0300-0499).
    // The M0330-M0332 block belongs here rather than under P (0100-0299)
    // because the diagnostics arise from a *functor*-level attribute
    // family. The task-writer's shorthand "P0330" was retained in
    // discussion text, but the P range does not extend past 0299, so the
    // canonical code lands under M — the category name lists functors
    // explicitly. See CHANGELOG-v0.32-M1-003.md.
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span, VecSink};

    fn file() -> FileId {
        FileId::new(1).unwrap()
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, Span::new(file(), byte_start, byte_len))
    }

    /// A canonical functor tail: `functor F(In : SigIn) -> SigOut { }`.
    /// The `start` argument gives the byte offset of the leading
    /// `functor` keyword; every subsequent token span is derived from it
    /// so the caller can splice attribute prefixes in front.
    fn functor_tail(start: u32) -> Vec<Token> {
        vec![
            tok(TokenKind::KwFunctor, start, 7),           // "functor"
            tok(TokenKind::Ident, start + 8, 1),           // F
            tok(TokenKind::LParen, start + 9, 1),
            tok(TokenKind::Ident, start + 10, 2),          // In
            tok(TokenKind::Colon, start + 13, 1),
            tok(TokenKind::Ident, start + 15, 5),          // SigIn
            tok(TokenKind::RParen, start + 20, 1),
            tok(TokenKind::Arrow, start + 22, 2),
            tok(TokenKind::Ident, start + 25, 6),          // SigOut
            tok(TokenKind::LBrace, start + 32, 1),
            tok(TokenKind::RBrace, start + 34, 1),
            tok(TokenKind::Eof, start + 35, 0),
        ]
    }

    fn functor_tail_source() -> String {
        "functor F(In : SigIn) -> SigOut { }".to_string()
    }

    fn diags_contain_code(diags: &[paideia_as_diagnostics::Diagnostic], number: u16) -> bool {
        // See `p_code` — the M0330-M0332 block lives under Category::M
        // ("Module system, imports, functors") because the P range only
        // extends to 0299.
        diags.iter().any(|d| {
            d.code().category().letter() == 'M' && d.code().number() == number
        })
    }

    #[test]
    fn parses_single_retain_attribute() {
        // "@retain functor F(In : SigIn) -> SigOut { }"
        //  0      8
        let src = format!("@retain {}", functor_tail_source());
        let mut toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 6),  // "retain"
        ];
        toks.extend(functor_tail(8));

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(
            result.is_ok(),
            "diagnostics: {:?}",
            sink.diagnostics()
        );
        let (attrs, decl) = result.unwrap();
        assert_eq!(attrs, vec![FunctorAttr::Retain]);
        assert_eq!(decl.name, "F");
        assert_eq!(decl.param_name, "In");
        assert_eq!(decl.param_sig, "SigIn");
        assert_eq!(decl.return_sig, "SigOut");
    }

    #[test]
    fn parses_single_immediate_attribute() {
        // "@immediate functor F(In : SigIn) -> SigOut { }"
        //  0         11
        let src = format!("@immediate {}", functor_tail_source());
        let mut toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 9),  // "immediate"
        ];
        toks.extend(functor_tail(11));

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(
            result.is_ok(),
            "diagnostics: {:?}",
            sink.diagnostics()
        );
        let (attrs, _decl) = result.unwrap();
        assert_eq!(attrs, vec![FunctorAttr::Immediate]);
    }

    #[test]
    fn parses_zero_attributes_bare_functor() {
        // "functor F(In : SigIn) -> SigOut { }" — no prefix.
        let src = functor_tail_source();
        let toks = functor_tail(0);

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(
            result.is_ok(),
            "diagnostics: {:?}",
            sink.diagnostics()
        );
        let (attrs, decl) = result.unwrap();
        assert!(attrs.is_empty());
        assert_eq!(decl.name, "F");
    }

    #[test]
    fn rejects_retain_and_immediate_together_m0330() {
        // "@retain @immediate functor F(In : SigIn) -> SigOut { }"
        //  0      8         19
        let src = format!("@retain @immediate {}", functor_tail_source());
        let mut toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 6),   // "retain"
            tok(TokenKind::At, 8, 1),
            tok(TokenKind::Ident, 9, 9),   // "immediate"
        ];
        toks.extend(functor_tail(19));

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        assert!(
            diags_contain_code(sink.diagnostics(), 330),
            "expected M0330, got {:?}",
            sink.diagnostics()
        );
    }

    #[test]
    fn rejects_immediate_and_retain_together_m0330() {
        // The reversed order should still be rejected.
        // "@immediate @retain functor F(In : SigIn) -> SigOut { }"
        //  0         11     19
        let src = format!("@immediate @retain {}", functor_tail_source());
        let mut toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 9),   // "immediate"
            tok(TokenKind::At, 11, 1),
            tok(TokenKind::Ident, 12, 6),  // "retain"
        ];
        toks.extend(functor_tail(19));

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        assert!(diags_contain_code(sink.diagnostics(), 330));
    }

    #[test]
    fn rejects_retain_on_fn_m0331() {
        // "@retain fn foo" — a `fn` declaration is not a functor.
        // 0       8  11
        let src = "@retain fn foo".to_string();
        let toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 6),   // "retain"
            tok(TokenKind::KwFn, 8, 2),    // "fn"
            tok(TokenKind::Ident, 11, 3),  // "foo"
            tok(TokenKind::Eof, 14, 0),
        ];

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        assert!(
            diags_contain_code(sink.diagnostics(), 331),
            "expected M0331, got {:?}",
            sink.diagnostics()
        );
    }

    #[test]
    fn rejects_immediate_on_struct_m0331() {
        // "@immediate struct S" — a `struct` decl is not a functor.
        // 0          11
        let src = "@immediate struct S".to_string();
        let toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 9),      // "immediate"
            tok(TokenKind::KwStruct, 11, 6),
            tok(TokenKind::Ident, 18, 1),
            tok(TokenKind::Eof, 19, 0),
        ];

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        assert!(diags_contain_code(sink.diagnostics(), 331));
    }

    #[test]
    fn rejects_unknown_attribute_name_m0332() {
        // "@bogus functor F(...) -> SigOut { }" — unknown attribute.
        // 0     7
        let src = format!("@bogus {}", functor_tail_source());
        let mut toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 5),   // "bogus"
        ];
        toks.extend(functor_tail(7));

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        assert!(diags_contain_code(sink.diagnostics(), 332));
    }

    #[test]
    fn rejects_duplicate_retain_m0332() {
        // "@retain @retain functor F(In : SigIn) -> SigOut { }" —
        // repeating the same attribute is redundant.
        //  0      8     16
        let src = format!("@retain @retain {}", functor_tail_source());
        let mut toks = vec![
            tok(TokenKind::At, 0, 1),
            tok(TokenKind::Ident, 1, 6),   // "retain"
            tok(TokenKind::At, 8, 1),
            tok(TokenKind::Ident, 9, 6),   // "retain"
        ];
        toks.extend(functor_tail(16));

        let mut sink = VecSink::new();
        let result = parse_functor_with_attrs(&toks, &src, file(), &mut sink);
        assert!(result.is_err());
        assert!(diags_contain_code(sink.diagnostics(), 332));
    }
}
