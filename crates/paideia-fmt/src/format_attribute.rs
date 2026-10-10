//! FMT-8: attribute formatters (paideia-as#1572).
//!
//! # Scope
//!
//! Renders every `@...` attribute shape that `paideia-as-ast` currently
//! represents, as a reusable per-attribute helper surface. The main
//! callers are:
//!
//! * `format_item::fmt_let` — six trailing symbol attributes on a Let
//!   (`@align`, `@ring`, `@link_section`, `@abi`, `@no_frame`,
//!   `@interrupt` / `@interrupt_error`). Follow-up slices add
//!   `@atomic(...)` (`ItemAtomicTable`) and the per-field `@endian`
//!   when `fmt_struct` structural-izes.
//! * `format_expr::format_match` — the leading `@jump_table` between the
//!   scrutinee and the body `{`.
//! * `format_item::fmt_struct` (future) — struct-level `@packed_struct`
//!   from `StructAttrTable`, and per-field `@endian` from
//!   `StructFieldAttrTable`.
//! * `format_item::fmt_functor` / `fmt_functor_decl` (future) —
//!   `@retain` / `@immediate` from `FunctorAttrTable`.
//!
//! # No dedicated `NodeKind`
//!
//! None of the attributes above have their own `NodeKind` variant.
//! They live either as:
//!
//! * inline fields of the owning item (`ItemData::Let`'s `align`,
//!   `ring`, `link_section`, `abi`, `no_frame`, `interrupt`), or
//! * side-tables keyed on the owning item's `NodeId`
//!   (`StructAttrTable`, `StructFieldAttrTable`, `FunctorAttrTable`,
//!   `ItemAtomicTable`), or
//! * inline fields of their parent expression (`MatchAttrs.jump_table`
//!   on `ExprData::Match`).
//!
//! That means the walker's `format_attribute(ctx, nid)` entry point
//! can't dispatch by node kind — there is no attribute node. It is
//! kept as a safe fallback that slices the node's own span (so a
//! defensive caller passing an unexpected id still gets *something*
//! printed), and the real work happens through the public per-shape
//! emit helpers below.
//!
//! # Spec §7 — one attribute per line in a stack
//!
//! When several attributes stack on one item (e.g. a struct with both
//! `@packed_struct` and (future) `@repr(...)`), callers compose with
//! [`format_attribute_stack`] which inserts a hardline between each
//! attribute Doc. Inline trailing attributes on a Let share a line with
//! the Let header and are concatenated with a leading space by the
//! caller (`fmt_let`) — that's its explicit layout choice, not a
//! property of the emit helpers.
//!
//! # Return convention
//!
//! Every `emit_*` helper returns a `Doc` whose rendered form starts
//! with `@` and ends with the attribute's last token (either the closing
//! `)` for parenthesised shapes or the attribute identifier for bare
//! shapes). The callers are responsible for whatever leading /
//! trailing whitespace / newline the context demands.

use paideia_as_ast::{
    AtomicOrdering, CallingConvention, Endianness, FunctorAttr, InterruptAttr, NodeId,
};

use crate::doc::{self, Doc};
use crate::walker::{FormatContext, fallback_slice};

