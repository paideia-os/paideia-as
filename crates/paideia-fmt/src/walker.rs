//! AST walker scaffold for the paideia-as formatter (FMT-3,
//! paideia-as#1565).
//!
//! # Scope
//!
//! This is the *critical-path expander*. It stands up the dispatch
//! surface the per-kind formatters (FMT-4..8) will plug into, without
//! implementing any one of them. Every per-kind arm today returns the
//! same safe fallback: `Doc::text(source[node.span])`. The top-level
//! entry [`format_module`] goes one step further and returns the entire
//! source verbatim, which is the only way to preserve byte-identity
//! when none of the per-kind formatters have landed — see the "fallback
//! contract" note below.
//!
//! # Dispatch surface (what FMT-4..8 fill in)
//!
//! | ticket     | function                                           |
//! |------------|----------------------------------------------------|
//! | FMT-4a/4b  | [`format_item`] — module items (Let, Struct, etc.) |
//! | FMT-5a/5b  | [`format_expr`] — expressions                      |
//! | FMT-6      | [`format_type`] — type expressions                 |
//! | FMT-7      | [`format_pattern`] — patterns                      |
//! | FMT-8      | [`format_attribute`] — `#[...]` / `#![...]`        |
//!
//! Each of these is a public function so the per-kind sprints can swap
//! its body without touching the dispatch helper [`format_node`] or the
//! top-level entry.
//!
//! # Fallback contract — "source[span] today, structural tomorrow"
//!
//! * The dispatch helper [`format_node`] matches on
//!   [`NodeKind`](paideia_as_ast::NodeKind) and routes to the per-kind
//!   formatter. Every arm today returns [`fallback_slice`] —
//!   `Doc::text(source[node.span])`.
//! * [`format_module`] is *special*: a module/structure node's own
//!   span covers only its first token (see
//!   [`parse_source_file`](paideia_as_parser) in paideia-as-parser),
//!   not the whole file. For FMT-3's byte-identity contract we
//!   therefore bypass the dispatch for the root and emit
//!   `source.content()` as a single `Doc::text`. FMT-4a will replace
//!   this with a structural walk over
//!   `ItemData::Structure { items, inner_attrs, .. }`.
//! * The [`FormatContext`] threads the [`TriviaStream`], the
//!   [`TriviaAttachmentMap`], the [`AstArena`], the source, and the
//!   [`FormatOptions`] through every dispatch call so per-kind
//!   formatters have all four at hand without reconstruction.
//!
//! # Trivia emission — scaffold only
//!
//! The sprint spec asks the walker to "emit leading trivia before each
//! node, trailing after". For the FMT-3 scaffold this is *dormant
//! scaffolding*: the helpers [`emit_leading_trivia`] and
//! [`emit_trailing_trivia`] exist, read from the attachment map, and
//! produce a Doc; but [`fallback_slice`] does NOT call them because
//! `source[span]` already contains inner comments, and double-emitting
//! leading comments *outside* the slice would break byte-identity. The
//! per-kind formatters coming in FMT-4..8 will adopt the helpers once
//! their Doc construction stops emitting the whole span as one Text —
//! at that point structural reconstruction demands explicit trivia
//! placement, and the attachment map is the canonical source. Inner
//! trivia placement remains per-kind-formatter responsibility (where it
//! lands depends on syntax — before a brace, after a comma, …).

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_lexer::{SourceText, Trivia, TriviaKind};

use crate::FormatOptions;
use crate::attach::TriviaAttachmentMap;
use crate::cst::TriviaStream;
use crate::doc::{self, Doc};

/// Read-only bundle threaded through every walker call.
///
/// The walker never mutates the context; per-kind formatters receive it
/// by `&FormatContext<'_>`. All five borrows live for the same arena
/// lifetime `'a`, which the top-level [`crate::format`] entry ties to
/// one `format()` invocation.
pub struct FormatContext<'a> {
    /// Source text (validated UTF-8, lexed verbatim).
    pub source: &'a SourceText,
    /// Lossless token+trivia stream (FMT-1a).
    pub stream: &'a TriviaStream,
    /// AST arena produced by `parse_source_file`.
    pub arena: &'a AstArena,
    /// Trivia-to-node attachment map (FMT-1b).
    pub trivia_map: &'a TriviaAttachmentMap,
    /// Caller-chosen formatter knobs (tab width, blank-line cap, etc.).
    /// Per-kind formatters consult only the fields they need.
    pub opts: &'a FormatOptions,
}

