//! FMT-4a Bucket A + FMT-4b Bucket B: per-item formatters.
//!
//! # Scope (paideia-as#1566, paideia-as#1567)
//!
//! Bucket A of the FMT-4a sprint — nine `ItemData` variants:
//!
//! | variant     | strategy                                                |
//! |-------------|---------------------------------------------------------|
//! | `Let`       | structural reconstruction from AST fields               |
//! | `Module`    | structural: `module <name> = <body>`                    |
//! | `Structure` | span-slice (whole-decl span covers `structure {...}`)   |
//! | `Signature` | span-slice (whole-decl span covers `signature ... = ..`)|
//! | `Enum`      | span-slice (whole-decl span)                            |
//! | `Struct`    | span-slice (whole-decl span)                            |
//! | `Trait`     | span-slice (whole-decl span)                            |
//! | `Impl`      | span-slice (whole-decl span)                            |
//!
//! Bucket B of the FMT-4b sprint — Functor-family item variants:
//!
//! | variant        | strategy                                             |
//! |----------------|------------------------------------------------------|
//! | `Functor`      | structural: `functor (p1)(p2) -> <body>`             |
//! | `FunctorParam` | structural: `(<name> : <sig>)`                       |
//! | `FunctorDecl`  | span-slice (synthesised ident spans — see below)     |
//!
//! # FMT-4b Bucket B — slice-brief vs AST-reality mapping
//!
//! The FMT-4b slice brief names eight shapes: `Functor`, `PackedStruct`,
//! `GpuContext`, `Timeline`, `EndianAttr`, `MacroExpandDirective`,
//! `AtomicItem`, `FunctorAttr`. Only the first three in the Functor
//! family exist as [`ItemData`] variants in the AST today
//! (`ItemData::Functor`, `ItemData::FunctorParam`,
//! `ItemData::FunctorDecl`). The remaining five are either side-tables
//! keyed on an owning item, standalone directive tables with no
//! `NodeId`, block-scope markers lowered at the statement level, or
//! have no AST landing at all. They are addressed by follow-up slices:
//!
//! - **PackedStruct.** Not a dedicated item variant.
//!   [`paideia_as_ast::StructAttr::Packed { align }`] is attached to a
//!   `Struct` item through
//!   [`paideia_as_ast::StructAttrTable`]. The `@packed_struct(...)`
//!   prefix lives inside the enclosing `Struct` item's span, so
//!   [`fmt_struct`]'s existing span-slice preserves it byte-identically.
//!   Structural decomposition lands with FMT-6 (type formatter) + a
//!   follow-up slice on [`fmt_struct`].
//!
//! - **EndianAttr.** Not a dedicated item variant.
//!   [`paideia_as_ast::FieldAttr::Endian`] is a per-field attribute
//!   stored in [`paideia_as_ast::StructFieldAttrTable`]. The
//!   `@endian(be|le)` prefix lives inside the enclosing `Struct`
//!   item's span, so [`fmt_struct`]'s span-slice preserves it. Pending
//!   the same follow-up slice as `PackedStruct`.
//!
//! - **MacroExpandDirective.** Not an item node at all —
//!   [`paideia_as_ast::MacroExpandDirectiveTable`] stores a flat
//!   `Vec<MacroExpandDirective>` of plain owned structs, keyed by
//!   neither `NodeId` nor arena index. The parser records these
//!   `@macro_expand([...])` directives independent of the item stream;
//!   their byte range IS inside the source file but outside every
//!   [`ItemData`] span. Routing them belongs to the top-level
//!   [`crate::walker::format_module`], not this per-item dispatcher.
//!   Byte-identity is preserved today only via the walker's whole-
//!   source scaffold; a dedicated interleave slice will structural-ize
//!   the directive stream.
//!
//! - **AtomicItem.**  Not a dedicated variant.
//!   [`paideia_as_ast::ItemAtomicTable`] keys an
//!   [`paideia_as_ast::AtomicOrdering`] on a [`ItemData::Let`]'s
//!   `NodeId`. The `@atomic(Ordering)` trails the Let's value
//!   expression — OUTSIDE [`ItemData::Let`]'s structural span (same
//!   way `@align` / `@no_frame` are outside the span, which [`fmt_let`]
//!   already handles). A follow-up slice extends [`fmt_let`] to read
//!   the side-table and emit the `@atomic(...)` suffix.
//!
//! - **FunctorAttr.** Not a dedicated variant.
//!   [`paideia_as_ast::FunctorAttrTable`] keys a `Vec<FunctorAttr>` on
//!   a [`ItemData::FunctorDecl`]'s `NodeId`. The `@retain` / `@immediate`
//!   prefix is consumed BEFORE `parse_functor` begins, so the attribute
//!   bytes live outside the `FunctorDecl` span. A follow-up slice
//!   extends [`fmt_functor_decl`] to query the side-table and prepend
//!   the attribute(s).
//!
//! - **GpuContext.** Not an item variant. Lives as
//!   [`paideia_as_ast::BlockScope::GpuContext`] on an `ExprBlock`
//!   statement body. FMT-5b handles it when it decomposes expression
//!   blocks.
//!
//! - **Timeline.** `@timeline_wait` / `@timeline_signal` have no parser,
//!   no `NodeKind`, no `ItemData` variant, and no side-table today.
//!   Pending a parser-side primitive before any formatter work.
//!
//! All eight paths return a [`Doc`] built from the FMT-2 layout
//! combinators (`doc::group`, `doc::nest`, `doc::line`, `doc::if_break`,
//! `doc::text`). The `Let` path demonstrates full structural Doc
//! composition; the others wrap `fallback_slice` on the item's span in
//! `doc::group` as the FMT-4a baseline — the item spans produced by
//! `paideia-as-parser` already cover the full declaration from the
//! leading keyword to the closing brace, so byte-identity is preserved
//! by construction. FMT-4b and the follow-up tickets (FMT-5/6/7 for
//! expressions, types, patterns) will decompose each of the span-slice
//! paths into fully-structural reconstructions as the per-kind
//! formatters they depend on land.
//!
//! # Why fewer than 9 structural paths today
//!
//! The seven span-slice paths all depend on sub-formatters that have
//! not shipped yet: structural `Struct` needs the type formatter
//! (FMT-6) to re-emit field types; structural `Enum` needs it for the
//! variant payloads; `Trait`/`Impl` need expression (FMT-5) and type
//! formatters for method signatures and default bodies; `Module` and
//! `Structure` need to recursively dispatch into every nested item
//! kind, including the FMT-4b items (`Functor`, `Effect`, `Capability`,
//! `UnsafeBlock`, `MacroDecl`) not covered here. Doing a half-structural
//! reconstruction today would either lose attributes that live outside
//! the span (`#[derive(...)]` on `Struct`/`Enum`) or embed sub-formatter
//! placeholders. The pragmatic FMT-4a bucket A contract is: give each
//! variant its own `fn fmt_<kind>` dispatch slot, prove the plumbing
//! with the one well-shaped path (`Let`), and keep the rest on
//! byte-identical fallback so FMT-4b can hollow each out in place
//! without regressing the pipeline.
//!
//! # `pub` visibility note
//!
//! Only `ItemData::Let` tracks the `pub` prefix in the AST (`public:
//! bool`). The parser (`parse_item.rs:68-88`) rejects `pub` on every
//! other item kind at the dispatcher level — `pub struct`, `pub enum`,
//! `pub trait`, `pub impl` are parse-time errors (P0110). So the
//! absence of a `pub` prefix on the span-slice paths is correct by
//! construction: the item spans for `Struct`/`Enum`/`Trait`/`Impl`
//! never had a `pub ` to lose. For `Let`, the span starts at the `let`
//! keyword (see `parse_item/let_item/mod.rs:227`), so this module
//! re-emits the `pub ` prefix from the `public` flag.
//!
//! # Trailing-attribute note (`Let`)
//!
//! `ItemData::Let` carries optional symbol-level attributes
//! (`@align(N)`, `@ring(...)`, `@link_section("...")`, `@abi("...")`,
//! `@no_frame`, `@interrupt(...)`) as named fields, but the item's
//! `span` ends at the value expression and does NOT cover the trailing
//! attribute tail. [`fmt_let`] re-emits each attribute from its field
//! value when the structural path is taken (no generics present —
//! generics require the type/trait-bound formatters that FMT-6 has
//! not shipped). For a `Let` with generic parameters, we fall back to
//! the whole-item span (loses trailing attributes if present; FMT-4b
//! will structural-ize both branches once FMT-6 lands).

