# paideia-as examples

This directory is a curated, tutorial-oriented catalog of the `paideia-as`
surface language. Each `.pdx` file isolates one core feature and explains —
in the file itself — what the feature is, why it exists, how it is encoded,
and how it maps onto the runtime and calling convention. The examples are
meant to be read as a sequence; later files presume the vocabulary
established by earlier ones.

The canonical specifications for the surface language are:

- `design/toolchain/custom-assembler.md` — master spec
- `design/toolchain/syntax-reference.md` — normative lexical / grammar reference
- `design/toolchain/macros-phase1.md` — pattern-based macro system (phase-1)
- `design/toolchain/calling-convention.md` — register file, ABI, R15 handler table
- `design/toolchain/debug-info.md` — DWARF + PaideiaOS vendor extensions

Every example carries a status header. Three values appear:

- **compiles end-to-end** — the file round-trips through `paideia-as build`
  to an ELF64 object.
- **parses cleanly** — the file is accepted by `paideia-as check` and the
  parser produces a well-formed AST; semantic passes downstream may or may
  not be wired through end-to-end yet.
- **language-intent only** — the file uses canonical syntax from the
  design corpus but exercises constructs whose semantic passes are not yet
  wired through the lex → parse → lower pipeline. The file shows what the
  language is *for*; it is not yet a build artifact.

Some files carry a hybrid status: the file as written parses cleanly, but
illustrates phase-2-only constructs (e.g., `perform`, handler-value
declaration with `op ... =>` clauses) inside commentary. The status header
calls this out explicitly.

## Examples

| File | Summary | Status |
|---|---|---|
| `01_hello_module.pdx`         | Minimal module + structure + `let` bindings | parses cleanly |
| `02_functions.pdx`            | Function definitions; the canonical `fn x -> x + 1` → `lea rax, [rdi+1]; ret` leaf | parses cleanly |
| `03_substructural_lattice.pdx`| Ordered / Linear / Affine / Unrestricted (Walker 2005); class-kw type modifiers | parses cleanly |
| `04_effects_basic.pdx`        | Declare an effect; install a handler with `with ... handle`. `perform` shown as commented intent | parses cleanly + language-intent for `perform` |
| `05_effects_polymorphic.pdx`  | Row-polymorphic signature `forall e. ... !{Io \| e}`; local-handler discharge | parses cleanly + language-intent for `perform` |
| `06_capabilities.pdx`         | `@{...}` capability sets on signatures; R12/R13 in the convention | parses cleanly |
| `07_macros_simple.pdx`        | Single-rule pattern macros with `$x:expr`, `$x:ident`, `$x:type`, `$x:literal`, `$x:block`; multi-rule form documented | parses cleanly + language-intent for multi-rule |
| `08_macros_hygiene.pdx`       | Lean 4 / Ullrich (2020) hygiene; canonical `temp` example shown in commentary | language-intent only |
| `09_handlers_resume.pdx`      | Handler-value declaration with `op =>` clauses; `resume`, `finally`; aborting clauses | language-intent only |
| `10_pure_function.pdx`        | `!{}` empty effect row; what `!{}` forbids; local-handler escape | parses cleanly |
| `11_unsafe_block.pdx`         | `unsafe { effects, capabilities, justification, block }` — all four mandatory | parses cleanly |
| `12_calling_convention.pdx`   | RDI/RSI/RDX/RCX argument passing; R12 capability handle; R15-rooted handler table; functor declaration | parses cleanly |
| `13_factorial.pdx`            | Tail-recursive accumulator factorial; paideia-as equivalent of `asm-reference/algorithms/factorial.asm`. m9-008 TCO lowers to the NASM loop | parses cleanly |
| `14_fibonacci.pdx`            | Three-arg tail recursion threading `(a, b)` rotation; paideia-as equivalent of `asm-reference/algorithms/fibonacci.asm` | parses cleanly |
| `15_sum_array.pdx`            | Indexed array walk via tail recursion + abstract `Array` type; paideia-as equivalent of `asm-reference/algorithms/sum_array.asm` | parses cleanly |
| `16_memcpy.pdx`               | REP MOVSB bulk copy via `unsafe { }` escape; paideia-as equivalent of `asm-reference/algorithms/memcpy.asm` | parses cleanly |
| `17_strlen.pdx`               | Hybrid: per-byte read via `unsafe`, scan loop as tail recursion; paideia-as equivalent of `asm-reference/algorithms/strlen.asm` | parses cleanly |

All seventeen files are accepted by `paideia-as check` at HEAD; the
"language-intent" qualifications above flag constructs that the parser
currently sidesteps via commentary.

