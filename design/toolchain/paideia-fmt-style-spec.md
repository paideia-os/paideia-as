# paideia-fmt Style Specification

**Issue:** [#1561](https://github.com/PaideiaOS/paideia-as/issues/1561)
(FMT-0 — Wave 61 paideia-fmt completion chain head).
**Status:** v1 — settles every style decision the subsequent 20 FMT issues
depend on. Non-authoritative on parsing (that is the elaborator spec) and
non-authoritative on tool-surface (that is FMT-11/FMT-12). This spec is the
contract between the formatter's output and a human reviewer: *given a
parseable `.pdx` file, what does the canonical formatting look like?*

**Audience.** (a) Future FMT-N implementors, who read this to decide a
specific rule without ambiguity; (b) humans reviewing a `paideia-as fmt`
diff on a PR; (c) editor plugins and LSP clients that re-use the engine.

**Scope.** Every rule here is testable: it either produces an observable
difference in the formatted output, or constrains it (e.g. the max-width
guarantee). Rules that are not testable are omitted. Priorities between
rules are explicit — when two rules conflict on a shape, the earlier
section wins unless an exception is called out in the later section.

**Non-goals.** This document does not specify (a) the IR used internally
(FMT-2), (b) comment-attachment mechanics beyond the user-visible outcome
(FMT-9), (c) the CLI flag surface beyond default-value references
(FMT-11/FMT-12), (d) semantics of the formatted language itself (that is
`design/toolchain/syntax-reference.md` plus the per-feature design docs).

---

## 1. Fundamental invariants

The formatter is a function `fmt : Source -> Source` on parseable input.
It MUST satisfy, over the entire paideia-as `.pdx` corpus (`accept/`,
`examples/`, `pdx/`, `fixtures/`, `tests/`):

1. **Parseability preservation.** `parse(fmt(src))` succeeds whenever
   `parse(src)` succeeded, and the resulting ASTs are structurally equal
   (ignoring trivia and spans). Verified by FMT-14's corpus harness.
2. **Idempotence.** `fmt(fmt(src)) == fmt(src)` byte-for-byte. Verified by
   FMT-13's corpus harness.
3. **Comment preservation.** Every `Trivia::LineComment`, `BlockComment`,
   `DocLineComment`, `DocBlockComment` in the input appears exactly once
   in the output, with the same text and attached to the same AST node
   (defined by §9). Verified by FMT-15's corpus harness.
4. **Max-width best-effort.** No line exceeds the configured `max_width`
   (default 100) *unless* the line contains a single atomic token (a long
   string literal, a long identifier, a long path, a `///` doc comment the
   user authored) that cannot be split. Over-length lines are permitted
   only when every alternative formatting would introduce a hard syntax
   error or lose comment attachment.
5. **Lossless fallback.** If `parse(src)` fails, `fmt(src)` falls back to
   the pre-FMT-3 text pass (whitespace strip + blank-line cap + glyph
   substitution) and returns a byte-stable result. The formatter never
   corrupts a file that the parser refuses.
6. **Determinism.** For a given `(src, opts)` pair, `fmt(src, opts)` is
   byte-identical across runs, machines, OS, and crate rebuilds. No
   `HashMap` iteration, no system time, no PID, no locale.

Rules in §§3–14 refine but never override §1.

---

## 2. Width and indentation

### 2.1 `max_width` = 100 columns

**Default.** `FormatOptions::max_width = 100`.

**Rationale.**
- 80 columns is historical (VT100 glass TTY). Our common split is a
  code+LSP+terminal three-pane layout at 1600px wide, which gives ~110
  columns per editor pane on a 10-pt mono font — 100 fits comfortably.
- `.pdx` has pervasive effect-row + capability-row annotations:
  `!{RawMem, IO} @{paideia.raw_mem, paideia.io}` is 48 columns on its
  own. Adding a one-token signature (`(u64) -> u64`) and a left-hand
  `let name : ` already consumes 70+ columns before any content. 80 is
  death-by-wrap for the paideia-as surface.
- Rustfmt default is 100. Linux kernel moved from 80 to 100 in 5.7.
  Prettier is 80 by language, 100 by codebase convention. Settling on 100
  aligns with modern Rust-adjacent practice.

**Override.** `max_width` is a stable `FormatOptions` field (FMT-11). The
`.pdxfmt` manifest key is `[fmt] max_width = <N>` for 60 ≤ N ≤ 200. Values
outside that range emit a config diagnostic and fall back to 100.

### 2.2 `indent_width` = 2 spaces; tabs forbidden

**Default.** `FormatOptions::indent_width = 2`, `use_tabs = false`.

**Rationale.**
- 2-space is the de-facto convention in the existing `.pdx` corpus; every
  `examples/*.pdx` file uses 2-space indent. A 4-space default would
  force a corpus-wide rewrite with no readability win.
- At `max_width = 100`, deep nesting (match inside unsafe inside fn inside
  module) reaches 8 levels — at 4-space that is 32 columns of indent
  before any content. 2-space keeps 84 columns of content width at depth
  8, which is enough for a typical pattern.
- Tabs are forbidden in output *regardless* of input, because tab width
  renders inconsistently across terminals and because our max-width
  invariant becomes undecidable with user-visible tabs.

**Override.** `FormatOptions::indent_width` MAY be set to 2 or 4 (no
other values). `use_tabs = true` is reserved for a future opt-in path
with a hard-coded 1-column tab assumption; it is NOT enabled in v1 and
the config key is a no-op warning in `.pdxfmt`.

**Grandfathering.** Any input containing `\t` is normalized to spaces by
the FMT-3 text-level fallback *and* by the AST-level walker. There is no
corpus file with tabs; this rule is purely defensive.

### 2.3 Continuation indent = +2 (one level)

A soft-break under FMT-2's `Group` adds exactly one `indent_width` to the
current column. Multi-level continuation (e.g. a long effect row inside a
long function signature inside a long module-let) stacks additional
levels, each `indent_width` wide.

Example (max_width = 100, forcing a wrap on the effect row):

```pdx
let read_then_log : (u64) -> u64
  !{RawMem, IO}
  @{paideia.raw_mem, paideia.io} =
    fn (addr : u64) -> 0
```

The `!{…}` and `@{…}` land at column 2 (one continuation level); the `=`
breaks and `fn` lands at column 4 (two levels, matching the inside-fn
convention from §4.2).

### 2.4 No trailing whitespace, exactly one trailing newline

Every line is `str::trim_end`-equivalent. The file ends in `\n` and no
blank-line run at the file tail. Blank-line runs anywhere in the file are
capped at 2 consecutive (one blank line between top-level items is
customary; two is permitted as a stronger section break; three or more
collapses to two).

---

## 3. Trailing commas

### 3.1 Multi-line: trailing comma MANDATORY

In any multi-line delimited construct — struct literal, record pattern,
tuple expression spanning multiple lines, match arm block, let-block
field list, enum variant list, function parameter list, generic
parameter list, trait bound list, effect-row body, capability-row body,
array/slice literal — the final element MUST be followed by a trailing
comma on its own tail (not on a new line).

```pdx
Point {
  x: 0,
  y: 0,            // mandatory trailing comma
}

enum Color {
  Red,
  Green,
  Blue,            // mandatory trailing comma
}

match n {
  0 => 1,
  _ => n * fact(n - 1),   // mandatory trailing comma
}
```

**Rationale.** Diff hygiene: adding a new variant does not touch the
previous line, which keeps VCS blame readable. Also aligns with Rust,
Swift, Go-generics, and Pythonic `black`/`ruff` convention.

### 3.2 Single-line: trailing comma FORBIDDEN

A one-line struct literal, tuple, or arm list does not get a trailing
comma. `Point { x: 0, y: 0 }` is canonical; `Point { x: 0, y: 0, }` is
rewritten to drop the comma.

### 3.3 Enum variant payloads

Tuple-variant payloads follow §3.1/§3.2: `Some(T)` (one-line, no comma)
versus a hypothetical multi-line payload. Record-variant payloads follow
record rules (§5.2).

### 3.4 Grandfathering

Current `.pdx` corpus is inconsistent:

| File | Convention |
|---|---|
| `examples/05_patterns.pdx` | No trailing commas in match arms |
| `examples/16_factorial.pdx` | Trailing commas present |
| `examples/17_fibonacci.pdx` | Trailing commas present |

The spec normalizes to §3.1. The formatter will rewrite `05_patterns.pdx`
on first `fmt` run; this is the intended corpus-cleanup.

---

## 4. Function and binding shape

### 4.1 `let` bindings

- Module-level `let` is always on its own line, terminated by newline (no
  `;`). The `=` is on the same line as the LHS unless the RHS forces a
  wrap by exceeding `max_width`.
- When wrapped, the first break is before `=` with the `=` moved to the
  start of the next line is **not** chosen; instead, the `=` stays at
  end-of-line and the RHS indents by one level:

```pdx
let factorial_acc : (u64, u64) -> u64 =
  fn (n : u64) (acc : u64) -> match n {
    0 => acc,
    _ => factorial_acc(n - 1, n * acc),
  }
```

- The `=` always has one space on each side.
- A type annotation (`let name : Type = …`) uses the colon formatting
  from §6.3: one space before, one space after. Rationale: the colon
  binds a *name* to a *type*, which is a different grammatical role from
  the field-colon case in §5; the OCaml precedent uses space-around-colon
  for named bindings.
- `pub let` renders with one space between keywords: `pub let name = …`.
- `let mut` renders with one space: `let mut counter = 0`.

### 4.2 `fn` lambdas

- The space after `fn` and before the parameter list is **mandatory**:
  `fn (x : u64) -> …`. This matches 15 of 20 `examples/*.pdx` files and
  avoids the visual confusion between a `fn`-keyword and a function-call
  `fn(...)` (which is not legal syntax, but reads adjacently).
- Parameters use parameter-colon formatting (§6.3): `fn (x : u64)`
  with space on each side of `:`.
- Multi-arg curried form: `fn (x : u64) (y : u64)` (one space between
  closing `)` and the next `(`).
- `|x|` pipe-lambda is preserved as-is when the body is a single
  short expression. Multi-arg pipe-lambdas (`|x, y|`) are formatted with
  one space after each comma, no space before, matching Rust closure
  syntax. The pipe-lambda is NEVER expanded into a long `fn (...)` form;
  it is a user authorship choice preserved by the formatter.
- `-> body` renders with one space around the arrow regardless of body
  complexity.
- When a lambda's body is a `match`, the `match` starts on the same line
  as `->` and the arms indent at +2 from the `let` keyword, not from
  `match`:

```pdx
let rank = fn (c : Choice) -> match c {
  First  => 0,
  Second => 1,
  Third  => 2,
}
```

### 4.3 Grandfathering: parameter-colon spacing

The corpus is **inconsistent**:

| Form | Example | Files using it |
|---|---|---|
| `addr : u64` (space-space) | `examples/09_effects.pdx` | 15/20 |
| `addr: u64` (no space before) | `examples/05_patterns.pdx` | 5/20 |

The spec chooses `addr : u64` as canonical. Rationale: dominant usage,
and consistency with the type-annotation form `let x : T` in §4.1 — in
both cases the colon separates a *name-binding* from a *type*, and the
space-around form reads less dense in a long parameter list. The
formatter will rewrite the 5 minority files.

Trade-off disclosed: Rust uses the dense form (`x: u64`). We diverge. The
divergence is deliberate and settled here.

---

## 5. Containers (records, enums, modules, structures)

### 5.1 One-line vs multi-line threshold

A container (record literal, enum variant list, module body, structure
body, tuple, array literal, slice literal) is formatted **one-line** if
and only if its full one-line rendering fits in `max_width` starting
from the column where its opening delimiter would land, taking into
account any trailing suffix on the same line (type annotation, effect
row, assignment).

Otherwise it is formatted **multi-line**, with one element per line,
one indent level deeper than the opening-delimiter column.

```pdx
let origin = Point { x: 0, y: 0 }         // one-line (fits in 100)

let frame = Frame {                       // multi-line (would exceed)
  origin: Point { x: 0, y: 0 },
  scale: 1,
}
```

### 5.2 Record literals

- Field colon has **no space before, one space after**: `x: 0`. This
  matches 100% of the corpus and is the opposite of the parameter-colon
  rule in §4.3. The two colon roles are distinct: parameter-colons bind
  a value name to a type (a *static* relation), field-colons bind a key
  to a value (a *data* relation). The data-colon follows Rust, Swift,
  and JSON convention; the type-colon follows OCaml/Haskell.
- One space after each comma in one-line form.
- Multi-line form: one field per line, trailing comma mandatory (§3.1).

### 5.3 Struct declarations

```pdx
struct Point {
  x: u64,
  y: u64,
}
```

- `struct <Name> {` on one line. The `{` has one space before it.
- One field per line unless the whole declaration fits in `max_width`
  (then it may be one-line, but this is NOT canonical for structs with
  2+ fields — the multi-line form is always chosen for 2+ fields).
- A unit struct `struct Marker;` keeps the semicolon.
- Generic parameters go directly after the name with no space:
  `struct Pair<A, B> {`. Generic bounds use one space after each `:`:
  `struct Container<T: Clone> {`.

### 5.4 Enum declarations

```pdx
enum Color {
  Red,
  Green,
  Blue,
}
```

- One variant per line is the canonical form, even for 1-variant enums
  and even when a one-line rendering would fit. Rationale: enums grow,
  one-line form reverses into multi-line on every variant add, so skip
  the churn.
- Payload shapes: unit (`Red`), tuple (`Some(T)`), record
  (`Point { x: u64, y: u64 }`). Record-payload uses §5.2 format for its
  interior.

### 5.5 Module / structure bodies

- `module Name = structure {` on one line; close `}` on its own line.
- Items inside a `structure {}` are each on their own line, separated by
  one blank line between top-level items of different kinds (two `let`s
  may be adjacent without a blank line; a `let` followed by a `fn`-shaped
  `let` of a different concern SHOULD have a blank line, but this is a
  authorship hint and not formatter-enforced).
- A `functor` body follows the same rules as `structure`.

### 5.6 Tuples and arrays

- `()` and `(x,)` (unit and 1-tuple) are left on one line.
- `(a, b)` and `(a, b, c)` are one-line if they fit.
- Multi-line tuples use one element per line with trailing comma.
- Array literals `[1, 2, 3]` follow the same rule. `[]` is one-line.

---

## 6. Patterns, types, and symbols

### 6.1 Pattern formatting

- **Wildcard** `_`: never spaced; always a bare `_`.
- **Literal patterns**: as written (`0u64`, `"foo"`, `true`).
- **Binding** `name @ Pat`: one space around `@`.
- **Or-pattern** `A | B | C`: one space around each `|`. If the whole
  pattern exceeds `max_width`, break before each `|` with the `|` at the
  start of the next line at the same indent as `A`.
- **Range pattern** `a..=b` or `a..b`: no spaces around `..` / `..=`.
- **Slice pattern** `[a, b, ..rest]`: standard array-literal rules.
- **Tuple pattern** `(a, b)`: standard tuple rules (§5.6).
- **Record pattern** `Point { x, y }`: field punning uses just the
  name; renamed binding uses `field: binding` with the data-colon
  format (§5.2); rest-binding is `..`.

### 6.2 Type formatting

- **Path type** `Mod::Sub::Name`: no spaces around `::`.
- **Function type** `(u64, u64) -> u64`: one space around `->`; parameter
  tuple uses standard tuple rules; parameter tuples with one element
  render as `(T,)` (comma) when the parser requires disambiguation and
  as `(T)` (no comma) when the parser does not — follow the parser's
  surface. For the formatter v1, always emit `(T)` for 1-arg function
  types since the parser never requires the comma there.
- **Effect row** `!{E1, E2}`: no space between `!` and `{`; no space
  inside the braces at the edges; one space after each comma.
  `!{}` is the canonical empty effect row.
- **Capability row** `@{paideia.raw_mem}`: same spacing as effect rows.
  Dotted capability names have no spaces around `.`.
- **Memref type** `memref<Shape>`: angle-bracket parameters, no inside
  spaces.
- **Tuple type** `(u64, u64)`: standard tuple rules.
- **Array type** `[u64; N]`: one space after `;`, no space before.
- **Reference types** `&T`, `&mut T`: no space between `&` and the type;
  one space after `mut`.
- **Pointer type** `*T`, `*mut T`: no space between `*` and the type;
  one space after `mut`.
- **Generic application** `Vec<T>`, `HashMap<K, V>`: no space inside
  angle brackets; one space after each comma.
- **Trait bounds** `T: Clone + Debug`: one space around `:`, one space
  around `+`. Multi-line bound lists break before each `+`.

### 6.3 Colon roles — summary table

| Role | Form | Example | Section |
|---|---|---|---|
| Name-to-type (binding, param) | ` : ` (space-both) | `x : u64` | §4.3 |
| Key-to-value (field, record) | `: ` (space-after) | `x: 0` | §5.2 |
| Trait bound | `: ` (space-after) | `T: Clone` | §6.2 |
| Type ascription in expr | ` : ` (space-both) | `(42 : u64)` | §7.1 |

Rationale for the split: name-to-type participates in a *declaration*
(what kind of thing is this?), while key-to-value participates in a
*construction* (what are the values of this?). Visual separation of the
two roles makes a function parameter and a record field distinguishable
at a glance, which pays off in long effect-row signatures where both
appear nearby.

---

## 7. Expressions

### 7.1 Operator precedence and spacing

- Binary operators (`+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`,
  `>=`, `&&`, `||`, `&`, `|`, `^`, `<<`, `>>`): one space on each side.
- Unary operators (`-`, `!`, `~`): no space between the operator and the
  operand (`-x`, `!flag`).
- Dereference `*p` and address-of `&x` / `&mut x`: no space between the
  sigil and the operand.
- Index `xs[i]`, call `f(x)`, field `p.x`, method `p.foo()`: no space
  before `[`, `(`, or `.`.
- Type ascription `(expr : Type)`: parentheses required by parser; use
  space-around-colon (§6.3 row 4).

### 7.2 Control-flow expressions

- `if cond then_expr else else_expr` renders one-line only if it fits;
  otherwise `if cond {`, newline, body, newline, `} else {`, newline,
  body, newline, `}`.
- `match scrutinee { arms }` body follows §8.
- `loop {}`, `while cond {}`, `for pat in iter {}` render with one space
  before and after `{` and body on new lines unless the whole form fits
  on one line (rare).
- `break`, `continue`, `return expr`: one space after the keyword if an
  operand follows; bare otherwise.

### 7.3 Lambdas and closures

See §4.2.

### 7.4 Call and application

- `f(x)` and `f(x, y, z)`: no space between callee and `(`; one space
  after each comma.
- Multi-line call arguments: one argument per line, trailing comma
  mandatory, closing `)` on its own line at the callee's column.
- Method chains `a.b().c().d()` are left on one line if they fit.
  Otherwise, break before each `.`, with each `.method()` call at the
  same indent one level deeper than `a`:

```pdx
let total = iter
  .map(|x| x + 1)
  .filter(|x| x > 0)
  .fold(0, |acc, x| acc + x);
```

---

## 8. Match expressions

### 8.1 Arm format

Each arm is `pattern => body`. In multi-line arm bodies, `pattern =>` sits
on its own line and the body indents at +2.

```pdx
match c {
  First  => 0,
  Second => 1,
  Third  => 2,
}
```

### 8.2 `=>` alignment within an arm block (ON by default)

When all arms in a block fit on one line each, the `=>` columns MUST be
aligned by padding the pattern with trailing spaces. The alignment column
is `max(len(pattern)) + 1`.

**Exceptions.**
- If the longest pattern exceeds `max_width - 10`, alignment is dropped
  for that block (every arm starts without padding).
- Arms whose body is multi-line do not participate in alignment of their
  own `=>`; they place the `=>` directly after the pattern with one
  space.
- Guard clauses `pattern if guard => body` extend the pattern for
  alignment purposes to include the guard.

**Rationale.** Readability: a column of `=>` turns a match block into a
visual lookup table. The exception for very wide patterns prevents a
single long pattern from pushing the entire block off-screen.

### 8.3 Arm body wrapping

If a single arm body exceeds `max_width`, it breaks after `=>` with the
body on the next line at +2 indent, in a block:

```pdx
match tok {
  Short(x) => x,
  Long(name, span) =>
    error_at_span(span, format!("long form '{}' not supported here", name)),
}
```

A trailing comma still follows the last line of the multi-line body.

---

## 9. Comments and doc comments

### 9.1 Attachment rules (user-visible outcome)

Three attachment categories, matching the FMT-1 CST design:

- **Leading**: a line/block comment appearing immediately before an item,
  with no blank line separating them, attaches to that item. It is
  emitted at the item's indent level, on its own line, above the item.
  Multi-line leading comment groups preserve their internal line order
  and spacing.
- **Trailing**: a line comment appearing on the same line as a token (at
  end of line, after the last significant token on that line) attaches
  to the AST node that owned that token. It is emitted at the end of
  that same line in the output, with at least 2 spaces of separation
  from the preceding code.
- **Inner**: a comment appearing after a container's opening delimiter
  and before its first element attaches to the container. It is emitted
  on its own line immediately inside the delimiter, at +1 indent.

Comments with a blank line separating them from the next item attach to
*nothing*; they are "floating" and are emitted at the preceding item's
indent level, preserving their blank-line separator (capped at 2 blanks
per §2.4).

### 9.2 Doc-comment attachment

- `///` lines group with the item **immediately following** them at the
  same scope level, with no blank line between the last `///` and the
  item. The group is emitted above the item, each `///` on its own line.
- `//!` lines attach to the enclosing item (`//!` inside a module body
  attaches to that module, not to the following item). The group is
  emitted at the top of the enclosing item's body.
- `/** ... */` and `/*! ... */` block doc-comments follow the same
  attachment rules as the `///` and `//!` line forms respectively.
- Exact wording is preserved: no reflow, no capitalization change, no
  punctuation normalization. The only change is the common-leading-space
  pattern inside `/** */` is left untouched.

### 9.3 Line and block comments inside expressions

A comment inside a container (e.g. between two fields of a record
literal) must round-trip. The formatter attaches it to the *next* field
in the container, emitting it on its own line immediately before that
field. Trailing comments on the same line as a field are preserved on
that same line in the output.

### 9.4 Blank-line policy

- At most 2 consecutive blank lines anywhere in the file (same rule as
  §2.4).
- Blank lines inside a container (between record fields, between enum
  variants, between match arms) are preserved but capped at 1 blank line
  (not 2) — the container visual density matters more.
- The formatter never inserts a blank line that was not present in the
  input *except* at the file-end-trailing-newline position (§2.4).

---

## 10. Attributes

### 10.1 Position

Attributes on an item go on their own line **above** the item they
attribute, at the item's indent column. One attribute per line.

```pdx
@abi("sysv")
@endian(be)
let read_u32_be : (*u8) -> u32 !{RawMem} @{paideia.raw_mem} =
  fn (p : *u8) -> …
```

Multiple attributes stack in author order; the formatter never reorders
attributes (an attribute order change is semantic in some cases, e.g.
`@packed` + `@endian`).

### 10.2 Argument spacing

- `@abi("sysv")`: no space between `@` and name; no space between name
  and `(`; no space inside `(…)` except after each comma.
- `@endian(be)`, `@endian(le)`: same spacing. Bare identifier arguments
  do not take quotes.
- `@interrupt(x86_64, with_error_code)`: multi-arg form uses one space
  after each comma.
- `@packed`: argument-less attributes have no `()`.
- `@link_section(".text.boot")`: string argument quoted.
- `@jump_table`, `@jump_table(dense)`: argument-less or single-id form.
- `@align(16)`, `@ring(0, 3)`: numeric arguments bare, one space after
  each comma.
- `@fingerprint("…")`, `@dsl_parser("…")`: string argument quoted.

### 10.3 In-type and in-expression attributes (future)

Reserved. v1 only formats top-level item attributes. Inner attributes
(`#![attr]`-shaped) are preserved verbatim at file top with blank-line
separation from the first item.

---

## 11. Unsafe blocks

Per `examples/15_unsafe.pdx`, the canonical unsafe-block shape has four
required fields in fixed order: `effects:`, `capabilities:`,
`justification:`, `block:`.

### 11.1 Field alignment (ON by default — OPTION C adaptive)

All four field-colons are aligned by padding each key with trailing
spaces to the longest key's length (`justification` at 13 chars → every
colon lands at column `base + 14`, so content starts at `base + 15`).