use paideia_as_ast::{ItemData, NodeData, NodeId, NodeKind};

use crate::doc::{self, Doc};
use crate::format_attribute;
use crate::walker::{FormatContext, fallback_slice};

/// FMT-4a Bucket A dispatcher — route an item node to its per-kind
/// formatter.
///
/// Falls back to [`fallback_slice`] on `data.span` for item kinds not
/// yet covered by Bucket A (`Functor`, `FunctorParam`, `FunctorDecl`,
/// `GenericParam`, `Effect`, `OpSig`, `Capability`, `UnsafeBlock`,
/// `MacroDecl`, `MacroPattern`, `MacroTemplate`) — those land in FMT-4b.
pub fn format_item(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => *d,
        None => return doc::nil(),
    };

    match data.kind {
        NodeKind::Let => fmt_let(ctx, nid, data),
        NodeKind::Module => fmt_module(ctx, nid, data),
        NodeKind::Structure => fmt_structure(ctx, nid, data),
        NodeKind::Signature => fmt_signature(ctx, nid, data),
        NodeKind::Enum => fmt_enum(ctx, nid, data),
        NodeKind::Struct => fmt_struct(ctx, nid, data),
        NodeKind::Trait => fmt_trait(ctx, nid, data),
        NodeKind::Impl => fmt_impl(ctx, nid, data),
        // FMT-4b Bucket B: Functor-family variants.
        NodeKind::Functor => fmt_functor(ctx, nid, data),
        NodeKind::FunctorParam => fmt_functor_param(ctx, nid, data),
        NodeKind::FunctorDecl => fmt_functor_decl(ctx, nid, data),
        // Remaining item kinds — Effect / OpSig / Capability /
        // UnsafeBlock / MacroDecl / MacroPattern / MacroTemplate —
        // retain the byte-identical fallback until their dedicated
        // slices land.
        _ => fallback_slice(ctx, data.span),
    }
}

