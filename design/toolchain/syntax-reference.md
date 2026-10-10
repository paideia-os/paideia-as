# paideia-as Syntax Reference

**Status:** Stub (resurrected FMT-0, 2026-10-10). Enumerates the current
parser-accepted `.pdx` surface, production by production, with one
example and one parser citation per shape.

**What this document is.** A navigation aid for anyone who has to touch
the parser, the formatter, or the pretty-printer. It describes *what the
parser accepts today*. Each production links to the parser function that
implements it, so the parser is the ground truth and this file is a
one-stop index onto it.

**What this document is NOT.** It is not the elaborator spec; type and
effect semantics are covered by the per-feature design docs
(`design/toolchain/effect-rows.md`, `generics-and-traits-phase4.md`,
`unsafe-item.md`, etc.) and the elaborator design (TBD). It is not the
formatter style spec; see `paideia-fmt-style-spec.md`. It is not a
BNF/EBNF — the parser is a hand-written Pratt-style recursive descent
and no single grammar artifact exists; this file enumerates the shapes
that drive it.

**Numbered sections.** The pre-existing refs scattered across the lexer
and diagnostic crates (`syntax-reference.md §2.1`, `§3.4`, `§4`, `§5.3`,
etc.) target lexical-level shapes. This resurrected stub covers only the
**item and expression surface**; the lexical-level sections (§1–§6 in
the pre-existing refs) are placeholders here, to be filled when the
corresponding areas are audited. They are listed in §A.

---

## 1. Source file

A `.pdx` source file is a sequence of items. The parser treats the file
as a synthetic `Structure` node containing every top-level item (plus a
side-table of `@macro_expand` directives) and emits `M0306` for a second
`module` declaration in the same file.

- **Parser:** `Parser::parse_source_file`
  (`crates/paideia-as-parser/src/parse_item.rs:172`).
- **Entry dispatch:** `Parser::parse_item`
  (`crates/paideia-as-parser/src/parse_item.rs:49`) — switches on the
  leading `Token::Kind` to one of the per-item parsers below.
- **File-structure invariant:** one `.pdx` file = one structure named
  after the file basename in PascalCase (`examples/01_hello.pdx` →
  `module Hello = structure { … }`).

---

## 2. Items

The 11 top-level productions:

### 2.1 `module` declaration

```pdx
module Hello = structure {
  let answer = 42
}
```

- **Parser:** `Parser::parse_module_decl`
  (`crates/paideia-as-parser/src/parse_item/module_decl.rs:11`).
- **Variants:**
  - `module <Name> = structure { … }` — concrete module.
  - `module <Name> = functor (<Param> : <Signature>) -> <ResultSig> { … }`
    — parameterised module.
  - `module <Name> : <Signature> = …` — signature-ascribed module.
- The RHS of `=` is parsed by `parse_module_body`
  (`parse_item/module_decl.rs:57`), which dispatches to `parse_structure`
  or `parse_functor`.

### 2.2 `signature` declaration

```pdx
signature Point = sig {
  let x : u64
  let y : u64
}
```

- **Parser:** `Parser::parse_signature_decl`
  (`crates/paideia-as-parser/src/parse_item/module_decl.rs:213`).
- Members are `let name : Type` entries (no RHS). Used to constrain
  functor applications and module ascriptions.

### 2.3 `use` / `import`

```pdx
use paideia::io::{write_byte, write_u64};
import ThirdParty.Lib;
```

- **Parser:** dispatched inside `parse_item` for `KwUse` / `KwImport`
  (not surfaced in the sibling table above — the parser recognises both
  keywords; see `parse_item/module_decl.rs` + the lexer reserved-word
  table at `crates/paideia-as-lexer/src/reserved.rs`).
- **Note:** `use` and `import` are both reserved; `use` is the preferred
  item surface and `import` is retained for staged removal.

### 2.4 `pub let` and `let` (module-level bindings)

```pdx
pub let answer = 42
let magic : u64 = 0xCAFE
let mut counter = 0
```

- **Parser:** `Parser::parse_let_decl_with_visibility`
  (`crates/paideia-as-parser/src/parse_item/let_item/mod.rs:225`),
  wrapping `parse_let_decl`
  (`parse_item/let_item/mod.rs:411`).
- **Shapes:** `let <pat>`, `let <pat> : <Type>`, `let <pat> = <expr>`,
  `let <pat> : <Type> = <expr>`, with an optional leading `pub` or
  interstitial `mut`.