impl<'a> FormatContext<'a> {
    /// Construct a new walker context. Prefer this over direct struct
    /// literal so field additions don't ripple through call sites.
    #[must_use]
    pub fn new(
        source: &'a SourceText,
        stream: &'a TriviaStream,
        arena: &'a AstArena,
        trivia_map: &'a TriviaAttachmentMap,
        opts: &'a FormatOptions,
    ) -> Self {
        Self {
            source,
            stream,
            arena,
            trivia_map,
            opts,
        }
    }

    /// Slice the source by byte-span. Returns an empty slice when the
    /// span is out of bounds (defensive — a well-formed arena never
    /// produces such a span, but the formatter is a non-fatal path and
    /// we would rather emit nothing than panic).
    #[must_use]
    pub fn slice(&self, span: Span) -> &'a str {
        let content = self.source.content();
        let start = span.byte_start() as usize;
        let end = span.byte_end() as usize;
        if start <= content.len() && end <= content.len() && start <= end {
            &content[start..end]
        } else {
            ""
        }
    }
}

/// Format the whole source file.
///
/// FMT-3 fallback: emits `source.content()` as a single `Doc::text`.
/// This is byte-identical by construction — the layout renderer never
/// scans `Text` for newlines, so the string passes through verbatim.
///
/// FMT-4a will replace this body with a structural walk over the root
/// `Structure`'s `items` + `inner_attrs`, calling [`format_item`] per
/// item and interleaving explicit blank-line policy from
/// [`FormatOptions`].
pub fn format_module(ctx: &FormatContext<'_>, _root: NodeId) -> Doc {
    doc::text(ctx.source.content().to_string())
}

/// Dispatch a single AST node to its per-kind formatter.
///
/// The match is **non-exhaustive** by design: `NodeKind` is
/// `#[non_exhaustive]`, and every future variant falls into the catch-
/// all arm that returns [`fallback_slice`]. New arms added by FMT-4..8
/// move specific `NodeKind`s off the catch-all and into their category
/// formatter.
pub fn format_node(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        // Caller handed us an id this arena never minted. Emit nothing;
        // the Doc layer treats Nil as the empty identity.
        None => return doc::nil(),
    };

    match data.kind {
        // Items.
        NodeKind::Module
        | NodeKind::Signature
        | NodeKind::Structure
        | NodeKind::Functor
        | NodeKind::FunctorParam
        | NodeKind::FunctorDecl
        | NodeKind::GenericParam
        | NodeKind::Effect
        | NodeKind::OpSig
        | NodeKind::Capability
        | NodeKind::Let
        | NodeKind::Struct
        | NodeKind::Enum
        | NodeKind::Trait
        | NodeKind::Impl
        | NodeKind::UnsafeBlock
        | NodeKind::MacroDecl
        | NodeKind::MacroPattern
        | NodeKind::MacroTemplate => format_item(ctx, nid),

        // Expressions.
        NodeKind::ExprLambda
        | NodeKind::ExprActionBlock
        | NodeKind::ExprWithHandler
        | NodeKind::ExprUnsafe
        | NodeKind::ExprInfix
        | NodeKind::ExprPrefix
        | NodeKind::ExprPostfix
        | NodeKind::ExprCast
        | NodeKind::ExprLiteral
        | NodeKind::ExprPath
        | NodeKind::ExprCall
        | NodeKind::ExprBlock
        | NodeKind::ExprMatch
        | NodeKind::ExprIf
        | NodeKind::ExprLoop
        | NodeKind::ExprFor
        | NodeKind::ExprBreak
        | NodeKind::ExprContinue
        | NodeKind::ExprPerform
        | NodeKind::ExprResume
        | NodeKind::ExprHandlerValue
        | NodeKind::ExprQuote
        | NodeKind::ExprAntiquote
        | NodeKind::ExprFunctorApp
        | NodeKind::ExprPack
        | NodeKind::ExprUnpack
        | NodeKind::ExprLetModule
        | NodeKind::ExprRecordCons
        | NodeKind::ExprFieldAccess
        | NodeKind::ExprString
        | NodeKind::ExprByteString
        | NodeKind::ExprInlineBytes
        | NodeKind::ExprInlineStr
        | NodeKind::ExprBorrow
        | NodeKind::ExprDeref
        | NodeKind::ExprArrayLit
        | NodeKind::ExprArrayRepeat
        | NodeKind::ExprUninit
        | NodeKind::ExprRange
        | NodeKind::ExprTuple
        // `OperandMemoryRef` is a sub-operand kind, not an expression
        // in its own right in most parse paths, but its `ExprData` is
        // routed through [`format_expr`] by FMT-5b (paideia-as#1569)
        // so the memref bracket form is structural.
        | NodeKind::OperandMemoryRef => format_expr(ctx, nid),

        // Types.
        NodeKind::TypeName
        | NodeKind::TypeFnPtr
        | NodeKind::TypeClosure
        | NodeKind::TypeTuple
        | NodeKind::TypeLinearClass
        | NodeKind::TypeEffectRow
        | NodeKind::TypePtr
        | NodeKind::TypeRef
        | NodeKind::TypeRecord
        | NodeKind::TypeEnum
        | NodeKind::TypeSelfQualifiedPath
        | NodeKind::TypeArray
        | NodeKind::TypeForall => format_type(ctx, nid),

        // Patterns.
        NodeKind::PatWildcard
        | NodeKind::PatIdent
        | NodeKind::PatLiteral
        | NodeKind::PatTuple
        | NodeKind::PatStruct
        | NodeKind::PatEnumVariant
        | NodeKind::PatOr
        | NodeKind::PatBinding
        | NodeKind::PatRange
        | NodeKind::PatReference
        | NodeKind::PatSlice
        | NodeKind::PatRest => format_pattern(ctx, nid),

        // Statements aren't a top-level dispatch target in FMT-3 — the
        // enclosing item/expression formatter owns their emission. The
        // dispatch keeps a safe path if a caller does hand one over.
        NodeKind::StmtLet
        | NodeKind::StmtExpr
        | NodeKind::StmtReturn
        | NodeKind::StmtInstruction
        | NodeKind::StmtLabel => fallback_slice(ctx, data.span),

        // Operands similarly default-fallback until FMT-5 claims them.
        // OperandMemoryRef has moved above into the expression bucket
        // (FMT-5b, paideia-as#1569) because it carries an `ExprData`
        // payload with a formatter.
        NodeKind::OperandRegister
        | NodeKind::OperandImmediate => fallback_slice(ctx, data.span),

        // Identifier / Placeholder leaves — always fall back to source
        // slice. There is no structural choice to make.
        NodeKind::Ident | NodeKind::Placeholder => fallback_slice(ctx, data.span),

        // Any variant added after this scaffold was authored falls into
        // the fallback automatically. `#[non_exhaustive]` on NodeKind
        // forces this arm.
        _ => fallback_slice(ctx, data.span),
    }
}

