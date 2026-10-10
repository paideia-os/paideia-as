//! Pattern formatter (FMT-7, paideia-as#1571).
//!
//! Implements structural rendering for every [`PatternData`] variant that
//! `paideia-as-ast` currently carries. The walker's `format_pattern`
//! stub in `walker.rs` delegates to [`format_pattern_data`] below.
//!
//! # Coverage (2026-10-10)
//!
//! | PatternData variant             | Rendered form                      |
//! |---------------------------------|------------------------------------|
//! | `Wildcard`                      | `_`                                |
//! | `Ident { name, mutable }`       | `name` / `mut name`                |
//! | `Literal { lit }`               | source slice of `lit`              |
//! | `Tuple { elements }`            | `(p1, p2, …)`                      |
//! | `Struct { path, fields }`       | `Point { x, y }` (shorthand kept)  |
//! | `EnumVariant { path, args }`    | `Variant(p1, p2)` / `None`         |
//! | `Or { alternatives }`           | `A \| B \| C` (break before `\|`)  |
//! | `Binding { name, inner }`       | `name @ inner`                     |
//! | `Range { start, end }`          | `a..b` / `..b` / `a..` / `..`      |
//! | `Reference { inner, mutable }`  | `&p` / `&mut p`                    |
//! | `Slice { elements }`            | `[a, b, ..rest]`                   |
//! | `Rest { binder }`               | `..` / `..name`                    |
//!
//! # Style-spec backing (§4.6, §6.1)
//!
//! * `_` is a bare glyph; no surrounding space is introduced by this
//!   formatter — spacing against the enclosing construct (match arm,
//!   tuple element, …) is the caller's responsibility.
//! * `name @ Pat`: single space on each side of `@` (§6.1).
//! * `A | B | C`: one space around each `|` when flat; multi-line form
//!   breaks *before* each `|` so the `|` lines up at the start of each
//!   continuation line (§4.6 or-pattern rule).
//! * Range patterns: no space around `..` / `..=` (§6.1). The current
//!   AST carries only exclusive `..` (no `..=` in phase-1 patterns, per
//!   `patterns.rs` module header); this formatter is already shaped to
//!   accept a future `inclusive: bool` without a signature break — see
//!   [`fmt_range`].
//! * Struct patterns: field punning (shorthand `{ x, y }`) is preserved
//!   by detecting the parser's convention — a field whose `pattern` is
//!   a `PatIdent` with the same source span as `field.name` renders as
//!   just the name; everything else renders as `field: pattern` using
//!   the data-colon spacing (`: `, §5.2).
//! * Slice / Rest: `..rest` has *no* space between `..` and the binder.
//!   `..` alone is the standalone rest sub-pattern inside a slice.
//!
//! # Fallback contract
//!
//! Every helper returns a Doc built from structural pieces. If a child
//! `NodeId` is unknown to the arena (defensive — should not happen on a
//! well-formed parse) the helper slices its source text via
//! [`walker::fallback_slice`]. The outer dispatch in
//! [`format_pattern_data`] also falls back if `pattern_data(nid)` is
//! `None` — e.g. a node tagged as a pattern kind that did not populate
//! the parallel `patterns` vector.
//!
//! # Spec gaps (as of 2026-10-10)
//!
//! * `PatternData::Range` has no `inclusive` flag: `..=` cannot be
//!   round-tripped. Tracked in the AST module header (PAS-DEBT-B2-013).
//! * `PatternData::EnumVariant` loses the type qualifier on path-form
//!   variants: the parser builds `Option::None` as `EnumVariant {
//!   path: <Ident:"None">, args: [] }`, discarding `Option`. Round-trip
//!   on path-qualified variants is therefore lossy.
//! * `PatternData::Struct` has no `rest: bool` flag, so a struct
//!   pattern's trailing `..` (field-elide) is not representable; the
//!   formatter emits only the fields the AST carries. The sprint brief
//!   calls for a trailing `..`; adding it waits on an AST-side flag.

use paideia_as_ast::{NodeId, NodeKind, PatField, PatternData};

use crate::doc::{self, Doc};
use crate::walker::{FormatContext, fallback_slice, format_pattern};

