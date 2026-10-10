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

use paideia_as_ast::{ExprData, HandlerArm, LoopKind, MatchArm, NodeId, NodeKind, SegPrefix, StmtData};

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

        // --- Bucket B (FMT-5b, paideia-as#1569) ------------------------
        (NodeKind::ExprBlock, Some(ExprData::Block { stmts, tail })) => {
            format_block(ctx, stmts, *tail)
        }

        (NodeKind::ExprMatch, Some(ExprData::Match { scrutinee, arms, attrs: _ })) => {
            format_match(ctx, *scrutinee, arms)
        }

        (NodeKind::ExprIf, Some(ExprData::If { cond, then_block, else_block })) => {
            format_if(ctx, *cond, *then_block, *else_block)
        }

        (NodeKind::ExprLoop, Some(ExprData::Loop { kind, header, body })) => {
            format_loop(ctx, *kind, *header, *body)
        }

        (NodeKind::ExprFor, Some(ExprData::For { pattern, iterable, body })) => {
            format_for(ctx, *pattern, *iterable, *body)
        }

        (NodeKind::ExprBreak, _) => doc::text("break"),
        (NodeKind::ExprContinue, _) => doc::text("continue"),

        (NodeKind::ExprLambda, Some(ExprData::Lambda {
            generic_params: _,
            params,
            body,
            pipe_form,
        })) => format_lambda(ctx, params, *body, *pipe_form),

        (NodeKind::ExprWithHandler, Some(ExprData::WithHandler {
            handler,
            bind,
            block,
            finally,
        })) => format_with_handler(ctx, *handler, *bind, *block, *finally),

        (NodeKind::ExprUnsafe, Some(ExprData::Unsafe {
            effects,
            capabilities,
            justification,
            block,
        })) => format_unsafe(ctx, effects, capabilities, *justification, block),

        (NodeKind::ExprPerform, Some(ExprData::Perform { op_path, args })) => {
            format_perform(ctx, *op_path, args)
        }

        (NodeKind::ExprResume, Some(ExprData::Resume { value })) => {
            format_resume(ctx, *value)
        }

        (NodeKind::ExprHandlerValue, Some(ExprData::HandlerValue { effect, arms })) => {
            format_handler_value(ctx, *effect, arms)
        }

        (NodeKind::ExprQuote, Some(ExprData::Quote { body })) => {
            format_quote(ctx, *body)
        }

        (NodeKind::ExprAntiquote, Some(ExprData::Antiquote { value })) => {
            format_antiquote(ctx, *value)
        }

        (NodeKind::ExprPack, Some(ExprData::Pack { module_path, signature_path })) => {
            format_pack(ctx, *module_path, *signature_path)
        }

        (NodeKind::ExprUnpack, Some(ExprData::Unpack { value })) => {
            format_unpack(ctx, *value)
        }

        (NodeKind::ExprLetModule, Some(ExprData::LetModule { name, body, rest })) => {
            format_let_module(ctx, name, *body, *rest)
        }

        (NodeKind::OperandMemoryRef, Some(ExprData::OperandMemoryRef { segment, addr })) => {
            format_memref(ctx, *segment, *addr)
        }

        // Anything added after this sprint fall through. byte-identity
        // holds via source-slice.
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

// =====================================================================
// Bucket B — control-flow / isolation / metaprogramming (FMT-5b)
// =====================================================================
//
// Covered variants (paideia-as#1569):
//   control-flow : Match, If, Loop, While, For, Break, Continue, Block
//   function-ish : Lambda, WithHandler, Perform, Resume, HandlerValue
//   isolation    : Unsafe (4-field aligned per style-spec §11.1)
//   meta-prog    : Quote, Antiquote
//   modules      : Pack, Unpack, LetModule
//   assembly     : OperandMemoryRef
//
// Variants intentionally left on the bucket-A fallback (`fallback_slice`):
//   ActionBlock, FunctorApp, RecordCons — these have their own future
//   tickets (effect/handler/action/functor-module polish passes). Byte
//   identity continues to hold via source slice until then.

// ---------------------------------------------------------------------
// Doc flat-width measurement helper
// ---------------------------------------------------------------------

