# Generics and traits (Phase 4 m9)

**Status:** Phase 4 m9 closure appendix.
**Scope:** Documents generic-parameter grammar, kind system, trait + impl declarations, bound resolution, monomorphisation table, associated types, and derive-macro infrastructure shipped across m9-001..009.

## 0. Why m9 ran before m10 (allocator)

Under the PaideiaOS-aware re-ordering the user requested, m7 (records/enums) was the Phase 4 opener; m9 was originally planned to run later. Two cross-milestone dependencies surfaced when m10 (allocator) began:

- `trait Allocator { fn alloc(...) -> *u8; }` — needs the `trait` keyword (m9-003).
- `Box<T>` — needs generic parameters (m9-001).

Both are m9 deliverables. The honest re-order: m7 → m9 → m10 → m8 → m11 → m1 → m2 → m3 → m4 → m5 → m6 → m12 → m13 → m14.

## 1. Q2 resolution: generics flavour

The Phase 4 plan's §15 Q2 open question asked which generics flavour to ship:

1. **Monomorphisation-only** — every generic call site duplicates code per instantiation. Simple to implement; binary-size explosion.
2. **Monomorphisation-with-kinds** — kinds (`*`, `* -> *`) explicit; substitution machinery; still monomorphisation per instantiation. *Default per the plan.*
3. **Full HKTs + dictionary-passing** — higher-kinded types + runtime trait dispatch. Maximum expressive power; multi-milestone implementation effort.

**Decision: option 2 (monomorphisation-with-kinds).** Per the plan default. The kind machinery shipped in m9-002 (`Kind::Star`, `Kind::Arrow`) accommodates a future move to option 3 if HKTs become load-bearing for PaideiaOS, but the immediate driver — stdlib + PaideiaOS subsystems — is well-served by monomorphisation.

Trade-offs accepted:
- No higher-kinded `Functor<F>` / `Monad<M>` traits.
- Generic functions don't dispatch at runtime; every (function, type-args) pair generates a fresh monomorphic instance.
- Binary size grows with the number of instantiations. The m9-006 monomorphisation table dedupes shared instantiations.

## 2. Grammar (m9-001 / m9-003 / m9-004 / m9-007)

### 2.1 Generic parameters

```
GenericParams ::= '<' GenericParam (',' GenericParam)* (',')? '>'
GenericParam  ::= Ident (':' Path (',' Path)* )?
```

Attached to functions, records, enums, and impl blocks. Trailing commas accepted. Bounds parsed but resolved later (m9-005).

### 2.2 Trait declarations

```
TraitDecl   ::= 'trait' Ident GenericParams? '{' TraitMember* '}'
TraitMember ::= 'type' Ident ';'                                    -- associated type
             |  'fn' Ident GenericParams? '(' Params? ')' '->' Type
                  EffectRow? CapRow? (';' | '{' Expr '}')           -- method
```

Default-method bodies allowed (m9-003). Associated-type slots parse as `type Item;` and bind a name available in the trait's scope as `Self::Item` (m9-007).

### 2.3 Impl blocks

```
ImplDecl ::= 'impl' GenericParams? (Path GenericArgs? 'for')? Type '{' Item* '}'
```

Two shapes:
- **Inherent**: `impl<T> Foo<T> { ... }` — methods defined directly on a type.
- **Trait**: `impl<T> MyTrait<T> for Foo { ... }` — methods that satisfy a trait.

Disambiguated by the `for` keyword presence.

### 2.4 Associated-type projections

```
TypePath ::= 'Self' '::' Ident                                      -- Self::Item
GenericArg ::= Type
            |  Ident '=' Type                                       -- Item = u64
```

`<I: Iterator<Item = u64>>` constrains a parameter `I` to implement `Iterator` with the projection `Item` fixed to `u64`.

### 2.5 Derive attributes

```
ItemAttribute ::= '#' '[' 'derive' '(' Path (',' Path)* ')' ']'
```

`#[derive(Eq, Hash, Debug)]` attached to a record or enum declaration triggers elaborator-time synthesis of `impl Eq for T`, `impl Hash for T`, `impl Debug for T` (m9-008).

## 3. Kind system (m9-002)

Phase 4 m9 ships a small kind language:

```
Kind ::= '*'                                                        -- type kind
       | Kind '->' Kind                                              -- type-constructor kind
```

Encoded as:

```rust
pub enum Kind {
    Star,
    Arrow(Box<Kind>, Box<Kind>),
}
```

Every type parameter declared today has `Kind::Star`. Type constructors emerge from `Type::Record { fields: SmallVec<[(_, TypeId)]> }` etc. — their kind is computed by `kind_of_type_constructor(arity)`:

- `Vec` (1-arg) → `* -> *`.
- `Pair`, `Result` (2-arg) → `* -> * -> *`.

Type variables (`Type::Var { name, kind }`) carry their kind explicitly. Unification of two `Type::Var` requires kind equality; mismatch fires `UnifyError::KindMismatch`.

## 4. Trait-bound resolution (m9-005)

`check_scope_subsumption_with_row_poly` in m7-004 handled effect/capability subsumption. m9-005 adds the analog for trait bounds.

Algorithm (`paideia-as-elaborator::check_bounds::resolve_bound`):