/// Top-level pattern formatter — the walker's `format_pattern` arm
/// delegates here.
///
/// Returns the Doc the layout engine should render for the pattern node
/// at `nid`. On `None` pattern data (defensive guard) falls back to a
/// slice of the node's source span.
#[must_use]
pub fn format_pattern_data(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return doc::nil(),
    };
    let pd = match ctx.arena.pattern_data(nid) {
        Some(p) => p,
        None => {
            // Node is tagged as a pattern kind but carries no payload —
            // slice its source bytes verbatim as the safe fallback.
            return fallback_slice(ctx, data.span);
        }
    };

    match pd {
        PatternData::Wildcard => fmt_wildcard(),
        PatternData::Ident { name, mutable } => fmt_ident(ctx, *name, *mutable),
        PatternData::Literal { lit } => fmt_literal(ctx, *lit),
        PatternData::Tuple { elements } => fmt_tuple(ctx, elements),
        PatternData::Struct { path, fields } => fmt_struct(ctx, *path, fields),
        PatternData::EnumVariant { path, args } => fmt_enum_variant(ctx, *path, args),
        PatternData::Or { alternatives } => fmt_or(ctx, alternatives),
        PatternData::Binding { name, inner } => fmt_binding(ctx, *name, *inner),
        PatternData::Range { start, end } => fmt_range(ctx, *start, *end),
        PatternData::Reference { inner, mutable } => fmt_reference(ctx, *inner, *mutable),
        PatternData::Slice { elements } => fmt_slice(ctx, elements),
        PatternData::Rest { binder } => fmt_rest(ctx, *binder),
    }
}

// -----------------------------------------------------------------
// Per-variant helpers
// -----------------------------------------------------------------

/// `_`. Always a bare glyph (§6.1).
fn fmt_wildcard() -> Doc {
    doc::text("_")
}

/// `name` / `mut name`. The style spec is silent on `mut` in pattern
/// position, so we follow the AST flag directly: `mutable=true` prepends
/// `mut ` with a single trailing space, consistent with reference and
/// let-binding spacing elsewhere.
fn fmt_ident(ctx: &FormatContext<'_>, name: NodeId, mutable: bool) -> Doc {
    if mutable {
        doc::concat(vec![doc::text("mut "), ident_text(ctx, name)])
    } else {
        ident_text(ctx, name)
    }
}

/// Literal pattern. The AST's `lit` child is a placeholder node that
/// carries only the literal's source span (the parser does not
/// re-synthesise a typed literal here), so verbatim-slicing is both
/// correct and lossless — `0u64` stays `0u64`, `"foo"` stays `"foo"`.
fn fmt_literal(ctx: &FormatContext<'_>, lit: NodeId) -> Doc {
    match ctx.arena.get(lit) {
        Some(nd) => fallback_slice(ctx, nd.span),
        None => doc::nil(),
    }
}

/// Tuple pattern `(p1, p2, …)`. Standard tuple rules (§5.6):
/// one-line if it fits, else one element per line with a mandatory
/// trailing comma. Empty tuple renders as `()`.
fn fmt_tuple(ctx: &FormatContext<'_>, elements: &[NodeId]) -> Doc {
    if elements.is_empty() {
        return doc::text("()");
    }
    let items: Vec<Doc> = elements.iter().map(|e| format_pattern(ctx, *e)).collect();
    doc::group(doc::concat(vec![
        doc::text("("),
        doc::nest(
            2,
            doc::concat(vec![
                doc::softline(),
                separate_commas(items),
                doc::if_break(doc::text(","), doc::nil()),
            ]),
        ),
        doc::softline(),
        doc::text(")"),
    ]))
}

/// Struct (record) pattern `Path { f1, f2: p2, … }`.
///
/// Field punning (`{ x, y }`) is detected and preserved: a field whose
/// `pattern` is a `PatIdent` with the same source span as `field.name`
/// renders as just the name. Everything else uses the data-colon
/// (`field: pattern`, §5.2). Multi-line form takes the one-field-per-
/// line + trailing-comma shape from §3.1.
fn fmt_struct(ctx: &FormatContext<'_>, path: NodeId, fields: &[PatField]) -> Doc {
    let head = ident_text(ctx, path);
    if fields.is_empty() {
        return doc::concat(vec![head, doc::text(" {}")]);
    }
    let items: Vec<Doc> = fields.iter().map(|f| fmt_struct_field(ctx, f)).collect();
    doc::concat(vec![
        head,
        doc::text(" "),
        doc::group(doc::concat(vec![
            doc::text("{"),
            doc::nest(
                2,
                doc::concat(vec![
                    doc::line(),
                    separate_commas(items),
                    doc::if_break(doc::text(","), doc::nil()),
                ]),
            ),
            doc::line(),
            doc::text("}"),
        ])),
    ])
}

