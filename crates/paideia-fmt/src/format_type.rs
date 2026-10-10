//! Type-expression formatter (FMT-6, paideia-as#1570).
//!
//! Implements structural rendering for every [`TypeData`] variant that
//! `paideia-as-ast` currently carries. The walker's `format_type` stub
//! in `walker.rs` delegates to [`format_type`] below.
//!
//! # Coverage (2026-10-10)
//!
//! | TypeData variant        | Rendered form                            |
//! |-------------------------|------------------------------------------|
//! | `Name { name, args }`   | `Ident` or `Ident(T1, T2)`               |
//! | `FnPtr { … }`           | `(P1, P2) -> R !{E1, …} @{C1, …}`        |
//! | `Closure { … }`         | `\|P1, P2\| -> R !{E1, …} @{C1, …}`      |
//! | `Tuple { elements }`    | `(T1, T2)` (standard tuple rules)        |
//! | `LinearClass { … }`     | `linear T` / `affine T` / `~T` / `↓ T`   |
//! | `EffectRow { … }`       | row body `E1, E2 \| r` (brace-less)      |
//! | `Ptr { pointee }`       | `*T`                                     |
//! | `Ref { pointee, mut }`  | `&T` / `&mut T`                          |
//! | `Record { fields }`     | `record { a: T, b: U }`                  |
//! | `Enum { variants }`     | `enum { Unit, Tup(T), Rec { f: T } }`    |
//! | `SelfQualifiedPath { }` | `Self::Item`                             |
//! | `Array { element, N }`  | `[T; N]`                                 |
//! | `Forall { bound, body }`| `forall a b. T`                          |
//!
//! The brief for #1570 lists sixteen variants but `paideia-as-ast` only
//! names thirteen: scalars (`u64`, `bool`, …) are encoded as
//! `TypeData::Name`, slice `[T]` is not a distinct kind (the parser
//! represents it as `Array` with a wildcard length — not emitted today
//! — or via a named `Slice(T)`), and `&mut T` is `Ref { mutable: true }`
//! rather than its own `MutRef` variant. All three are handled through
//! their actual carriers; see the per-helper comments.
//!
//! # Width policy
//!
//! Every multi-element construct wraps itself in a `Group` so the
//! layout engine picks flat-fits-else-break on a per-type basis. Record
//! and enum bodies take the one-line-if-it-fits-else-one-per-line form
//! from §5.1 of the style spec. Function and closure types `group` at
//! the full signature and break between `(params)`, `-> R`, `!{...}`,
//! `@{...}` sections when they do not fit.
//!
//! # Fallback contract
//!
//! Every helper returns a Doc built from structural pieces. If a child
//! `NodeId` is unknown to the arena (defensive — should not happen on a
//! well-formed parse) the helper slices its source text via
//! [`walker::fallback_slice`]. The outer dispatch in [`format_type`]
//! also falls back if `type_data(nid)` is `None` (e.g., a node that
//! parsed as a type but did not populate the parallel `types` vector).

use paideia_as_ast::{EnumVariant, LinClass, NodeId, TypeData};

use crate::doc::{self, Doc};
use crate::walker::{FormatContext, fallback_slice, format_type};