```
resolve_bound(coherence, bounded_params, target_type, required_trait):
  if bounded_params contains (target_type, required_trait):
    return BoundedParam { ... }
  if coherence contains (required_trait, target_type):
    return ConcreteImpl { ... }
  return Missing { ... } → emit T0514
```

T0514 is "Unsatisfied trait bound" (type-system region; in the T 0500-0899 range, contiguous with T0511 / T0512 / T0513).

Phase-4-m9-005 minimum: the check function ships; the elaborator-side wiring (call it at every method-call site against a type variable) is gated on the m1 walker chokepoint.

## 5. Monomorphisation (m9-006)

`paideia-as-ir::monomorphisation::MonomorphisationTable`:

```rust
pub struct MonoKey {
    pub function_id: IrNodeId,
    pub type_args: Vec<TypeId>,
}

pub struct MonomorphisationTable {
    entries: HashMap<MonoKey, IrNodeId>,
    insertion_order: Vec<MonoKey>,  // for DDC determinism
}
```

API: `intern_or_get(key, generator)` returns the IrNodeId of the monomorphic instance; calls `generator` to produce it on miss.

The `insertion_order` Vec preserves stable iteration so the DDC harness can verify byte-identity across runs.

## 6. Coherence (m9-004)

Each `(trait, type)` pair has at most one impl. The check (`paideia-as-elaborator::check_coherence::CoherenceChecker`):

```
record_impl(trait_id, for_type_id, impl_id):
  if (trait_id, for_type_id) already in table:
    emit T0513 "Duplicate impl for the same (trait, type) pair"
  else:
    insert
```

T0513 lives in the type-system region (in the T 0500-0899 range, contiguous with T0512 m7-006 exhaustiveness).

## 7. Derive macros (m9-008)

`#[derive(Eq, Hash, Debug)]` parses + dispatches to elaborator-time synthesis (`paideia-as-elaborator::derive`):

- **Eq**: per-field equality + AND-combine.
- **Hash**: per-field hash + combine (xor or polynomial; default xor).
- **Debug**: per-field `field_name = field_value` rendering.

Each synthesis returns a `SyntheticImpl { trait_name, type_name, method_bodies }` that the elaborator can splice into the IR. Phase-4-m9-008 minimum: synthesis functions + tests; elaborator-side splicing gated on the m1 walker chokepoint.

## 8. Diagnostic catalog

| Code  | Severity | Title                                       | Range  |
|-------|----------|---------------------------------------------|--------|
| P0200 | error    | Malformed generic parameter list            | parser |
| P0201 | error    | Malformed trait declaration                 | parser |
| P0202 | error    | Malformed impl block                        | parser |
| T0513 | error    | Duplicate impl for (trait, type)            | type   |
| T0514 | error    | Unsatisfied trait bound                     | type   |

Conventions:
- P-region (parser): 0100-0299.
- T-region (type-system pattern): 0500-0899. T0511 (m1-003 borrowed-ref reserved), T0512 (m7-006 exhaustiveness), T0513 (m9-004 coherence), T0514 (m9-005 bounds).

## 9. Corpus (m9-009)

24 fixtures in `tests/data/codes/m9_*` (flat layout per repo convention; the issue spec said `corpus/generics-*/` etc., but the codebase uses flat-with-prefix — same convention as m7-009):

- `m9_generic_*` × 12 — generic functions / records / enums / Vec<T>-style / Option<T> / Result<T,E> / Iterator trait / monomorphisation target.
- `m9_trait_*` × 12 — simple traits, default methods, multi-method, inheritance via bounds, impl shapes (inherent / for-record / for-enum / generic / with-assoc-type), derive-Eq / derive-multi / Iterator impl.

Each `.pdx` paired with `.expect` snapshot. All 24 paste cleanly via `paideia-as check`. Full "compiles end-to-end" gates on the m1 walker chokepoint that activates the elaborator-side wiring; verified at the parser + elaborator helper-function level via the m9-001..008 unit tests.

## 10. Deferred to m10 / m11 / later

- **Stdlib types**: `Option<T>`, `Result<T, E>`, `Box<T>`, `Vec<T>`, `HashMap<K, V>` — depend on this m9 substrate; ship in m11 (after m10 allocator).
- **Higher-kinded types** (`Functor<F>`, `Monad<M>`) — Q2 deferred to a Phase 5+ decision.
- **`for<'a>` (HRTBs) for borrowed references** — gates on Phase 4 m4-m6 (region calculus + borrow checker).
- **Specialisation** (overlapping impls with priority order) — out of scope; coherence stays strict.
- **Negative bounds** (`T: !Send`) — out of scope.
- **Const generics** (`Array<T, const N: usize>`) — out of scope.

## 11. Forward links

- **m10 allocator** depends on m9-003 (`trait`) + m9-001 (`<T>`) to express `trait Allocator { fn alloc(self: &mut Self, layout: Layout) -> *u8; }`.
- **m11 stdlib bring-up** depends on m9 entirely.
- **m1 walker hookups** activates the elaborator-side wiring for m9-005 (bound checks at call sites), m9-006 (monomorphisation pass), m9-008 (derive splice).
- **m13 self-hosting groundwork** depends on m9 to express paideia-as's own generic data structures.
