# Records and enums (Phase 4 m7)

**Status:** Phase 4 m7 closure appendix.
**Scope:** Documents the records + enums + pattern bindings + match-exhaustiveness work shipped across m7-001..009.

## 0. Why m7 opened Phase 4

Under the all-assembly PaideiaOS constraint, every kernel data structure that isn't NASM-resident becomes a paideia-as data structure. Without records, every page-table entry, process descriptor, IPC header, capability token has to live inside an `unsafe { }` block with hand-rolled byte layouts. m7 retires that constraint by giving the typed surface real product + sum types.

The Q1 open question (which milestone opens Phase 4) flipped from m1 walker-hookups (the Phase-3-plan default) to m7 records-and-enums under the PaideiaOS-aware critical path.

## 1. Grammar (m7-001 / m7-003)

### 1.1 Record type

```
Type ::=
  | 'record' '{' (Ident ':' Type (',' Ident ':' Type)* (',')?)? '}'
  | ... existing forms
```

The `record` keyword is reserved (m7-001 lexer addition). Trailing commas are accepted. Empty records (`record { }`) are valid.

Pretty-printer prints in declaration order; not sorted.

### 1.2 Record construction

```
Expr ::=
  | TypeName '{' Ident ':' Expr (',' Ident ':' Expr)* (',')? '}'
  | ... existing forms
```

Disambiguated from block expressions by the leading `TypeName` — bare `{ ... }` stays a block. Conservative resolution: record-cons triggers only when an `Ident` primary is followed by `LBrace` AND the next two tokens are `Ident ':'`. Scrutinee positions in `match` / `if` / `while` / `for` use a context flag to suppress record-cons recognition.

### 1.3 Field access

```
PostfixExpr ::=
  | Expr '.' Ident
  | ... existing forms
```

Field access binds tighter than function call, so `r.f(x)` parses as `(r.f)(x)`. Chains: `r.f.g.h` left-associates.

### 1.4 Enum type

```
Type ::=
  | 'enum' '{' (Variant (',' Variant)* (',')?)? '}'
  | ... existing forms

Variant ::=
  | Ident                                          (Unit)
  | Ident '(' Type (',' Type)* (',')? ')'          (Tuple)
  | Ident '{' Ident ':' Type (',' ...)? '}'        (Record)
```

Three variant shapes per the AC. The `enum` keyword is reserved (m7-003 lexer addition).

### 1.5 Pattern bindings (m7-005)

```
LetDecl ::=
  | 'let' Pattern '=' Expr
  | ... existing forms

Pattern ::=
  | Ident                                         (irrefutable binder)
  | '_'                                           (irrefutable wildcard)
  | '(' Pattern (',' Pattern)* ')'                (tuple — irrefutable if inner are)
  | TypeName '{' Ident ':' Pattern (',' ...)? '}' (record — irrefutable if inner are AND type is record)
  | EnumName '::' Ident ('(' Pattern (',' ...)? ')')?  (enum variant — REFUTABLE)
  | Literal                                       (REFUTABLE)
```

Or-patterns (`Pat1 | Pat2`) and binding patterns (`name @ Pat`) also parse; binding-pattern semantics activate with elaborator wiring.

## 2. Memory layout (m7-002 / m7-004)

### 2.1 Record layout

Sequential, with per-field alignment padding and tail-pad to the record's overall alignment:

```
align(record) = max(align(field) for field in fields)
offset(field_0) = 0
offset(field_n) = align_up(offset(field_{n-1}) + size(field_{n-1}), align(field_n))
size(record) = align_up(offset(last_field) + size(last_field), align(record))
```

Empty records have size 0 and alignment 1.

### 2.2 Enum layout

Tagged union with 8-byte discriminant:

```
discriminant: 8 bytes at offset 0, 8-byte aligned
payload: starts at offset 8 (after discriminant)
align(enum) = max(8, max(align(payload) for variant in variants))
size(enum) = align_up(8 + max(size(payload) for variant), align(enum))
```

Phase-4-m7-004 minimum: always 8-byte discriminant. Niche-filling optimisations (e.g., `Option<*T>` packed into 8 bytes via null pointer) are a future opt PR.

### 2.3 Worked examples

- `record { x: u8, y: u64 }` → discriminant 0, padding 7, payload 8 = size 16, align 8.
- `enum { A, B, C }` → size 8 (just discriminant), align 8.
- `enum { Some(u64), None }` → size 16 (8 discr + 8 payload), align 8.
- `enum { Pair { a: u8, b: u8 } }` → size 16 (8 discr + 2 payload + 6 pad), align 8.

## 3. IR lowering (m7-007)

Four new IR kinds, each with a side-table per the m1-006 LoadStoreSideTable / m2-001 InstructionSideTable pattern:

- `IrKind::RecordCons` — children: per-field value nodes. Side-table `RecordLayoutTable: IrNodeId → TypeId`.
- `IrKind::FieldAccess` — children: [record_value]. Side-table `FieldAccessSideTable: IrNodeId → (TypeId, field_index)`.
- `IrKind::EnumCons` — children: per-payload value nodes. Side-table `EnumConsSideTable: IrNodeId → (TypeId, variant_index)`.
- `IrKind::EnumDiscriminant` — children: [enum_value]. Side-table `EnumDiscriminantSideTable: IrNodeId → TypeId`.

`IrNodeData` remains ≤ 48 bytes (const_assert pinned).

