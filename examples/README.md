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

All twelve files are accepted by `paideia-as check` at HEAD; the
"language-intent" qualifications above flag constructs that the parser
currently sidesteps via commentary.

## Running an example

The canonical front-end invocation is:

```
paideia-as check examples/<file>.pdx
```

For files headed `compiles end-to-end`, the build invocation is:

```
paideia-as build examples/<file>.pdx --emit elf64 -o /tmp/<file>.o
```

No example currently carries `compiles end-to-end` status: phase-1
deliverable 8 (the ELF64 emitter, per `STATUS.md`) is wired through the CLI
but the elaborator's downstream passes for effects, substructural classes,
and capability sets are not yet driven end-to-end for every file in this
directory. The recommended path is via the project's wrapper scripts
(`./tools/dev/build`, `./tools/dev/test`) per
`02-development-environment.md` §7.4.

## Phase-1 deferrals

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