/// Format a `Let` item structurally.
///
/// Structural path covers the common shape:
/// `[pub ]let [mut ]<name>[ : <type>] = <value>[ <symbol-attrs>]`.
///
/// The whole-item fallback is taken when the AST holds generic
/// parameters (requires FMT-6's type formatter for bound rendering) —
/// callers that need per-field reconstruction on a generic `let` wait
/// for FMT-4b.
///
/// # Attributes
///
/// Order of re-emission matches the parser's accept order (see
/// `parse_item/let_item/mod.rs::parse_optional_symbol_attributes`):
/// `@align → @ring → @link_section → @abi → @no_frame → @interrupt`.
/// A canonical source file that follows this order byte-identically
/// round-trips; a file that uses a different order is normalised to
/// the canonical one.
fn fmt_let(ctx: &FormatContext<'_>, nid: NodeId, data: NodeData) -> Doc {
    let item = match ctx.arena.item_data(nid) {
        Some(i) => i,
        None => return fallback_slice(ctx, data.span),
    };
    let (
        public,
        mutable,
        name,
        generic_params,
        ty,
        value,
        align,
        ring,
        link_section,
        abi,
        no_frame,
        interrupt,
    ) = match item {
        ItemData::Let {
            public,
            mutable,
            name,
            generic_params,
            ty,
            value,
            align,
            ring,
            link_section,
            abi,
            no_frame,
            interrupt,
            ..
        } => (
            *public,
            *mutable,
            *name,
            generic_params,
            *ty,
            *value,
            *align,
            *ring,
            link_section,
            *abi,
            *no_frame,
            interrupt,
        ),
        _ => return fallback_slice(ctx, data.span),
    };

    // Generics branch: hand off until FMT-6 (needs type formatter for
    // bound rendering). The whole-item span is still byte-identical for
    // generic Lets that carry no trailing attributes.
    if !generic_params.is_empty() {
        return fallback_slice(ctx, data.span);
    }

    let mut parts: Vec<Doc> = Vec::with_capacity(10);
    if public {
        parts.push(doc::text("pub "));
    }
    parts.push(doc::text("let "));
    if mutable {
        parts.push(doc::text("mut "));
    }
    parts.push(child_slice(ctx, name));
    if let Some(ty_id) = ty {
        parts.push(doc::text(" : "));
        parts.push(child_slice(ctx, ty_id));
    }
    parts.push(doc::text(" = "));
    parts.push(child_slice(ctx, value));

    // Trailing symbol attributes (not inside data.span).
    //
    // FMT-8 (paideia-as#1572) factors each `@...` into a shared emit
    // helper in [`crate::format_attribute`] so re-emission is one place.
    // The space prefix is this layout's choice (trailing attributes
    // share the same line as the Let header); the helpers themselves
    // emit bare `@name(args)` with no surrounding whitespace.
    if let Some(n) = align {
        parts.push(doc::text(" "));
        parts.push(format_attribute::emit_align(n));
    }
    if let Some((slots, slot_size)) = ring {
        parts.push(doc::text(" "));
        parts.push(format_attribute::emit_ring(slots, slot_size));
    }
    if let Some(section_name) = link_section {
        parts.push(doc::text(" "));
        parts.push(format_attribute::emit_link_section(section_name));
    }
    if let Some(cc) = abi {
        parts.push(doc::text(" "));
        parts.push(format_attribute::emit_abi(cc));
    }
    if no_frame {
        parts.push(doc::text(" "));
        parts.push(format_attribute::emit_no_frame());
    }
    if let Some(attr) = interrupt {
        parts.push(doc::text(" "));
        parts.push(format_attribute::emit_interrupt(attr));
    }

    doc::group(doc::concat(parts))
}