/// Top-level type formatter — the walker's `format_type` arm delegates
/// here.
///
/// Returns the Doc the layout engine should render for the type node at
/// `nid`. On `None` type data (defensive guard) falls back to a slice
/// of the node's source span.
#[must_use]
pub fn format_type_data(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return doc::nil(),
    };
    let td = match ctx.arena.type_data(nid) {
        Some(t) => t,
        None => {
            // The node is tagged as a type-kind but carries no
            // category data — slice the raw source instead.
            return fallback_slice(ctx, data.span);
        }
    };

    match td {
        TypeData::Name { name, args } => fmt_name(ctx, *name, args),
        TypeData::FnPtr {
            params,
            param_names,
            ret,
            effects,
            capabilities,
        } => fmt_fnptr(ctx, params, param_names, *ret, *effects, *capabilities),
        TypeData::Closure {
            params,
            ret,
            effects,
            capabilities,
        } => fmt_closure(ctx, params, *ret, *effects, *capabilities),
        TypeData::Tuple { elements } => fmt_tuple(ctx, elements),
        TypeData::LinearClass { class, inner } => fmt_linear_class(ctx, *class, *inner),
        TypeData::EffectRow { items, rest } => fmt_effect_row_body(ctx, items, *rest),
        TypeData::Ptr { pointee } => fmt_ptr(ctx, *pointee),
        TypeData::Ref { pointee, mutable } => fmt_ref(ctx, *pointee, *mutable),
        TypeData::Record { fields } => fmt_record(ctx, fields),
        TypeData::Enum { variants } => fmt_enum(ctx, variants),
        TypeData::SelfQualifiedPath { item } => fmt_self_qualified(ctx, *item),
        TypeData::Array { element, length } => fmt_array(ctx, *element, *length),
        TypeData::Forall { bound, body } => fmt_forall(ctx, bound, *body),
    }
}

// -----------------------------------------------------------------
// Per-variant helpers
// -----------------------------------------------------------------

/// Named type `Ident` or generic application `Ident(T1, T2, …)`.
///
/// Scalars (`u64`, `bool`, `char`, `str`, …) are a fast-path: they are
/// `Name` nodes with an empty `args` vector and emit as a single
/// `Doc::text`. For generic application, the parser's surface syntax is
/// parentheses — not angle brackets — so we keep `Vec(T)` on round-trip
/// to preserve parseability. The style spec's `<T>` form (§6.2) is
/// aspirational and tracked under a separate ticket for the parser.
fn fmt_name(ctx: &FormatContext<'_>, name: NodeId, args: &[NodeId]) -> Doc {
    let head = ident_text(ctx, name);
    if args.is_empty() {
        return head;
    }
    // Wrap args in a group so a long generic application breaks across
    // lines with one argument per line and a trailing comma (§3.1).
    let arg_docs = args.iter().map(|a| format_type(ctx, *a)).collect::<Vec<_>>();
    let inner = doc::group(doc::concat(vec![
        doc::text("("),
        doc::nest(
            2,
            doc::concat(vec![
                doc::softline(),
                separate_commas(arg_docs),
                doc::if_break(doc::text(","), doc::nil()),
            ]),
        ),
        doc::softline(),
        doc::text(")"),
    ]));
    doc::concat(vec![head, inner])
}

/// Function-pointer type `(P1, P2) -> R !{…} @{…}`.
///
/// The signature is wrapped in one `Group` so the whole signature
/// either stays flat on one line or breaks between `(params)`, the
/// return type, the effect row, and the capability row. The parameter
/// list itself is a nested group with its own fits-flat-else-break
/// decision (so `(T1, T2)` can stay flat even if the ret or effect
/// chunks force the outer wrap).
fn fmt_fnptr(
    ctx: &FormatContext<'_>,
    params: &[NodeId],
    param_names: &[Option<NodeId>],
    ret: NodeId,
    effects: Option<NodeId>,
    capabilities: Option<NodeId>,
) -> Doc {
    let params_doc = fmt_param_list(ctx, params, param_names);
    let ret_doc = format_type(ctx, ret);

    let mut parts: Vec<Doc> = Vec::with_capacity(6);
    parts.push(params_doc);
    parts.push(doc::line());
    parts.push(doc::text("-> "));
    parts.push(ret_doc);
    if let Some(e) = effects {
        parts.push(doc::line());
        parts.push(fmt_effect_row(ctx, e));
    }
    if let Some(c) = capabilities {
        parts.push(doc::line());
        parts.push(fmt_capability_row(ctx, c));
    }

    doc::group(doc::nest(2, doc::concat(parts)))
}