/// FMT-4a/4b target — module-item formatter.
///
/// FMT-4a Bucket A (paideia-as#1566) delegates to
/// [`crate::format_item::format_item`], which covers nine item kinds:
/// `Let` (structural), plus `Module`, `Structure`, `Signature`, `Enum`,
/// `Struct`, `Trait`, and `Impl` (byte-identical fallback on the whole
/// item span until FMT-4b replaces each with a structural
/// decomposition). Variants not in Bucket A (`Functor`, `FunctorParam`,
/// `FunctorDecl`, `Effect`, `OpSig`, `Capability`, `UnsafeBlock`,
/// `MacroDecl`, `MacroPattern`, `MacroTemplate`) fall through to
/// [`fallback_slice`] inside [`crate::format_item::format_item`].
pub fn format_item(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    crate::format_item::format_item(ctx, nid)
}

/// FMT-5a/5b target — expression formatter.
///
/// FMT-5a (paideia-as#1568) delegates to
/// [`crate::format_expr::format_expr`] for the primary / postfix / call
/// / binop / prefix buckets. Bucket B (match / if / loop / for / block
/// / lambda / with-handler / unsafe / perform / resume / handler-value
/// / quote / antiquote / functor-app / pack / unpack / let-module /
/// record-cons) still falls through to [`fallback_slice`] inside the
/// dispatcher and will light up with FMT-5b.
pub fn format_expr(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return doc::nil(),
    };
    crate::format_expr::format_expr(ctx, nid, data.kind)
}