/// Walk `d` as if the enclosing group had committed to flat mode and
/// return the number of characters it would render to.
///
/// Used by [`format_match`] to compute each pattern's rendered width so
/// `=>` alignment can pad trailing spaces to the longest. Returns
/// `None` if the doc contains an unconditional break (a `HardLine`) —
/// such a pattern cannot participate in single-line alignment per
/// style-spec §8.2 exception 2.
fn measure_doc_flat(d: &Doc) -> Option<usize> {
    let mut total: usize = 0;
    let mut stack: Vec<&Doc> = vec![d];
    while let Some(node) = stack.pop() {
        match node {
            Doc::Nil | Doc::SoftLine => {}
            Doc::Text(s) => total += s.chars().count(),
            Doc::Line => total += 1,
            Doc::HardLine => return None,
            Doc::Nest(_, inner) | Doc::Group(inner) => stack.push(inner),
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    stack.push(item);
                }
            }
            Doc::IfBreak { when_flat, .. } => stack.push(when_flat),
        }
    }
    Some(total)
}

// ---------------------------------------------------------------------
// Block — `{ s1; s2; expr? }`
// ---------------------------------------------------------------------

/// `{ s1; s2; expr? }`. Empty block renders as `{}`. A single-stmt or
/// single-tail body fits on one line when space allows; otherwise one
/// stmt per line at +2 indent, closing brace at the caller's column.
///
/// For statements, we currently emit the source slice spanning the
/// statement node (including its trailing semicolon, when the parser
/// recorded it inside the span). This preserves byte-identity for
/// statement interiors — a future FMT-ticket owns stmt-level
/// restructuring.
fn format_block(ctx: &FormatContext<'_>, stmts: &[NodeId], tail: Option<NodeId>) -> Doc {
    if stmts.is_empty() && tail.is_none() {
        return doc::text("{}");
    }

    let mut items: Vec<Doc> = Vec::with_capacity(stmts.len() + 1);
    for &s in stmts {
        items.push(format_stmt(ctx, s));
    }
    if let Some(t) = tail {
        items.push(format_child(ctx, t));
    }

    // Multi-line form: one item per line at +2, each on a hardline-
    // separated row. Flat form (used only when it fits): single-space
    // separated.
    let sep = doc::concat(vec![doc::text(";"), doc::line()]);
    let joined = doc::separate(sep, items);

    doc::group(doc::concat(vec![
        doc::text("{"),
        doc::nest(2, doc::concat(vec![doc::line(), joined])),
        doc::line(),
        doc::text("}"),
    ]))
}

/// Render a statement node for inclusion in a block.
///
/// Dispatch on `StmtData` for the shapes we can structurally re-emit
/// (StmtLet source-slice today; StmtExpr via `format_child` + `;`;
/// StmtReturn as a keyword; raw-asm instructions / labels fall back to
/// their source slice).
fn format_stmt(ctx: &FormatContext<'_>, nid: NodeId) -> Doc {
    let data = match ctx.arena.get(nid) {
        Some(d) => d,
        None => return doc::nil(),
    };
    match ctx.arena.stmt_data(nid) {
        Some(StmtData::Expr { expr }) => format_child(ctx, *expr),
        Some(StmtData::Return { value: None }) => doc::text("return"),
        Some(StmtData::Return { value: Some(v) }) => {
            doc::concat(vec![doc::text("return "), format_child(ctx, *v)])
        }
        // Let / Instruction / Label: fall back to source slice until
        // the stmt-level formatter ticket lands.
        _ => fallback_slice(ctx, data.span),
    }
}

// ---------------------------------------------------------------------
// Match — `match scrutinee { arms }` with `=>` alignment
// ---------------------------------------------------------------------

