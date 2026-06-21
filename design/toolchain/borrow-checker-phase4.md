# Borrow checker (Phase 4 m6)

**Status:** Phase 4 m6 closure appendix.
**Scope:** Documents the borrow checker built on top of the m4 borrowed-references grammar + m5 region calculus.

## 0. Why m6 closes the borrow stack

m4 ships `&T` / `&mut T` syntax + Type::Ref interner + substructural classes (Affine / Linear) + IR `Borrow / BorrowMut / Deref` kinds. m5 ships `RegionId` + `RegionGraph` + region inference + lifetime variables + elision rules.

m6 is the **enforcement layer**: walkers that consume the m4 + m5 substrate and reject programs violating the aliasing, lifetime, and mutation rules.

## 1. The three walkers (m6-001..003)

### 1.1 BorrowWalker (m6-001) — aliasing rules

`paideia-as-elaborator::borrow_walker::BorrowWalker` tracks active borrows per binding.

```rust
pub enum BorrowKind { Immutable, Mutable }

pub struct BorrowWalker {
    active: HashMap<u32 /*binding_id*/, Vec<(BorrowKind, u32 /*region_id*/)>>,
}

impl BorrowWalker {
    pub fn borrow_immutable(&mut self, binding: u32, region: u32) -> Result<(), String>;
    pub fn borrow_mutable(&mut self, binding: u32, region: u32) -> Result<(), String>;
    pub fn drop_region(&mut self, region: u32);
    pub fn mark_last_use(&mut self, binding: u32, region: u32);  // m6-005 NLL
}
```

Rules enforced:

- `borrow_immutable` while a mutable borrow is live → **S0906**.
- `borrow_mutable` while any borrow is live → **S0906** (if outstanding is immutable) or **S0907** (if outstanding is mutable).
- `mark_last_use` releases a borrow at its last-use point, not scope-end (NLL — m6-005).

### 1.2 LifetimeWalker (m6-002) — outlives rules

`paideia-as-elaborator::lifetime_walker::LifetimeWalker` rejects borrows that outlive their source.

```rust
pub fn check_borrow(
    &mut self,
    borrow_region: RegionId,
    source_region: RegionId,
) -> Result<(), String>;
```

Algorithm: query the m5-001 `RegionGraph` for `source_region` outlives `borrow_region`. If not, fire **S0908** "borrowed value does not live long enough".

`'static` outlives everything; reflexivity (a outlives a); transitivity (composed via `close_transitively()`).

### 1.3 MutationWalker (m6-003) — assign-while-borrowed rules

`paideia-as-elaborator::mutation_walker::MutationWalker` rejects mutation of a binding while it has any active borrow.

```rust
pub fn check_assignment(&mut self, binding: u32) -> Result<(), String>;
```

Queries `BorrowWalker.is_borrowed(binding)`. If borrowed, fire **S0909** "cannot assign to value while borrowed".

## 2. Two-phase borrows (m6-004)

Method-call sites like `vec.push(vec.len())` need a special pattern: the `&mut vec` receiver is **reserved** during argument evaluation (allowing concurrent immutable reads of `vec.len()`), then **activated** to exclusive access for the call itself.

`paideia-as-elaborator::two_phase`:

```rust
pub fn reserve_two_phase_borrow(walker, binding, region) -> TwoPhaseReservation;
pub fn activate_reservation(walker, &mut reservation) -> Result<(), String>;
```

During the reservation window the walker treats the borrow as immutable; on activation it promotes to exclusive (S0907 firing on subsequent reads).

## 3. NLL precise drop semantics (m6-005)

Non-Lexical Lifetimes: borrows end at their last-use point, not at the enclosing scope's lexical end.

`paideia-as-elaborator::last_use::LastUseAnalyzer` walks the IR recording the highest IrNodeId where each (binding, region) pair is used. The BorrowWalker's `mark_last_use(binding, region)` releases the borrow at that point, enabling idioms like:

```paideia
let r = &v;     // immutable borrow of v
print(r);       // last use of r
let r2 = &mut v;  // OK — NLL released r after print(r)
```

Without NLL, this would fire S0906.

## 4. Diagnostic UX (m6-006)

`paideia-as-diagnostics::extended::ExtendedBorrowDiagnostic` carries structured multi-span reasoning:

```rust
pub struct ExtendedBorrowDiagnostic {
    pub code: String,                       // S0906 / 0907 / 0908 / 0909
    pub primary_message: String,
    pub borrow_originates_at: Option<u32>,  // IR node of &x / &mut x
    pub borrow_ends_at: Option<u32>,        // scope-end or NLL last-use
}
```

Renders as:
```
error[S0906]: cannot borrow as mutable while immutably borrowed
  borrow originates at IR node 42
  borrow ends at IR node 67
```

SARIF output emits the spans as `relatedLocations` for tooling integration. The `--explain` flag (m12 tooling) extends the rendering with a textual walk of the borrow graph.

## 5. Code collision: A0700 → S0906 catalog