/// Format a `Module` item structurally: `module <name> = <body>`.
///
/// `<body>` is routed back through [`format_item`] so a module whose
/// body is a `Structure` (the common case) gets its items formatted
/// by [`fmt_structure`] rather than slurped as one text blob. The
/// item span DOES cover the full module decl (see
/// `parse_item/module_decl.rs:35-39`), so [`fallback_slice`] is a
/// correct safety net if `body` resolution fails.
fn fmt_module(ctx: &FormatContext<'_>, nid: NodeId, data: NodeData) -> Doc {
    let item = match ctx.arena.item_data(nid) {
        Some(i) => i,
        None => return fallback_slice(ctx, data.span),
    };
    let (name, sig, body) = match item {
        ItemData::Module {
            name, sig, body, ..
        } => (*name, *sig, *body),
        _ => return fallback_slice(ctx, data.span),
    };

    // Signature-ascribed module: `module Name : Sig = Body`. FMT-4b
    // will decompose this; for Bucket A the whole-item span suffices.
    if sig.is_some() {
        return fallback_slice(ctx, data.span);
    }

    doc::group(doc::concat(vec![
        doc::text("module "),
        child_slice(ctx, name),
        doc::text(" = "),
        format_item(ctx, body),
    ]))
}

/// Format a `Structure` item.
///
/// Two shapes exist on an AST produced by `parse_source_file`:
///
/// * A regular `structure { items }` (span covers the whole decl —
///   see `parse_item/module_decl.rs:131-136`). Bucket A emits this
///   via [`fallback_slice`] on the whole span, which is byte-
///   identical for canonical input. FMT-4b will replace the inner
///   `{ items }` with structural per-item recursion.
///
/// * The synthetic root structure allocated by `parse_source_file`
///   (span is the EOF token's position, zero-length). The walker's
///   top-level [`format_module`](crate::walker::format_module) does
///   not route the root through `format_item`, so this function
///   never sees the synthetic root in practice. A zero-length span
///   slice renders as the empty string, which is the safe fallback
///   if we ever did.
fn fmt_structure(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// Format a `Signature` item.
///
/// The item span covers `signature Name = <structure-body>` end-to-end
/// (see `parse_item/module_decl.rs:229-233`). Byte-identical fallback.
/// FMT-4b will emit `signature <name> = <body>` with recursion into
/// the body.
fn fmt_signature(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// Format an `Enum` item.
///
/// Item span covers `enum Name[<Gens>] { variants }`. `#[derive(...)]`
/// outer attributes land BEFORE the span and are lost on the Bucket A
/// fallback — the regression tests here avoid attributed enums. FMT-4b
/// will structural-decompose variants (unit / tuple / record payloads
/// via FMT-6) and emit outer attributes on their own lines.
fn fmt_enum(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// Format a `Struct` item.
///
/// Item span covers `struct Name[<Gens>] { fields }`. `#[derive(...)]`
/// outer attributes land BEFORE the span and are lost on the Bucket A
/// fallback — avoid attributed structs in Bucket A inputs. FMT-4b
/// decomposes fields through FMT-6's type formatter.
fn fmt_struct(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// Format a `Trait` item.
///
/// Item span covers `trait Name[<Gens>] { assoc-types; methods }`.
/// FMT-4b will emit method signatures through FMT-5/6 and associated
/// types through FMT-6.
fn fmt_trait(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// Format an `Impl` item.
///
/// Item span covers `impl[<Gens>] [Trait for] Type { methods }`.
/// FMT-4b decomposes the header through FMT-6 (trait path, generic
/// args, `for Type`) and the body through FMT-4a Bucket A (methods
/// are `Let`s with lambda RHS, so [`fmt_let`] already handles their
/// per-item structure once the body is iterated).
fn fmt_impl(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// FMT-4b Bucket B: format a module-parameterised `Functor` item.
///
/// Shape:
///
/// ```text
/// functor (<p1-name> : <p1-sig>)(<p2-name> : <p2-sig>) -> <body>
/// ```
///
/// The body is routed back through [`format_item`] so a `structure {
/// ... }` body picks up [`fmt_structure`] rather than being slurped as
/// one opaque blob. Params are structural because `FunctorParam`'s own
/// span covers only the name token (see
/// `parse_item/module_decl.rs::parse_functor`), so a `fallback_slice`
/// on the param node would drop the surrounding `( ... : ... )`.
///
/// Multi-line wrap: when the one-line form overflows `max_width`, each
/// param lands on its own line at the continuation indent, followed by
/// the `-> <body>` on its own line. The style spec §5.5 says a functor
/// body follows the same rules as `structure {}` — body wrap is the
/// body formatter's own group's responsibility.
///
/// Canonical param-colon spacing: `<name> : <sig>` with spaces around
/// `:` (style spec §4.3). An input written as `(S: Sig)` is normalised
/// to `(S : Sig)` — the one deliberate byte-identity departure here.
fn fmt_functor(ctx: &FormatContext<'_>, nid: NodeId, data: NodeData) -> Doc {
    let item = match ctx.arena.item_data(nid) {
        Some(i) => i,
        None => return fallback_slice(ctx, data.span),
    };
    let (params, body) = match item {
        ItemData::Functor { params, body, .. } => (params.clone(), *body),
        _ => return fallback_slice(ctx, data.span),
    };

    if params.is_empty() {
        // A functor with no params is not legal source
        // (`parse_functor` loops until it sees no `(`), but defend
        // against malformed arenas: fall back to the span-slice so we
        // emit *something* rather than a bare `functor -> body`.
        return doc::group(fallback_slice(ctx, data.span));
    }

    // Each param renders to its own `(<name> : <sig>)` doc. We never
    // intersperse commas — the surface syntax juxtaposes param-tuples
    // directly, so adjacent params are back-to-back in flat mode and
    // one-per-line in break mode.
    let param_docs: Vec<Doc> = params
        .iter()
        .map(|p| format_item(ctx, *p))
        .collect();

    let params_doc = doc::separate(doc::softline(), param_docs);

    doc::group(doc::nest(
        2,
        doc::concat(vec![
            doc::text("functor "),
            params_doc,
            doc::line(),
            doc::text("-> "),
            format_item(ctx, body),
        ]),
    ))
}

/// FMT-4b Bucket B: format a `FunctorParam` item — `(<name> : <sig>)`.
///
/// The `FunctorParam` AST carries both child `NodeId`s with their own
/// token spans (see `parse_item/module_decl.rs::parse_functor`), so
/// [`child_slice`] re-emits the names losslessly.
///
/// Not published to the dispatcher's expression / type callers — this
/// function is reachable only via a parent [`fmt_functor`]. If a caller
/// hands us a bare `FunctorParam` id, the dispatcher routes here just
/// the same and we emit the same `( <name> : <sig> )` text.
fn fmt_functor_param(ctx: &FormatContext<'_>, nid: NodeId, data: NodeData) -> Doc {
    let item = match ctx.arena.item_data(nid) {
        Some(i) => i,
        None => return fallback_slice(ctx, data.span),
    };
    let (name, sig) = match item {
        ItemData::FunctorParam { name, sig } => (*name, *sig),
        _ => return fallback_slice(ctx, data.span),
    };
    doc::group(doc::concat(vec![
        doc::text("("),
        child_slice(ctx, name),
        doc::text(" : "),
        child_slice(ctx, sig),
        doc::text(")"),
    ]))
}

/// FMT-4b Bucket B: format a session-typed `FunctorDecl` item.
///
/// Shape (as recognised by `parse_functor_with_attrs_into_arena`):
///
/// ```text
/// functor <Name>(<Param> : <ParamSig>) -> <ReturnSig> (with <S> : session)? { ... }
/// ```
///
/// **Why span-slice, not structural.** The parser
/// (`toolkit_attrs::parse_functor_with_attrs_into_arena`) anchors every
/// synthesised `Ident` child (name / param_name / param_sig /
/// return_sig) on the whole-decl span, because the standalone
/// `FunctorDecl` struct predates arena awareness and does not carry
/// per-field byte offsets (toolkit_attrs.rs:233-242). A structural
/// reconstruction via [`child_slice`] would therefore re-emit the full
/// decl text per child, which is incorrect. Span-slice on the decl
/// span IS byte-identical today — the parser's own span covers from
/// `functor` to the body's closing `}`.
///
/// **Attribute prefix.** `@retain` / `@immediate` are stored on
/// [`paideia_as_ast::FunctorAttrTable`] keyed by this decl's `NodeId`.
/// The attribute bytes live BEFORE the span's start (the attribute
/// prefix is consumed by `parse_functor_with_attrs` before
/// `parse_functor` begins), so a follow-up slice must query
/// [`FormatContext::arena`]'s side-table and prepend the prefix. For
/// this slice the attributes are not emitted — any caller that relies
/// on the attribute prefix should use the pre-existing byte-identical
/// walker fallback through [`crate::walker::format_module`].
fn fmt_functor_decl(ctx: &FormatContext<'_>, _nid: NodeId, data: NodeData) -> Doc {
    doc::group(fallback_slice(ctx, data.span))
}

/// Fallback slice of a child node (identifier, type, expression,
/// body) by its own span.
///
/// Used by [`fmt_let`] and [`fmt_module`] to re-emit a sub-node's
/// source text while the per-kind formatter for that category
/// (FMT-5/6/7) has not yet shipped. Resolving an invalid child id
/// returns [`doc::nil`] rather than panic — a malformed AST should
/// not kill the formatter.
fn child_slice(ctx: &FormatContext<'_>, id: NodeId) -> Doc {
    let child = match ctx.arena.get(id) {
        Some(c) => c,
        None => return doc::nil(),
    };
    fallback_slice(ctx, child.span)
}

#[cfg(test)]
mod tests {
    //! FMT-4a Bucket A unit tests.
    //!
    //! Pipeline:
    //! 1. Build a `SourceText`, lex, parse via
    //!    [`paideia_as_parser::Parser::parse_source_file`].
    //! 2. Pull the first top-level item id from the synthetic root
    //!    `Structure`'s `items` vector.
    //! 3. Call [`format_item`] on that id with a fresh
    //!    [`FormatContext`], render through the FMT-2 layout engine
    //!    at the default `max_width = 100`.
    //! 4. Assert on byte-identity (regression target) or on a known
    //!    canonical rendering.

    use super::*;
    use crate::FormatOptions;
    use crate::attach::{TriviaAttachmentMap, attach_trivia};
    use crate::cst::tokenize_with_trivia;
    use crate::layout::{self, FormatOptions as LayoutOptions};
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, VecSink};
    use paideia_as_lexer::SourceText;
    use paideia_as_parser::Parser;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    /// Parse the source, grab the first top-level item id (from the
    /// synthetic root `Structure`), and render it through
    /// [`format_item`]. Returns `None` on parse failure — tests then
    /// pattern-match to assert the failure mode they expect.
    fn render_first_item(src: &str) -> Option<String> {
        let source = SourceText::from_bytes(file_id(), src.as_bytes()).ok()?;
        let mut sink = VecSink::new();
        let mut lexer = paideia_as_lexer::Lexer::new(file_id(), &source);
        let tokens = lexer.collect_tokens(&mut sink);
        let mut arena = AstArena::new();
        let mut parse_sink = VecSink::new();
        let root_id = {
            let mut p = Parser::new(
                &tokens,
                source.content(),
                file_id(),
                &mut arena,
                &mut parse_sink,
            );
            p.parse_source_file().ok()?
        };
        // The synthetic root is a Structure; its items[0] is the first
        // real top-level item.
        let first_id = match arena.item_data(root_id)? {
            ItemData::Structure { items, .. } if !items.is_empty() => items[0],
            _ => return None,
        };
        let stream = tokenize_with_trivia(file_id(), &source, &mut VecSink::new());
        let tmap = attach_trivia(&stream, &arena, root_id);
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let d = format_item(&ctx, first_id);
        Some(layout::render(&d, &LayoutOptions::default()))
    }

    // --- Let ----------------------------------------------------------

    #[test]
    fn fmt_let_simple_pub() {
        // Canonical: pub let x : u64 = 1
        let src = "pub let x : u64 = 1";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_let_without_type() {
        let src = "let x = 1";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_let_mutable() {
        let src = "let mut counter = 0";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_let_pub_mut_with_type() {
        let src = "pub let mut y : u64 = 42";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_let_structural_reconstruction_is_byte_identical() {
        // Byte-identity: fmt_let takes the structural branch (no
        // generics, no attrs) and rebuilds from AST fields. The
        // round-trip must match the source exactly.
        let src = "pub let answer : u64 = 42";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Module -------------------------------------------------------

    #[test]
    fn fmt_module_wraps_body() {
        // The module body is a Structure parsed with its own span.
        // fmt_module emits `module <name> = ` + fmt_structure(body).
        // fmt_structure today span-slices, so the full decl round-
        // trips byte-identically.
        let src = "module M = structure { let x = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_module_multi_item_body() {
        let src = "module Multi = structure { let x = 1 let y = 2 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Structure ----------------------------------------------------

    #[test]
    fn fmt_structure_appears_as_module_body() {
        // Structures appear as module bodies, so we exercise them via
        // the module path. Byte-identity is the FMT-4a contract.
        let src = "module S = structure { let a = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Signature ----------------------------------------------------

    #[test]
    fn fmt_signature_byte_identity() {
        // `signature Name = structure { ... }` — see
        // parse_item/module_decl.rs::parse_signature_decl.
        let src = "signature Sig = structure { let x = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Enum ---------------------------------------------------------

    #[test]
    fn fmt_enum_unit_variants_byte_identity() {
        let src = "enum Color { Red, Green, Blue }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_enum_tuple_variant_byte_identity() {
        let src = "enum Shape { Point, Line(u64) }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_enum_multiline_source_preserved() {
        let src = "enum Color {\n  Red,\n  Green,\n  Blue,\n}";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Struct -------------------------------------------------------

    #[test]
    fn fmt_struct_two_fields_byte_identity() {
        let src = "struct Point { x: u64, y: u64 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_struct_multiline_source_preserved() {
        let src = "struct Point {\n  x: u64,\n  y: u64,\n}";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Trait --------------------------------------------------------

    #[test]
    fn fmt_trait_minimal_byte_identity() {
        let src = "trait WithDefault {\n  fn get_default() -> u64 { 42u64 }\n}";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Impl ---------------------------------------------------------

    #[test]
    fn fmt_impl_minimal_byte_identity() {
        let src = "impl WithDefault for Counter {\n  let get_default = fn(x: ()) -> u64 = 0u64\n}";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- Dispatcher plumbing -----------------------------------------

    #[test]
    fn dispatcher_on_invalid_node_id_returns_nil() {
        // A NodeId past the arena length routes through the None guard
        // in format_item and yields the empty Doc.
        let source = SourceText::from_bytes(file_id(), b" ").unwrap();
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut sink);
        let arena = AstArena::new();
        let tmap = TriviaAttachmentMap::new();
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let bogus = NodeId::new(999).unwrap();
        let d = format_item(&ctx, bogus);
        let out = layout::render(&d, &LayoutOptions::default());
        assert_eq!(out, "");
    }

    #[test]
    fn fmt_let_without_type_is_idempotent_through_double_format() {
        // The structural Let path is a fixed point: feeding format_item
        // its own output back through the pipeline yields the same
        // string. Protects against subtle whitespace drift inside
        // doc::group / doc::concat boundaries.
        let src = "let x = 1";
        let once = render_first_item(src).unwrap();
        let twice = render_first_item(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn fmt_module_body_recursion_preserves_nested_let() {
        // module -> structure body -> let items. Verifies the
        // recursive dispatch through format_item does not corrupt the
        // inner let's text.
        let src = "module Nested = structure { pub let n : u64 = 7 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // =====================================================================
    //  FMT-4b Bucket B (paideia-as#1567) — Functor-family + absent-variant
    //  byte-identity coverage.
    // =====================================================================

    // --- Functor (ItemData::Functor) --------------------------------------

    #[test]
    fn fmt_functor_single_param_byte_identity() {
        // Canonical form: `(T : Sig)` with space-around colon
        // (style spec §4.3). `module M = ` prefix means the Functor
        // item is reached through fmt_module → format_item(body).
        let src = "module M = functor (T : Sig) -> structure { let x = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_functor_two_params_byte_identity() {
        // Juxtaposed param-tuples with no separator: `(A : S1)(B : S2)`.
        // In flat mode the softline between them collapses to empty,
        // matching source byte-for-byte.
        let src = "module M = functor (A : S1)(B : S2) -> structure { let x = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_functor_three_params_byte_identity() {
        let src =
            "module M = functor (A : S1)(B : S2)(C : S3) -> structure { let x = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    #[test]
    fn fmt_functor_normalises_dense_colon_in_param() {
        // Source uses the dense `(T: Sig)` colon spacing (the
        // minority corpus form, style spec §4.3). The structural
        // formatter normalises to the canonical space-around form.
        let dense = "module M = functor (T: Sig) -> structure { let x = 1 }";
        let canonical = "module M = functor (T : Sig) -> structure { let x = 1 }";
        assert_eq!(render_first_item(dense).as_deref(), Some(canonical));
    }

    #[test]
    fn fmt_functor_many_params_wraps_multiline() {
        // Build an input whose one-line functor header exceeds
        // max_width = 100 so the enclosing group breaks. The exact
        // broken shape depends on the layout engine (nest = 2, soft
        // breaks between params, line before `-> `), so we only
        // assert that:
        //   (a) the formatter does not panic,
        //   (b) the output contains a newline (break mode engaged),
        //   (c) each param still carries the canonical colon spacing.
        let src = "module M = functor \
                   (AAAAAAAAAA : Sig1111111)(BBBBBBBBBB : Sig2222222)(CCCCCCCCCC : Sig3333333)(DDDDDDDDDD : Sig4444444) \
                   -> structure { let x = 1 }";
        let out = render_first_item(src).expect("parse OK");
        assert!(out.contains('\n'), "expected multi-line wrap, got: {out}");
        for name in ["AAAAAAAAAA", "BBBBBBBBBB", "CCCCCCCCCC", "DDDDDDDDDD"] {
            assert!(out.contains(name), "lost param name {name}: {out}");
        }
        // Every param kept its canonical colon spacing.
        assert!(out.contains(" : Sig1111111"));
        assert!(out.contains(" : Sig2222222"));
        assert!(out.contains(" : Sig3333333"));
        assert!(out.contains(" : Sig4444444"));
    }

    #[test]
    fn fmt_functor_single_param_idempotent_double_format() {
        // Round-trip: once we normalise, re-formatting the output is
        // a fixed point. Guards against subtle whitespace drift
        // between `doc::group`, `doc::separate`, and `doc::nest`.
        let src = "module M = functor (T : Sig) -> structure { let x = 1 }";
        let once = render_first_item(src).unwrap();
        let twice = render_first_item(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn fmt_functor_body_body_recursion_preserves_let() {
        // The functor's `structure { ... }` body routes through
        // fmt_structure (span-slice today). Verify a nested let
        // survives untouched.
        let src =
            "module M = functor (T : Sig) -> structure { pub let n : u64 = 7 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- FunctorParam (reached via Functor) -------------------------------

    #[test]
    fn fmt_functor_param_structural_emits_parens() {
        // FunctorParam's own span is just the name token (see
        // parse_item/module_decl.rs::parse_functor), so a fallback
        // slice would drop `( : )`. The structural path puts them back.
        let src = "module M = functor (Target : Signature) -> structure { let a = 1 }";
        let out = render_first_item(src).expect("parse OK");
        assert!(out.contains("(Target : Signature)"), "got: {out}");
    }

    #[test]
    fn fmt_functor_param_handles_short_names() {
        // Smallest legal name/sig identifiers; the formatter must not
        // introduce incidental whitespace or lose characters.
        let src = "module M = functor (A : B) -> structure { let x = 1 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- MacroExpandDirective (absent as ItemData — Placeholder fallback) -

    #[test]
    fn fmt_macro_expand_directive_byte_identity_via_fallback() {
        // paideia-as#1556: top-level `@macro_expand([<macro>, "<input>"])`
        // is pushed onto MacroExpandDirectiveTable AND leaves a
        // `NodeKind::Placeholder` entry in the root Structure's items[].
        // The placeholder's span covers the directive, so the dispatcher's
        // `_ => fallback_slice` arm emits the directive verbatim —
        // byte-identical through the walker's `_` fallback.
        let src = "@macro_expand([foo, \"42\"])";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }

    // --- FunctorDecl fallback-slice (not reachable via parse_source_file;
    //  verifies the dispatcher is wired without exercising a specific shape).
    //  Reaching FunctorDecl requires parse_functor_with_attrs_into_arena,
    //  which is tested in the parser crate.

    #[test]
    fn fmt_dispatcher_routes_functor_nodekind() {
        // Smoke-test the dispatcher-level routing for `NodeKind::Functor`:
        // feed a module whose body is a functor and verify the output
        // contains `functor ` (the dispatcher chose fmt_functor and
        // emitted the keyword structurally; a `_` fallback would also
        // contain the keyword, but it would not reconstruct the colon
        // spacing — so pair the keyword check with a canonical-colon
        // assertion for `fmt_functor_param`).
        let src = "module M = functor (T: Sig) -> structure { let x = 1 }";
        let out = render_first_item(src).expect("parse OK");
        assert!(out.contains("functor "), "missing `functor ` in: {out}");
        assert!(out.contains("(T : Sig)"), "colon not normalised: {out}");
    }

    // --- Absent variants: byte-identity through enclosing fallbacks -------

    #[test]
    fn fmt_struct_with_endian_field_byte_identity() {
        // `@endian(be)` is a field-level attribute stored on
        // StructFieldAttrTable. The attribute bytes live INSIDE the
        // `Struct` item's span, so fmt_struct's existing fallback
        // preserves them. Guards against regression if a later slice
        // decomposes struct bodies but forgets to re-emit field attrs.
        let src = "struct Header { @endian(be) magic: u32, length: u16 }";
        assert_eq!(render_first_item(src).as_deref(), Some(src));
    }
}