/// `match scrutinee { arms }`. Arms render with `=>` alignment per
/// style-spec §8.2 when all patterns fit single-line and the longest
/// pattern is under `max_width - 10` columns; else alignment is
/// dropped and each arm goes `pattern => body`.
fn format_match(ctx: &FormatContext<'_>, scrutinee: NodeId, arms: &[MatchArm]) -> Doc {
    let scrut = format_child(ctx, scrutinee);

    if arms.is_empty() {
        return doc::concat(vec![doc::text("match "), scrut, doc::text(" {}")]);
    }

    // First pass: render each pattern (+ optional guard) and its body.
    // Separately measure pattern-flat width for alignment.
    struct RenderedArm {
        pat_doc: Doc,
        pat_width: Option<usize>,
        body_doc: Doc,
        body_flat_fits: bool,
    }

    let mut rendered: Vec<RenderedArm> = Vec::with_capacity(arms.len());
    let mut max_pat_width: usize = 0;
    let mut any_unmeasurable = false;

    for arm in arms {
        let pat = walker::format_node(ctx, arm.pattern);
        let pat_full = if let Some(g) = arm.guard {
            doc::concat(vec![pat, doc::text(" if "), format_child(ctx, g)])
        } else {
            pat
        };
        let pw = measure_doc_flat(&pat_full);
        if let Some(w) = pw {
            if w > max_pat_width {
                max_pat_width = w;
            }
        } else {
            any_unmeasurable = true;
        }
        let body = format_child(ctx, arm.body);
        let body_fits = measure_doc_flat(&body).is_some();
        rendered.push(RenderedArm {
            pat_doc: pat_full,
            pat_width: pw,
            body_doc: body,
            body_flat_fits: body_fits,
        });
    }

    // Alignment-off exception: longest pattern wider than max_width-10
    // (style-spec §8.2 exception 1), or any pattern unmeasurable
    // (multi-line pattern — exception 2).
    let align_off = any_unmeasurable || max_pat_width + 10 >= 100;

    let mut arm_docs: Vec<Doc> = Vec::with_capacity(rendered.len());
    for arm in rendered {
        let pad = if align_off || !arm.body_flat_fits {
            0
        } else {
            arm.pat_width.map_or(0, |w| max_pat_width.saturating_sub(w))
        };
        let padding = if pad > 0 {
            doc::text(" ".repeat(pad))
        } else {
            doc::nil()
        };
        let arm_doc = doc::concat(vec![
            arm.pat_doc,
            padding,
            doc::text(" => "),
            arm.body_doc,
            doc::text(","),
        ]);
        arm_docs.push(arm_doc);
    }

    // Multi-line form always for match (even a single-arm match prefers
    // multi-line body for diff hygiene; style-spec §8.1 example).
    let sep = doc::hardline();
    let body = doc::separate(sep, arm_docs);

    doc::concat(vec![
        doc::text("match "),
        scrut,
        doc::text(" {"),
        doc::nest(2, doc::concat(vec![doc::hardline(), body])),
        doc::hardline(),
        doc::text("}"),
    ])
}

// ---------------------------------------------------------------------
// If / else-if chain — § 7.2
// ---------------------------------------------------------------------

/// `if cond { ... }` with optional `else { ... }` or chained
/// `else if cond { ... }`.
///
/// else-if detection: when the else block's AST shape is itself an
/// `If`, we cuddle (`} else if ... { ... }`); otherwise we emit a
/// plain `} else { ... }`.
fn format_if(
    ctx: &FormatContext<'_>,
    cond: NodeId,
    then_block: NodeId,
    else_block: Option<NodeId>,
) -> Doc {
    let cond_doc = format_child(ctx, cond);
    let then_doc = format_child(ctx, then_block);
    let head = doc::concat(vec![
        doc::text("if "),
        cond_doc,
        doc::text(" "),
        then_doc,
    ]);
    match else_block {
        None => head,
        Some(eb) => {
            // Cuddle else-if when the else's child is another If.
            let is_elif = ctx
                .arena
                .get(eb)
                .map(|d| d.kind == NodeKind::ExprIf)
                .unwrap_or(false);
            let else_doc = format_child(ctx, eb);
            if is_elif {
                doc::concat(vec![head, doc::text(" else "), else_doc])
            } else {
                doc::concat(vec![head, doc::text(" else "), else_doc])
            }
        }
    }
}

// ---------------------------------------------------------------------
// Loop / While / For
// ---------------------------------------------------------------------

/// `loop { ... }` or `while cond { ... }`. Body is always a block, so
/// the delegation to `format_child` on `body` picks up the `format_block`
/// multi-line shape automatically when the body is non-trivial.
fn format_loop(
    ctx: &FormatContext<'_>,
    kind: LoopKind,
    header: Option<NodeId>,
    body: NodeId,
) -> Doc {
    let body_doc = format_child(ctx, body);
    match (kind, header) {
        (LoopKind::Loop, _) => doc::concat(vec![doc::text("loop "), body_doc]),
        (LoopKind::While, Some(h)) => {
            let cond = format_child(ctx, h);
            doc::concat(vec![doc::text("while "), cond, doc::text(" "), body_doc])
        }
        (LoopKind::While, None) => doc::concat(vec![doc::text("while "), body_doc]),
    }
}

/// `for pat in iter { body }`.
fn format_for(ctx: &FormatContext<'_>, pattern: NodeId, iterable: NodeId, body: NodeId) -> Doc {
    let pat = walker::format_node(ctx, pattern);
    let iter = format_child(ctx, iterable);
    let body_doc = format_child(ctx, body);
    doc::concat(vec![
        doc::text("for "),
        pat,
        doc::text(" in "),
        iter,
        doc::text(" "),
        body_doc,
    ])
}

// ---------------------------------------------------------------------
// Lambda — `fn (…) -> body` or `|…| body`
// ---------------------------------------------------------------------