The original issue specs used `A07xx` codes for borrow-check diagnostics. The diagnostic catalog has no `A` category — substructural lattice (`S`, range 0900-1099) is the semantically correct home, and m7's S0902 / S0904 / S0905 established the precedent of using S for linearity/aliasing rules.

The renaming:

| Spec   | Actual | Rule                                                          |
|--------|--------|---------------------------------------------------------------|
| A0700  | S0906  | Cannot borrow as mutable while immutably borrowed             |
| A0701  | S0907  | Cannot borrow as mutable more than once                       |
| A0702  | S0908  | Borrowed value does not live long enough                      |
| A0703  | S0909  | Cannot assign to value while borrowed                         |

All four codes ship with full descriptions in `catalog.toml` + SARIF-snapshot inclusion.

## 6. Elision-vs-explicit ergonomics

Phase 4 m5-005 ships Rust-style elision rules. m6's borrow checker honours them: a function signature without explicit lifetimes runs through the elision-rule resolver first, then the m6-002 LifetimeWalker checks the resolved regions.

Default discipline (matches Rust):

- One input borrow + one output borrow → output inherits input lifetime (no annotation needed).
- Method with `&self` → output borrows inherit `self`'s lifetime.
- Multiple input borrows + no method receiver → **L2001** (warning) "ambiguous lifetime elision; annotate explicitly".

The borrow checker fires its own S09xx errors only on resolved regions; elision is a separate layer.

## 7. Corpus (m6-007)

`tests/borrow-corpus/` ships **40 fixtures** across the four S09xx codes (10 each, 5 accept + 5 reject). The `every_borrow_code_has_at_least_two_reject_fixtures` regression test ensures every code has ≥2 reject fixtures.

| Code  | Accept | Reject | Pattern                                  |
|-------|--------|--------|------------------------------------------|
| S0906 | 5      | 5      | immut-mut conflict                        |
| S0907 | 5      | 5      | two-mut conflict                          |
| S0908 | 5      | 5      | source-shorter-than-borrow                |
| S0909 | 5      | 5      | mutate-while-borrowed                     |

## 8. What activates today vs. m11 walker

Phase-4-m6 honest scope: the three walkers + diagnostics catalog + two-phase + NLL helpers + corpus ship as standalone testable units. **Activation in the elaborator's full IR walk is incremental** — each walker plugs into the m1 walker-hookups chain (m1-001..006) at the relevant per-node entry.

What's wired today:
- BorrowWalker per-borrow methods are callable; unit tests exercise the rules directly.
- LifetimeWalker reads the RegionGraph; unit tests exercise the outlives check.
- MutationWalker queries BorrowWalker; unit tests exercise the assign rule.
- ExtendedBorrowDiagnostic renders correctly.

What's gated on m11 (PaideiaOS-aware future):
- The walkers running automatically during elaborate() over user code.
- The fixture-side reject tests producing the S09xx diagnostics end-to-end.

This mirrors the Phase 3 / Phase 4 m1 LSP pattern: lookup paths + walker logic ship in isolated, unit-tested form; per-walker activation lands incrementally as the elaborator threads them through.

## 9. PaideiaOS impact

With m6 closed, paideia-as has a complete borrow-checking story for PaideiaOS kernel code:

- **Page-table mutation under exclusive access**: `&mut PageTable` enforced by S0906 / S0907.
- **IPC message-buffer immutability**: `&IpcMessage` parameters can't be mutated by callees.
- **Capability-handle sharing**: multiple `&Cap` reads OK; single `&mut Cap` for state updates.
- **Reborrow for syscall fast paths**: `&*r` reborrow chain (m4-003) avoids alias violations.

Kernel code that previously needed `unsafe { }` blocks for shared mutation now has a typed surface.

## 10. Diagnostic catalog summary

| Code  | Severity | Title                                             | Region |
|-------|----------|---------------------------------------------------|--------|
| S0906 | error    | Cannot borrow as mutable while immutably borrowed | sub    |
| S0907 | error    | Cannot borrow as mutable more than once           | sub    |
| S0908 | error    | Borrowed value does not live long enough          | sub    |
| S0909 | error    | Cannot assign to value while borrowed             | sub    |

All in the substructural lattice region (S, 0900-1099); contiguous with the m7 / Phase 3 S-codes (S0900..S0905).

## 11. Forward links

- **m11 stdlib** ships `&mut Self` method receivers (Vec::push, String::push_str, HashMap::insert); the borrow checker activates per-method as those walkers wire through.
- **m12 tooling** adds `paideia-as check --explain S0906` displaying the borrow-graph reasoning.
- **PaideiaOS m1**: the first kernel subsystem written in paideia-as uses `&T` / `&mut T` idiomatically with the borrow checker enforcing safety.
- **Future Phase 5+**: NLL refinements (region-merging, region-erasure for opt passes); HRTBs (`for<'a> Fn(&'a u8)`); `Pin<&mut T>` for async pinning.
