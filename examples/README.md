# paideia-as examples

Curated, tutorial-oriented catalog of the paideia-as surface language. Each `.pdx` file isolates one feature and exercises only syntax the CLI's `paideia-as check` accepts today.

The 20 examples follow a teaching order: language fundamentals → type system → effects/capabilities → polymorphism → stdlib → unsafe → asm-reference algorithms.

## Examples status

| # | File | Topic | Check | Build | Build block reason |
|---|------|-------|-------|-------|-------------------|
| 1 | `01_hello.pdx`        | module + structure + `let` bindings | ✓ | ✓ | *none* — Phase 5 m4 in scope |
| 2 | `02_functions.pdx`    | `fn` / `\|x\|` lambdas + calling convention | ✓ | ✓ | *none* — Phase 5 m1 in scope |
| 3 | `03_records.pdx`      | `struct { field: T }` + construction | ✓ | ⊘ | Phase 4 m7; elaborator record codegen deferred to Phase 6 |
| 4 | `04_enums.pdx`        | sum types with unit, tuple, generic-tuple payloads | ✓ | ⊘ | Phase 4 m7; elaborator enum codegen deferred to Phase 6 |
| 5 | `05_patterns.pdx`     | `let pat = expr` + `match` exhaustiveness | ✓ | ⊘ | Phase 4 m1; pattern matching in elaborator deferred to Phase 6 |
| 6 | `06_loops.pdx`        | loop forms (tail recursion + while overview) | ✓ | ⊘ | Phase 4 m8; loop elaborator codegen deferred to Phase 6 |
| 7 | `07_pointers.pdx`     | `*T` raw pointers + `index_*` intrinsics | ✓ | ⊘ | Phase 3 m1; pointer deref codegen deferred to Phase 6 |
| 8 | `08_references.pdx`   | `&T` / `&mut T` borrowed references | ✓ | ⊘ | Phase 4 m4; borrow checker + reference codegen deferred to Phase 6 |
| 9 | `09_effects.pdx`      | effect rows `!{Eff}` on signatures | ✓ | ⊘ | Phase 2 m3; effect-handler codegen deferred to Phase 6+ |
| 10 | `10_capabilities.pdx` | capability sets `@{Cap}` on signatures | ✓ | ⊘ | Phase 3 m1; capability reification deferred to Phase 6+ |
| 11 | `11_generics.pdx`     | `<T>` type parameters + trait bounds | ✓ | ⊘ | Phase 4 m9; monomorphisation deferred to Phase 6+ |
| 12 | `12_traits.pdx`       | `trait` + `impl` + associated types | ✓ | ⊘ | Phase 4 m9; trait method resolution + codegen deferred to Phase 6+ |
| 13 | `13_stdlib.pdx`       | `Option` / `Result` enum constructors | ✓ | ⊘ | Phase 4 m11; stdlib type codegen deferred to Phase 6+ |
| 14 | `14_iterators.pdx`    | `Iterator` trait + adapters | ✓ | ⊘ | Phase 4 m11; iterator trait + adapters deferred to Phase 6+ |
| 15 | `15_unsafe.pdx`       | `unsafe { ... }` escape with all 4 fields | ✓ | ✓ | *none* — Phase 5 m3 in scope |
| 16 | `16_factorial.pdx`    | iterative factorial via tail recursion | ✓ | ⊘ | Phase 4 m8; TCO + loop elaboration deferred to Phase 6 |
| 17 | `17_fibonacci.pdx`    | iterative Fibonacci | ✓ | ⊘ | Phase 4 m8; TCO + loop elaboration deferred to Phase 6 |
| 18 | `18_sum_array.pdx`    | indexed array walk via `*u64` + `index_u64` | ✓ | ⊘ | Phase 3 m1; array indexing codegen deferred to Phase 6 |
| 19 | `19_memcpy.pdx`       | bulk byte copy via `REP MOVSB` in `unsafe` | ✓ | ⊘ | Phase 5 m2; `rep movsb` encoder added m2-009; unsafe block codegen deferred to Phase 6 |
| 20 | `20_strlen.pdx`       | NUL-scan via `*u8` + `index_u8` + `ptr_sub_bytes` | ✓ | ⊘ | Phase 3 m1; pointer+string codegen deferred to Phase 6 |