/// Closure type `|P1, P2| -> R !{…} @{…}`.
///
/// Shape mirrors [`fmt_fnptr`] but uses `|…|` bars for the parameter
/// list. Closure types do not carry `param_names` in the AST (they are
/// parsed only as positional), so the parameter list is always
/// positional.
fn fmt_closure(
    ctx: &FormatContext<'_>,
    params: &[NodeId],
    ret: NodeId,
    effects: Option<NodeId>,
    capabilities: Option<NodeId>,
) -> Doc {
    let param_docs = params.iter().map(|p| format_type(ctx, *p)).collect::<Vec<_>>();
    let params_doc = doc::group(doc::concat(vec![
        doc::text("|"),
        doc::nest(
            2,
            doc::concat(vec![
                doc::softline(),
                separate_commas(param_docs),
                doc::if_break(doc::text(","), doc::nil()),
            ]),
        ),
        doc::softline(),
        doc::text("|"),
    ]));
    let ret_doc = format_type(ctx, ret);

    let mut parts: Vec<Doc> = Vec::with_capacity(6);
    parts.push(params_doc);
    parts.push(doc::line());
    parts.push(doc::text("-> "));
    parts.push(ret_doc);
    if let Some(e) = effects {
        parts.push(doc::line());
        parts.push(fmt_effect_row(ctx, e));
    }
    if let Some(c) = capabilities {
        parts.push(doc::line());
        parts.push(fmt_capability_row(ctx, c));
    }
    doc::group(doc::nest(2, doc::concat(parts)))
}