/// Single struct-field entry. Returns `name` when the field is a
/// shorthand pun, `name: pattern` otherwise.
fn fmt_struct_field(ctx: &FormatContext<'_>, field: &PatField) -> Doc {
    if is_field_shorthand(ctx, field) {
        return ident_text(ctx, field.name);
    }
    doc::concat(vec![
        ident_text(ctx, field.name),
        doc::text(": "),
        format_pattern(ctx, field.pattern),
    ])
}

/// Detect the parser's shorthand convention. The parser builds the
/// shorthand `{ x, y }` as `PatField { name: <Ident:"x">,
/// pattern: PatIdent { name: <Ident:"x"> } }` — the two Ident children
/// share the same source span. If the pattern is anything other than a
/// `PatIdent`, or its name span diverges from the field name's, the
/// field is a `name: pattern` entry.
fn is_field_shorthand(ctx: &FormatContext<'_>, field: &PatField) -> bool {
    let field_name_node = match ctx.arena.get(field.name) {
        Some(n) => n,
        None => return false,
    };
    let pat_node = match ctx.arena.get(field.pattern) {
        Some(n) => n,
        None => return false,
    };
    if pat_node.kind != NodeKind::PatIdent {
        return false;
    }
    // Pull the pattern's inner name NodeId; compare its span text.
    let inner_name = match ctx.arena.pattern_data(field.pattern) {
        Some(PatternData::Ident { name, mutable: false }) => *name,
        _ => return false,
    };
    let inner_name_node = match ctx.arena.get(inner_name) {
        Some(n) => n,
        None => return false,
    };
    // Span equality (not slice equality): the parser's shorthand
    // convention points the two Idents at the *same* source bytes
    // (one allocation, one byte range). Textually equal but
    // byte-position-distinct Idents (e.g. an explicit `x: x` typed by
    // the user) therefore round-trip as the explicit form rather than
    // being silently rewritten to a pun.
    field_name_node.span == inner_name_node.span
}

/// Enum-variant pattern `Variant(p1, …)` or just `Variant` for the
/// unit form. The AST stores only the final variant Ident (path
/// qualifiers like `Option::` are discarded by the parser — see the
/// Spec-gaps note in the module header), so this helper always
/// renders the stored Ident; it never reconstructs a qualified path.
fn fmt_enum_variant(ctx: &FormatContext<'_>, path: NodeId, args: &[NodeId]) -> Doc {
    let head = ident_text(ctx, path);
    if args.is_empty() {
        return head;
    }
    let items: Vec<Doc> = args.iter().map(|a| format_pattern(ctx, *a)).collect();
    doc::concat(vec![
        head,
        doc::group(doc::concat(vec![
            doc::text("("),
            doc::nest(
                2,
                doc::concat(vec![
                    doc::softline(),
                    separate_commas(items),
                    doc::if_break(doc::text(","), doc::nil()),
                ]),
            ),
            doc::softline(),
            doc::text(")"),
        ])),
    ])
}

/// Or pattern `A | B | C`.
///
/// Flat form: `A | B | C` (one space around each `|`). Break form:
/// each `|` starts a continuation line at the same indent as the
/// first alternative. We encode this with an interleaved separator
/// that is `" | "` flat and `\n| ` broken.
///
/// A single-alternative Or (should never occur in a well-formed parse,
/// but defensive) renders the one alternative directly, no bars.
fn fmt_or(ctx: &FormatContext<'_>, alternatives: &[NodeId]) -> Doc {
    match alternatives.len() {
        0 => doc::nil(),
        1 => format_pattern(ctx, alternatives[0]),
        _ => {
            let items: Vec<Doc> = alternatives
                .iter()
                .map(|a| format_pattern(ctx, *a))
                .collect();
            // Separator: " | " flat / "\n| " broken. `Line` renders as
            // " " flat and "\n<indent>" broken, so prepending "|" after
            // the line in break mode places `|` at the start of the
            // continuation. In flat we want it between with spaces:
            // concat(" " [via Line in flat], "| ").  We get that by
            // using doc::line() and `if_break` to drop the preceding
            // space in break mode:
            //
            //   flat:  <A> " " "| " <B>  →  "<A> | <B>"
            //   break: <A> "\n<indent>" "| " <B>  →  "<A>\n| <B>"
            //
            // The space-vs-nothing before `|` is handled by `Line` on
            // its own: in flat mode `Line` is one space; in break mode
            // it is a newline (no trailing space). So we insert
            // `line(), text("| ")` as the separator and let `Line`
            // choose. The `|` glyph then sits at the right side of the
            // separator in both modes.
            let sep = doc::concat(vec![doc::line(), doc::text("| ")]);
            doc::group(doc::separate(sep, items))
        }
    }
}

