//! Expression formatter — bucket A (FMT-5a, paideia-as#1568).
//!
//! # Scope
//!
//! This module replaces the walker's `format_expr` scaffold stub for
//! the following five expression categories:
//!
//! | category | AST variants                                                   |
//! |----------|----------------------------------------------------------------|
//! | Primary  | `Literal`, `Path`, `Tuple`, `ArrayLit`, `ArrayRepeat`,         |
//! |          | `Range`, `Uninit`, `StringLiteral`, `ByteStringLiteral`,       |
//! |          | `InlineBytes`, `InlineStr`                                     |
//! | Postfix  | `FieldAccess`, `Postfix` (`?`), `Cast`                         |
//! | Call     | `Call` (and method-call = `Call{callee=FieldAccess, ..}`)      |
//! | BinOp    | `Infix` (every binary op token: `+ - * / % & | ^ << >> == !=  |
//! |          | < > <= >= && || =`)                                            |
//! | Prefix   | `Prefix` (`! - ~ $`), `Borrow` (`& / &mut`), `Deref` (`*`)     |
//!
//! Non-exhaustive match: every other `ExprData` variant (bucket B —
//! `Match`, `If`, `Loop`, `For`, `Block`, `Lambda`, `WithHandler`,
//! `Unsafe`, `Perform`, `Resume`, `HandlerValue`, `Quote`, `Antiquote`,
//! `FunctorApp`, `Pack`, `Unpack`, `LetModule`, `RecordCons`, …) falls
//! through to [`walker::fallback_slice`]. FMT-5b will claim those; this
//! sprint leaves them emitting the raw source bytes so byte-identity
//! still holds for mixed-content files.
//!
//! # Precedence / paren rule — structural, not source-tracking
//!
//! The parser *unwraps* grouping parens (`(x)` → `x`), so the AST does
//! not record user-written redundant parens. The formatter re-inserts
//! parens *only when precedence demands it* — using the pair
//!
//! * `left_child.right_bp < parent.left_bp` → wrap left operand
//! * `right_child.left_bp < parent.right_bp` → wrap right operand
//!
//! which exactly inverts the parser's Pratt-loop admission rule and so
//! round-trips `format(parse(s))` to a tree that re-parses to the same
//! shape. The spec's "keep user parens even where redundant" directive
//! cannot be fully honoured at this layer because the parser has
//! already discarded that information; FMT-10 (round-trip audit) is
//! the right place to decide whether to add a source-span paren probe
//! or leave this as the structural minimum. See the sprint report for
//! the exact gap statement.
//!
//! # Call-argument shape (style-spec §7.4)
//!
//! * Single-line: `f(a, b, c)` — one space after each comma; no
//!   trailing comma; no space between callee and `(`.
//! * Multi-line: one argument per line at `callee.col + 2`; trailing
//!   comma MANDATORY; `)` on its own line at `callee.col`.
//!
//! The Doc idiom is `group([callee, "(", nest(2, [softline, args]),
//! if_break(",", nil), softline, ")"])` — see [`format_call`].
//!
//! # BinOp shape (sprint spec)
//!
//! Operator at the **start** of the continuation line in break mode.
//! Doc idiom: `group([lhs, line, text(op), text(" "), rhs])`. In flat
//! this renders `lhs <op> rhs`; in break, `lhs\n<indent><op> rhs`.
//!
//! # Postfix-chain shape (style-spec §7.4)
//!
//! A chain like `a.b().c().d()` stays on one line if it fits; else it
//! breaks before each `.`. We encode this with
//! `group([receiver, nest(2, [softline, ".field"])])` — a Doc engine
//! that commits each enclosing group independently will break the
//! outermost first, so the chain unfolds right-to-left into one
//! `.method()` per line, each at `+2` indent from the chain root.

use paideia_as_ast::{ExprData, NodeId, NodeKind};

use crate::doc::{self, Doc};
use crate::walker::{self, FormatContext, fallback_slice};