- **Attributes:** parsed by `parse_optional_symbol_attributes`
  (`parse_item/let_item/mod.rs:88`); covers `@abi(…)`, `@endian(…)`,
  `@interrupt(…)`, `@align(…)`, `@ring(…)`, `@link_section(…)`,
  `@atomic(…)`, `@fingerprint(…)`, `@dsl_parser(…)`, `@packed`,
  `@jump_table`, `@no_frame`, `@sret_demote`.
- **RHS forms** below in §3.

### 2.5 `pub fn` lambda (module-level anonymous function via `let`)

paideia-as has no `fn` *item* form distinct from `let`; functions are
value-level `fn (…) -> …` lambdas bound by `let`. See §3.4.

### 2.6 `effect` declaration

```pdx
effect ReadByte {
  perform read(addr: u64) -> u8
}
```

- **Parser:** `Parser::parse_effect_decl`
  (`crates/paideia-as-parser/src/parse_item/effect_cap.rs:11`).
- Declares a named effect with 1+ `perform` operations.

### 2.7 `capability` declaration

```pdx
capability paideia.raw_mem;
```

- **Parser:** `Parser::parse_capability_decl`
  (`crates/paideia-as-parser/src/parse_item/effect_cap.rs:99`).
- Declares a dotted capability name available on the effect-row surface.

### 2.8 `struct` declaration

```pdx
struct Point {
  x: u64,
  y: u64,
}
```

- **Parser:** `Parser::parse_struct_decl`
  (`crates/paideia-as-parser/src/parse_item/struct_enum.rs:15`).
- Supports generic parameters: `struct Pair<A, B> { … }`.
- Unit-struct form `struct Marker;` and tuple-struct `struct Pair(A, B);`
  are both parsed.

### 2.9 `enum` declaration

```pdx
enum Color {
  Red,
  Green,
  Blue,
}

enum Option<T> {
  Some(T),
  None,
}
```

- **Parser:** `Parser::parse_enum_decl`
  (`crates/paideia-as-parser/src/parse_item/struct_enum.rs:158`).
- Three payload shapes: unit (`Red`), tuple (`Some(T)`), record
  (`Err { code: u64, msg: String }`).

### 2.10 `trait` declaration and `impl` block

```pdx
trait Iterator<Item> {
  fn next() -> (Option Item);
}

impl Iterator for Counter {
  let next = fn (x: ()) -> 0u64
}
```

- **Parser:** `Parser::parse_trait_decl`
  (`crates/paideia-as-parser/src/parse_item/trait_impl.rs:11`);
  `Parser::parse_impl_decl`
  (`crates/paideia-as-parser/src/parse_item/trait_impl.rs:537`).
- Trait methods via `parse_trait_method` (`trait_impl.rs:168`).
- Associated types via `parse_trait_associated_type`
  (`trait_impl.rs:495`).
- Default-method bodies supported; generic parameters supported on both
  sides: `impl<T> Iterator for (Counter)`.

### 2.11 `functor` declaration

```pdx
module Make = functor (M : MonoidSig) -> MonoidImpl = structure {
  let combine = M.combine
}
```

- **Parser:** `Parser::parse_functor`
  (`crates/paideia-as-parser/src/parse_item/module_decl.rs:150`;
  wrapper at `crates/paideia-as-parser/src/functor.rs:106`).
- Session-clause support in `parse_with_session_clause`
  (`crates/paideia-as-parser/src/functor.rs:228`).

### 2.12 `unsafe` item

```pdx
unsafe {
  effects:       { RawMem },
  capabilities:  { paideia.raw_mem },
  justification: "mov r64, r64",
  block: {
    mov rax, rdi
  }
}
```

- **Parser:** `Parser::parse_unsafe_item`
  (`crates/paideia-as-parser/src/parse_item/unsafe_item.rs:11`),
  wrapping `parse_unsafe`
  (`crates/paideia-as-parser/src/parse_unsafe.rs:95`).
- The four fields (`effects`, `capabilities`, `justification`, `block`)
  are all required.

### 2.13 `macro` declaration + macro invocation

```pdx
macro add_one {
  (x) => x + 1,
}

add_one!(42)
```

- **Decl parser:** `Parser::parse_macro_decl`
  (`crates/paideia-as-parser/src/parse_macro.rs:50`).
- Rules via `parse_macro_rule` / `parse_macro_rule_inner`
  (`parse_macro.rs:140`, `parse_macro.rs:147`).