/// Binding pattern `name @ inner`. One space on each side of `@`
/// (§6.1). If the inner pattern is a multi-alternative Or (`x @ A | B`)
/// the parser builds the Or in place, so we recurse via
/// [`format_pattern`] which handles the width-wrap on the inner Or.
fn fmt_binding(ctx: &FormatContext<'_>, name: NodeId, inner: NodeId) -> Doc {
    doc::concat(vec![
        ident_text(ctx, name),
        doc::text(" @ "),
        format_pattern(ctx, inner),
    ])
}

/// Range pattern `a..b`, `..b`, `a..`, or `..`.
///
/// No spaces around `..` (§6.1). The current AST carries only
/// exclusive `..` (no `..=`); when a future AST variant adds an
/// `inclusive` flag, flip `op` to `"..="` on that branch.
///
/// Both bounds are rendered via [`format_pattern`] because the AST
/// comment admits either a Pattern or an Expression for each end;
/// `format_pattern` routes to the generic walker for non-pattern
/// kinds, so an expression-shaped bound still passes through its
/// own category formatter.
fn fmt_range(ctx: &FormatContext<'_>, start: Option<NodeId>, end: Option<NodeId>) -> Doc {
    let op = doc::text("..");
    match (start, end) {
        (Some(s), Some(e)) => doc::concat(vec![
            format_pattern_or_expr(ctx, s),
            op,
            format_pattern_or_expr(ctx, e),
        ]),
        (Some(s), None) => doc::concat(vec![format_pattern_or_expr(ctx, s), op]),
        (None, Some(e)) => doc::concat(vec![op, format_pattern_or_expr(ctx, e)]),
        (None, None) => op,
    }
}

/// Reference pattern `&p` or `&mut p`. No space between `&` and the
/// pointee (§6.2); one space after `mut`.
fn fmt_reference(ctx: &FormatContext<'_>, inner: NodeId, mutable: bool) -> Doc {
    if mutable {
        doc::concat(vec![doc::text("&mut "), format_pattern(ctx, inner)])
    } else {
        doc::concat(vec![doc::text("&"), format_pattern(ctx, inner)])
    }
}

/// Slice pattern `[a, b, ..rest]`. Standard array-literal rules
/// (§5.6 / §4.6): one-line if it fits, else one element per line with
/// trailing comma. The `..` / `..name` sub-pattern is represented in
/// the AST as `PatternData::Rest`, so we route through [`format_pattern`]
/// per-element and the rest sub-pattern formats itself (see
/// [`fmt_rest`]). Empty slice renders as `[]`.
fn fmt_slice(ctx: &FormatContext<'_>, elements: &[NodeId]) -> Doc {
    if elements.is_empty() {
        return doc::text("[]");
    }
    let items: Vec<Doc> = elements.iter().map(|e| format_pattern(ctx, *e)).collect();
    doc::group(doc::concat(vec![
        doc::text("["),
        doc::nest(
            2,
            doc::concat(vec![
                doc::softline(),
                separate_commas(items),
                doc::if_break(doc::text(","), doc::nil()),
            ]),
        ),
        doc::softline(),
        doc::text("]"),
    ]))
}

/// Rest sub-pattern `..` or `..name`. Appears only as an element of a
/// slice (and in the surface struct-pattern elide form, which is not
/// yet AST-representable — see Spec gaps). No space between `..` and
/// the optional binder.
fn fmt_rest(ctx: &FormatContext<'_>, binder: Option<NodeId>) -> Doc {
    match binder {
        Some(b) => doc::concat(vec![doc::text(".."), ident_text(ctx, b)]),
        None => doc::text(".."),
    }
}

// -----------------------------------------------------------------
// Small shared helpers
// -----------------------------------------------------------------

/// Slice an Ident node's source text into a `Doc::Text`. Falls back to
/// `nil` for unknown ids (defensive — a well-formed arena never
/// produces one).
fn ident_text(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    match ctx.arena.get(nid) {
        Some(nd) => fallback_slice(ctx, nd.span),
        None => doc::nil(),
    }
}