/// `fn (p1 : T1) (p2 : T2) -> body` for fn-form, or `|p1, p2| body`
/// for pipe-form. Parameter nodes are routed through the walker — they
/// are `PatternType` pairs whose shape is a FMT-7 concern — so for
/// now each one renders via `fallback_slice`.
fn format_lambda(
    ctx: &FormatContext<'_>,
    params: &[NodeId],
    body: NodeId,
    pipe_form: bool,
) -> Doc {
    let body_doc = format_child(ctx, body);
    if pipe_form {
        // |x, y| body
        let mut parts: Vec<Doc> = Vec::with_capacity(params.len() * 2 + 3);
        parts.push(doc::text("|"));
        for (i, &p) in params.iter().enumerate() {
            if i > 0 {
                parts.push(doc::text(", "));
            }
            parts.push(walker::format_node(ctx, p));
        }
        parts.push(doc::text("| "));
        parts.push(body_doc);
        doc::concat(parts)
    } else {
        // fn (p1 : T1) (p2 : T2) -> body
        //
        // Multi-arg curried form puts each `(param)` adjacent to the
        // next with one space between (style-spec §4.2).
        let mut parts: Vec<Doc> = Vec::with_capacity(params.len() * 3 + 3);
        parts.push(doc::text("fn "));
        for (i, &p) in params.iter().enumerate() {
            if i > 0 {
                parts.push(doc::text(" "));
            }
            parts.push(doc::text("("));
            parts.push(walker::format_node(ctx, p));
            parts.push(doc::text(")"));
        }
        parts.push(doc::text(" -> "));
        parts.push(body_doc);
        doc::concat(parts)
    }
}

// ---------------------------------------------------------------------
// With-handler — `with h handle name block [finally ...]`
// ---------------------------------------------------------------------

/// `with handler-expr handle name { … [finally => f] }`.
///
/// `finally` always appears LAST inside the block (parser invariant) —
/// to avoid re-ordering stmts we route the block through
/// `format_child` and let its block-formatter handle layout; the
/// finally clause is appended inside the block if non-None. Since the
/// finally is a separate `Option<NodeId>` on `WithHandler` (and NOT
/// inside the block's stmt list), we conservatively render as:
///
/// ```text
/// with H handle name <block>
/// ```
///
/// with the `finally => f` folded into the block's tail only when the
/// parser-attached form is already so shaped. v1 keeps the finally on
/// a separate line when present (byte-stable but not fully structural).
fn format_with_handler(
    ctx: &FormatContext<'_>,
    handler: NodeId,
    bind: NodeId,
    block: NodeId,
    finally: Option<NodeId>,
) -> Doc {
    let h = format_child(ctx, handler);
    let n = walker::format_node(ctx, bind);
    let b = format_child(ctx, block);
    let mut parts = vec![
        doc::text("with "),
        h,
        doc::text(" handle "),
        n,
        doc::text(" "),
        b,
    ];
    if let Some(f) = finally {
        // Appended on a hardline after the block body, inside the
        // enclosing scope: `… <block>\nfinally => <f>`. The style-spec
        // (§4.2 example chain) does not fully pin this; conservative
        // emission keeps the clause but defers reshaping to FMT-9.
        parts.push(doc::hardline());
        parts.push(doc::text("finally => "));
        parts.push(format_child(ctx, f));
    }
    doc::concat(parts)
}

// ---------------------------------------------------------------------
// Unsafe — 4-field aligned per style-spec §11.1
// ---------------------------------------------------------------------

