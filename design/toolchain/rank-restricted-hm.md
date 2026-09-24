# Rank-Restricted Let-Polymorphism (R220.M11)

Status: **spec landing** (paideia-as#1425, R220.M11).
Blocks: semantic-shell R225 (unified HM checker across pipeline / Datalog /
lambda sub-languages) and R225.M6 (rank-restricted let-polymorphism
enforcement in the shell).
Builds on: R220.M8 (`paideia_as_effects::Substitution::apply` /
`Substitution::compose`, landed v0.36.6) for the row-substitution
machinery that the type-restriction pass sits alongside.

This spec fixes the sound HM subset paideia-as adopts for phase-1
inference across the three sub-languages the semantic shell composes
(§4 R220.M11 of `design/terminal/semantic-shell-language-plan.md`, §6
R2 risk mitigation). It is the "informal proof" posture per
`01-foundational-decisions.md` §3 tension 1: a decidability sketch, a
restriction check, a machine-checkable test corpus, and a stated escape
hatch — not a mechanized proof.

---

## 1. What "rank" means

Following the standard Church / Odersky-Läufer 1996 / Peyton Jones-
Vytiniotis-Weirich 2007 stratification, a polymorphic type is
classified by the depth at which universal quantifiers `∀` may appear
inside function-argument positions.

The grammar splits into three layers:

```
τ ::= a | T | τ → τ                  -- monotype (no ∀ anywhere)
ρ ::= τ | σ → ρ                      -- rho: ∀ may appear in argument
σ ::= ρ | ∀α. σ                      -- polytype: outermost ∀
```

Rank is defined by:

```
rank(τ)         = 0                                            -- monotype
rank(σ₁ → σ₂)   = max(promote(σ₁), rank(σ₂))
                  where promote(σ) = rank(σ) + 1  if σ has any ∀
                                     rank(σ)      otherwise
rank(∀α. σ)     = max(1, rank(σ))
```

Products (tuples and records) are treated like curried arguments — a
`∀`-bearing field is promoted the same way a `∀`-bearing function
parameter is:

```
rank((σ₁, …, σₙ))          = max_i promote(σᵢ)
rank({f₁: σ₁, …, fₙ: σₙ}) = max_i promote(σᵢ)
```

Worked ranks (used verbatim in the accept/reject corpus below):

| Type                              | Rank | Reason                              |
|-----------------------------------|------|-------------------------------------|
| `Int`                             | 0    | monotype                            |
| `(Int) → Int`                     | 0    | monotype                            |
| `((Int) → Int) → Int`             | 0    | higher-order but no ∀               |
| `∀α. α → α`                       | 1    | prenex ∀, monotype body             |
| `∀α β. (α → β) → α`               | 1    | prenex, no ∀ inside arg             |
| `(∀α. α → α) → Int`               | 2    | ∀ in argument of outermost arrow    |
| `∀α. (∀β. β → β) → α`             | 2    | inner ∀ inside arg promotes         |
| `((∀α. α → α) → Int) → Bool`      | 3    | ∀ two arrows deep                   |
| `(∀α. α → α, Int)`                | 2    | ∀ inside tuple component            |
| `{f: ∀α. α → α, g: Int}`          | 2    | ∀ inside record field               |

**"Prenex form"** is the shape `∀α₁ … ∀αₙ. ρ` — a maximal outer sequence
of universal quantifiers followed by a rho with **no further ∀**
anywhere. Rank-1 forms are exactly the prenex forms. Rank ≥ 2 forms
are non-prenex by definition (a ∀ appears somewhere below an arrow, a
tuple, or a record).

---

## 2. Why unrestricted HM becomes semi-decidable

Damas-Milner 1982 proved that inference for rank-1 (prenex) HM is
decidable and complete: `Algorithm W` always terminates with the
principal type or a well-defined "not typable" answer.

Wells 1999 proved that **type inference for the full rank-∞
System F** — allowing `∀` anywhere in the type — is **undecidable**.
The obstruction is *impredicative instantiation*: a rank-2 or higher
type variable can be instantiated at a polymorphic type, and no
finite unification can decide when to do so.

The intermediate strata soften this in three stages:

- **Rank-2** inference is decidable (Kfoury-Wells 1994), but complete
  algorithms are impractically expensive and produce non-principal
  types in general — the choice of instantiation branches at every
  rank-2 argument.
- **Predicative rank-N with explicit annotations** (Odersky-Läufer 1996;
  Peyton Jones-Vytiniotis-Weirich 2007 §7 "boxy types"; Vytiniotis-
  Peyton Jones-Weirich 2008 "FPH") is decidable, principal in the
  rank-1 sub-fragment, and requires an explicit type annotation at
  every rank-2+ occurrence. This is GHC's `RankNTypes` shape.
- **Row-polymorphic effects** (Leijen 2005; the M8 substrate) are
  *orthogonal* to type rank; they add polymorphism over a *label* row,
  not over the type of a value stored in a row. Row unification stays
  decidable at every rank.

The paideia-as R220 goal — a single HM checker running across
pipeline, Datalog, and lambda code inside the semantic shell — needs a
sub-fragment that:

1. is inferrable without annotations for the code any realistic
   command / query / pipeline stage will write;
2. accepts annotated rank-2 forms so a user can express real
   higher-rank patterns (Church-encoded lists, ST-style
   scoped-effect handles, GADT-shaped visitors) when they want them;
3. rejects rank-3+ outright — the annotation burden is unbounded and
   principal-type reconstruction is not available.

This is the **rank-restricted let-polymorphism** subset.

---

## 3. The subset paideia-as adopts

### 3.1 Formal statement

paideia-as accepts a type `σ` at a term position under sub-language
`L ∈ {Pipeline, Datalog, Lambda}` and annotation flag `A ∈ {none,
@annotate_type_boundary}` iff `rank(σ) ≤ maxRank(L, A)` where:

| L        | A                         | maxRank |
|----------|---------------------------|---------|
| Pipeline | any                       | 1       |
| Datalog  | any                       | 1       |
| Lambda   | none                      | 1       |
| Lambda   | `@annotate_type_boundary` | 2       |

Rank ≥ 3 is **rejected in every sub-language, in every annotation
mode**. Rank-4+ is not a supported form in phase-1 paideia-as; the
elaborator emits **T0700** at the offending position (see §5).

### 3.2 Design rationale

- **Rank-1 anywhere-and-always**: this is Damas-Milner. Inference is
  decidable, complete, and principal. Every command, every pipeline
  stage, every Datalog predicate the shell will lower fits here.
- **Rank-2 only in lambda bodies, only with the escape-hatch
  attribute**: the escape hatch prevents accidental rank-2 forms
  from leaking into a corpus that Damas-Milner cannot infer. When a
  user genuinely needs `(∀α. α → α) → (Int, Bool)`, they write it, and
  the elaborator instantiates the polymorphic argument at the call
  site the annotation names.
- **Pipeline and Datalog stay strictly at rank ≤ 1**: pipeline stages
  and Datalog predicates are relational forms whose implementation
  does not include the machinery for higher-rank instantiation.
  Trying to write `(∀α. α → α) → PipelineStage` is a category error;
  T0700 catches it before the shell tries to lower it.
- **Rank 3+ is never accepted**: the annotation-burden argument
  (Peyton Jones 2007 §7.4) generalizes — every rank-3 form requires
  an annotation *inside* the rank-2 annotation, and users have no
  intuition for those. paideia-as folds "supported ceiling" at 2.

### 3.3 Where each sub-language's ceiling lives

| Sub-language | Where the check runs                                       |
|--------------|-------------------------------------------------------------|
| Pipeline     | at each `pipeline { … }` stage's declared and inferred type |
| Datalog      | at each `datalog { … }` predicate's declared type           |
| Lambda       | at every `let`-binding's generalized type (Damas-Milner)    |

The check operates on the elaborator's already-lowered type — a
`TypeShape` (§5). Callers threading through the elaborator carry the
sub-language tag through the walker context (pipeline / Datalog / lambda
frames push and pop their own tags).

### 3.4 Composition across sub-languages

The three sub-languages share the same type universe (§4 R220.M8 —
effect rows already unify across them). Rank restriction composes
cleanly because it is a *local* property of a type: no cross-sub-
language type can have a higher rank than every sub-language it appears
in permits.

Concretely: a value of type `∀α. α → α` (rank 1) flowing from a lambda
into a pipeline stage is accepted at both sides (both allow rank ≤ 1).
A value of type `(∀α. α → α) → Int` (rank 2) can be *constructed*
inside a lambda with `@annotate_type_boundary` (Lambda + annotated
allows rank 2), but cannot be **passed into** a pipeline stage or a
Datalog predicate — the receiving side would need to accept a rank-2
argument, and Pipeline/Datalog ceilings forbid it. The receiving-side
check catches this: even though the caller was allowed to build the
rank-2 value, the pipeline-stage-argument position is checked at the
pipeline's ceiling and emits T0700.

This is the R2 mitigation from `semantic-shell-language-plan.md` §6:
the union of sub-languages is decidable because the shared rank ceiling
is the minimum of the sub-language ceilings that participate at any
composition point. Rank-1 is the meet of all three ceilings, and the
shell's cross-sub-language paths are guaranteed rank-1 or lower unless
the user opts into a rank-2 lambda-local island.

---

## 4. The escape hatch: `@annotate_type_boundary`

When a user must express a rank-2 type in a lambda body, they annotate
the boundary — the call site or let-binding position where the rank-2
value materializes — with the `@annotate_type_boundary` attribute:

```
let apply_id = @annotate_type_boundary (f: forall a. a -> a) -> (Int, Bool) {
    (f 42, f true)
}
```

The elaborator:

1. Sees the `@annotate_type_boundary` attribute on the let-binding.
2. Sets the sub-language frame's `annotated = true` bit.
3. Runs `check_rank_restricted` on the annotated type at that binding
   with `maxRank(Lambda, annotated) = 2`.
4. Emits T0700 only if `rank > 2` (an over-annotated rank-3 form still
   fails — the attribute lifts to rank 2, not rank ∞).

**Contract:** the attribute names one binding. It does not lift the
ceiling for enclosing or child expressions. A rank-2 argument the user
passes into a call must have its own annotation at its own construction
site (composition of annotated islands is explicit).

**Non-goal:** the attribute does not select the *shape* of the rank-2
instantiation. That is inference's job at the call site (the standard
"guessed instantiation" of Peyton Jones 2007 §4.6, restricted to the
one annotated argument position).