/// Attribute dispatch tag — identifies which attribute shape the
/// caller wants rendered.
///
/// Attributes have no dedicated `NodeKind`, so this enum stands in for
/// the role `NodeKind` would otherwise play at the dispatch boundary.
/// Each variant carries exactly the arguments its surface form prints
/// — no span, no owning `NodeId`, because the attribute's bytes do not
/// live in a dedicated AST node to slice from. New attribute primitives
/// append a variant here.
#[derive(Clone, Debug)]
pub enum AttrKind<'a> {
    /// `@endian(be)` / `@endian(le)` — per-field byte-order
    /// (`StructFieldAttrTable::Endian`).
    Endian(Endianness),
    /// `@interrupt("vec")` / `@interrupt_error("vec")` ISR sugar on a
    /// Let (`ItemData::Let::interrupt`).
    Interrupt(&'a InterruptAttr),
    /// `@abi("ms")` / `@abi("sysv")` — calling-convention on a Let
    /// (`ItemData::Let::abi`).
    Abi(CallingConvention),
    /// `@link_section("name")` — ELF-section override on a Let
    /// (`ItemData::Let::link_section`).
    LinkSection(&'a str),
    /// `@jump_table` — bare marker on a Match expression
    /// (`MatchAttrs::jump_table`).
    JumpTable,
    /// `@packed_struct` / `@packed_struct(align=N)` — struct-level
    /// layout attribute (`StructAttrTable::Packed`).
    PackedStruct {
        /// Forced alignment in bytes (positive power of two), or
        /// `None` for the bare form.
        align: Option<u32>,
    },
    /// `@retain` / `@immediate` — functor-level capability-flow
    /// discipline (`FunctorAttrTable`).
    Functor(FunctorAttr),
    /// `@atomic(Ordering)` — memory-ordering discipline on a Let
    /// (`ItemAtomicTable`).
    Atomic(AtomicOrdering),
    /// `@align(N)` — per-symbol alignment on a Let
    /// (`ItemData::Let::align`).
    Align(u32),
    /// `@ring(slots=M, slot_size=K)` — ring-buffer directive on a Let
    /// (`ItemData::Let::ring`).
    Ring {
        /// Number of slots in the ring.
        slots: u32,
        /// Per-slot size in bytes.
        slot_size: u32,
    },
    /// `@no_frame` — SysV prologue opt-out on a Let
    /// (`ItemData::Let::no_frame`).
    NoFrame,
}

/// FMT-8 dispatcher — route by `kind` to the matching emit helper.
///
/// `nid` is unused today (attributes have no dedicated AST node), but
/// is kept in the signature so a future AST landing that gives an
/// attribute its own node can route span-based diagnostics through the
/// same entry without a signature break.
#[must_use]
pub fn format_attribute(_ctx: &FormatContext<'_>, _nid: NodeId, kind: AttrKind<'_>) -> Doc {
    emit(kind)
}

/// Dispatch by `AttrKind` to the per-shape emit helper. Prefer this
/// over calling the helpers directly when the caller is already
/// working with an `AttrKind`.
#[must_use]
pub fn emit(kind: AttrKind<'_>) -> Doc {
    match kind {
        AttrKind::Endian(e) => emit_endian(e),
        AttrKind::Interrupt(attr) => emit_interrupt(attr),
        AttrKind::Abi(cc) => emit_abi(cc),
        AttrKind::LinkSection(name) => emit_link_section(name),
        AttrKind::JumpTable => emit_jump_table(),
        AttrKind::PackedStruct { align } => emit_packed_struct(align),
        AttrKind::Functor(fa) => emit_functor_attr(fa),
        AttrKind::Atomic(ao) => emit_atomic(ao),
        AttrKind::Align(n) => emit_align(n),
        AttrKind::Ring { slots, slot_size } => emit_ring(slots, slot_size),
        AttrKind::NoFrame => emit_no_frame(),
    }
}

/// `@endian(be)` or `@endian(le)`.
#[must_use]
pub fn emit_endian(e: Endianness) -> Doc {
    let arg = match e {
        Endianness::Be => "be",
        Endianness::Le => "le",
    };
    doc::text(format!("@endian({})", arg))
}

/// `@interrupt("name")` or `@interrupt_error("name")`.
///
/// The `has_error_code` flag selects between the two spellings. The
/// `name` field preserves the original source spelling (either a
/// canonical vector name like `page_fault` or a numeric spelling).
#[must_use]
pub fn emit_interrupt(attr: &InterruptAttr) -> Doc {
    let kw = if attr.has_error_code {
        "interrupt_error"
    } else {
        "interrupt"
    };
    doc::text(format!("@{}(\"{}\")", kw, attr.name))
}

/// `@abi("ms")` or `@abi("sysv")`.
#[must_use]
pub fn emit_abi(cc: CallingConvention) -> Doc {
    let s = match cc {
        CallingConvention::Ms => "ms",
        CallingConvention::Sysv => "sysv",
    };
    doc::text(format!("@abi(\"{}\")", s))
}

/// `@link_section(".name")`. The `name` string is embedded verbatim in
/// a double-quoted literal — the parser canonicalises the inner bytes
/// (no embedded quote / newline), so no escaping is required here.
#[must_use]
pub fn emit_link_section(name: &str) -> Doc {
    doc::text(format!("@link_section(\"{}\")", name))
}

/// Bare `@jump_table` — no arguments.
#[must_use]
pub fn emit_jump_table() -> Doc {
    doc::text("@jump_table")
}

/// `@packed_struct` (when `align = None`) or
/// `@packed_struct(align=N)`. The parser canonical form uses no spaces
/// around `=` inside the parens (`align=4`, not `align = 4`).
#[must_use]
pub fn emit_packed_struct(align: Option<u32>) -> Doc {
    match align {
        None => doc::text("@packed_struct"),
        Some(n) => doc::text(format!("@packed_struct(align={})", n)),
    }
}

/// `@retain` or `@immediate` — functor capability-flow discipline.
#[must_use]
pub fn emit_functor_attr(fa: FunctorAttr) -> Doc {
    match fa {
        FunctorAttr::Retain => doc::text("@retain"),
        FunctorAttr::Immediate => doc::text("@immediate"),
    }
}

/// `@atomic(Ordering)` — the ordering name is rendered PascalCase
/// (`Relaxed`, `Acquire`, `Release`, `SeqCst`) to match the parser's
/// accept vocabulary and the C11 / Rust mental model.
#[must_use]
pub fn emit_atomic(o: AtomicOrdering) -> Doc {
    let s = match o {
        AtomicOrdering::Relaxed => "Relaxed",
        AtomicOrdering::Acquire => "Acquire",
        AtomicOrdering::Release => "Release",
        AtomicOrdering::SeqCst => "SeqCst",
    };
    doc::text(format!("@atomic({})", s))
}

/// `@align(N)`.
#[must_use]
pub fn emit_align(n: u32) -> Doc {
    doc::text(format!("@align({})", n))
}

/// `@ring(slots=M, slot_size=K)`.
///
/// Two kv-arguments; style spec §7 asks that when a multi-arg
/// attribute wraps, each argument lands on its own line and a trailing
/// comma is mandatory. The emit helper groups the body so the layout
/// engine makes the flat-vs-break choice per the enclosing width
/// budget.
#[must_use]
pub fn emit_ring(slots: u32, slot_size: u32) -> Doc {
    // Flat form: `@ring(slots=M, slot_size=K)`.
    // Break form:
    //   @ring(
    //     slots=M,
    //     slot_size=K,
    //   )
    let args = doc::concat(vec![
        doc::text(format!("slots={}", slots)),
        doc::text(","),
        doc::line(),
        doc::text(format!("slot_size={}", slot_size)),
        // Trailing comma only in break mode.
        doc::if_break(doc::text(","), doc::nil()),
    ]);
    doc::group(doc::concat(vec![
        doc::text("@ring("),
        doc::nest(
            2,
            doc::concat(vec![
                doc::if_break(doc::hardline(), doc::nil()),
                args,
            ]),
        ),
        doc::if_break(doc::hardline(), doc::nil()),
        doc::text(")"),
    ]))
}

/// Bare `@no_frame` — no arguments.
#[must_use]
pub fn emit_no_frame() -> Doc {
    doc::text("@no_frame")
}

/// Walker-side fallback for an attribute `NodeId`.
///
/// No `NodeKind` variant describes attributes today, so this is a
/// defensive catch for any caller that routes an unexpected id through
/// `walker::format_attribute`. The fallback is byte-identical for the
/// node's own span.
#[must_use]
pub fn format_attribute_fallback(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return doc::nil(),
    };
    fallback_slice(ctx, data.span)
}