/// Dispatch entry — the walker calls this for every expression node in
/// bucket A. `kind` is the already-extracted `NodeKind` (saves a second
/// arena lookup in the caller). Unknown / bucket-B variants return
/// `fallback_slice(ctx, span)` so byte-identity is preserved.
#[must_use]
pub fn format_expr(ctx: &FormatContext<'_>, nid: NodeId, kind: NodeKind) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return doc::nil(),
    };
    let span = data.span;
    let expr = ctx.arena.expr_data(nid);

    match (kind, expr) {
        // --- Primary ---------------------------------------------------
        (NodeKind::ExprLiteral, _)
        | (NodeKind::ExprString, _)
        | (NodeKind::ExprByteString, _)
        | (NodeKind::ExprInlineBytes, _)
        | (NodeKind::ExprInlineStr, _) => fallback_slice(ctx, span),

        (NodeKind::ExprUninit, _) => doc::text("uninit"),

        (NodeKind::ExprPath, Some(ExprData::Path { segments })) => {
            format_path(ctx, segments)
        }

        (NodeKind::ExprTuple, Some(ExprData::Tuple { elements })) => {
            format_tuple(ctx, elements)
        }

        (NodeKind::ExprArrayLit, Some(ExprData::ArrayLit(elements))) => {
            format_array_lit(ctx, elements)
        }

        (NodeKind::ExprArrayRepeat, Some(ExprData::ArrayRepeat { expr, count })) => {
            format_array_repeat(ctx, *expr, *count)
        }

        (NodeKind::ExprRange, Some(ExprData::Range { start, end })) => {
            format_range(ctx, *start, *end)
        }

        // --- Prefix ----------------------------------------------------
        (NodeKind::ExprPrefix, Some(ExprData::Prefix { op, expr, kind: _ })) => {
            format_prefix(ctx, *op, *expr)
        }

        (NodeKind::ExprBorrow, Some(ExprData::Borrow { expr, mutable })) => {
            format_borrow(ctx, *expr, *mutable)
        }

        (NodeKind::ExprDeref, Some(ExprData::Deref { expr })) => {
            format_deref(ctx, *expr)
        }

        // --- Postfix ---------------------------------------------------
        (NodeKind::ExprFieldAccess, Some(ExprData::FieldAccess { receiver, field })) => {
            format_field_access(ctx, *receiver, *field)
        }

        (NodeKind::ExprPostfix, Some(ExprData::Postfix { expr, op })) => {
            format_postfix_op(ctx, *expr, *op)
        }

        (NodeKind::ExprCast, Some(ExprData::Cast { expr, target_ty })) => {
            format_cast(ctx, *expr, *target_ty)
        }

        // --- Call ------------------------------------------------------
        (NodeKind::ExprCall, Some(ExprData::Call { callee, args })) => {
            format_call(ctx, *callee, args)
        }

        // --- BinOp -----------------------------------------------------
        (NodeKind::ExprInfix, Some(ExprData::Infix { lhs, op, rhs })) => {
            format_infix(ctx, *lhs, *op, *rhs)
        }

        // Bucket B (match/if/loop/block/lambda/…) and anything added
        // after this sprint fall through. byte-identity holds.
        _ => fallback_slice(ctx, span),
    }
}

// ---------------------------------------------------------------------
// Primary
// ---------------------------------------------------------------------

/// `seg1::seg2::seg3`. Each segment is an `Ident` node; we emit its
/// source slice and join with `::`. Using slices preserves raw-identifier
/// spelling (e.g. `r#match`) that the parser doesn't normalise.
fn format_path(ctx: &FormatContext<'_>, segments: &[NodeId]) -> Doc {
    if segments.is_empty() {
        return doc::nil();
    }
    let mut parts: Vec<Doc> = Vec::with_capacity(segments.len() * 2 - 1);
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            parts.push(doc::text("::"));
        }
        parts.push(format_child(ctx, *seg));
    }
    doc::concat(parts)
}

