//! FMT-4a Bucket A: per-item formatters.
//!
//! # Scope (paideia-as#1566)
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

use paideia_as_ast::{CallingConvention, ItemData, NodeData, NodeId, NodeKind};

use crate::doc::{self, Doc};
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
        // FMT-4b: Functor / FunctorParam / FunctorDecl / Effect / OpSig
        // / Capability / UnsafeBlock / MacroDecl / MacroPattern /
        // MacroTemplate. The scaffold's byte-identical fallback is
        // retained until those arms land.
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
    if let Some(n) = align {
        parts.push(doc::text(format!(" @align({})", n)));
    }
    if let Some((slots, slot_size)) = ring {
        parts.push(doc::text(format!(
            " @ring(slots={}, slot_size={})",
            slots, slot_size
        )));
    }
    if let Some(section_name) = link_section {
        parts.push(doc::text(format!(" @link_section(\"{}\")", section_name)));
    }
    if let Some(cc) = abi {
        let s = match cc {
            CallingConvention::Ms => "ms",
            CallingConvention::Sysv => "sysv",
        };
        parts.push(doc::text(format!(" @abi(\"{}\")", s)));
    }
    if no_frame {
        parts.push(doc::text(" @no_frame"));
    }
    if let Some(attr) = interrupt {
        let kw = if attr.has_error_code {
            "interrupt_error"
        } else {
            "interrupt"
        };
        parts.push(doc::text(format!(" @{}(\"{}\")", kw, attr.name)));
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
}