/// Format a Range endpoint. Range endpoints can be either patterns
/// (the AST's primary intent) or expressions (the AST comment admits
/// both). The walker's [`format_pattern`] routes patterns correctly;
/// an expression-kind endpoint would otherwise return `fallback_slice`
/// via the walker's catch-all. Preserving this helper as a named
/// callsite documents the intent and gives a single place to swap in
/// an expression dispatch if the AST tightens up.
fn format_pattern_or_expr(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    format_pattern(ctx, nid)
}

/// Join a vector of docs with `", "` in flat mode and a line break in
/// break mode. Standard list-element separator for every multi-element
/// construct in a pattern.
fn separate_commas(items: Vec<Doc>) -> Doc {
    doc::separate(doc::concat(vec![doc::text(","), doc::line()]), items)
}

#[cfg(test)]
mod tests {
    //! Pattern-formatter unit tests.
    //!
    //! Two test shapes:
    //!
    //! 1. **Match-arm parse path.** For variants the parser produces
    //!    (`Ident`, `Literal`, `Struct`, `EnumVariant`, `Or`, `Binding`,
    //!    plus the parser's shorthand struct field), we tunnel the
    //!    pattern through `match x { <PAT> => 0, _ => 0 }` and fish the
    //!    arm's pattern NodeId out of the arena.
    //! 2. **Direct arena construction.** The parser does not yet build
    //!    `Wildcard`, `Tuple`, `Range`, `Reference`, `Slice`, or `Rest`
    //!    patterns, so we construct those directly via
    //!    `arena.alloc_pattern`. The byte-identity check in those tests
    //!    is against the formatter's own expected output, not a parser
    //!    span.
    //!
    //! Rendering is routed through `format_pattern_data` with a bogus
    //! file + an empty `TriviaAttachmentMap`; the formatter never reads
    //! leading / trailing trivia for pattern bodies in this sprint.
    //! Width-wrap tests pass a narrow `max_width` and assert the output
    //! contains a `\n`.