/// Stack several attribute Docs — one per line.
///
/// Spec §7 (paideia-as#1561): when attributes stack on one item, each
/// attribute lands on its own line. The caller provides pre-rendered
/// attribute Docs (typically from the `emit_*` helpers); this function
/// composes them with [`doc::hardline`] separators. Empty input returns
/// [`doc::nil`] so a caller can unconditionally prepend the stack
/// without probing for emptiness.
#[must_use]
pub fn format_attribute_stack(attrs: Vec<Doc>) -> Doc {
    if attrs.is_empty() {
        return doc::nil();
    }
    doc::separate(doc::hardline(), attrs)
}

#[cfg(test)]
mod tests {
    //! FMT-8 unit tests — direct emit-helper byte-identity plus
    //! layout-level wrap behaviour on the one multi-arg attribute.

    use super::*;
    use crate::layout::{self, FormatOptions as LayoutOptions};
    use paideia_as_ast::{AtomicOrdering, CallingConvention, Endianness, FunctorAttr};

    fn render(d: &Doc) -> String {
        layout::render(d, &LayoutOptions::default())
    }

    fn render_at(d: &Doc, max_width: usize) -> String {
        layout::render(d, &LayoutOptions { max_width })
    }

    #[test]
    fn endian_be_byte_identity() {
        let d = emit_endian(Endianness::Be);
        assert_eq!(render(&d), "@endian(be)");
    }