/// `(e)` for a 1-tuple (trailing comma MANDATORY to disambiguate from
/// grouping — parser invariant), `(e1, e2, …, eN)` for N >= 2. Multi-
/// line form adds the usual trailing comma via `if_break`.
fn format_tuple(ctx: &FormatContext<'_>, elements: &[NodeId]) -> Doc {
    if elements.is_empty() {
        // `()` is a unit *literal*, not a 0-tuple; shouldn't reach here,
        // but fall back safely.
        return doc::text("()");
    }
    if elements.len() == 1 {
        // 1-tuple MUST keep its trailing comma; the parser already
        // recorded `(x,)` as a Tuple (vs. `(x)` which was unwrapped).
        return doc::concat(vec![
            doc::text("("),
            format_child(ctx, elements[0]),
            doc::text(",)"),
        ]);
    }
    delimited_list(ctx, "(", ")", elements)
}

/// `[]`, `[e1, e2, …]`. Multi-line form adds trailing comma.
fn format_array_lit(ctx: &FormatContext<'_>, elements: &[NodeId]) -> Doc {
    if elements.is_empty() {
        return doc::text("[]");
    }
    delimited_list(ctx, "[", "]", elements)
}

/// `[expr; count]`.
fn format_array_repeat(ctx: &FormatContext<'_>, expr: NodeId, count: NodeId) -> Doc {
    doc::concat(vec![
        doc::text("["),
        format_child(ctx, expr),
        doc::text("; "),
        format_child(ctx, count),
        doc::text("]"),
    ])
}

/// `start..end`, `..end`, `start..`, `..`.
fn format_range(ctx: &FormatContext<'_>, start: Option<NodeId>, end: Option<NodeId>) -> Doc {
    let s = start
        .map(|id| format_child(ctx, id))
        .unwrap_or_else(doc::nil);
    let e = end.map(|id| format_child(ctx, id)).unwrap_or_else(doc::nil);
    doc::concat(vec![s, doc::text(".."), e])
}

// ---------------------------------------------------------------------
// Prefix
// ---------------------------------------------------------------------

/// `op expr` — no space between op and operand (spec §7.1). The op's
/// identity lives on `ExprData::Prefix::kind` but the exact source
/// bytes live on the `op` Placeholder node; we emit the slice so
/// atypical operators (e.g. `$` for `PrefixOp::Other`) round-trip.
fn format_prefix(ctx: &FormatContext<'_>, op: NodeId, expr: NodeId) -> Doc {
    let op_text = op_slice(ctx, op);
    let operand = format_child_at(ctx, expr, PREFIX_BP);
    doc::concat(vec![doc::text(op_text), operand])
}

/// `&expr` / `&mut expr` — no space between `&` and operand; one space
/// between `&mut` and operand.
fn format_borrow(ctx: &FormatContext<'_>, expr: NodeId, mutable: bool) -> Doc {
    let operand = format_child_at(ctx, expr, PREFIX_BP);
    if mutable {
        doc::concat(vec![doc::text("&mut "), operand])
    } else {
        doc::concat(vec![doc::text("&"), operand])
    }
}

/// `*expr` — no space between `*` and operand.
fn format_deref(ctx: &FormatContext<'_>, expr: NodeId) -> Doc {
    let operand = format_child_at(ctx, expr, PREFIX_BP);
    doc::concat(vec![doc::text("*"), operand])
}

// ---------------------------------------------------------------------
// Postfix
// ---------------------------------------------------------------------

/// `receiver.field` — single-line if it fits; otherwise break before
/// the `.` (chain idiom). We nest at +2 so each `.chain()` lines up
/// one level deeper than its root.
fn format_field_access(ctx: &FormatContext<'_>, receiver: NodeId, field: NodeId) -> Doc {
    let r = format_child_at(ctx, receiver, POSTFIX_BP);
    let f = format_child(ctx, field);
    doc::group(doc::concat(vec![
        r,
        doc::nest(2, doc::concat(vec![doc::softline(), doc::text("."), f])),
    ]))
}