    use super::*;
    use crate::FormatOptions as CrateFormatOptions;
    use crate::attach::TriviaAttachmentMap;
    use crate::cst::tokenize_with_trivia;
    use crate::layout::{self, FormatOptions as LayoutOptions};
    use paideia_as_ast::{AstArena, ExprData};
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};
    use paideia_as_parser::Parser;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    /// Render a pattern NodeId through the formatter, returning the
    /// laid-out string.
    fn render_pattern(
        source: &SourceText,
        arena: &AstArena,
        pat_id: NodeId,
        max_width: usize,
    ) -> String {
        let mut lex_sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), source, &mut lex_sink);
        // Attachment map keyed on the pattern root; patterns in isolation
        // never carry outer trivia so an empty map is fine for the
        // direct-arena construction path.
        let tmap = TriviaAttachmentMap::new();
        let opts = CrateFormatOptions {
            max_width,
            ..Default::default()
        };
        let ctx = FormatContext::new(source, &stream, arena, &tmap, &opts);
        let doc_tree = format_pattern_data(&ctx, pat_id);
        layout::render(
            &doc_tree,
            &LayoutOptions {
                max_width,
                ..Default::default()
            },
        )
    }

    /// Parse `pub let _p = match x { <PAT> => 0 }` (an item-level
    /// surface so `parse_source_file` accepts the input), then dig out
    /// the match's arm pattern and render it. Returns
    /// `(rendered, raw_source_slice)` where the raw slice spans the
    /// pattern's bytes.
    fn render_match_pattern(pat_src: &str, max_width: usize) -> (String, String) {
        let src = format!("pub let _p = match x {{ {pat_src} => 0 }}");
        let source = SourceText::from_bytes(file_id(), src.as_bytes()).unwrap();
        let mut lex_sink = VecSink::new();
        let mut lexer = Lexer::new(file_id(), &source);
        let tokens = lexer.collect_tokens(&mut lex_sink);
        let mut arena = AstArena::new();
        let mut parse_sink = VecSink::new();
        let _root = {
            let mut p = Parser::new(
                &tokens,
                source.content(),
                file_id(),
                &mut arena,
                &mut parse_sink,
            );
            p.parse_source_file().expect("parse failed")
        };
        // Match expressions aren't a top-level item surface; the match
        // was parsed as the top-level let's initializer. Easier: walk
        // the arena for the first ExprMatch node and take its first arm.
        let pat_id = find_first_match_arm_pattern(&arena).expect("no match arm pattern found");
        let span = arena.get(pat_id).unwrap().span;
        let raw = source.content()[span.byte_start() as usize..span.byte_end() as usize].to_string();
        let out = render_pattern(&source, &arena, pat_id, max_width);
        (out, raw)
    }

    /// Scan the arena for the first `ExprMatch` and return its first
    /// arm's pattern NodeId. The match-pattern tests all follow the
    /// shape `match x { <PAT> => 0 }`, so there is exactly one match
    /// node and one arm.
    fn find_first_match_arm_pattern(arena: &AstArena) -> Option<NodeId> {
        let n = u32::try_from(arena.len()).ok()?;
        for i in 1..=n {
            let nid = NodeId::new(i)?;
            if let Some(ExprData::Match { arms, .. }) = arena.expr_data(nid) {
                return arms.first().map(|a| a.pattern);
            }
        }
        None
    }

    /// Build a `SourceText` from an arbitrary byte source. Used by the
    /// direct-arena tests, which still need a SourceText so
    /// `fallback_slice` has something to read when sub-patterns slice
    /// their span. The byte content is chosen to match the span layout
    /// each test sets up by hand.
    fn make_source(bytes: &[u8]) -> SourceText {
        SourceText::from_bytes(file_id(), bytes).unwrap()
    }

    /// Allocate an Ident node whose span covers bytes `[start, start+len)`
    /// of the source. Returned id can be used anywhere an Ident is
    /// expected (names, binders, enum variants, …).
    fn alloc_ident(arena: &mut AstArena, start: u32, len: u32) -> NodeId {
        arena.alloc(
            NodeKind::Ident,
            Span::new(file_id(), start, len),
        )
    }

    // ---- Match-arm parse path --------------------------------------

    #[test]
    fn ident_pattern_simple() {
        let (got, raw) = render_match_pattern("x", 100);
        assert_eq!(got, "x");
        assert_eq!(got, raw);
    }

    #[test]
    fn literal_pattern_int() {
        let (got, raw) = render_match_pattern("42", 100);
        assert_eq!(got, "42");
        assert_eq!(got, raw);
    }

    #[test]
    fn literal_pattern_string() {
        let (got, _raw) = render_match_pattern("\"hi\"", 100);
        assert_eq!(got, "\"hi\"");
    }

    #[test]
    fn enum_variant_unit_bare() {
        // Bare variant without args parses as a plain Ident pattern,
        // which the formatter renders as the ident. We cannot test the
        // path-qualified unit form (`Option::None`) through the match
        // path because the parser drops the type qualifier.
        let (got, _raw) = render_match_pattern("None", 100);
        assert_eq!(got, "None");
    }

    #[test]
    fn enum_variant_with_single_arg() {
        let (got, _raw) = render_match_pattern("Some(x)", 100);
        assert_eq!(got, "Some(x)");
    }

    #[test]
    fn enum_variant_with_multi_args() {
        let (got, _raw) = render_match_pattern("Pair(a, b)", 100);
        assert_eq!(got, "Pair(a, b)");
    }

    #[test]
    fn or_pattern_two_alternatives_flat() {
        let (got, _raw) = render_match_pattern("1 | 2", 100);
        assert_eq!(got, "1 | 2");
    }

    #[test]
    fn or_pattern_three_alternatives_flat() {
        let (got, _raw) = render_match_pattern("1 | 2 | 3", 100);
        assert_eq!(got, "1 | 2 | 3");
    }

    #[test]
    fn or_pattern_breaks_before_pipe_under_narrow_width() {
        let (got, _raw) = render_match_pattern("foo | bar | baz | qux", 10);
        // Expect a line break before at least one `|`.
        assert!(got.contains('\n'), "expected multi-line, got {got:?}");
        // At break, the `|` should start a line (allowing leading
        // indent whitespace).
        let has_pipe_at_line_start = got
            .lines()
            .skip(1)
            .any(|l| l.trim_start().starts_with('|'));
        assert!(
            has_pipe_at_line_start,
            "expected `|` at start of continuation line, got {got:?}"
        );
    }

    #[test]
    fn or_pattern_nested_under_binding() {
        // `x @ A | B` — parser wraps the `|` into an Or inside the
        // binding, so the formatter renders `x @ A | B`.
        let (got, _raw) = render_match_pattern("x @ 1 | 2", 100);
        assert_eq!(got, "x @ 1 | 2");
    }

    #[test]
    fn binding_pattern_simple() {
        let (got, _raw) = render_match_pattern("name @ 1", 100);
        assert_eq!(got, "name @ 1");
    }

    #[test]
    fn struct_pattern_shorthand_two_fields() {
        let (got, _raw) = render_match_pattern("Point { x, y }", 100);
        assert_eq!(got, "Point { x, y }");
    }

    #[test]
    fn struct_pattern_with_rename() {
        // `Point { x: a, y: b }` — not shorthand, renamed bindings.
        let (got, _raw) = render_match_pattern("Point { x: a, y: b }", 100);
        assert_eq!(got, "Point { x: a, y: b }");
    }

    #[test]
    fn struct_pattern_wraps_when_narrow() {
        let (got, _raw) = render_match_pattern(
            "Big { aaa: xxx, bbb: yyy, ccc: zzz }",
            18,
        );
        assert!(got.contains('\n'), "expected multi-line struct, got {got:?}");
        assert!(got.contains("ccc: zzz,"), "trailing comma missing: {got:?}");
    }

    // ---- Direct-arena construction (variants not in parser) --------

    #[test]
    fn wildcard_renders_underscore() {
        let source = make_source(b"_");
        let mut arena = AstArena::new();
        let pat_id = arena.alloc_pattern(
            NodeKind::PatWildcard,
            Span::new(file_id(), 0, 1),
            PatternData::Wildcard,
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "_");
    }

    #[test]
    fn ident_pattern_mutable() {
        // Source "mut n"; we alloc an Ident spanning "n" (bytes 4..5).
        let source = make_source(b"mut n");
        let mut arena = AstArena::new();
        let name = alloc_ident(&mut arena, 4, 1);
        let pat_id = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 0, 5),
            PatternData::Ident { name, mutable: true },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "mut n");
    }

    #[test]
    fn tuple_pattern_three_elements_flat() {
        // Source backs the Ident spans: `(a, b, c)` → letters at
        // positions 1, 4, 7.
        let source = make_source(b"(a, b, c)");
        let mut arena = AstArena::new();
        let a = alloc_ident(&mut arena, 1, 1);
        let b = alloc_ident(&mut arena, 4, 1);
        let c = alloc_ident(&mut arena, 7, 1);
        let pa = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 1, 1),
            PatternData::Ident { name: a, mutable: false },
        );
        let pb = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 4, 1),
            PatternData::Ident { name: b, mutable: false },
        );
        let pc = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 7, 1),
            PatternData::Ident { name: c, mutable: false },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatTuple,
            Span::new(file_id(), 0, 9),
            PatternData::Tuple { elements: vec![pa, pb, pc] },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "(a, b, c)");
    }

    #[test]
    fn range_pattern_exclusive_both_bounds() {
        // Build `0..10` by hand: PatLiteral(0) and PatLiteral(10), the
        // literal child is a Placeholder whose span slices to the
        // numeric text. Source "0..10": "0" at 0..1, "10" at 3..5.
        let source = make_source(b"0..10");
        let mut arena = AstArena::new();
        let lit0_placeholder = arena.alloc(
            NodeKind::Placeholder,
            Span::new(file_id(), 0, 1),
        );
        let lit0 = arena.alloc_pattern(
            NodeKind::PatLiteral,
            Span::new(file_id(), 0, 1),
            PatternData::Literal { lit: lit0_placeholder },
        );
        let lit10_placeholder = arena.alloc(
            NodeKind::Placeholder,
            Span::new(file_id(), 3, 2),
        );
        let lit10 = arena.alloc_pattern(
            NodeKind::PatLiteral,
            Span::new(file_id(), 3, 2),
            PatternData::Literal { lit: lit10_placeholder },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatRange,
            Span::new(file_id(), 0, 5),
            PatternData::Range { start: Some(lit0), end: Some(lit10) },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "0..10");
    }

    #[test]
    fn range_pattern_open_start() {
        // `..10`.
        let source = make_source(b"..10");
        let mut arena = AstArena::new();
        let lit10_placeholder = arena.alloc(
            NodeKind::Placeholder,
            Span::new(file_id(), 2, 2),
        );
        let lit10 = arena.alloc_pattern(
            NodeKind::PatLiteral,
            Span::new(file_id(), 2, 2),
            PatternData::Literal { lit: lit10_placeholder },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatRange,
            Span::new(file_id(), 0, 4),
            PatternData::Range { start: None, end: Some(lit10) },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "..10");
    }

    #[test]
    fn range_pattern_open_end() {
        // `0..`.
        let source = make_source(b"0..");
        let mut arena = AstArena::new();
        let lit0_placeholder = arena.alloc(
            NodeKind::Placeholder,
            Span::new(file_id(), 0, 1),
        );
        let lit0 = arena.alloc_pattern(
            NodeKind::PatLiteral,
            Span::new(file_id(), 0, 1),
            PatternData::Literal { lit: lit0_placeholder },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatRange,
            Span::new(file_id(), 0, 3),
            PatternData::Range { start: Some(lit0), end: None },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "0..");
    }

    #[test]
    fn range_pattern_fully_open() {
        let source = make_source(b"..");
        let mut arena = AstArena::new();
        let pat_id = arena.alloc_pattern(
            NodeKind::PatRange,
            Span::new(file_id(), 0, 2),
            PatternData::Range { start: None, end: None },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "..");
    }

    #[test]
    fn reference_pattern_shared() {
        let source = make_source(b"&p");
        let mut arena = AstArena::new();
        let p = alloc_ident(&mut arena, 1, 1);
        let inner = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 1, 1),
            PatternData::Ident { name: p, mutable: false },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatReference,
            Span::new(file_id(), 0, 2),
            PatternData::Reference { inner, mutable: false },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "&p");
    }

    #[test]
    fn reference_pattern_mutable() {
        let source = make_source(b"&mut p");
        let mut arena = AstArena::new();
        let p = alloc_ident(&mut arena, 5, 1);
        let inner = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 5, 1),
            PatternData::Ident { name: p, mutable: false },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatReference,
            Span::new(file_id(), 0, 6),
            PatternData::Reference { inner, mutable: true },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "&mut p");
    }

    #[test]
    fn slice_pattern_with_rest_binding() {
        // `[a, b, ..rest]` — Idents a (1), b (4); Rest binder "rest"
        // at bytes 9..13; closing bracket at 13.
        let source = make_source(b"[a, b, ..rest]");
        let mut arena = AstArena::new();
        let a = alloc_ident(&mut arena, 1, 1);
        let b = alloc_ident(&mut arena, 4, 1);
        let rest_binder = alloc_ident(&mut arena, 9, 4);
        let pa = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 1, 1),
            PatternData::Ident { name: a, mutable: false },
        );
        let pb = arena.alloc_pattern(
            NodeKind::PatIdent,
            Span::new(file_id(), 4, 1),
            PatternData::Ident { name: b, mutable: false },
        );
        let prest = arena.alloc_pattern(
            NodeKind::PatRest,
            Span::new(file_id(), 7, 6),
            PatternData::Rest { binder: Some(rest_binder) },
        );
        let pat_id = arena.alloc_pattern(
            NodeKind::PatSlice,
            Span::new(file_id(), 0, 14),
            PatternData::Slice { elements: vec![pa, pb, prest] },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "[a, b, ..rest]");
    }

    #[test]
    fn slice_pattern_empty() {
        let source = make_source(b"[]");
        let mut arena = AstArena::new();
        let pat_id = arena.alloc_pattern(
            NodeKind::PatSlice,
            Span::new(file_id(), 0, 2),
            PatternData::Slice { elements: vec![] },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "[]");
    }

    #[test]
    fn rest_standalone_no_binder() {
        let source = make_source(b"..");
        let mut arena = AstArena::new();
        let pat_id = arena.alloc_pattern(
            NodeKind::PatRest,
            Span::new(file_id(), 0, 2),
            PatternData::Rest { binder: None },
        );
        let got = render_pattern(&source, &arena, pat_id, 100);
        assert_eq!(got, "..");
    }

    // ---- Fallback / defensive --------------------------------------

    #[test]
    fn format_pattern_data_falls_back_on_missing_data() {
        // Allocate a bare node that is tagged as a pattern kind but
        // never had pattern-data populated. The outer dispatch should
        // slice its source span verbatim.
        let source = make_source(b"xxx");
        let mut arena = AstArena::new();
        let bare = arena.alloc(
            NodeKind::PatWildcard,
            Span::new(file_id(), 0, 3),
        );
        let got = render_pattern(&source, &arena, bare, 100);
        assert_eq!(got, "xxx");
    }

}
