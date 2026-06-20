# Borrowed references (Phase 4 m4)

**Status:** Phase 4 m4 closure appendix.
**Scope:** Documents `&T` and `&mut T` syntax, the type interner extension, substructural class decisions, IR lowering, and the deferral of lifetime inference to Phase 4 m5.

## 0. Why m4 ships borrowed references

The Phase 3 plan's §15 Q4 deferred borrowed references explicitly. Phase 4 m4 ships them as the foundation for m5 (region calculus) + m6 (borrow checker), which together resolve the deferral.

Phase 4 m4 alone gives:
- `&T` / `&mut T` parse at every type-grammar position.
- `&x` / `&mut x` parse as expressions.
- `*r` parses as deref.
- Type::Ref interns deterministically.
- Substructural classes: `&T = Affine`, `&mut T = Linear`.
- IR kinds `Borrow / BorrowMut / Deref` exist with side-table.
- Codegen emits the same SIB-form pointer encoding as `*T` (safety lives in the type system).

What m4 does **not** ship: lifetime inference, scope tracking, borrow-checker enforcement, alias analysis. Those are m5 + m6.

## 1. Grammar (m4-001 / m4-003)

### 1.1 Reference types

```
Type ::=
  | '&' Type                          -- immutable reference
  | '&' 'mut' Type                    -- mutable reference
  | '&' Lifetime Type                 -- with lifetime (m5 activation)
  | '&' 'mut' Lifetime Type           -- mutable with lifetime
  | ... existing forms
```

Lifetimes parse cleanly today (`&'a u8`) but are ignored by the elaborator until m5 region calculus activates region inference.

### 1.2 Borrow + deref expressions

```
PrefixExpr ::=
  | '&' Expr                          -- immutable borrow
  | '&' 'mut' Expr                    -- mutable borrow
  | '*' Expr                          -- deref
  | ... existing forms
```

The reborrow idiom `&*r` parses naturally as `Borrow(Deref(r))`. The double-deref idiom `**r` parses as `Deref(Deref(r))`.

Precedence: `&`, `&mut`, `*` bind tighter than function call. `&foo(x)` is `&(foo(x))` not `(&foo)(x)`.

### 1.3 Disambiguation

- `&T` at type position vs `&x` at expression position: the parser-context flag distinguishes.
- `*T` at type position (raw pointer from m1-001) vs `*x` at expression position (deref): same flag.

P0196 fires on bare `&` with no operand or malformed lifetime.

## 2. Type interner (m4-002)

`paideia-as-types::Type::Ref`:

```rust
pub enum Type {
    // ...
    Ref {
        pointee: TypeId,
        mutable: bool,
        lifetime: u32,    // 0 = 'static; nonzero = m5 region id
    },
}
```

Hash on `(pointee, mutable, lifetime)`. Two refs with same triplet intern to the same `TypeId`.

Unifier: refs unify if mutability matches + pointees unify. Lifetime ignored under the m4 flag; m5 activates region-aware unification.

## 3. Substructural class (m4-004)

`type_kind(Type::Ref { mutable, .. })`:

- `&T` → `LinClass::Affine` — at-most-once usage within the borrow scope. Can be dropped without harm; cannot be aliased and re-used freely without inviting use-after-free.
- `&mut T` → `LinClass::Linear` — exclusive access; must be used (or dropped) exactly once.

Neither class depends on the pointee. A `&linear MmioRegion` is still Affine at the reference layer; the pointee's linearity discipline is enforced when the reference is dereferenced (m6 borrow checker).

The lifetime field is ignored for kind derivation; m5 region calculus + m6 borrow checker together enforce that references don't outlive their source.

## 4. IR lowering (m4-005)

Three new IR kinds:

- `IrKind::Borrow` — children `[source]`. Side-table `BorrowSideTable` records `(source_binding, lifetime_id, mutable=false)`.
- `IrKind::BorrowMut` — same shape with `mutable=true`. Kept distinct from `Borrow` so opt-pass dispatch can branch on mutability.
- `IrKind::Deref` — children `[reference]`. No side-table; the type system tracks the pointee.

`BorrowMeta { source_binding: u32, lifetime_id: u32, mutable: bool }` lives in the per-arena `BorrowSideTable`. `IrNodeData` remains ≤ 48 bytes (const_assert pinned).