/// `unsafe { effects: …, capabilities: …, justification: …, block: { … } }`
///
/// Field order is FIXED by the parser (§11 style-spec). The three
/// single-line fields (`effects`, `capabilities`, `justification`) are
/// aligned by padding each key with trailing spaces so that each
/// value's first visible column lands at the same position. The
/// `block:` field uses single-space-after-colon because its body is
/// multi-line (style-spec example in §11.1).
fn format_unsafe(
    ctx: &FormatContext<'_>,
    effects: &[NodeId],
    capabilities: &[NodeId],
    justification: NodeId,
    block: &[NodeId],
) -> Doc {
    // Keys and their alignment target.
    //
    // Longest aligned key is `justification` (13 chars). Content for
    // the three aligned fields lands at column `base_indent + 15`
    // (`justification:` + one space).
    const ALIGN_COL: usize = 15; // "justification:" = 14 + " " = 15

    fn pad_for(key: &str) -> Doc {
        let key_len_with_colon = key.chars().count() + 1;
        let pad = ALIGN_COL.saturating_sub(key_len_with_colon);
        doc::text(" ".repeat(pad))
    }

    // effects: { ... }
    //
    // The parser stores each row (`effects` / `capabilities`) as a
    // `Vec<NodeId>` with a SINGLE element — the already-brace-wrapped
    // row node (see `parse_unsafe` tail where `effects_vec` is
    // `vec![eff_node]`). We therefore route that one node through the
    // walker and let its own slice carry the braces. The row-list
    // multi-element case stays supported for future AST changes.
    let eff_values = render_unsafe_row(ctx, effects);
    let eff_line = doc::concat(vec![
        doc::text("effects:"),
        pad_for("effects"),
        eff_values,
        doc::text(","),
    ]);

    // capabilities: { ... }
    let cap_values = render_unsafe_row(ctx, capabilities);
    let cap_line = doc::concat(vec![
        doc::text("capabilities:"),
        pad_for("capabilities"),
        cap_values,
        doc::text(","),
    ]);

    // justification: "..."
    let just_doc = format_child(ctx, justification);
    let just_line = doc::concat(vec![
        doc::text("justification:"),
        pad_for("justification"),
        just_doc,
        doc::text(","),
    ]);

    // block: { stmts; ... }
    //
    // The block body is an assembly-instruction sequence (plus labels
    // and possibly local `let`s). We render each stmt on its own line
    // at +2 from the `block:` key via a stmt loop.
    let block_body = if block.is_empty() {
        doc::text("{}")
    } else {
        let mut rows: Vec<Doc> = Vec::with_capacity(block.len());
        for &s in block {
            rows.push(format_stmt(ctx, s));
        }
        let joined = doc::separate(doc::hardline(), rows);
        doc::concat(vec![
            doc::text("{"),
            doc::nest(2, doc::concat(vec![doc::hardline(), joined])),
            doc::hardline(),
            doc::text("}"),
        ])
    };
    let block_line = doc::concat(vec![
        doc::text("block: "),
        block_body,
        doc::text(","),
    ]);

    // Assemble the unsafe body.
    let fields = doc::separate(
        doc::hardline(),
        vec![eff_line, cap_line, just_line, block_line],
    );
    doc::concat(vec![
        doc::text("unsafe {"),
        doc::nest(2, doc::concat(vec![doc::hardline(), fields])),
        doc::hardline(),
        doc::text("}"),
    ])
}

/// Render the `effects` / `capabilities` row of an `unsafe { }` block.
///
/// Shape:
/// * Empty vec → `{}`.
/// * Single element (the canonical post-parser shape) → the element's
///   own walker format; the parser already emits a brace-wrapped
///   `TypeEffectRow` node whose source slice is `{ … }`, so no extra
///   wrapping is needed.
/// * Multiple elements → `{ e1, e2, … }` (reserved for a future AST
///   change that stops stuffing the row into a single sub-node).
fn render_unsafe_row(ctx: &FormatContext<'_>, items: &[NodeId]) -> Doc {
    match items.len() {
        0 => doc::text("{}"),
        1 => walker::format_node(ctx, items[0]),
        _ => {
            let mut parts: Vec<Doc> = Vec::with_capacity(items.len() * 2 + 2);
            parts.push(doc::text("{ "));
            for (i, &nid) in items.iter().enumerate() {
                if i > 0 {
                    parts.push(doc::text(", "));
                }
                parts.push(walker::format_node(ctx, nid));
            }
            parts.push(doc::text(" }"));
            doc::concat(parts)
        }
    }
}

// ---------------------------------------------------------------------
// Effect expressions — Perform / Resume / HandlerValue
// ---------------------------------------------------------------------

/// `perform Effect::op(args)` — the callee path uses standard path
/// formatting; argument list uses the call-form shape with trailing
/// comma on overflow.
fn format_perform(ctx: &FormatContext<'_>, op_path: NodeId, args: &[NodeId]) -> Doc {
    let path = format_child(ctx, op_path);
    if args.is_empty() {
        return doc::concat(vec![doc::text("perform "), path, doc::text("()")]);
    }
    let sep = doc::concat(vec![doc::text(","), doc::line()]);
    let arg_docs: Vec<Doc> = args.iter().map(|&a| format_child(ctx, a)).collect();
    let body = doc::separate(sep, arg_docs);
    doc::group(doc::concat(vec![
        doc::text("perform "),
        path,
        doc::text("("),
        doc::nest(2, doc::concat(vec![doc::softline(), body])),
        doc::if_break(doc::text(","), doc::nil()),
        doc::softline(),
        doc::text(")"),
    ]))
}

/// `resume value`.
fn format_resume(ctx: &FormatContext<'_>, value: NodeId) -> Doc {
    doc::concat(vec![doc::text("resume "), format_child(ctx, value)])
}