/// FMT-6 target — type-expression formatter.
///
/// Delegates to [`crate::format_type::format_type_data`], which covers
/// every `TypeData` variant currently exposed by `paideia-as-ast`
/// (Name, FnPtr, Closure, Tuple, LinearClass, EffectRow, Ptr, Ref,
/// Record, Enum, SelfQualifiedPath, Array, Forall). Falls back to a
/// source slice when the arena has no type-data for `nid`.
pub fn format_type(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    crate::format_type::format_type_data(ctx, nid)
}

/// FMT-7 target — pattern formatter.
///
/// Delegates to [`crate::format_pattern::format_pattern_data`], which
/// covers every `PatternData` variant currently carried by
/// `paideia-as-ast`: `Wildcard`, `Ident`, `Literal`, `Tuple`, `Struct`,
/// `EnumVariant`, `Or`, `Binding`, `Range`, `Reference`, `Slice`, and
/// `Rest`. Falls back to a source slice when the arena has no
/// pattern-data for `nid`.
pub fn format_pattern(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    crate::format_pattern::format_pattern_data(ctx, nid)
}

/// FMT-8 target — attribute formatter (paideia-as#1572).
///
/// Attributes have no dedicated [`NodeKind`] variant in `paideia-as-ast`
/// today — they live on side-tables (`StructAttrTable`,
/// `StructFieldAttrTable`, `FunctorAttrTable`, `ItemAtomicTable`,
/// `MacroExpandDirectiveTable`) or as inline fields of their owning
/// item (`ItemData::Let`'s `@align`/`@ring`/...; `ExprData::Match`'s
/// `MatchAttrs::jump_table`). The real per-shape emit helpers live in
/// [`crate::format_attribute`] and are called directly by the item /
/// expression formatters that know which attributes to re-emit.
///
/// This dispatch-surface function therefore delegates to the module's
/// defensive fallback: a byte-identical slice of the given node's own
/// span. It is reached only if a future NodeKind variant describes an
/// attribute node, or if a caller routes an unexpected id through the
/// walker; neither path exists in the shipping AST.
pub fn format_attribute(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    crate::format_attribute::format_attribute_fallback(ctx, nid)
}

/// The one safe fallback — a `Doc::text` whose content is the source
/// bytes covered by `span`.
///
/// Every per-kind stub routes here today. The function is public so
/// FMT-4..8 formatters can still fall back for the handful of
/// pathological or incomplete-syntax cases they do not want to
/// restructure (e.g., malformed-but-recovered sub-trees).
#[must_use]
pub fn fallback_slice(ctx: &FormatContext<'_>, span: Span) -> Doc {
    doc::text(ctx.slice(span).to_owned())
}

/// Dormant helper (FMT-3): format the leading-comment trivia attached
/// to `nid` by [`crate::attach::attach_trivia`] into a Doc.
///
/// Not called by any stub today — see the "Trivia emission" note in the
/// module header. Each comment lands as `Doc::text(source[comment.span])`
/// followed by a [`doc::hardline`]; non-comment trivia (whitespace,
/// newlines) is ignored on purpose because its placement is a
/// per-kind-formatter concern. FMT-4..8 call this helper from each
/// per-kind formatter as its Doc construction switches from
/// whole-span slices to structural composition.
#[must_use]
pub fn emit_leading_trivia(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let att = match ctx.trivia_map.get(nid) {
        Some(a) => a,
        None => return doc::nil(),
    };
    emit_trivia_vec(ctx, &att.leading, /* trailing = */ false)
}

/// Dormant helper (FMT-3): format the trailing-comment trivia attached
/// to `nid` into a Doc.
///
/// See [`emit_leading_trivia`] for the current status. Trailing
/// comments are same-line comments after the node's last token; the
/// helper emits them as `Doc::text(source[comment.span])` preceded by a
/// single space (the comment keeps its original leading `//` or
/// `/* */` markers because the slice is taken from the raw source).
#[must_use]
pub fn emit_trailing_trivia(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let att = match ctx.trivia_map.get(nid) {
        Some(a) => a,
        None => return doc::nil(),
    };
    emit_trivia_vec(ctx, &att.trailing, /* trailing = */ true)
}