    #[test]
    fn endian_le_byte_identity() {
        let d = emit_endian(Endianness::Le);
        assert_eq!(render(&d), "@endian(le)");
    }

    #[test]
    fn interrupt_plain_byte_identity() {
        let attr = InterruptAttr {
            has_error_code: false,
            vector: 3,
            name: "breakpoint".to_string(),
        };
        let d = emit_interrupt(&attr);
        assert_eq!(render(&d), "@interrupt(\"breakpoint\")");
    }

    #[test]
    fn interrupt_error_byte_identity() {
        let attr = InterruptAttr {
            has_error_code: true,
            vector: 14,
            name: "page_fault".to_string(),
        };
        let d = emit_interrupt(&attr);
        assert_eq!(render(&d), "@interrupt_error(\"page_fault\")");
    }

    #[test]
    fn abi_ms_byte_identity() {
        let d = emit_abi(CallingConvention::Ms);
        assert_eq!(render(&d), "@abi(\"ms\")");
    }

    #[test]
    fn abi_sysv_byte_identity() {
        let d = emit_abi(CallingConvention::Sysv);
        assert_eq!(render(&d), "@abi(\"sysv\")");
    }

    #[test]
    fn link_section_byte_identity() {
        let d = emit_link_section(".boot.text");
        assert_eq!(render(&d), "@link_section(\".boot.text\")");
    }

    #[test]
    fn jump_table_bare_byte_identity() {
        let d = emit_jump_table();
        assert_eq!(render(&d), "@jump_table");
    }

    #[test]
    fn packed_struct_bare_byte_identity() {
        let d = emit_packed_struct(None);
        assert_eq!(render(&d), "@packed_struct");
    }

    #[test]
    fn packed_struct_with_align_byte_identity() {
        let d = emit_packed_struct(Some(4));
        assert_eq!(render(&d), "@packed_struct(align=4)");
    }

    #[test]
    fn functor_attr_retain_byte_identity() {
        let d = emit_functor_attr(FunctorAttr::Retain);
        assert_eq!(render(&d), "@retain");
    }

    #[test]
    fn functor_attr_immediate_byte_identity() {
        let d = emit_functor_attr(FunctorAttr::Immediate);
        assert_eq!(render(&d), "@immediate");
    }