The R225.M6 landing on `paideia-os` will surface T0700 as `E0980` at
the semantic-shell layer with a fix-it suggestion referencing this
attribute verbatim (per §6 R2 mitigation).

---

## 5. Elaborator restriction check

### 5.1 Module

The check lives in `crates/paideia-as-elaborator/src/rank_restrict.rs`
(new module; re-exported through `paideia_as_elaborator::rank_restrict`).
It sits alongside `effect_infer` (R220.M8) rather than inside the
existing per-node walkers so callers can invoke it at:

- let-binding generalisation (the primary caller);
- pipeline-stage type acceptance;
- Datalog-predicate type acceptance;
- direct testing (the R220.M11 corpus below).

### 5.2 Surface data model

Because phase-1 `paideia-as-types::Type` is monomorphic (no explicit
`Scheme` / `ForAll` variant yet — see §7.1), the rank check operates on
its own thin **`TypeShape`** — a rank-analysis surface built from the
elaborator's lowered type on demand:

```rust
pub enum TypeShape {
    Concrete,                                       // primitives, named T, refs
    Var(u32),                                       // bound TyVar
    Arrow { params: Vec<TypeShape>, ret: Box<TypeShape> },
    Forall { vars: Vec<u32>, body: Box<TypeShape> },
    Tuple(Vec<TypeShape>),
    Record(Vec<(u32, TypeShape)>),
}
```