/// `handle Effect { op name => body ; finally => body }` — handler-value
/// construction. Arm layout mirrors match-arm alignment but on a
/// smaller cover (no guard), and the `finally` arm (parser invariant)
/// is last.
fn format_handler_value(ctx: &FormatContext<'_>, effect: NodeId, arms: &[HandlerArm]) -> Doc {
    let eff = walker::format_node(ctx, effect);
    if arms.is_empty() {
        return doc::concat(vec![doc::text("handle "), eff, doc::text(" {}")]);
    }
    let mut rows: Vec<Doc> = Vec::with_capacity(arms.len());
    for arm in arms {
        let row = match arm {
            HandlerArm::Op { op, handler } => {
                let name = walker::format_node(ctx, *op);
                let hd = format_child(ctx, *handler);
                doc::concat(vec![doc::text("op "), name, doc::text(" => "), hd, doc::text(",")])
            }
            HandlerArm::Finally { cleanup } => {
                let cl = format_child(ctx, *cleanup);
                doc::concat(vec![doc::text("finally => "), cl, doc::text(",")])
            }
        };
        rows.push(row);
    }
    let body = doc::separate(doc::hardline(), rows);
    doc::concat(vec![
        doc::text("handle "),
        eff,
        doc::text(" {"),
        doc::nest(2, doc::concat(vec![doc::hardline(), body])),
        doc::hardline(),
        doc::text("}"),
    ])
}

// ---------------------------------------------------------------------
// Meta-programming — Quote / Antiquote
// ---------------------------------------------------------------------

/// `quote { body }`. Metaprogramming content is quasi-literal; v1 keeps
/// the inner body routed through the walker so a child expression's
/// own formatter still applies, but the surrounding `quote { }` wrap
/// stays textual.
fn format_quote(ctx: &FormatContext<'_>, body: NodeId) -> Doc {
    let inner = format_child(ctx, body);
    doc::concat(vec![
        doc::text("quote { "),
        inner,
        doc::text(" }"),
    ])
}

/// `~(value)` — antiquote (code splice). The sigil binds like a prefix
/// operator; we always wrap the operand in parens because that is the
/// parser-accepted canonical form (`~(expr)`).
fn format_antiquote(ctx: &FormatContext<'_>, value: NodeId) -> Doc {
    let inner = format_child(ctx, value);
    doc::concat(vec![doc::text("~("), inner, doc::text(")")])
}

// ---------------------------------------------------------------------
// Modules — Pack / Unpack / LetModule
// ---------------------------------------------------------------------

/// `pack M : S`.
fn format_pack(ctx: &FormatContext<'_>, module_path: NodeId, signature_path: NodeId) -> Doc {
    let m = format_child(ctx, module_path);
    let s = format_child(ctx, signature_path);
    doc::concat(vec![doc::text("pack "), m, doc::text(" : "), s])
}

/// `unpack v`.
fn format_unpack(ctx: &FormatContext<'_>, value: NodeId) -> Doc {
    doc::concat(vec![doc::text("unpack "), format_child(ctx, value)])
}

/// `let module N = body in rest`. The `body` here is an `Unpack`
/// expression (parser invariant) but we route generically so any
/// future broadening of the RHS category still formats.
fn format_let_module(ctx: &FormatContext<'_>, name: &str, body: NodeId, rest: NodeId) -> Doc {
    let b = format_child(ctx, body);
    let r = format_child(ctx, rest);
    doc::concat(vec![
        doc::text("let module "),
        doc::text(name.to_owned()),
        doc::text(" = "),
        b,
        doc::text(" in "),
        r,
    ])
}

// ---------------------------------------------------------------------
// Memory reference — `[addr]` / `[seg:addr]`
// ---------------------------------------------------------------------