Pattern-binding lowering (m7-005's `let pat = expr`): the elaborator walks the pattern and produces:
- `Ident` → one `IrKind::Var` with the binder's symbol.
- `Tuple` → N `IrKind::Var` nodes, each with a `FieldAccess` projection.
- `Record` → similar with named projections.

Phase-4-m7-007 honest scope: the pattern-lowering helpers are shipped and unit-tested; the elaborator-side chokepoint that calls them lands incrementally as the walker grows (same gating as m4 LSP-side population).

## 4. Codegen (m7-008)

x86_64 sequences emitted via paideia-as-encoder:

### 4.1 Field access (`mov rax, [rdi + 8]`)

```
48 8b 47 08
^^^^^^^^^^^
REX.W       opcode (MOV r64, r/m64)
   ^^       ModR/M (mod=01 [disp8], reg=000 RAX, rm=111 RDI)
      ^^    disp8 = 8
```

Offset > 127 or < -128 uses disp32 (mod=10). Offset 0 uses mod=00.

### 4.2 Field store (`mov [rdi + 8], rsi`)

```
48 89 77 08
^^^^^^^^^^^
REX.W       opcode (MOV r/m64, r64)
   ^^       ModR/M (mod=01, reg=110 RSI, rm=111 RDI)
      ^^    disp8 = 8
```

### 4.3 Record construction

A `record { a: u64, b: u64 }` construction with `a = rax`, `b = rcx`, base `rdi`:

```
48 89 07     mov [rdi + 0], rax    ; field a
48 89 4f 08  mov [rdi + 8], rcx    ; field b
```

### 4.4 Enum construction

`enum { Some(u64), None }`, constructing `Some(rax)` at base `rdi`:

```
48 c7 07 01 00 00 00  mov qword [rdi + 0], 1   ; discriminant = 1 (Some)
48 89 47 08           mov [rdi + 8], rax       ; payload
```

### 4.5 Match-on-enum

Linear `cmp + jcc` chain (m7-008 minimum). Jump-table optimisation for many-arm matches is a future m9-or-later opt PR.

```
mov rax, [rdi + 0]    ; load discriminant
cmp rax, 0            ; variant 0?
je .arm_0
cmp rax, 1            ; variant 1?
je .arm_1
...
```

## 5. Exhaustiveness check (m7-006)

Diagnostic `T0512` (type-system pattern region; the issue spec said M0900 but M is the Module-system category with range 0300-0499 — T0512 is the in-range, semantically correct landing, contiguous with m1-003's T0511 borrowed-reference reservation).

`check_exhaustiveness(interner, scrutinee_type, arm_patterns)` returns:
- `Exhaustive` if any arm is `Wildcard` OR every enum variant has a covering arm.
- `MissingVariants(Vec<String>)` listing the names of unmatched variants.

Phase-4-m7-006 minimum: check function + tests. Elaborator-side wiring on every `match` lands with the match-expression walker (gated on the Phase-4 m1 walker chokepoint).

Useless-arm detection, or-pattern exhaustiveness, and wildcard-redundancy warnings are out of scope.

## 6. Diagnostic catalog

| Code  | Severity | Title                                         | Range  |
|-------|----------|-----------------------------------------------|--------|
| P0197 | error    | Malformed record type                         | parser |
| P0198 | error    | Malformed enum type                           | parser |
| P0199 | error    | Refutable pattern in let binding              | parser |
| T0512 | error    | Non-exhaustive match                          | type   |

Catalog-code conventions:
- P-region (parser): 0100-0299.
- T-region (type-system pattern): 0500-0899. T0511 reserved for borrowed references (m1-003).

## 7. Corpus (m7-009)

24 fixtures in `tests/end-to-end/codes/m7_*` (flat layout per repo convention; the issue spec said `corpus/record-*/` etc., but the codebase uses the flat-with-prefix pattern):

- `m7_record_*` × 8.
- `m7_enum_*` × 8.
- `m7_pattern_*` × 8.

Each `.pdx` paired with a `.expect` snapshot. All 24 paste cleanly today via `paideia-as check`. Full "compiles end-to-end + objdump-d verification of field-access displacement" gates on the m7-007 elaborator chokepoint activation; verified at the encoder level via the m7-008 unit tests.

## 8. Deferred to m9 (generics + traits)

Records and enums become **monomorphic** under m7. Several common patterns require generics for ergonomic expression:

- `Option<T>`, `Result<T, E>` — enum generics.
- `Pair<A, B>`, `Box<T>` — record generics.
- `Iterator<Item = T>` — type classes / traits.

m9 (generics-and-traits) extends both `Type::Record` and `Type::Enum` with type-parameter slots and a substitution mechanism. The m7 layout algorithm composes naturally — sizes and alignments compute on the monomorphised instances. m7 doesn't anticipate the substitution machinery; m9 introduces it cleanly.

## 9. Forward links

- **m1 walker hookups**: activates the elaborator-side wiring for pattern-binding lowering + match-exhaustiveness check.
- **m9 generics-and-traits**: parameterises records and enums (deferred per §8).
- **m11 stdlib bring-up**: depends on m7 for `Option`, `Result`, `Pair`, basic struct types. Activates with m9.
- **Niche-filling optimisations**: future opt PR that packs `Option<*T>` into 8 bytes via null-pointer encoding.
- **Match jump-table optimisation**: m7-008 ships linear cmp+jcc chains; many-arm matches become jump tables in a future opt PR (m3 or successor).
- **Borrowed references** (Phase 4 m4-m6): records gain `&` projections for shared mutable access; the m7 layout doesn't change but the access discipline does.