/// Shared body for the two emit helpers above. Keeps the "comment-only"
/// filter and the Doc-construction convention in one place.
fn emit_trivia_vec(ctx: &FormatContext<'_>, trivia: &[Trivia], trailing: bool) -> Doc {
    let mut parts: Vec<Doc> = Vec::new();
    for (idx, t) in trivia.iter().enumerate() {
        if !is_comment(t.kind) {
            continue;
        }
        let text = ctx.slice(t.span).to_owned();
        if trailing {
            // Trailing comments ride on the same line as the preceding
            // token. Convention: one space separator before the first,
            // a soft space between consecutive trailing comments.
            parts.push(doc::text(" "));
            parts.push(doc::text(text));
        } else {
            // Leading comments stack above the node. Each gets its own
            // hard line (per FMT-1b, leading comments are always on
            // their own source line in paideia-as style).
            if idx > 0 {
                parts.push(doc::hardline());
            }
            parts.push(doc::text(text));
            // Terminate the last leading comment with a hardline so the
            // node body begins on a fresh line.
            if idx + 1 == trivia.len() {
                parts.push(doc::hardline());
            }
        }
    }
    if parts.is_empty() {
        doc::nil()
    } else {
        doc::concat(parts)
    }
}

fn is_comment(k: TriviaKind) -> bool {
    matches!(
        k,
        TriviaKind::LineComment
            | TriviaKind::BlockComment
            | TriviaKind::DocLineComment
            | TriviaKind::DocBlockComment
    )
}

/// Dormant helper (FMT-9): format the OUTER doc blocks attached to
/// `nid` by [`crate::doc_comment::attach_doc_blocks`] into a Doc.
///
/// Only [`crate::doc_comment::DocBlock`] entries with `is_inner ==
/// false` are emitted here — these are the `///` / `/**` style docs
/// that belong above the node. Each line of a block is a `Doc::text`
/// separated by [`doc::hardline`]; the block itself is followed by a
/// final `doc::hardline` so the node body begins on a fresh line.
///
/// Not called by any stub today — see the "Trivia emission" note in
/// the module header. FMT-4..8 structural formatters pick this up as
/// they transition from span-slice to structural emission.
#[must_use]
pub fn emit_outer_doc_blocks(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let att = match ctx.trivia_map.get(nid) {
        Some(a) => a,
        None => return doc::nil(),
    };
    emit_doc_blocks_filtered(att, /* want_inner = */ false)
}

/// Dormant helper (FMT-9): format the INNER doc blocks attached to
/// `nid` into a Doc.
///
/// Only [`crate::doc_comment::DocBlock`] entries with `is_inner ==
/// true` are emitted — these are the `//!` / `/*!` style docs that
/// belong at the TOP of this node's body (inner leading), plus docs
/// that detached from a following item via the §9.2 blank-line rule.
/// Structural formatters for module / structure / block call this
/// after emitting the opening delimiter and before the first child.
#[must_use]
pub fn emit_inner_doc_blocks(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let att = match ctx.trivia_map.get(nid) {
        Some(a) => a,
        None => return doc::nil(),
    };
    emit_doc_blocks_filtered(att, /* want_inner = */ true)
}

fn emit_doc_blocks_filtered(
    att: &crate::attach::TriviaAttachment,
    want_inner: bool,
) -> Doc {
    let mut parts: Vec<Doc> = Vec::new();
    for (bi, block) in att.doc_blocks.iter().enumerate() {
        if block.is_inner != want_inner {
            continue;
        }
        if block.lines.is_empty() {
            continue;
        }
        if bi > 0 && !parts.is_empty() {
            // Blank line between blocks preserves the visual separator
            // the source carried; a single hardline would merge them.
            parts.push(doc::hardline());
        }
        for (li, line) in block.lines.iter().enumerate() {
            if li > 0 {
                parts.push(doc::hardline());
            }
            parts.push(doc::text(line.clone()));
        }
        // Terminate each block with a hardline so the node body (or
        // the next block) begins on a fresh line.
        parts.push(doc::hardline());
    }
    if parts.is_empty() {
        doc::nil()
    } else {
        doc::concat(parts)
    }
}

#[cfg(test)]
mod tests {
    //! Walker-level unit tests. The crate-level `format()` tests in
    //! `lib.rs` cover the whole pipeline (lex → parse → attach → walk →
    //! render); these tests isolate the walker's dispatch and the
    //! `FormatContext` plumbing.