Legend: ✓ = passes; ⊘ = deferred to later phase (documented reason).

All 20 files check cleanly via:

```sh
paideia-as check examples/<file>.pdx
```

Per Phase 5 m7 closure, only examples 01, 02, 15 emit non-empty `.text` sections via:

```sh
paideia-as build --emit elf64 examples/XX_*.pdx -o /tmp/out.o
```

## asm-reference equivalents (16–20)

Examples 16–20 mirror the hand-written NASM programs under `asm-reference/algorithms/`. Each file calls out the specific `.asm` reference in its header. The mapping discipline:

- **Pure-functional** (factorial, fibonacci): tail-recursive accumulator. m9-008 TCO lowers to the NASM iterative loop.
- **Indexed reads** (sum_array): typed `*T` + `index_u64`. Lowers to `mov rax, [rdi + rcx * 8]` (SIB form `48 8b 04 cf`).
- **Raw memory** (memcpy): `unsafe { rep movsb }`. Only the one untypeable instruction stays unsafe.
- **Hybrid** (strlen): `index_u8` + `ptr_sub_bytes`. No `unsafe` block needed.

The bootloader (`asm-reference/bootloader/boot.asm`) has no paideia-as equivalent: 16-bit real mode vs. x86_64 long mode.

## Running an example

```sh
paideia-as check examples/01_hello.pdx
```

Future `paideia-as build --emit elf64 examples/<file>.pdx -o /tmp/out.o` activates per-example end-to-end compilation as the elaborator chokepoints close (Phase 4 m1 walker hookups for most; m6 borrow checker for #08, #11–14; m6+ for fn-body forms).

## Phase 4 stdlib walkthrough

The Phase 4 m11 stdlib bring-up shipped these types and traits. Each is exercised by the relevant example file:

| Stdlib feature                | Where exercised                                      | m11 source         |
|-------------------------------|------------------------------------------------------|--------------------|
| `Option<T> { Some, None }`    | `04_enums.pdx`, `13_stdlib.pdx`, `14_iterators.pdx`  | m11-001            |
| `Result<T, E> { Ok, Err }`    | `04_enums.pdx`, `13_stdlib.pdx`                      | m11-002            |
| `struct` types                | `03_records.pdx`, `12_traits.pdx`, `14_iterators.pdx`| m7-001             |
| `enum` sum types              | `04_enums.pdx`, `13_stdlib.pdx`                      | m7-004             |
| `<T>` generics                | `11_generics.pdx`                                    | m9-001             |
| `trait` + `impl`              | `11_generics.pdx`, `12_traits.pdx`, `14_iterators.pdx`| m9-003             |
| Associated types `type Item`  | `12_traits.pdx`, `14_iterators.pdx`                  | m9-007             |
| `&T` / `&mut T` borrowing     | `08_references.pdx`                                  | m4-001..008        |
| Effect rows `!{Eff}`          | `07_pointers.pdx`, `09_effects.pdx`, `13_stdlib.pdx` | m11-006 (IO eff)   |
| Capability sets `@{Cap}`      | `07_pointers.pdx`, `09_effects.pdx`, `10_capabilities.pdx`| m11-006 (paideia.io) |
| Tail-recursion + match        | `06_loops.pdx`, `16_factorial.pdx`, `17_fibonacci.pdx`| m7-005 (T0512)    |

The full stdlib API (Vec, String, HashMap, Iterator adapters, File/Read/Write traits) ships in `crates/paideia-stdlib/pdx/` source files. The examples here demonstrate the **types and surfaces**; calling stdlib methods end-to-end gates on the elaborator's walker activation (Phase 4 m1-005/006 / Phase 5 self-host).

## Surface coverage gaps in CLI `check`

Some surface ships in tests via the harness but isn't yet accepted by CLI `check`:

- Bare top-level `fn name() { ... }` (works only inside `module M = structure { ... }`).
- `let mut x = ...` (allowed only inside `fn { }` bodies; modules use immutable `let`).
- `break` / `continue` (lex-bug on CLI path; loop tests run via cargo test).
- New-style effect arrow `(args) -!{Eff}-> ret` (CLI uses older `-> ret !{Eff}` form).
- `if cond { ... }` expression in module-let position (use `match` instead).

These activate as the parser layers consolidate. The 20 examples here exercise the lowest-common-denominator surface.
