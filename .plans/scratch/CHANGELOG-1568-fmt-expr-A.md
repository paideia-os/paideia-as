# CHANGELOG — paideia-as#1568 (FMT-5a: expression formatters bucket A)

## Scope
Replaces the walker's `format_expr` scaffold stub for five expression
categories: Primary (literal / path / tuple / array / range / uninit /
string / byte-string / inline-bytes / inline-str), Postfix (field-access
/ `?` / cast), Call (`f(a, b, c)` with trailing-comma break shape),
BinOp (`Infix` — every binary operator token), and Prefix (`Prefix` /
`Borrow` / `Deref`). Bucket B (match / if / loop / for / block /
lambda / with-handler / unsafe / perform / resume / handler-value /
quote / antiquote / functor-app / pack / unpack / let-module /
record-cons) still falls through to `fallback_slice` — FMT-5b will claim
those.

## Changes

### New module
- `crates/paideia-fmt/src/format_expr.rs` (new, ~560 LOC)
  - `pub fn format_expr(ctx, nid, kind) -> Doc` — dispatcher; non-
    exhaustive match (bucket B + any new variant falls through to
    `fallback_slice`).
  - Per-kind formatters: `format_path`, `format_tuple`,
    `format_array_lit`, `format_array_repeat`, `format_range`,
    `format_prefix`, `format_borrow`, `format_deref`,
    `format_field_access`, `format_postfix_op`, `format_cast`,
    `format_call`, `format_infix`.
  - Shared `delimited_list(open, close, elems)` helper for tuple /
    array / call multi-line shape (`group([open, nest(2,
    [softline, sep]), if_break(",", nil), softline, close])`).
  - Structural paren-insertion based on parser-mirroring left/right-bp
    (`infix_bp_of`, `child_left_bp`, `child_right_bp`,
    `format_child_as_left/right`, `format_child_at`). Rule:
    * left-operand parens iff `child.right_bp < parent.left_bp`,
    * right-operand parens iff `child.left_bp < parent.right_bp`,
    * receiver (prefix / cast / postfix / call callee) parens iff
      `child.right_bp < demand`.

### Updated module
- `crates/paideia-fmt/src/walker.rs` — `format_expr` body now
  delegates to `crate::format_expr::format_expr(ctx, nid, data.kind)`.
  Everything else (dispatcher, trivia helpers, fallback contract) is
  unchanged.
- `crates/paideia-fmt/src/lib.rs` — added `pub mod format_expr;`.

### New tests (23 inside `format_expr::tests`)
- Primary: literal int, literal string, single-segment path,
  multi-segment path, 2-tuple, 1-tuple-trailing-comma, array literal,
  array repeat.
- Call: zero-arg, three-arg flat, multi-line break with trailing comma.
- Infix: flat add, left-assoc same-prec no parens, mul-tighter-than-add,
  precedence-driven paren preservation on lhs (`(a + b) * c`),
  operator-at-start-of-continuation on break.
- Prefix: `-x`, `!flag`, `&x`, `&mut x`, `*p`, `-(a + b)` keeps parens.
- Postfix: field access, method call (`.m()`), `result?`.

## Deferred
- `Await`: no `ExprData::Await` variant in phase-1 — nothing to
  dispatch on. Falls into bucket B until the parser lands it.
- `Underscore` / `Paren`: not AST variants either — `_` is pattern-side
  only, and `(x)` is unwrapped by the parser. See the "precedence /
  paren rule" note in the module header.
- `AssignOp` compound assignments (`+=` etc.): the lexer does not emit
  these tokens today; nothing to format.
- Bucket B of ExprData falls through to `fallback_slice` by design.

## Precedence / paren gaps settled
- **User-redundant parens dropped.** The parser already unwraps
  grouping `(e)` with no trailing comma, so a reformatted
  `a + (b * c)` round-trips as `a + b * c` (semantically identical;
  parses to the same tree). Preserving user-written redundant parens
  would require a source-span probe inside the parser or a FMT-9
  round-trip audit; FMT-5a emits the structural minimum. Flagged for
  FMT-10 to decide whether to add a tail-byte probe.
- **Range bp**: used the parser's dedicated `RANGE_BP = 55` so a
  reformatted `a..b + c` wraps `a..b` correctly (`+`'s left_bp is
  100 > 55 → the range operand gets parens).
- **`as` paren-wrapping**: `CAST_BP = 105` matches the parser's cast
  bp; `(a + b) as T` keeps parens, `a * b as T` does not.

## Build status
Not run — main should invoke `bash tools/build.sh` and re-invoke me
with the error tail if it fails.