`TypeShape` is a *rank-computation view*, not a competing type
representation: it exists to make the rank check testable without
plumbing `TypeId` through the diagnostic layer. When the elaborator's
own `Scheme` lands (R225-adjacent), `TypeShape` becomes a thin adapter
over it.

### 5.3 API

```rust
pub const T_RANK_VIOLATION: u16 = 700;

pub enum SubLanguage { Pipeline, Datalog, Lambda }

pub fn rank_of(ty: &TypeShape) -> u32;
pub fn is_prenex(ty: &TypeShape) -> bool;

pub fn check_rank_restricted(
    ty: &TypeShape,
    sublang: SubLanguage,
    annotated: bool,
    span: Span,
) -> Vec<Diagnostic>;
```

- `rank_of` computes the syntactic rank per §1.
- `is_prenex` is `true` iff every `∀` occurs at the outermost prefix
  (equivalently, `rank_of ≤ 1`).
- `check_rank_restricted` emits at most one **T0700** at `span` when
  `rank_of(ty) > maxRank(sublang, annotated)`.

### 5.4 Diagnostic

**Code:** `T0700` (Category::T, code 700 — well inside the T range
500..=899). Message shape:

```
type-rank violation: this <sub-language> position accepts up to rank <max>,
    got rank <actual>
    <actual>-ranked type: ∀…. (∀…. …) → …
    hint: add @annotate_type_boundary to lift the ceiling to rank 2
          (rank ≥ 3 is never accepted)
```

The hint changes shape by sub-language and annotation state:

- `Pipeline` / `Datalog`: no hint — pipeline and Datalog have no
  escape hatch, so the fix is to rewrite the type to rank ≤ 1.
- `Lambda` unannotated: hint proposes `@annotate_type_boundary` (works
  only if `actual ≤ 2`).
- `Lambda` annotated with `actual ≥ 3`: hint explains that rank ≥ 3 is
  not accepted; user must decompose the type.

### 5.5 Wiring