    #[test]
    fn atomic_seqcst_byte_identity() {
        let d = emit_atomic(AtomicOrdering::SeqCst);
        assert_eq!(render(&d), "@atomic(SeqCst)");
    }

    #[test]
    fn align_byte_identity() {
        let d = emit_align(16);
        assert_eq!(render(&d), "@align(16)");
    }

    #[test]
    fn ring_flat_fits_in_one_line() {
        // Default width = 100: the flat form fits.
        let d = emit_ring(8, 64);
        assert_eq!(render(&d), "@ring(slots=8, slot_size=64)");
    }

    #[test]
    fn ring_narrow_width_wraps_with_trailing_comma() {
        // Force the group to break by rendering at width = 20. In
        // break mode, each arg is on its own line and a trailing comma
        // follows the last arg (spec §7).
        let d = emit_ring(1024, 128);
        let out = render_at(&d, 20);
        assert!(out.contains('\n'), "expected multi-line wrap, got {out:?}");
        assert!(out.contains("slots=1024,"), "missing first-arg comma: {out}");
        assert!(
            out.contains("slot_size=128,"),
            "missing trailing comma on last arg: {out}"
        );
        // Opening paren opens the first line, closing paren starts the
        // last line (dedented back to column 0).
        assert!(out.starts_with("@ring("), "opening line not canonical: {out}");
        assert!(out.ends_with(")"), "closing paren missing: {out}");
    }

    #[test]
    fn no_frame_bare_byte_identity() {
        let d = emit_no_frame();
        assert_eq!(render(&d), "@no_frame");
    }

    #[test]
    fn attribute_stack_one_per_line() {
        // Three attributes stacked. Spec §7: hardline between each,
        // no trailing hardline (caller composes the follow-on item).
        let docs = vec![
            emit_packed_struct(Some(4)),
            emit_align(16),
            emit_no_frame(),
        ];
        let out = render(&format_attribute_stack(docs));
        assert_eq!(out, "@packed_struct(align=4)\n@align(16)\n@no_frame");
    }

    #[test]
    fn attribute_stack_empty_is_nil() {
        let out = render(&format_attribute_stack(Vec::new()));
        assert_eq!(out, "");
    }

    #[test]
    fn dispatcher_routes_each_kind() {
        // Smoke-test that AttrKind -> emit routes every variant to its
        // canonical string. Keeps the dispatcher in sync with the
        // helpers; adding a variant without updating `emit` lights
        // this up at compile time (exhaustive match) and this test
        // catches a wrong-arm routing bug.
        assert_eq!(render(&emit(AttrKind::Endian(Endianness::Be))), "@endian(be)");
        assert_eq!(render(&emit(AttrKind::Abi(CallingConvention::Ms))), "@abi(\"ms\")");
        assert_eq!(render(&emit(AttrKind::LinkSection(".text"))), "@link_section(\".text\")");
        assert_eq!(render(&emit(AttrKind::JumpTable)), "@jump_table");
        assert_eq!(
            render(&emit(AttrKind::PackedStruct { align: None })),
            "@packed_struct"
        );
        assert_eq!(
            render(&emit(AttrKind::Functor(FunctorAttr::Retain))),
            "@retain"
        );
        assert_eq!(
            render(&emit(AttrKind::Atomic(AtomicOrdering::Acquire))),
            "@atomic(Acquire)"
        );
        assert_eq!(render(&emit(AttrKind::Align(32))), "@align(32)");
        assert_eq!(
            render(&emit(AttrKind::Ring {
                slots: 2,
                slot_size: 8
            })),
            "@ring(slots=2, slot_size=8)"
        );
        assert_eq!(render(&emit(AttrKind::NoFrame)), "@no_frame");
        let ia = InterruptAttr {
            has_error_code: false,
            vector: 0,
            name: "divide_error".to_string(),
        };
        assert_eq!(
            render(&emit(AttrKind::Interrupt(&ia))),
            "@interrupt(\"divide_error\")"
        );
    }
}