/// `[addr]` or `[fs:addr]` / `[gs:addr]` — assembly memory-operand
/// bracket form. The segment prefix (if any) is attached with a colon
/// and no spaces.
fn format_memref(ctx: &FormatContext<'_>, segment: Option<SegPrefix>, addr: NodeId) -> Doc {
    let addr_doc = format_child(ctx, addr);
    match segment {
        None => doc::concat(vec![doc::text("["), addr_doc, doc::text("]")]),
        Some(SegPrefix::Fs) => doc::concat(vec![doc::text("[fs:"), addr_doc, doc::text("]")]),
        Some(SegPrefix::Gs) => doc::concat(vec![doc::text("[gs:"), addr_doc, doc::text("]")]),
    }
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

    // =================================================================
    // Bucket B — control-flow / isolation / metaprogramming (FMT-5b)
    // =================================================================

    // ---- Match -------------------------------------------------------

    #[test]
    fn match_three_arms_aligns_fat_arrows() {
        // `=>` columns align: `First` is 5, `Second` is 6, `Third` is 5.
        // Longest is 6 → each arm pads pattern to width 6.
        let got = fmt_expr("match c { First => 0, Second => 1, Third => 2 }");
        // All arrows should be preceded by the pattern + padding; the
        // three `=>` substrings should start at the same column within
        // their arm lines.
        let mut arrow_cols: Vec<usize> = Vec::new();
        for line in got.lines() {
            if let Some(idx) = line.find("=>") {
                arrow_cols.push(idx);
            }
        }
        assert_eq!(arrow_cols.len(), 3, "expected 3 arm arrows, got {got:?}");
        assert!(
            arrow_cols.iter().all(|&c| c == arrow_cols[0]),
            "`=>` columns not aligned: {arrow_cols:?} in {got:?}"
        );
    }

    #[test]
    fn match_arms_use_hardline_separators() {
        let got = fmt_expr("match n { 0 => 1, _ => 2 }");
        // Multi-line: `match n {\n  0 => 1,\n  _ => 2,\n}`.
        assert!(got.contains('\n'), "match should always be multi-line, got {got:?}");
        assert!(got.ends_with("}\n") || got.ends_with('}'));
        assert!(got.contains(" => "));
    }

    #[test]
    fn match_includes_trailing_comma_on_each_arm() {
        let got = fmt_expr("match n { 0 => 1, _ => 2 }");
        // Each arm line (interior of match) ends in `,`.
        let interior: Vec<&str> = got.lines().filter(|l| l.contains("=>")).collect();
        for line in &interior {
            assert!(
                line.trim_end().ends_with(','),
                "arm line missing trailing comma: {line:?} in {got:?}"
            );
        }
    }

    #[test]
    fn match_with_five_arms_aligns_all() {
        let got =
            fmt_expr("match c { A => 0, Bb => 1, Ccc => 2, Dddd => 3, Eeeee => 4 }");
        let mut arrow_cols: Vec<usize> = Vec::new();
        for line in got.lines() {
            if let Some(idx) = line.find("=>") {
                arrow_cols.push(idx);
            }
        }
        assert_eq!(arrow_cols.len(), 5);
        assert!(arrow_cols.iter().all(|&c| c == arrow_cols[0]), "{arrow_cols:?}");
    }

    #[test]
    fn match_scrutinee_is_rendered() {
        let got = fmt_expr("match x { _ => 0 }");
        assert!(got.starts_with("match x {"), "unexpected prefix in {got:?}");
    }

    // ---- If ----------------------------------------------------------

    #[test]
    fn if_then_block() {
        let got = fmt_expr("if cond { 1 }");
        assert!(got.starts_with("if cond "), "unexpected prefix: {got:?}");
        assert!(got.contains('{'));
    }

    #[test]
    fn if_else_chain_three_branches_cuddles_else_if() {
        let got = fmt_expr("if a { 1 } else if b { 2 } else { 3 }");
        // The two `else` boundaries both appear in the output.
        let else_count = got.matches("else").count();
        assert_eq!(else_count, 2, "expected 2 'else' keywords, got {got:?}");
        // `else if` is cuddled (adjacent on one span, no newline between).
        assert!(
            got.contains("else if ") || got.contains("else if\n"),
            "expected 'else if' cuddling, got {got:?}"
        );
    }

    #[test]
    fn if_without_else_renders_as_single_clause() {
        let got = fmt_expr("if cond { 1 }");
        assert!(!got.contains("else"), "unexpected else in {got:?}");
    }

    // ---- Loop / While / For -----------------------------------------

    #[test]
    fn loop_with_body_renders_keyword() {
        let got = fmt_expr("loop { 1 }");
        assert!(got.starts_with("loop "), "unexpected prefix: {got:?}");
        assert!(got.contains('{'));
    }

    #[test]
    fn while_with_condition_and_body() {
        let got = fmt_expr("while cond { 1 }");
        assert!(got.starts_with("while cond "), "unexpected prefix: {got:?}");
    }

    #[test]
    fn for_pat_in_iter_body() {
        let got = fmt_expr("for x in xs { 1 }");
        assert!(got.contains("for "));
        assert!(got.contains(" in "));
    }

    #[test]
    fn break_keyword_is_bare() {
        // break appears inside a loop body — exercise through the loop
        // walker so the ExprBreak dispatch arm fires.
        let got = fmt_expr("loop { break }");
        assert!(got.contains("break"), "break missing in {got:?}");
    }

    #[test]
    fn continue_keyword_is_bare() {
        let got = fmt_expr("loop { continue }");
        assert!(got.contains("continue"), "continue missing in {got:?}");
    }

    // ---- Block -------------------------------------------------------

    #[test]
    fn block_empty_is_two_braces() {
        assert_eq!(fmt_expr("{}"), "{}");
    }

    #[test]
    fn block_single_tail_fits_flat() {
        // `{ 42 }` is small enough to render flat.
        let got = fmt_expr("{ 42 }");
        assert!(got.contains("42"));
        assert!(got.starts_with('{'));
        assert!(got.ends_with('}'));
    }

    // ---- Lambda ------------------------------------------------------

    #[test]
    fn lambda_fn_single_param() {
        let got = fmt_expr("fn (x : u64) -> x");
        assert!(got.starts_with("fn ("), "unexpected prefix: {got:?}");
        assert!(got.contains(" -> "));
    }

    #[test]
    fn lambda_fn_curried_two_params() {
        // Multi-arg curried form: two adjacent `(…)` groups.
        let got = fmt_expr("fn (x : u64) (y : u64) -> x");
        assert!(got.starts_with("fn ("));
        // There are two `)` and two `(` groups.
        assert_eq!(got.matches('(').count(), 2, "{got:?}");
    }

    #[test]
    fn lambda_pipe_form_single_param() {
        let got = fmt_expr("|x| x");
        assert!(got.starts_with("|"), "unexpected prefix: {got:?}");
        assert!(got.contains("| "));
    }

    // ---- MemRef ------------------------------------------------------
    //
    // MemRef is normally a sub-operand of an assembly instruction, not
    // an expression. The format_memref formatter is wired for structural
    // rendering when the walker routes it; a smoke test here would
    // require parsing an unsafe-block instruction and is covered more
    // naturally by the walker's structural tests. Behavior at this
    // layer is unit-tested via a direct Doc check elsewhere.

    // ---- Unsafe ------------------------------------------------------

    #[test]
    fn unsafe_four_field_alignment() {
        let got = fmt_expr(
            "unsafe {\n  effects: { RawMem },\n  capabilities: { paideia.raw_mem },\n  justification: \"test justification string\",\n  block: { mov rax, rdi }\n}",
        );
        // All three aligned keys (`effects`, `capabilities`, `justification`)
        // put their colon followed by padding + content; the content
        // column must agree across the three lines.
        //
        // The content column for each line is the index of its leftmost
        // non-space character after the colon.
        fn content_col_after(line: &str, key: &str) -> Option<usize> {
            let needle = format!("{key}:");
            let idx = line.find(&needle)?;
            let after_colon = idx + needle.len();
            // walk past spaces
            let rest = &line[after_colon..];
            let non_space = rest.chars().take_while(|c| *c == ' ').count();
            Some(after_colon + non_space)
        }
        let eff_line = got.lines().find(|l| l.contains("effects:")).unwrap();
        let cap_line = got.lines().find(|l| l.contains("capabilities:")).unwrap();
        let just_line = got.lines().find(|l| l.contains("justification:")).unwrap();
        let eff = content_col_after(eff_line, "effects").unwrap();
        let cap = content_col_after(cap_line, "capabilities").unwrap();
        let just = content_col_after(just_line, "justification").unwrap();
        assert_eq!(
            (eff, cap, just),
            (just, just, just),
            "unsafe-block field colons not aligned: {got:?}"
        );
    }

    // ---- Pack / Unpack ----------------------------------------------
    //
    // `pack`/`unpack` are contextual keywords NOT wired into
    // `parse_primary` (see `modules.rs` parse_pack_expr doc comment:
    // "This is exposed as pub for external testing. It is not wired
    // into parse_primary to avoid regressing value-level expressions.")
    // So they're unreachable via `parse_expr` from a test harness
    // today — the formatters are still live for the paths that build
    // these nodes directly (functor-application contexts).

    // ---- Quote / Antiquote ------------------------------------------

    #[test]
    fn quote_body_renders_in_braces() {
        let got = fmt_expr("quote { x }");
        assert!(got.starts_with("quote {"), "{got:?}");
        assert!(got.ends_with('}'));
    }

    #[test]
    fn antiquote_renders_tilde_parens() {
        // Antiquote only occurs inside a quote; wrap it.
        let got = fmt_expr("quote { ~(y) }");
        assert!(got.contains("~("), "{got:?}");
        assert!(got.contains(')'));
    }

    // ---- Perform / Resume -------------------------------------------

    #[test]
    fn perform_zero_args_renders_paren_pair() {
        let got = fmt_expr("perform E::op()");
        assert!(got.starts_with("perform "), "{got:?}");
        assert!(got.ends_with("()"));
    }

    #[test]
    fn resume_keyword_plus_operand() {
        let got = fmt_expr("resume x");
        assert_eq!(got, "resume x");
    }
}