/// `expr?` (the only `ExprData::Postfix` form on landing — `.await` is
/// not an AST variant in phase-1).
fn format_postfix_op(ctx: &FormatContext<'_>, expr: NodeId, op: NodeId) -> Doc {
    let op_text = op_slice(ctx, op);
    let r = format_child_at(ctx, expr, POSTFIX_BP);
    doc::concat(vec![r, doc::text(op_text)])
}

/// `expr as T` — space-around `as` (§7.1 treats `as` like an infix for
/// spacing purposes). Target type is routed back through the walker so
/// FMT-6 will own its shape once it lands; today it returns the type's
/// source slice.
fn format_cast(ctx: &FormatContext<'_>, expr: NodeId, target_ty: NodeId) -> Doc {
    let e = format_child_at(ctx, expr, CAST_BP);
    let t = walker::format_node(ctx, target_ty);
    doc::concat(vec![e, doc::text(" as "), t])
}

// ---------------------------------------------------------------------
// Call
// ---------------------------------------------------------------------

/// `f(a, b, c)` / multi-line with trailing comma.
///
/// The callee is formatted at `POSTFIX_BP` so an `Infix` callee (which
/// cannot actually occur post-parse: callees are atomic in phase-1,
/// never raw binops) still wraps correctly in paren-emission.
fn format_call(ctx: &FormatContext<'_>, callee: NodeId, args: &[NodeId]) -> Doc {
    let callee_doc = format_child_at(ctx, callee, POSTFIX_BP);
    if args.is_empty() {
        return doc::concat(vec![callee_doc, doc::text("()")]);
    }

    // Single-line: callee(a, b, c). Multi-line: one arg per line with
    // trailing comma. The group's fit probe decides which.
    //
    // Doc shape:
    //   group([ callee, "(", nest(2, [ softline, a, ",", line, b, ",", line, c ]),
    //           if_break(",", nil), softline, ")" ])
    let args_sep = doc::concat(vec![doc::text(","), doc::line()]);
    let mut arg_docs: Vec<Doc> = Vec::with_capacity(args.len());
    for &a in args {
        arg_docs.push(format_child(ctx, a));
    }
    let args_body = doc::separate(args_sep, arg_docs);

    doc::group(doc::concat(vec![
        callee_doc,
        doc::text("("),
        doc::nest(2, doc::concat(vec![doc::softline(), args_body])),
        doc::if_break(doc::text(","), doc::nil()),
        doc::softline(),
        doc::text(")"),
    ]))
}

// ---------------------------------------------------------------------
// Infix
// ---------------------------------------------------------------------

/// `lhs op rhs` with operator-at-start-of-continuation break shape.
///
/// Flat:  `lhs op rhs`
/// Break: `lhs\n  op rhs`
///
/// Paren emission follows the structural rule in the module header;
/// the operator text is read from the op Placeholder's span (the AST
/// does not re-record the token kind).
fn format_infix(ctx: &FormatContext<'_>, lhs: NodeId, op: NodeId, rhs: NodeId) -> Doc {
    let op_text = op_slice(ctx, op);
    let (parent_l, parent_r) = infix_bp_of(&op_text);

    let l = format_child_as_left(ctx, lhs, parent_l);
    let r = format_child_as_right(ctx, rhs, parent_r);

    // Break before the operator: the operator sits at the start of the
    // continuation line (per sprint spec).
    doc::group(doc::concat(vec![
        l,
        doc::nest(
            2,
            doc::concat(vec![doc::line(), doc::text(op_text), doc::text(" "), r]),
        ),
    ]))
}

// ---------------------------------------------------------------------
// Child formatting + paren insertion
// ---------------------------------------------------------------------

/// Postfix-side binding power (field access, `?`, call) — anchors the
/// paren-insertion decision when a non-infix parent wraps an infix
/// receiver.
const POSTFIX_BP: u8 = 140;
/// `as`'s left bp — between additive and multiplicative.
const CAST_BP: u8 = 105;
/// Prefix operator right-bp (sits above every infix level).
const PREFIX_BP: u8 = 130;