- **Invocation:** parsed as a postfix form inside expressions
  (`crates/paideia-as-parser/src/parse_postfix.rs`); not a top-level
  item.

### 2.14 `@macro_expand` directive

```pdx
@macro_expand([add_one, "fn (x : u64) -> x"])
```

- **Parser:** `Parser::parse_top_level_at_directive`
  (`crates/paideia-as-parser/src/parse_macro_expand_directive.rs:64`).
- Internal-only (per the comment at the parser site); recorded in the
  arena's `macro_expand_directives` side-table and consumed by
  `paideia-as-elaborator::macro_expand_pass`.

### 2.15 `handler` block

```pdx
let h = with handler {
  Fail(msg) -> perform Report(msg),
  return x  -> x,
} do body
```

- **Parsers:** `Parser::parse_with_handler`
  (`crates/paideia-as-parser/src/parse_handler.rs:23`) for the
  statement-level form; `Parser::parse_handler_value`
  (`parse_handler.rs:238`) for the expression-level form.
- Arms via `parse_handler_arm` (`parse_handler.rs:339`).

---

## 3. Expressions (summary)

Full enumeration of 39 `ExprData` variants is beyond this stub (see
§A.1); the headline shapes:

### 3.1 Literal, path, ident

Integer (`42`, `0xCAFE_F00D`, `0u64`), float, bool (`true`/`false`),
unit `()`, string, byte, byte-string. Scanned by
`crates/paideia-as-lexer/src/scan_number.rs:1`,
`scan_char.rs:1`, `scan_string.rs:1`.

### 3.2 Call, index, field, method

`f(x)`, `xs[i]`, `p.x`, `p.foo()`. Parsed by
`crates/paideia-as-parser/src/parse_postfix.rs`.

### 3.3 Prefix, infix, cast

`-x`, `!b`, `*p`, `&x`, `&mut x`, `x + y`, `x == y`, `x as u64`. Pratt
precedence table in `crates/paideia-as-parser/src/precedence.rs`;
prefix dispatch in `parse_prefix.rs`.

### 3.4 Lambda (`fn (…) -> expr`, `|x| expr`)

- **`fn` form:** `Parser::parse_lambda_fn`
  (`crates/paideia-as-parser/src/parse_lambda.rs:23`).
- **Pipe form:** `Parser::parse_lambda_pipe`
  (`parse_lambda.rs:136`).
- Multi-arg curried: `fn (x : T) (y : U) -> …`.

### 3.5 `match`

```pdx
match n {
  0 => 1,
  _ => n * fact(n - 1),
}
```

- **Parser:** `Parser::parse_match`
  (`crates/paideia-as-parser/src/parse_match.rs:20`).

### 3.6 `if` / `while` / `for` / `loop`

Standard forms. Dispatched inside `parse_control`
(`crates/paideia-as-parser/src/parse_control.rs`).

### 3.7 Block `{ … }` and `let`-in

Blocks parse a sequence of statements followed by an optional tail
expression; `parse_stmt.rs` governs the statement surface. `let x = e in
body` is also surfaced.

### 3.8 Record, tuple, array, slice literals

`Point { x: 0, y: 0 }`, `(a, b)`, `[1, 2, 3]`, `xs[..n]`. Parsed in
`parse_primary/collection.rs`.

### 3.9 Quote / splice

`` `x `` / `` ,x ``. See `crates/paideia-as-parser/src/quote.rs`.

### 3.10 Pack / unpack

Existential type packing. Not in the examples corpus; grammar in
`parse_primary`.

### 3.11 `unsafe` expression

Same shape as the §2.12 item, used inside a lambda body:

```pdx
fn (dst : *u8) -> unsafe {
  effects:       { RawMem },
  capabilities:  { paideia.raw_mem },
  justification: "…",
  block: { … }
}
```

Parsed by `parse_unsafe` (`parse_unsafe.rs:95`).

---

## 4. Types

- **Path type**: `Mod::Sub::Name`, generics `Vec<T>`.
- **Function type**: `(u64, u64) -> u64 !{E…} @{C…}`.
- **Tuple type**: `(T, U)`.
- **Array type**: `[T; N]`.
- **Slice type**: `[T]`.
- **Reference**: `&T`, `&mut T`.
- **Pointer**: `*T`, `*mut T`.
- **Memref**: `memref<Shape>`
  (`crates/paideia-as-parser/src/parse_memref.rs`).
- **Record / enum (as types)**: by name.
- **Trait object**: `dyn Trait` (reserved; not fully implemented in
  Phase 4).
- **Lifetime param**: `'a`.
- **Effect row**: `!{E1, E2}`.
- **Capability row**: `@{C1, C2}`.
- **Generic application**: `Container<T>`.

