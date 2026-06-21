# paideia-as examples

Curated, tutorial-oriented catalog of the paideia-as surface language. Each `.pdx` file isolates one feature and exercises only syntax the CLI's `paideia-as check` accepts today.

The 20 examples follow a teaching order: language fundamentals → type system → effects/capabilities → polymorphism → stdlib → unsafe → asm-reference algorithms.

## Examples

| File | Topic |
|---|---|
| `01_hello.pdx`        | module + structure + `let` bindings |
| `02_functions.pdx`    | `fn` / `\|x\|` lambdas + calling convention |
| `03_records.pdx`      | `struct { field: T }` + construction |
| `04_enums.pdx`        | sum types with unit, tuple, generic-tuple payloads |
| `05_patterns.pdx`     | `let pat = expr` + `match` exhaustiveness |
| `06_loops.pdx`        | loop forms (tail recursion + while overview) |
| `07_pointers.pdx`     | `*T` raw pointers + `index_*` intrinsics |
| `08_references.pdx`   | `&T` / `&mut T` borrowed references |
| `09_effects.pdx`      | effect rows `!{Eff}` on signatures |
| `10_capabilities.pdx` | capability sets `@{Cap}` on signatures |
| `11_generics.pdx`     | `<T>` type parameters + trait bounds |
| `12_traits.pdx`       | `trait` + `impl` + associated types |
| `13_stdlib.pdx`       | `Option` / `Result` enum constructors |
| `14_iterators.pdx`    | `Iterator` trait + adapters |
| `15_unsafe.pdx`       | `unsafe { ... }` escape with all 4 fields |
| `16_factorial.pdx`    | iterative factorial via tail recursion (asm-reference equivalent) |
| `17_fibonacci.pdx`    | iterative Fibonacci (asm-reference equivalent) |
| `18_sum_array.pdx`    | indexed array walk via `*u64` + `index_u64` (asm-reference equivalent) |
| `19_memcpy.pdx`       | bulk byte copy via `REP MOVSB` in `unsafe` (asm-reference equivalent) |
| `20_strlen.pdx`       | NUL-scan via `*u8` + `index_u8` + `ptr_sub_bytes` (asm-reference equivalent) |

All 20 files paste cleanly via:

```sh
paideia-as check examples/<file>.pdx
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