OPTION C (FMT-10, paideia-as#1574) is **adaptive**: `block:` participates
in the alignment just like the other three fields when its body is
multi-line (the common case — assembly instruction sequences hard-break),
because the first rendered row of a multi-line body is just `{` and
padding never pushes the first line past `max_width`. For a single-line
`block:` body (empty `{}` or a one-instruction body that fits flat),
the formatter checks whether `base_indent + "block:" + pad + " " +
body_width` would exceed `max_width`; if yes, `block:` demotes to
single-space-after-colon for that one field, keeping the other three
aligned. This resolves the §11.1 ambiguity between the literal rule
("pad all four") and the earlier spec example (which padded only three).

```pdx
unsafe {
  effects:       { RawMem },
  capabilities:  { paideia.raw_mem },
  justification: "Raw memory access: mov r64, r64 in unsafe block.",
  block:         {
    mov rax, rdi
  },
}
```

Demote case (hypothetical, `max_width = 20`):

```pdx
unsafe {
  effects:       { },
  capabilities:  { },
  justification: "j",
  block: {},
}
```

The adaptive alignment is gated by `FormatOptions::align_unsafe_fields`
(default `true`); setting the flag to `false` emits every field with a
single space after the colon. FMT-11 will surface this as the
`[fmt] align = "off"` key in `.pdxfmt`. The parallel knob for match-arm
`=>` alignment is `FormatOptions::align_match_arms` (default `true`,
same spec gap resolution: honour the rule by default, flag off for
users who prefer unpadded output).

The leading `unsafe {` is on the same line as the preceding `->` or `=`.
Trailing comma mandatory on each of the four fields including `block:`.

### 11.2 Field value formatting

- `effects: { … }` and `capabilities: { … }` use effect/capability-row
  interior formatting (§6.2), but with a leading space inside the
  braces: `{ RawMem }` (one space after `{`, one space before `}`).
  This diverges from §6.2 effect rows to match the corpus.
- `justification` value is a string literal, single-line if it fits in
  `max_width - current_indent`, otherwise rendered across multiple
  lines as a raw multi-line string if the parser supports it (not v1 —
  v1 keeps it single-line; break at commit boundary and widen `max_width`
  by exception if needed, documented in a trailing-comment on that
  line).
- `block: { … }` body is an assembly-instruction sequence, each
  instruction on its own line at +2 from `block:`. Instruction mnemonic
  and operands follow the assembly-reference convention (not this spec).

### 11.3 Nested unsafe

Nested unsafe blocks indent at +2 from their parent. The parent and
child use independent alignment columns for their own four fields.

---

## 12. Macros and directives

### 12.1 `macro Name { … }` declarations

Each rule is on its own line within the macro body. Rule arrow renders
as `pattern => body` with one space on each side of `=>`. Multi-rule
alignment of `=>` follows the match-arm rule (§8.2).

### 12.2 Macro invocation

`Name!(args)`: no space between name and `!`; no space between `!` and
`(`. Argument list follows call formatting (§7.4). Block-form
`Name! { … }` has one space before `{`.

### 12.3 `@macro_expand([…])` directive

Internal-only (per `parse_macro_expand_directive.rs:64`) but
formatted as-is: `@macro_expand([<name>, "<input>"])` on one line.

### 12.4 `use` / `import` declarations

- `use <path>;` single-line. Terminal `;` is kept.
- `use <path>::{A, B, C};` single-line if it fits.
- Multi-line use lists: one import per line with trailing comma, matching
  §5.1 threshold.
- Nested group `use a::{b, c::{d, e}};` is one-line if it fits; otherwise
  breaks at the outermost `{`.

---

## 13. Glyph policy

### 13.1 Default: PRESERVE

The default `glyph_style = Preserve` means the formatter does NOT rewrite
`->` to `→` or `=>` to `⇒` or `\\` to `λ`. Author intent is preserved.

### 13.2 Opt-in: Ascii or Unicode

A workspace `.pdxfmt` may set `[fmt] glyph = "unicode"` to normalize ALL
ASCII arrow/lambda forms to their Unicode equivalents, or `"ascii"` to
normalize the other direction. The pairs are fixed:

- `->` ↔ `→`
- `=>` ↔ `⇒`
- `/=` ↔ `≠`
- `forall` ↔ `∀`
- `exists` ↔ `∃`
- `\` ↔ `λ` (lambda escape)

Rewriting is scalar (per `paideia_fmt::ascii_to_unicode`/`unicode_to_ascii`
today) and never changes string-literal or comment contents.

### 13.3 Per-file override

A file-top directive `//! fmt-glyph: preserve` (or `ascii` or `unicode`)
overrides the workspace setting for that single file.

---

## 14. CLI defaults (summary, authoritative in FMT-11/FMT-12)

- `paideia-as fmt FILE…` — in-place rewrite of each listed `.pdx` file.
- `paideia-as fmt --check FILE…` — exit 1 if any file would change; no
  write.
- `paideia-as fmt --diff FILE…` — unified diff to stdout.
- `paideia-as fmt --stdin` — read stdin, emit stdout.
- `paideia-as fmt --list-different FILE…` — print names that would
  change, nothing else (added in FMT-12).
- `paideia-as fmt --glob 'crates/**/*.pdx'` — glob expansion (FMT-12).

Defaults all come from this spec: `max_width = 100`, `indent_width = 2`,
`use_tabs = false`, `max_blank_lines = 2`, `glyph_style = Preserve`.

A `.pdxfmt` in the workspace root is discovered by walking parent
directories (git-root-stop); keys documented in FMT-11.

---

## 15. Grandfathered inconsistencies in the current corpus

The following corpus inconsistencies are deliberately normalized to the
rules above. The formatter's first-run diff will touch each site; this
is the intended cleanup and SHOULD be reviewed as a single bulk PR
rather than slipping into feature PRs.

1. **Parameter-colon spacing** (§4.3). Five `examples/*.pdx` files use
   `fn (x: T)` (no space before colon); 15 use `fn (x : T)`. Normalize
   to the space-around form.
2. **Match-arm trailing commas** (§3.4). `examples/05_patterns.pdx` and
   `examples/06_loops.pdx` lack trailing commas on match arms. Normalize
   to always-trailing.
3. **Hand-aligned `=` across adjacent `let`s** (`examples/01_hello.pdx`
   uses `let answer  = 42` with double-space before `=` to align with
   `let magic   = …`). The spec is **silent on cross-line `=` alignment**
   because an align-the-eqs rule turns every edit of one line into a
   realign of its neighbors — large diff churn for a small readability
   win. The formatter normalizes to single-space: `let answer = 42`,
   `let magic = …`. Humans who want alignment do it manually and the
   formatter will undo it on next run. This is a deliberate loss.
4. **Unsafe-block field alignment** is preserved and reinforced (§11.1).
   `examples/15_unsafe.pdx` already uses it.
5. **Lambda `fn ` vs `fn(`.** Normalize to `fn (` with a space (§4.2).

---

## 16. Open questions deferred past v1

- **Signature-line `=` placement.** v1 keeps `=` at end-of-line on wrap.
  An alternative convention (OCaml-style `and`-chains) puts `=` at the
  start of the next line. Deferred; the current convention wins because
  it matches rustfmt.
- **Comment reflow.** v1 never reflows comment text. If users
  consistently request it, FMT-N+1 might add an opt-in `--reflow-comments`
  flag; v1 ships without.
- **Doc-comment Markdown-aware formatting.** Deferred — would require a
  Markdown AST and is well beyond v1's remit.
- **Attribute reordering.** Reserved. The formatter never reorders
  attributes in v1 because the semantic effect of order (e.g.
  `@packed` + `@endian`) is not fully specified in the attribute
  design docs.

---

## 17. Three most contested decisions

For the FMT-N implementors who inherit this spec: these are the three
decisions that cost the most argument during authoring, with the losing
side's case so future readers can revisit if needed.

1. **`max_width = 100`.** 80 columns loses to 100 because of effect-row
   plus capability-row width (§2.1). The 80-column argument: diff-review
   on narrower screens. Settled 100 because effect-row surface is a
   persistent, user-facing shape — it has to fit.
2. **Trailing-comma ALWAYS in multi-line.** The no-comma camp argues for
   lower visual noise. The always-comma camp wins because adding a
   variant/field is a one-line diff with trailing comma and a two-line
   diff without. VCS hygiene beats punctuation purity.
3. **Parameter-colon vs field-colon spacing split (§6.3).** The
   consistency camp argues both should be `name: value`. The split camp
   wins because the two colons play distinct grammatical roles and
   because the dominant corpus convention (15/20 files) uses the split.
   This is paideia-as's largest deliberate divergence from Rust in the
   formatting space.