/// Dispatch a child without any precedence context — the parent is
/// not an operator (or the child is a non-expression leaf like an
/// `Ident`). Routes through the walker so non-expr kinds get their
/// own formatter.
fn format_child(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    walker::format_node(ctx, nid)
}

/// Dispatch a child in a context whose "left bp" demand is `demand`.
/// If the child is an infix / cast whose right-bp falls below `demand`,
/// wrap in parens. Used for prefix / postfix / cast / call receivers,
/// and for the right operand of an infix.
fn format_child_at(ctx: &FormatContext<'_>, nid: NodeId, demand: u8) -> Doc {
    let child_bp_r = child_right_bp(ctx, nid);
    let inner = format_child(ctx, nid);
    if child_bp_r < demand {
        doc::concat(vec![doc::text("("), inner, doc::text(")")])
    } else {
        inner
    }
}

/// Left-operand variant of [`format_child_at`] — paren when
/// `child.right_bp < parent.left_bp`.
fn format_child_as_left(ctx: &FormatContext<'_>, nid: NodeId, parent_left: u8) -> Doc {
    let child_bp_r = child_right_bp(ctx, nid);
    let inner = format_child(ctx, nid);
    if child_bp_r < parent_left {
        doc::concat(vec![doc::text("("), inner, doc::text(")")])
    } else {
        inner
    }
}

/// Right-operand variant — paren when
/// `child.left_bp < parent.right_bp`.
fn format_child_as_right(ctx: &FormatContext<'_>, nid: NodeId, parent_right: u8) -> Doc {
    let child_bp_l = child_left_bp(ctx, nid);
    let inner = format_child(ctx, nid);
    if child_bp_l < parent_right {
        doc::concat(vec![doc::text("("), inner, doc::text(")")])
    } else {
        inner
    }
}

/// Return the child expression's *left* binding power — used when the
/// child sits on the right side of an infix parent.
fn child_left_bp(ctx: &FormatContext<'_>, nid: NodeId) -> u8 {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return u8::MAX,
    };
    match data.kind {
        NodeKind::ExprInfix => {
            if let Some(ExprData::Infix { op, .. }) = ctx.arena.expr_data(nid) {
                let t = op_slice(ctx, *op);
                return infix_bp_of(&t).0;
            }
            u8::MAX
        }
        NodeKind::ExprCast => CAST_BP,
        NodeKind::ExprPrefix | NodeKind::ExprBorrow | NodeKind::ExprDeref => PREFIX_BP,
        NodeKind::ExprRange => 55, // non-assoc, matches parser's RANGE_BP
        _ => u8::MAX,
    }
}

/// Return the child expression's *right* binding power — used when the
/// child sits on the left side of an infix parent or as the receiver of
/// a prefix / cast / postfix.
fn child_right_bp(ctx: &FormatContext<'_>, nid: NodeId) -> u8 {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return u8::MAX,
    };
    match data.kind {
        NodeKind::ExprInfix => {
            if let Some(ExprData::Infix { op, .. }) = ctx.arena.expr_data(nid) {
                let t = op_slice(ctx, *op);
                return infix_bp_of(&t).1;
            }
            u8::MAX
        }
        NodeKind::ExprCast => CAST_BP,
        // Prefix/Borrow/Deref are RIGHT-side atomic from a parent's
        // perspective (they bind to their right operand only); their
        // right-bp is PREFIX_BP for paren-decision purposes.
        NodeKind::ExprPrefix | NodeKind::ExprBorrow | NodeKind::ExprDeref => PREFIX_BP,
        NodeKind::ExprRange => 55,
        _ => u8::MAX,
    }
}