    use super::*;
    use crate::attach::attach_trivia;
    use crate::cst::tokenize_with_trivia;
    use crate::layout::{self, FormatOptions as LayoutOptions};
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, VecSink};
    use paideia_as_parser::Parser;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    /// One-shot pipeline builder — returns the rendered output of
    /// `format_module` on the parsed root. Encapsulates the stream /
    /// arena / tmap plumbing so each test stays focused on behavior.
    ///
    /// `SourceText::from_bytes` rejects an empty byte slice; we
    /// short-circuit empty input to an empty output, matching the
    /// crate-level `format()` fast path.
    fn render_pipeline(src: &str) -> String {
        if src.is_empty() {
            return String::new();
        }
        let source = SourceText::from_bytes(file_id(), src.as_bytes())
            .expect("test source must be valid UTF-8");
        let mut lex_sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut lex_sink);
        // Parse to populate the arena.
        let mut lex_sink2 = VecSink::new();
        let mut lexer = paideia_as_lexer::Lexer::new(file_id(), &source);
        let tokens = lexer.collect_tokens(&mut lex_sink2);
        let mut arena = AstArena::new();
        let root_id = {
            let mut sink = VecSink::new();
            let mut p = Parser::new(
                &tokens,
                source.content(),
                file_id(),
                &mut arena,
                &mut sink,
            );
            p.parse_source_file().ok()
        };
        let root_id = root_id.unwrap_or_else(|| {
            // Fall back to a synthetic placeholder if parse failed; the
            // walker still has a NodeId to dispatch on. The outer
            // `crate::format` path handles the error-return contract.
            let span = paideia_as_diagnostics::Span::new(file_id(), 0, src.len() as u32);
            arena.alloc(NodeKind::Placeholder, span)
        });
        let tmap = attach_trivia(&stream, &arena, root_id);
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let doc_tree = format_module(&ctx, root_id);
        layout::render(&doc_tree, &LayoutOptions::default())
    }

    #[test]
    fn format_module_emits_entire_source_verbatim() {
        let src = "pub let x : u64 = 1";
        let got = render_pipeline(src);
        assert_eq!(got, src, "fallback must be byte-identical to input");
    }

    #[test]
    fn format_module_on_empty_source() {
        assert_eq!(render_pipeline(""), "");
    }

    #[test]
    fn format_module_preserves_leading_comment() {
        // Comments inside source[root.span]'s coverage (which, for the
        // whole-source fallback, is the whole file) must survive.
        let src = "// hello\nlet x = 1\n";
        assert_eq!(render_pipeline(src), src);
    }

    #[test]
    fn format_module_preserves_trailing_comment() {
        let src = "let x = 1 // trailing\n";
        assert_eq!(render_pipeline(src), src);
    }

    #[test]
    fn dispatch_on_invalid_node_id_returns_nil() {
        // A NodeId past the arena's end routes through the None guard
        // in format_node → doc::nil. Verifies the defensive path.
        // Use a non-empty byte slice because SourceText rejects empty.
        let source = SourceText::from_bytes(file_id(), b" ").unwrap();
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut sink);
        let arena = AstArena::new();
        let tmap = TriviaAttachmentMap::new();
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let bogus = NodeId::new(999).unwrap();
        let d = format_node(&ctx, bogus);
        let out = layout::render(&d, &LayoutOptions::default());
        assert_eq!(out, "");
    }

    #[test]
    fn fallback_slice_returns_span_text() {
        let src = "pub let a = 1";
        let source = SourceText::from_bytes(file_id(), src.as_bytes()).unwrap();
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut sink);
        let arena = AstArena::new();
        let tmap = TriviaAttachmentMap::new();
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        // Slice "pub" (bytes 0..3).
        let span = paideia_as_diagnostics::Span::new(file_id(), 0, 3);
        let d = fallback_slice(&ctx, span);
        let out = layout::render(&d, &LayoutOptions::default());
        assert_eq!(out, "pub");
    }

    #[test]
    fn emit_leading_trivia_from_empty_map_is_nil() {
        let source = SourceText::from_bytes(file_id(), b"let x").unwrap();
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut sink);
        let arena = AstArena::new();
        let tmap = TriviaAttachmentMap::new();
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let bogus = NodeId::new(1).unwrap();
        let d = emit_leading_trivia(&ctx, bogus);
        assert_eq!(layout::render(&d, &LayoutOptions::default()), "");
    }

    #[test]
    fn format_module_idempotent_on_byte_identical_fallback() {
        // The whole-source fallback is a fixed point of the pipeline:
        // feeding the output back in yields the same output. Guards
        // the FMT-3 byte-identity contract across the walker boundary.
        let src = "pub let x : u64 = 42\n";
        let once = render_pipeline(src);
        let twice = render_pipeline(&once);
        assert_eq!(once, twice);
    }
}
