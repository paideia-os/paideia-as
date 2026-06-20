# Strings and loops (Phase 4 m8)

**Status:** Phase 4 m8 closure appendix.
**Scope:** Documents the string-literal infrastructure, the heap `String` type, the four loop forms (`for` / `while` / `loop` / break+continue), and the m3-006 unroll re-wiring to consume explicit IR Loop nodes.

## 0. Why m8 came after m7 and m9/m10

The PaideiaOS-aware re-ordering put m7 → m9 → m10 → m8 → m11. Reasoning:

- m7 (records) is the surface foundation for every kernel data structure.
- m9 (generics) gates m10's `trait Allocator` and `Box<T>`.
- m10 ships the allocator that backs `String` and (later) `Vec<T>`.
- m8 (strings + loops) now has both the heap allocator (for `String`) and the surface idiom (records) to build cleanly.

## 1. String literals (m8-001)

Lexer recognises:

- `"hello"` → `TokenKind::StringLit(String)`.
- `b"hello"` → `TokenKind::ByteStringLit(Vec<u8>)`.

Escape sequences: `\n`, `\t`, `\r`, `\\`, `\"`, `\x{HH}` (hex byte), `\u{XXXX}` (Unicode codepoint).

AST:
```rust
pub enum ExprData {
    StringLiteral(String),
    ByteStringLiteral(Vec<u8>),
}
```

Plus `NodeKind::ExprString` / `NodeKind::ExprByteString`.

Diagnostics: E0010 (unterminated string), E0011 (bad escape) — lexer category (E 0001-0099).

## 2. Type::Str + fat pointer (m8-002)

`paideia-as-types::Type::Str` interns as a singleton. Layout: 16 bytes, 8-byte aligned (fat pointer: 8-byte data ptr + 8-byte length).

`crates/paideia-as-ir::string_literal::StringLiteralTable` maps `IrNodeId → (rodata_offset: u64, len: u64)`. The .rodata accumulation gates on m4 emitter integration; the side-table records the tuple the emitter will read.

Phase-4-m8-002 honest scope: `&str` parser syntax gates on Phase 4 m4 (borrowed references grammar). Today's surface uses `Str` directly (immutable byte slice with fat-pointer layout).

## 3. Heap String (m8-003)

`crates/paideia-stdlib/pdx/string.pdx`:

```paideia
record String {
  data: *u8,
  len: u64,
  cap: u64,
}

let string_new : () -> String !{RawMem} @{paideia.raw_mem}
let string_with_capacity : (u64) -> String !{RawMem} @{paideia.raw_mem}
let string_len : (String) -> u64 !{} @{}
let string_push : (linear String, u8) -> String !{RawMem} @{paideia.raw_mem}
let string_from_str : (Str) -> String !{RawMem} @{paideia.raw_mem}
```

Linearity: `string_push` takes `linear String` because in-place mutation requires single-owner discipline (m4-m6 borrow checker will activate the proper `&mut Self` form).

Allocator integration: `string_new`, `string_with_capacity`, `string_push`, `string_from_str` consume the ambient allocator (m10-006 dual-default). PaideiaOS targets get Arena; host targets get SystemAllocator.

UTF-8 validation, slicing, find/replace: out of scope for m8-003; m11 stdlib expansion.

## 4. Loop forms (m8-004 / m8-005)

### 4.1 for (m8-004)

```
ForExpr ::= 'for' Pattern 'in' Expr '{' Block '}'
```

AST: `ExprData::For { pattern, iterable, body }`. The pattern is the m7-005 pattern form (Ident / Tuple / Wildcard / Record / EnumVariant / Literal).

Phase-4-m8-004 honest scope: parses cleanly. Elaborator-side lowering to a `match`-driven recursive call (or IR Loop) gates on m8-006.

### 4.2 while (m8-005)

```
WhileExpr ::= 'while' Expr '{' Block '}'
```

AST: `ExprData::While { condition, body }`. The condition must evaluate to a boolean.

### 4.3 loop (m8-005)

```
LoopExpr ::= 'loop' '{' Block '}'
```

Infinite loop; body must contain a `break` to terminate.

### 4.4 break / continue (m8-005)

`ExprData::Break` / `ExprData::Continue`. Bare keywords. Phase-4 minimum: only valid inside loop bodies; the elaborator-side validation gates on m1 walker.

## 5. IR Loop / Break / Continue (m8-006)

New IR kinds:

- `IrKind::Loop` — children `[body]`. Side-table `LoopMetaTable` records `(entry_label, exit_label)`.
- `IrKind::Break` — no children.
- `IrKind::Continue` — no children.

`LoopMeta { entry_label: u32, exit_label: u32 }` lets the encoder emit consistent labels for the m1-007 SIB-form codegen path's branch targets.

`IrNodeData` remains ≤ 48 bytes (const_assert pinned).

## 6. m3-006 unroll over explicit Loops (m8-007)

Phase 3 m3-006 shipped the unroll pass with a tail-recursion-pattern stub. m8-007 rewires `is_unroll_safe` + `UnrollPass::apply` to consume `IrKind::Loop` directly:

- `is_unroll_safe(table, loop_id, factor)` checks the loop body for blockers (Call, RepMovsb).
- `UnrollPass::apply` iterates Loop nodes; emits O1511 per recognised candidate.

Phase-4-m8-007 honest scope: recognition path active; actual IR body-duplication + remainder-loop emission lands with m3-006 closure (a future PR).

## 7. Diagnostic catalog

| Code  | Severity | Title                                             | Range |
|-------|----------|---------------------------------------------------|-------|
| E0010 | error    | Unterminated string literal                       | lex   |
| E0011 | error    | Bad escape in string literal                      | lex   |

Both lie in the E category (encoding/lexer; range 0001-0099).

## 8. Corpus

Distributed across `crates/paideia-stdlib/pdx/` and `tests/data/codes/`:

- String literal corpus: `m8_string_*` × 8 accept + `r_m8_string_*` × 4 reject.
- String heap corpus: `string_new_creates_empty.pdx`, `string_with_capacity.pdx`, `string_push_appends_byte.pdx`, `string_from_str_coerces.pdx`.
- For-loop corpus: `m8_for_*` × 6.
- While/loop corpus: `m8_while_*` × 4 + `m8_loop_*` × 3.
- Unroll smoke: `m8_unroll_for_loop.pdx`.

Total: ~28 fixtures.

## 9. Deferred to m11 / m4-m6 / m3-006-closure

- **`&str` parser syntax**: depends on Phase 4 m4 (borrowed references grammar).
- **`&mut String` mutation**: depends on m4-m6 (borrow checker).
- **String methods (split, replace, find, bytes, chars)**: m11 stdlib expansion.
- **UTF-8 validation**: m11 stdlib expansion.
- **Iterator trait** integration for `for` loops: gates on m9 trait elaboration walker.
- **Real IR body-duplication for unroll**: m3-006 closure follow-up.
- **break/continue with labels (`break 'outer`)**: out of scope; can be added without compatibility break later.

## 10. Forward links

- **m11 stdlib bring-up**: depends on m8 String + the loop idioms.
- **m1 walker hookups**: activates the elaborator-side wiring for break/continue validity, for-loop lowering, while-loop type-check.
- **m3-006 unroll closure**: m8-007 wired recognition; body-duplication follows.
- **PaideiaOS m1**: the first kernel subsystem can use String for serial-console output buffers.