Parser entry: `Parser::parse_type`
(`crates/paideia-as-parser/src/parse_type.rs:39`). Effect rows handled
by `parse_type/effect_row.rs`.

---

## 5. Patterns

Eleven `PatternData` variants:

- Wildcard (`_`).
- Literal (`0`, `"x"`, `true`).
- Binding (`x`, `x @ Pat`).
- Tuple (`(a, b)`).
- Record (`Point { x, y: v }`).
- Enum variant (`Some(x)`, `Red`).
- Range (`0..=10`).
- Or (`A | B | C`).
- Guard (`x if x > 0`).
- Reference (`&x`, `&mut x`).
- Rest / slice-rest (`..`, `..rest`).

Parser: `Parser::parse_pattern`
(`crates/paideia-as-parser/src/parse_pattern.rs:25`).

---

## 6. Attributes

Attributes attach to `let`-bindings via trailing or leading symbol
annotations.

- `@abi("sysv" | "ms")` —
  `parse_abi_attr` (`parse_item/let_item/attrs_semantics.rs:79`).
- `@endian(be | le)` — `crates/paideia-as-parser/src/endian_attr.rs`.
- `@interrupt(x86_64, with_error_code?)` — `parse_interrupt_attr`
  (`parse_item/let_item/attrs_semantics.rs:192`).
- `@atomic(seq_cst | acq_rel | …)` — `parse_atomic_attr`
  (`parse_item/let_item/attrs_semantics.rs:278`).
- `@fingerprint("<hash>")` — `parse_fingerprint_attr`
  (`parse_item/let_item/attrs_semantics.rs:478`).
- `@dsl_parser("<path>")` — `parse_dsl_parser_attr`
  (`parse_item/let_item/attrs_semantics.rs:344`).
- `@align(N)` — `parse_align_attr`
  (`parse_item/let_item/attrs_layout.rs:16`).
- `@ring(current, allowed)` — `parse_ring_attr`
  (`parse_item/let_item/attrs_layout.rs:80`).
- `@link_section("<name>")` — `parse_link_section_attr`
  (`parse_item/let_item/attrs_layout.rs:262`).
- `@packed`, `@no_frame`, `@sret_demote`, `@jump_table` —
  argument-less attributes handled by
  `parse_optional_symbol_attributes` directly
  (`parse_item/let_item/mod.rs:88`).

---

## 7. Lexical level (placeholder)

The pre-existing cross-crate citations (`syntax-reference.md §2.1`,
`§3.4`, etc.) target the lexer. Those sections are listed here for
completeness and will be filled when the lexical surface is next
audited (not blocking FMT-N).

### §A — To-be-written sections

- **§A.1 Lexical structure.** Whitespace, tokens, trivia. Driver at
  `crates/paideia-as-lexer/src/lex.rs:1`.
- **§A.2 Identifiers.** Scanning in
  `crates/paideia-as-lexer/src/scan_ident.rs:1`. Reserved-word policy at
  `design/toolchain/reserved-word-policy.md`.
- **§A.3 Reserved words.** 71 keywords; see §3.4 of
  `design/toolchain/reserved-word-policy.md`.
- **§A.4 Operators and punctuation.** Glyph table in
  `crates/paideia-as-lexer/src/glyph_table.rs:2`;
  `crates/paideia-as-lexer/src/scan_op.rs:1`.
- **§A.5 Literals.** Numbers (`scan_number.rs`), strings
  (`scan_string.rs`), chars (`scan_char.rs`).
- **§A.6 Comments and doc comments.** Trivia enum in
  `crates/paideia-as-lexer/src/trivia.rs:15`.
- **§A.7 Operator precedence.** Pratt table in
  `crates/paideia-as-parser/src/precedence.rs`.

---

## 8. Non-authoritative notice

This file is the **reference**, not the **specification**. When a shape
differs between this document and the parser, the parser wins and this
file is the bug. When a shape differs between this document and the
elaborator spec (for anything beyond syntax surface), the elaborator spec
wins and this file is out of scope. Report drift as a documentation
issue referencing the parser-function citation that proves it.