The R220.M11 landing wires only the *check function and the
diagnostic*. The walker call-sites that invoke it land alongside their
sub-language substrates (R221.M4 for pipeline; the Datalog milestones
in R226; R225.M6 for lambda's let-generalisation on the shell side).
Testing the check directly against `TypeShape` values today, without
routing through a walker, is deliberate: it isolates the spec's
correctness from downstream integration timing.

---

## 6. Test corpus (60 fingerprints)

Landing in
`crates/paideia-as-elaborator/tests/rank_restricted_hm.rs` with
fingerprint tags `r220m11-rank-01` .. `r220m11-rank-60`. Split as:

| Class                                                    | Count | Tags                    |
|----------------------------------------------------------|-------|--------------------------|
| Rank-0 concretes (accept)                                | 5     | 01..05                   |
| First-order arrows (accept)                              | 5     | 06..10                   |
| Rank-1 prenex (accept)                                   | 5     | 11..15                   |
| Rank-2 annotated in lambda (accept)                      | 5     | 16..20                   |
| Sub-language accept (Pipeline / Datalog / Lambda rank-1) | 10    | 21..30                   |
| Rank-2 unannotated in lambda (reject T0700)              | 5     | 31..35                   |
| Non-prenex nested ∀ (reject T0700)                       | 5     | 36..40                   |
| Rank-3+ unannotated (reject T0700)                       | 3     | 41..43                   |
| Rank-3+ even when annotated (reject T0700)               | 2     | 44..45                   |
| Pipeline rank-2 (reject T0700)                           | 2     | 46..47                   |
| Datalog rank-2 (reject T0700)                            | 2     | 48..49                   |
| Record with ∀ field, unannotated (reject T0700)          | 3     | 50..52                   |
| Tuple with ∀ component, unannotated (reject T0700)       | 3     | 53..55                   |
| Cross-sub-language composition (reject T0700)            | 5     | 56..60                   |

**60 total; 30 accept + 30 reject.**

---

## 7. Explicit non-goals (deferred)

### 7.1 Explicit `Scheme` / `ForAll` in `paideia-as-types`

Phase-1 `Type` is monomorphic; polymorphism today is expressed
implicitly through unification variables. R225 (on `paideia-os`) or a
later paideia-as milestone will land a `Scheme` wrapper; when it does,
`TypeShape` becomes a thin adapter. The rank *semantics* in this spec
are stable across that refactor — only the surface changes.

### 7.2 Wiring the check into the walkers

Land with the sub-language substrates (R221.M4 pipeline; R226 Datalog;
R225.M6 lambda-side). M11's contract is the spec + the pure function;
walker-side calls are follow-on plumbing.

### 7.3 Impredicative rank-1

An impredicative rank-1 variant (allowing a `∀`-typed value to
instantiate an ordinary type variable) is a research direction, not
part of the R220 subset. Reserved for a later round if the shell's
corpus demands it.

### 7.4 Row-polymorphic higher-rank forms

Row polymorphism (R220.M8) is orthogonal — a row-polymorphic function
`∀e. Unit →!{Mem | e} Unit` is rank 1 as long as `e` is a row variable,
not a polytype. The check treats effect rows as monomorphic w.r.t.
type rank.

---

## 8. Cross-references

- **§4 R220.M11 landing row** —
  `design/terminal/semantic-shell-language-plan.md` §3 R220.
- **§6 R2 risk mitigation** —
  `design/terminal/semantic-shell-language-plan.md` §6 R2.
- **R220.M8 substrate** —
  `.plans/scratch/CHANGELOG-effect-row.md` (paideia-as v0.36.6);
  `paideia_as_effects::Substitution::apply` and
  `paideia_as_effects::Substitution::compose` verbatim.
- **Downstream consumer** — R225.M6 on `paideia-os`
  (semantic-shell-language-plan.md §3 R225 M6) surfaces T0700 as
  `E0980` with the `@annotate_type_boundary` suggestion.

## 9. References

- Damas L., Milner R. (1982) *Principal type-schemes for functional
  programs.* POPL.
- Odersky M., Läufer K. (1996) *Putting type annotations to work.*
  POPL.
- Wells J. B. (1999) *Typability and type checking in System F are
  equivalent and undecidable.* APAL.
- Peyton Jones S., Vytiniotis D., Weirich S., Shields M. (2007)
  *Practical type inference for arbitrary-rank types.* JFP.
- Vytiniotis D., Peyton Jones S., Weirich S. (2008) *FPH: First-class
  polymorphism for Haskell.* ICFP.
- Kfoury A. J., Wells J. B. (1994) *A direct algorithm for type
  inference in the rank-2 fragment of the second-order lambda-
  calculus.* LFP.
- Leijen D. (2005) *Extensible records with scoped labels.* TFP.