/// Parameter list for a function-pointer type. Named params render as
/// `name : T` (space-both colon, §4.3); unnamed params render as `T`.
fn fmt_param_list(
    ctx: &FormatContext<'_>,
    params: &[NodeId],
    param_names: &[Option<NodeId>],
) -> Doc {
    let mut items: Vec<Doc> = Vec::with_capacity(params.len());
    for (i, p) in params.iter().enumerate() {
        let name_opt = param_names.get(i).copied().flatten();
        let ty_doc = format_type(ctx, *p);
        let item = match name_opt {
            Some(name) => doc::concat(vec![ident_text(ctx, name), doc::text(" : "), ty_doc]),
            None => ty_doc,
        };
        items.push(item);
    }
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

/// Tuple type `(T1, T2, …)`. The AST does not surface the parser's
/// unit / 1-tuple shape distinction, so this helper assumes `elements`
/// is the raw element list.
fn fmt_tuple(ctx: &FormatContext<'_>, elements: &[NodeId]) -> Doc {
    if elements.is_empty() {
        return doc::text("()");
    }
    let items = elements
        .iter()
        .map(|e| format_type(ctx, *e))
        .collect::<Vec<_>>();
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

/// `<LinClass> T`. Keyword forms (`linear`/`affine`/`ordered`/`unrestricted`)
/// take a trailing space; marker forms (`↓`/`$`/`~`) stay glued to the
/// inner type.
fn fmt_linear_class(ctx: &FormatContext<'_>, class: LinClass, inner: NodeId) -> Doc {
    let inner_doc = format_type(ctx, inner);
    let prefix = match class {
        LinClass::Ordered => doc::text("ordered "),
        LinClass::Linear => doc::text("linear "),
        LinClass::Affine => doc::text("affine "),
        LinClass::Unrestricted => doc::text("unrestricted "),
        // Marker shorthands: no space between the glyph and the type.
        LinClass::LinearMark => doc::text("↓ "),
        LinClass::AffineMark => doc::text("~"),
    };
    doc::concat(vec![prefix, inner_doc])
}

/// `*T` — bare raw pointer. The style spec (§6.2) prescribes no space
/// between `*` and the pointee.
fn fmt_ptr(ctx: &FormatContext<'_>, pointee: NodeId) -> Doc {
    doc::concat(vec![doc::text("*"), format_type(ctx, pointee)])
}

/// `&T` or `&mut T`. One space after `mut`, none between `&` and the
/// type (§6.2).
fn fmt_ref(ctx: &FormatContext<'_>, pointee: NodeId, mutable: bool) -> Doc {
    if mutable {
        doc::concat(vec![doc::text("&mut "), format_type(ctx, pointee)])
    } else {
        doc::concat(vec![doc::text("&"), format_type(ctx, pointee)])
    }
}

/// `record { field: T, field: T, … }`. One-line if it fits, else one
/// field per line with a mandatory trailing comma (§3.1, §5.1).
fn fmt_record(ctx: &FormatContext<'_>, fields: &[(NodeId, NodeId)]) -> Doc {
    if fields.is_empty() {
        return doc::text("record {}");
    }
    let items: Vec<Doc> = fields
        .iter()
        .map(|(name, ty)| {
            doc::concat(vec![
                ident_text(ctx, *name),
                doc::text(": "),
                format_type(ctx, *ty),
            ])
        })
        .collect();
    doc::group(doc::concat(vec![
        doc::text("record {"),
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
    ]))
}

/// `enum { Variant, Variant(T), Variant { f: T } }`. Payload shapes
/// mirror the parser:
///
/// * `Unit`: just the name.
/// * `Tuple`: `Name(T1, T2)` — tuple-rules body.
/// * `Record`: `Name { f1: T1, f2: T2 }` — record-rules body.
fn fmt_enum(ctx: &FormatContext<'_>, variants: &[EnumVariant]) -> Doc {
    if variants.is_empty() {
        return doc::text("enum {}");
    }
    let items: Vec<Doc> = variants.iter().map(|v| fmt_enum_variant(ctx, v)).collect();
    doc::group(doc::concat(vec![
        doc::text("enum {"),
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
    ]))
}

fn fmt_enum_variant(ctx: &FormatContext<'_>, v: &EnumVariant) -> Doc {
    match v {
        EnumVariant::Unit { name } => ident_text(ctx, *name),
        EnumVariant::Tuple { name, payload } => {
            if payload.is_empty() {
                return ident_text(ctx, *name);
            }
            let items = payload.iter().map(|p| format_type(ctx, *p)).collect::<Vec<_>>();
            doc::concat(vec![
                ident_text(ctx, *name),
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
        EnumVariant::Record { name, fields } => {
            if fields.is_empty() {
                return doc::concat(vec![ident_text(ctx, *name), doc::text(" {}")]);
            }
            let items: Vec<Doc> = fields
                .iter()
                .map(|(fname, fty)| {
                    doc::concat(vec![
                        ident_text(ctx, *fname),
                        doc::text(": "),
                        format_type(ctx, *fty),
                    ])
                })
                .collect();
            doc::concat(vec![
                ident_text(ctx, *name),
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
    }
}

/// `Self::Item`. The leading `Self` is a literal keyword in the parser;
/// `item` is an Ident node whose text we slice.
fn fmt_self_qualified(ctx: &FormatContext<'_>, item: NodeId) -> Doc {
    doc::concat(vec![doc::text("Self::"), ident_text(ctx, item)])
}

/// `[T; N]`. One space after `;`, none before (§6.2). The length
/// expression's source bytes are sliced verbatim — type-position
/// expressions sit outside FMT-6's scope; FMT-5 will provide a Doc
/// here when it lands.
fn fmt_array(ctx: &FormatContext<'_>, element: NodeId, length: NodeId) -> Doc {
    let len_doc = match ctx.arena.get(length) {
        Some(nd) => fallback_slice(ctx, nd.span),
        None => doc::nil(),
    };
    doc::concat(vec![
        doc::text("["),
        format_type(ctx, element),
        doc::text("; "),
        len_doc,
        doc::text("]"),
    ])
}

/// `forall a b. T`. Bound-variable order is preserved as parsed; the
/// style spec is silent on reordering under alphabetisation or on
/// kinded bounds (`T: Trait1 + Trait2`), so we keep source order and
/// no bounds (bounds live on separate GenericParam items, not on the
/// `Forall` type's `bound` vec).
fn fmt_forall(ctx: &FormatContext<'_>, bound: &[NodeId], body: NodeId) -> Doc {
    let mut parts: Vec<Doc> = Vec::with_capacity(bound.len() * 2 + 3);
    parts.push(doc::text("forall "));
    for (i, b) in bound.iter().enumerate() {
        if i > 0 {
            parts.push(doc::text(" "));
        }
        parts.push(ident_text(ctx, *b));
    }
    parts.push(doc::text(". "));
    parts.push(format_type(ctx, body));
    doc::concat(parts)
}

/// Effect-row *reference* (as it appears on a function type): `!{...}`.
///
/// The row's `items` + optional `rest` are rendered by
/// [`fmt_effect_row_body`]; this helper wraps them in the `!{…}`
/// delimiters that the parser's EffectOpen token produces.
fn fmt_effect_row(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let body = fmt_effect_row_body_from_node(ctx, nid);
    doc::group(doc::concat(vec![doc::text("!{"), body, doc::text("}")]))
}

/// Capability-row *reference*: `@{...}`.
///
/// The AST reuses `TypeData::EffectRow` for both effect and capability
/// rows, but the capability-set parser flattens dotted paths
/// (`Mmio.read_cap` → `[Mmio, read_cap]`) with no structural dot
/// marker in the arena — so a comma-based rebuild would mis-reparse
/// as two separate capabilities. We slice the row's source span
/// (`@{…}`) verbatim instead. This preserves parseability at the cost
/// of per-item wrapping; the capability-row AST will need a dedicated
/// variant before this helper can go structural.
fn fmt_capability_row(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    match ctx.arena.get(nid) {
        Some(nd) => fallback_slice(ctx, nd.span),
        None => doc::nil(),
    }
}

/// Fetch a `TypeData::EffectRow` from `nid` and render its body
/// (the comma-separated items plus optional `| rest`). Falls back to a
/// source slice when the node is not an EffectRow in the arena.
fn fmt_effect_row_body_from_node(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    match ctx.arena.type_data(nid) {
        Some(TypeData::EffectRow { items, rest }) => fmt_effect_row_body(ctx, items, *rest),
        _ => {
            // Not an EffectRow — slice the source span as a fallback.
            match ctx.arena.get(nid) {
                Some(nd) => fallback_slice(ctx, nd.span),
                None => doc::nil(),
            }
        }
    }
}

/// Effect / capability row body: `E1, E2 | rest` or just `E1, E2`.
///
/// Items use the format `E1, E2` with one space after each comma
/// (§6.2); the optional rest variable, if present, is prefixed by
/// ` | ` (space-both). Multi-item rows that overflow break with one
/// item per line + trailing comma (§3.1).
fn fmt_effect_row_body(ctx: &FormatContext<'_>, items: &[NodeId], rest: Option<NodeId>) -> Doc {
    let item_docs: Vec<Doc> = items.iter().map(|i| format_type(ctx, *i)).collect();
    let items_part = if item_docs.is_empty() {
        doc::nil()
    } else {
        doc::group(doc::concat(vec![
            doc::nest(
                2,
                doc::concat(vec![
                    doc::softline(),
                    separate_commas(item_docs),
                    doc::if_break(doc::text(","), doc::nil()),
                ]),
            ),
            doc::softline(),
        ]))
    };
    match rest {
        Some(r) => {
            let rest_doc = format_type(ctx, r);
            doc::concat(vec![items_part, doc::text("| "), rest_doc])
        }
        None => items_part,
    }
}

// -----------------------------------------------------------------
// Small shared helpers
// -----------------------------------------------------------------

/// Slice an identifier node's source text into a `Doc::Text`. Used for
/// `Ident` leaves on names, bound vars, record field names, enum
/// variant names, and so on. Falls back to `nil` for unknown ids
/// (defensive — a well-formed arena never produces one).
fn ident_text(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    match ctx.arena.get(nid) {
        Some(nd) => fallback_slice(ctx, nd.span),
        None => doc::nil(),
    }
}

/// Join a vector of docs with `", "` in flat mode and a line break in
/// break mode. This is the standard list-element separator for every
/// multi-element construct in a type.
fn separate_commas(items: Vec<Doc>) -> Doc {
    doc::separate(doc::concat(vec![doc::text(","), doc::line()]), items)
}

#[cfg(test)]
mod tests {
    //! Byte-level and width-wrap tests for the FMT-6 type formatter.
    //!
    //! Each test parses a snippet whose top-level item carries the type
    //! under test (we tunnel through `pub let _x : <TY> = 0`), fishes
    //! out the `ty` NodeId, and renders it with `format_type_data`.

    use super::*;
    use crate::FormatOptions as CrateFormatOptions;
    use crate::attach::attach_trivia;
    use crate::cst::tokenize_with_trivia;
    use crate::layout::{self, FormatOptions as LayoutOptions};
    use paideia_as_ast::{AstArena, ItemData};
    use paideia_as_diagnostics::{FileId, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};
    use paideia_as_parser::Parser;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    /// Render the top-level `let`'s type annotation.
    ///
    /// Returns `(rendered, raw)` where `rendered` is the formatter's
    /// output and `raw` is the source slice over the type's span — the
    /// byte-identity reference for scalar cases.
    fn render_type(src: &str, max_width: usize) -> (String, String) {
        let source = SourceText::from_bytes(file_id(), src.as_bytes()).unwrap();
        let mut lex_sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut lex_sink);
        let mut lex_sink2 = VecSink::new();
        let mut lexer = Lexer::new(file_id(), &source);
        let tokens = lexer.collect_tokens(&mut lex_sink2);
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
            p.parse_source_file().expect("parse failed")
        };
        // Walk the Structure for the first Let item and pick its `ty`.
        let ty_id = find_let_type(&arena, root_id).expect("no let::ty found");
        let tmap = attach_trivia(&stream, &arena, root_id);
        let opts = CrateFormatOptions {
            max_width,
            ..Default::default()
        };
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let doc_tree = format_type_data(&ctx, ty_id);
        let out = layout::render(
            &doc_tree,
            &LayoutOptions {
                max_width,
                ..Default::default()
            },
        );
        let span = arena.get(ty_id).unwrap().span;
        let raw = source.content()[span.byte_start() as usize..span.byte_end() as usize].to_string();
        (out, raw)
    }

    /// Dig into a Structure root and return the first `Let`'s type id.
    fn find_let_type(arena: &AstArena, root: NodeId) -> Option<NodeId> {
        let item = arena.item_data(root)?;
        let items = match item {
            ItemData::Structure { items, .. } => items,
            _ => return None,
        };
        for id in items {
            if let Some(ItemData::Let { ty: Some(t), .. }) = arena.item_data(*id) {
                return Some(*t);
            }
        }
        None
    }

    // ---- Byte-identity on scalar names ------------------------------

    #[test]
    fn scalar_u64_renders_as_u64() {
        let (got, raw) = render_type("pub let x : u64 = 0", 100);
        assert_eq!(got, "u64");
        assert_eq!(got, raw);
    }

    #[test]
    fn scalar_bool_renders_as_bool() {
        let (got, _raw) = render_type("pub let x : bool = false", 100);
        assert_eq!(got, "bool");
    }

    #[test]
    fn scalar_str_renders_as_str() {
        // `str` is a plain TypeData::Name in paideia-as.
        let (got, _raw) = render_type("pub let x : str = \"\"", 100);
        assert_eq!(got, "str");
    }

    // ---- Compound ---------------------------------------------------

    #[test]
    fn tuple_two_elements_flat() {
        let (got, _raw) = render_type("pub let x : (u64, u64) = (0, 0)", 100);
        assert_eq!(got, "(u64, u64)");
    }

    #[test]
    fn array_type_u64_16() {
        let (got, _raw) = render_type("pub let x : [u64; 16] = uninit", 100);
        assert_eq!(got, "[u64; 16]");
    }

    #[test]
    fn ref_type_shared() {
        let (got, _raw) = render_type("pub let x : &u64 = &0", 100);
        assert_eq!(got, "&u64");
    }

    #[test]
    fn ref_type_mutable() {
        let (got, _raw) = render_type("pub let x : &mut u64 = &mut 0", 100);
        assert_eq!(got, "&mut u64");
    }

    #[test]
    fn ptr_type_bare() {
        let (got, _raw) = render_type("pub let x : *u64 = uninit", 100);
        assert_eq!(got, "*u64");
    }

    // ---- Record / enum ---------------------------------------------

    #[test]
    fn record_one_line_fits() {
        let (got, _raw) = render_type(
            "pub let x : record { a: u64, b: u64 } = uninit",
            100,
        );
        assert_eq!(got, "record { a: u64, b: u64 }");
    }

    #[test]
    fn record_wraps_when_narrow() {
        let (got, _raw) = render_type(
            "pub let x : record { a: u64, b: u64, c: u64 } = uninit",
            20,
        );
        // Expect: record {\n  a: u64,\n  b: u64,\n  c: u64,\n}
        assert!(got.contains('\n'), "expected multi-line, got {got:?}");
        assert!(got.ends_with("}"), "expected closing brace, got {got:?}");
        // Trailing comma on the last field per §3.1.
        assert!(got.contains("c: u64,"), "trailing comma missing: {got:?}");
    }

    // ---- Function type ---------------------------------------------

    #[test]
    fn fnptr_one_line_fits() {
        let (got, _raw) = render_type("pub let x : (u64) -> u64 = uninit", 100);
        assert_eq!(got, "(u64) -> u64");
    }

    #[test]
    fn fnptr_with_effects_flat() {
        let (got, _raw) = render_type(
            "pub let x : (u64) -> u64 !{io} = uninit",
            100,
        );
        assert_eq!(got, "(u64) -> u64 !{io}");
    }

    #[test]
    fn fnptr_with_effects_and_caps_flat() {
        let (got, _raw) = render_type(
            "pub let x : (u64) -> u64 !{io} @{mmio.read} = uninit",
            100,
        );
        assert_eq!(got, "(u64) -> u64 !{io} @{mmio.read}");
    }

    #[test]
    fn fnptr_multi_arg_wraps_under_narrow_width() {
        let (got, _raw) = render_type(
            "pub let x : (u64, u64, u64) -> u64 = uninit",
            12,
        );
        assert!(
            got.contains('\n'),
            "expected multi-line wrap at width=12, got {got:?}"
        );
    }

    #[test]
    fn fnptr_effect_row_wraps_under_narrow_width() {
        let (got, _raw) = render_type(
            "pub let x : (u64) -> u64 !{io, mem, net} = uninit",
            16,
        );
        assert!(
            got.contains('\n'),
            "expected effect-row wrap at width=16, got {got:?}"
        );
    }

    // ---- Pointer / tuple nesting -----------------------------------

    #[test]
    fn nested_record_inside_ref() {
        let (got, _raw) = render_type(
            "pub let x : &record { a: u8 } = uninit",
            100,
        );
        assert_eq!(got, "&record { a: u8 }");
    }

    // ---- Forall ----------------------------------------------------

    #[test]
    fn forall_single_var() {
        let (got, _raw) = render_type("pub let x : forall a. a = uninit", 100);
        assert_eq!(got, "forall a. a");
    }
}