Phase-4-m4-005 honest scope: the IR kinds + side-table ship + the lowerer's `ExprBorrow → IrKind::Borrow` hookup is a documented TODO. Real activation lands when m6 borrow checker threads scope through the IR walker.

## 5. Codegen (m4-006)

At the byte level, `&T`, `&mut T`, and `*T` are indistinguishable. All three lower to the same SIB-form pointer encoding:

- Load via reference (`*r` where `r: &u64`): `48 8b 04 cf` (m1-007 SIB form, identical to `*u64` load).
- Store via mutable reference (`*r = v` where `r: &mut u64`): `48 89 ...`.

Safety comes from the type system (m6 borrow checker), not codegen. The encoder doesn't care whether the source pointer is "owned" (`*T`) or "borrowed" (`&T` / `&mut T`).

## 6. Diagnostic catalog

| Code  | Severity | Title                                       | Range  |
|-------|----------|---------------------------------------------|--------|
| P0196 | error    | Malformed reference type                    | parser |

P0196 fires on bare `&` with no operand, malformed lifetime, or `&mut` with no type.

## 7. Corpus (m4-007)

8 fixtures in `tests/data/codes/m4_corpus_borrow_*` (flat layout per repo convention; the issue spec said `corpus/borrow-*/`):

- m4_corpus_borrow_immut_var.pdx — `let r: &u8 = &x`.
- m4_corpus_borrow_mut_var.pdx — `let r: &mut u64 = &mut x`.
- m4_corpus_borrow_in_param.pdx — `fn f(r: &u64) -> u64`.
- m4_corpus_borrow_mut_in_param.pdx — `fn f(r: &mut u64) -> ()`.
- m4_corpus_borrow_field.pdx — `let r: &u64 = &(rec.field)`.
- m4_corpus_borrow_reborrow_chain.pdx — `let r2: &u8 = &(*r1)`.
- m4_corpus_borrow_static_lifetime.pdx — `let r: &'static u8 = &x`.
- m4_corpus_borrow_returned.pdx — `fn f() -> &u64`.

All 8 paste cleanly via `paideia-as check`. Full elaborator-side end-to-end (with borrow-check enforcement) gates on m6.

## 8. Deferred to m5 / m6

Per the m4 milestone scope and the Phase 4 dependency chain:

- **Lifetime inference** (m5 region calculus): scope assignment to lifetime variables; relating lifetimes via subtyping.
- **Borrow checker** (m6): per-scope alias analysis; mutability-exclusivity check; use-after-borrow detection.
- **Borrow-checker diagnostics**: B17xx (binary emission), L20xx (lint) — gated on m6 final design.
- **Higher-ranked trait bounds** (`for<'a> Fn(&'a u8)`): out of scope for Phase 4; future Phase 5+.
- **NLL (non-lexical lifetimes)**: out of scope; m6 ships lexical-only.
- **`Pin<&mut T>`** / **`Box<T>` borrow semantics**: depends on Phase 5+ async work.

## 9. Operational impact

With m4 closed, paideia-as users can:

- Write functions that take `&T` / `&mut T` parameters — parse-clean, semantic-gated to m6.
- Express PaideiaOS subsystem data structures with borrowed-reference fields where appropriate.
- Use `*r` deref in expression position.

What still doesn't work (until m5 + m6):

- The borrow checker won't catch `let r: &T = &x; drop(x); *r;` use-after-free patterns.
- Aliasing rules for `&mut T` aren't enforced.
- Lifetime annotations (`&'a T`) parse but don't relate scopes.

PaideiaOS kernel code can use borrowed references today for clarity (signature documentation), with the understanding that safety lands at m6.

## 10. Forward links

- **m5 region calculus**: lifetime inference + scope tracking. Activates the lifetime field in Type::Ref.
- **m6 borrow checker**: alias analysis + mutability exclusivity. Activates the IR walker's borrow-check pass.
- **m10 Allocator trait `&mut Self`**: the m10-001 scaffolded `self: &mut Self` form activates with m6.
- **m11 stdlib `&mut`-method receivers**: Vec::push, String::push_str, HashMap::insert.
- **PaideiaOS m1**: first kernel subsystem written in paideia-as uses `&T` / `&mut T` idiomatically once m6 enforces.