## asm-reference equivalence (files 13–17)

The five algorithm examples 13–17 mirror the hand-written NASM programs
under `asm-reference/algorithms/`. Each file calls out the specific
`.asm` reference in its header and includes the NASM source verbatim as
a comment block, so the equivalence is reviewable side-by-side.

The mapping discipline:

- **Pure-functional algorithms** (factorial, fibonacci) use tail-recursive
  accumulator pattern; m9-008's `TailCallPass` lowers the recursive arm
  to a `jmp`, so the emitted bytecode shape converges with the NASM
  iterative loop.
- **Indexed reads** (sum_array) use an abstract `Array` type with a
  `read_index` primitive. The actual `mov rax, [rdi + rcx * 8]` lowering
  activates with the per-node IR instruction-payload work flagged in
  `design/toolchain/phase-transition-2.md` §2.
- **Raw memory access** (memcpy, strlen) uses the `unsafe { }` escape
  per `11_unsafe_block.pdx`. The block body is the canonical NASM
  instruction sequence; effect rows like `!{MemCopy}` / `!{MemRead}`
  advertise the raw-memory contract on the typed surface.

The bootloader (`asm-reference/bootloader/boot.asm`) has no paideia-as
equivalent: it's a 512-byte MBR running in 16-bit real mode under BIOS,
while paideia-as targets x86_64 long mode. The 16-bit real-mode artifact
is intentionally left as the NASM reference only.

## Running an example

The canonical front-end invocation is:

```
paideia-as check examples/<file>.pdx
```

For files headed `compiles end-to-end`, the build invocation is:

```
paideia-as build examples/<file>.pdx --emit elf64 -o /tmp/<file>.o
```

**Phase 3 m1 update**: examples 15 / 16 / 17 (the asm-reference equivalents — sum_array, memcpy, strlen) shipped their **language-side compiles-end-to-end** status as of m1-008 / m1-009 / m1-010. The source surface for each is now expressive enough to write the algorithm using typed `*T` raw pointers + `index_*` / `ptr_sub*` intrinsics + the RawMem effect — no `unsafe { }` wrapper required (16_memcpy retains a 1-instruction `unsafe { rep movsb }` block because REP MOVSB has no typed-surface equivalent today). The full `paideia-as build --emit elf64` path runs through the parser + elaborator + IR populate chokepoint (m2-003) + InstructionSideTable (m2-001) + SIB-form encoder (m1-007). The regression test `tests/end-to-end/tests/examples_compile.rs` (m1-012) pins the build for these three files.

Per-walker population for the LSP semantics (m4) is gated separately — that's the elaborator-side feature work tracked in `design/toolchain/lsp-phase3.md` §6. The compiler pipeline (parse → elaborate → IR → emit) is complete for 15 / 16 / 17.

## Phase-3 status legend (post-m1)

- **compiles end-to-end** — the file round-trips through `paideia-as build --emit elf64` to a valid ELF64 object. Examples 15 / 16 / 17 now carry this status.
- **parses cleanly** — the file is accepted by `paideia-as check`; downstream passes may scaffold their work pending Phase 4 closure activity (see `design/toolchain/phase-transition-3.md` §2 "What didn't ship").
- **language-intent only** — the file uses canonical syntax from the design corpus but exercises constructs whose semantic passes are not yet wired through the lex → parse → lower pipeline. The file shows what the language is *for*; it is not yet a build artifact.

## Phase-1 deferrals (retained for historical reference; some discharged by Phase 2 / Phase 3)

A `language-intent only` status means the file uses syntax that the design
corpus prescribes but that depends on parser or semantic-pass work whose
wiring is phase-2 (see `STATUS.md` for the per-deliverable status). The
specific phase-1 parser limitations that constrain these examples are:

- The expression grammar accepts a *single expression* as a function or
  block body; `let`-statements inside expression bodies are phase-2 work.
- `perform`, `resume`, `finally`, and the `handle Effect { op ... => ... }`
  handler-value form are reserved words (per `syntax-reference.md` §3.4)
  but lack phase-1 primary-expression productions.
- The `unsafe.capabilities:` field accepts bare identifiers; the dotted
  form `Mod.right` is supported in signature-side `@{...}` but is phase-2
  for the `unsafe`-field body.
- Macros sit at the source-file top level in phase 1; nesting them inside
  a `structure { ... }` is phase-2.
- `parse_macro_decl` extracts one rule per `macro` declaration; the
  multi-rule shape `($x) => ... ($x, $y) => ...` is documented in
  `macros-phase1.md` §1.1 and pending in the parser.

The phase-2 milestones in `design/toolchain/milestones.md` track when each
deferral will be discharged.