/// Operator-token → (left_bp, right_bp). Mirrors
/// `paideia_as_parser::precedence::infix_bp` so the formatter never
/// disagrees with the parser's admission rule. Unknown operators get
/// a "binds tightest" fallback (`(u8::MAX, u8::MAX)`) which is
/// paren-neutral.
fn infix_bp_of(op: &str) -> (u8, u8) {
    match op {
        "=" => (21, 20),
        "||" => (30, 31),
        "&&" => (40, 41),
        "==" | "!=" | "<" | ">" | "<=" | ">=" => (50, 51),
        "|" => (60, 61),
        "^" => (70, 71),
        "&" => (80, 81),
        "<<" | ">>" => (90, 91),
        "+" | "-" => (100, 101),
        "*" | "/" | "%" => (110, 111),
        _ => (u8::MAX, u8::MAX),
    }
}

// ---------------------------------------------------------------------
// Delimited lists (shared between tuple / array / call-args shapes)
// ---------------------------------------------------------------------

/// `open e1, e2, …, eN close` — single-line comma-sep in flat, one-per-
/// line + trailing comma in break. The engine's group-fit chooses.
fn delimited_list(
    ctx: &FormatContext<'_>,
    open: &'static str,
    close: &'static str,
    elements: &[NodeId],
) -> Doc {
    let sep = doc::concat(vec![doc::text(","), doc::line()]);
    let items: Vec<Doc> = elements.iter().map(|&e| format_child(ctx, e)).collect();
    doc::group(doc::concat(vec![
        doc::text(open),
        doc::nest(2, doc::concat(vec![doc::softline(), doc::separate(sep, items)])),
        doc::if_break(doc::text(","), doc::nil()),
        doc::softline(),
        doc::text(close),
    ]))
}

/// Owned copy of the operator-token source slice. The op node is
/// typically a `Placeholder` whose span covers exactly the operator's
/// bytes; a stray out-of-range span returns `""` (benign — the Doc
/// renderer treats empty strings as width-0).
fn op_slice(ctx: &FormatContext<'_>, op: NodeId) -> String {
    ctx.arena
        .get(op)
        .map(|nd| ctx.slice(nd.span).to_owned())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attach::{TriviaAttachmentMap, attach_trivia};
    use crate::cst::tokenize_with_trivia;
    use crate::layout::{self, FormatOptions as LayoutOptions};
    use crate::FormatOptions;
    use crate::walker::FormatContext;
    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, VecSink};
    use paideia_as_lexer::SourceText;
    use paideia_as_parser::Parser;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    /// Parse a single expression (not a whole module) and run it
    /// through [`format_expr`]. Returns the rendered output.
    fn fmt_expr(src: &str) -> String {
        fmt_expr_width(src, 100)
    }

    fn fmt_expr_width(src: &str, max_width: usize) -> String {
        let source = SourceText::from_bytes(file_id(), src.as_bytes())
            .expect("test source must be valid UTF-8");
        let mut lex_sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &source, &mut lex_sink);

        let mut lex2 = VecSink::new();
        let mut lexer = paideia_as_lexer::Lexer::new(file_id(), &source);
        let tokens = lexer.collect_tokens(&mut lex2);

        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let expr_id = {
            let mut p = Parser::new(
                &tokens,
                source.content(),
                file_id(),
                &mut arena,
                &mut sink,
            );
            p.parse_expr().expect("test expression must parse")
        };
        let tmap: TriviaAttachmentMap = attach_trivia(&stream, &arena, expr_id);
        let opts = FormatOptions::default();
        let ctx = FormatContext::new(&source, &stream, &arena, &tmap, &opts);
        let kind = arena.get(expr_id).unwrap().kind;
        let d = format_expr(&ctx, expr_id, kind);
        layout::render(&d, &LayoutOptions { max_width })
    }

    // ---- primary -----------------------------------------------------

    #[test]
    fn literal_integer_is_byte_identical() {
        assert_eq!(fmt_expr("42"), "42");
    }

    #[test]
    fn literal_string_is_byte_identical() {
        assert_eq!(fmt_expr("\"hello\""), "\"hello\"");
    }

    #[test]
    fn path_single_segment_is_byte_identical() {
        assert_eq!(fmt_expr("foo"), "foo");
    }

    #[test]
    fn path_multi_segment_joins_with_colon_colon() {
        assert_eq!(fmt_expr("a::b::c"), "a::b::c");
    }

    #[test]
    fn tuple_two_elements_flat() {
        assert_eq!(fmt_expr("(1, 2)"), "(1, 2)");
    }

    #[test]
    fn tuple_one_element_keeps_trailing_comma() {
        assert_eq!(fmt_expr("(1,)"), "(1,)");
    }

    #[test]
    fn array_literal_flat() {
        assert_eq!(fmt_expr("[1, 2, 3]"), "[1, 2, 3]");
    }

    #[test]
    fn array_repeat() {
        assert_eq!(fmt_expr("[0; 8]"), "[0; 8]");
    }

    // ---- call --------------------------------------------------------

    #[test]
    fn call_no_args_flat() {
        assert_eq!(fmt_expr("f()"), "f()");
    }

    #[test]
    fn call_three_args_flat() {
        assert_eq!(fmt_expr("f(a, b, c)"), "f(a, b, c)");
    }

    #[test]
    fn call_breaks_with_trailing_comma_when_overflowing() {
        // force a tight budget so the call must break
        let got = fmt_expr_width("f(aaaa, bbbb, cccc, dddd)", 10);
        // trailing comma MUST appear in multi-line form
        assert!(
            got.contains(",\n") && got.ends_with(",\n)"),
            "expected multi-line form with trailing comma, got {got:?}"
        );
    }

    // ---- binop -------------------------------------------------------

    #[test]
    fn infix_add_flat() {
        assert_eq!(fmt_expr("a + b"), "a + b");
    }

    #[test]
    fn infix_nested_left_assoc_no_parens() {
        // (a+b)+c reparses the same way; no parens needed on either side
        // of a left-assoc same-prec parent's LEFT operand.
        assert_eq!(fmt_expr("a + b + c"), "a + b + c");
    }

    #[test]
    fn infix_precedence_mul_before_add() {
        // a + b*c: no parens, mul binds tighter.
        assert_eq!(fmt_expr("a + b * c"), "a + b * c");
    }

    #[test]
    fn infix_mixed_precedence_lhs_wraps() {
        // (a + b) * c: lhs is lower-prec than *, must keep parens.
        assert_eq!(fmt_expr("(a + b) * c"), "(a + b) * c");
    }

    #[test]
    fn infix_breaks_with_operator_at_start_of_continuation() {
        let got = fmt_expr_width("aaaa + bbbb + cccc", 10);
        // operator sits at the START of the continuation line.
        for line in got.lines().skip(1) {
            let trimmed = line.trim_start();
            assert!(
                trimmed.starts_with('+'),
                "continuation line {line:?} must start with operator"
            );
        }
    }

    // ---- prefix / postfix --------------------------------------------

    #[test]
    fn prefix_neg_no_space() {
        assert_eq!(fmt_expr("-x"), "-x");
    }

    #[test]
    fn prefix_not_no_space() {
        assert_eq!(fmt_expr("!flag"), "!flag");
    }

    #[test]
    fn borrow_immutable() {
        assert_eq!(fmt_expr("&x"), "&x");
    }

    #[test]
    fn borrow_mutable_spaced() {
        assert_eq!(fmt_expr("&mut x"), "&mut x");
    }

    #[test]
    fn deref_no_space() {
        assert_eq!(fmt_expr("*p"), "*p");
    }

    #[test]
    fn prefix_wraps_infix_operand() {
        // -a is -x where x=a; -(a+b) must keep parens — else it parses
        // as (-a)+b, which is wrong.
        assert_eq!(fmt_expr("-(a + b)"), "-(a + b)");
    }

    #[test]
    fn field_access_flat() {
        assert_eq!(fmt_expr("obj.field"), "obj.field");
    }

    #[test]
    fn method_call_flat() {
        assert_eq!(fmt_expr("obj.method()"), "obj.method()");
    }

    #[test]
    fn question_postfix() {
        assert_eq!(fmt_expr("result?"), "result?");
    }
}
