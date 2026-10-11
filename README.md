# paideia-as

**Current workspace version:** `v0.37.0` (paideia-fmt COMPLETE at FMT-17; paideia-as-unicode COMPLETE at UNI-16).

`paideia-as` is the Rust-based toolchain for the
[PaideiaOS](https://github.com/paideia-os/paideia-os) project —
assembler, compiler, formatter, LSP, and the satellite crates
(`paideia-as-unicode`, `paideia-as-crypto`, `paideia-pq-sign`,
`paideia-as-shell-*`) that the OS and its userland tools link
against. The surface language is a small, statically-typed core
with substructural types, algebraic effects, capability-based
discipline, and post-quantum signing of build artifacts.

`paideia-as` compiles `.pdx` source through a typed elaborator into ELF64, PAX (PaideiaOS-native), or PE/COFF (UEFI) objects. The differentiated technical claim is that a typed surface — Ordered / Linear / Affine / Unrestricted classes (after Walker 2005), row-polymorphic effect rows, `@{...}` capability sets, typed raw pointers — lowers all the way down to the canonical x86_64 instruction sequence a hand-written assembler would emit, with the discipline preserved as DWARF vendor sections on the object. The target is x86_64 long mode; 16-bit real mode is out of scope.

## At a glance

- **Typed raw pointers** (`*u64`, `*u8`, …) with `index_*` / `ptr_sub*` intrinsics, gated on the `RawMem` effect and the `paideia.raw_mem` capability. `index_u64(xs, i)` lowers to `mov rax, [rdi + rcx * 8]` — the canonical SIB-form encoding `48 8b 04 cf`.
- **Substructural type lattice** — `ordered`, `linear`, `affine`, `unrestricted` written as type-side class keywords; linearity violations surface as structured diagnostics (`S0902` / `S0904` / `S0905`).
- **Row-polymorphic algebraic effects** — `forall e. (T) -> U !{Io | e}` signatures, `with ... handle` discharge, `perform` / `resume` / `finally`. Empty row `!{}` is a checkable purity claim.
- **Capability-based discipline** — `@{paideia.raw_mem}` on signatures; capability handles live in R12 / R13 at runtime per the calling convention.
- **Four object formats** — `placeholder`, `elf64` (kernel target), `pax` (PaideiaOS-native, BLAKE3-hashed, PQ-signature slot), `pe-coff` (UEFI, with a SysV ↔ MS-x64 thunk).
- **Language server + editor recipes** — `paideia-as lsp` ships hover, definition, references, completion, code actions, formatting, semantic tokens, and inlay hints over LSP. Ready-to-use configs for **VS Code**, **Helix**, **Emacs**, and **Neovim** under [`tools/editor/`](tools/editor/).
- **Post-quantum hybrid signing** — Ed25519 (RFC 8032) + ML-DSA-65 (FIPS-204) with AND-verification, RFC 3161 timestamping, and a JSON-lines revocation list. Hardware HSM backends (PKCS#11, YubiHSM2) with a hybrid-fallback rule.
- **Deterministic dual-bootstrap** — NASM (stage-0a) and GNU `as` (stage-0b) entry-points compile to byte-identical `.text` sections (`48 8d 47 01 c3`). The DDC harness (Diverse Double Compilation, Wheeler 2005) byte-compares two independently bootstrapped builds.
- **Optimisation pass catalogue** — eleven passes (`O1500`–`O1512`): peephole, instruction scheduling, macro fusion, DSE, REX/EVEX tightening, branch hints, alignment, pool constants, tail-call elimination, loop unrolling.

## Try it

Build the workspace (all crates + binaries):

```sh
cargo build --workspace --release
```

Just the CLI:

```sh
cargo build --release -p paideia-as
```

Run the full test suite (~1800 tests in under a minute on a modern laptop):

```sh
cargo test --workspace
```

Parse-check a tutorial example:

```sh
./target/release/paideia-as check examples/01_hello_module.pdx
```

Compile an array-sum to a relocatable ELF64 object:

```sh
./target/release/paideia-as build examples/15_sum_array.pdx --emit elf64 -o /tmp/sum.o
```

The source surface looks like this (excerpt from `examples/15_sum_array.pdx`):

```paideia
module SumArray = structure {

  let sum_acc : (*u64, u64, u64, u64) -> u64 !{RawMem} @{paideia.raw_mem} =
    fn (xs : *u64) (n : u64) (i : u64) (acc : u64) -> match i {
      n => acc,
      _ => sum_acc(xs, n, i + 1, acc + index_u64(xs, i))
    }

  let sum_array : (*u64, u64) -> u64 !{RawMem} @{paideia.raw_mem} =
    fn (xs : *u64) (n : u64) -> sum_acc(xs, n, 0, 0)
}
```

Three things are happening on the type side: `*u64` is a real raw pointer, `!{RawMem}` advertises that the function reads raw memory, and `@{paideia.raw_mem}` is the capability the caller must hold. On the code side, after tail-call lowering the recursive arm becomes a `jmp`, and `index_u64(xs, i)` emits the canonical indexed load:

```text
mov rax, [rdi + rcx * 8]      ; 48 8b 04 cf
```

— byte-for-byte the addressing form a hand-written NASM loop would use. The asm-reference equivalent is in [`asm-reference/algorithms/sum_array.asm`](asm-reference/algorithms/sum_array.asm).

The four `--emit` values are `placeholder` (pipeline smoke), `elf64` (kernel-image target), `pax` (PaideiaOS-native), and `pe-coff` (UEFI / Microsoft x64). Output selection is required — invoke with either `--emit <format>` or `--target <triplet>`. For smoke-test compatibility, use `--emit placeholder` to produce a `<stem>.placeholder` artifact next to the input (this was the default before issue #1110).

## The two-instruction leaf function

The smallest non-trivial example. From `examples/02_functions.pdx`:

```paideia
let add_one : (u64) -> u64 = fn (x : u64) -> x + 1
```

The signature reads: "take a `u64`, perform no effects, return a `u64`". The body lowers to exactly:

```text
lea rax, [rdi + 1]
ret
```

— the first integer argument arrives in `RDI` per the calling convention, the return value leaves in `RAX`, and `lea` computes the sum without touching flags. This is the canonical leaf-function shape and the bar every other example targets.

## Diagnostics

Diagnostics carry stable codes (`S0902` linear-shadow, `S0904` affine-consumed, `S0905` ordered-out-of-order, `B1700` / `B1701` linker, `O1500`–`O1512` optimiser, `Q0902` HSM-no-PQ-support, …) and are rendered three ways: human-readable for the CLI, SARIF for tooling (`<file>.pdx.sarif.json` next to each example), and LSP `PublishDiagnostics` for editors. Each code is forward-pointer-stable: a fix-it that worked once will keep working.

## Workspace crates

38 Rust crates. The table is grouped by role; **shipped** = in-tree and used by downstream consumers, **in-progress** = active wave, **planned** = scaffolded.

### Compiler core

| Crate | Role | Status | Entry docs |
|---|---|:-:|---|
| `paideia-as` | CLI front end (`check` / `build` / `lsp` / `fmt` / `test` / `doc`) | shipped | [`design/toolchain/syntax-reference.md`](design/toolchain/syntax-reference.md) |
| `paideia-as-lexer` | UTF-8 + BOM-aware source-text validation; token stream | shipped | [`design/toolchain/syntax-reference.md`](design/toolchain/syntax-reference.md) |
| `paideia-as-parser` | Hand-written Pratt + recursive-descent parser (P0100–P0299) | shipped | [`design/toolchain/syntax-reference.md`](design/toolchain/syntax-reference.md) §7, §8 |
| `paideia-as-ast` | Arena-backed surface AST | shipped | [`design/toolchain/syntax-reference.md`](design/toolchain/syntax-reference.md) |
| `paideia-as-types` | Monomorphic type interner + lattice-class kinds | shipped | [`design/toolchain/custom-assembler.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/custom-assembler.md) §5 (parent repo) |
| `paideia-as-effects` | Effect-row representation + row-polymorphism unifier | shipped | custom-assembler.md §4.2 |
| `paideia-as-ir` | Typed core IR + per-node instruction payload | shipped | [`design/toolchain/per-node-ir-payload-phase3.md`](design/toolchain/per-node-ir-payload-phase3.md) |
| `paideia-as-elaborator` | AST → IR lowering + lattice/effect/capability walkers | shipped | [`design/toolchain/walker-hookups-phase4.md`](design/toolchain/walker-hookups-phase4.md) |
| `paideia-as-encoder` | Shared x86_64 instruction encoder | shipped | [`design/encoding/32bit-mode-survey.md`](design/encoding/32bit-mode-survey.md) |
| `paideia-as-emit` | Stable public API for emitting x86_64 instructions at runtime | shipped | — |
| `paideia-as-emitter-elf` | ELF64 writer | shipped | custom-assembler.md §12.1 |
| `paideia-as-emitter-pax` | PAX (PaideiaOS Architectural Executable) emitter | shipped | [`design/toolchain/paideia-link.md`](design/toolchain/paideia-link.md) |
| `paideia-as-emitter-pe` | PE/COFF emitter (Microsoft x64 / UEFI) | shipped | — |
| `paideia-as-dwarf` | DWARF 5 + paideia vendor sections | shipped | [`design/toolchain/debug-info.md`](design/toolchain/debug-info.md) |
| `paideia-as-diagnostics` | Diagnostic substrate (SARIF + human + LSP) | shipped | — |
| `paideia-as-linker` | `paideia-link` — PAX-format linker (4-phase pipeline) | shipped | [`design/toolchain/paideia-link.md`](design/toolchain/paideia-link.md) |
| `paideia-as-runtime` | `no_std` runtime-compatible core types for instruction encoding | shipped | — |

### Lattice / macros / reflection

| Crate | Role | Status | Entry docs |
|---|---|:-:|---|
| `paideia-as-linear` | Linearity-type infrastructure | shipped | [`design/toolchain/borrow-checker-phase4.md`](design/toolchain/borrow-checker-phase4.md) |
| `paideia-as-macro` | Macro-system infrastructure (phase-1 pattern + phase-2 reflection hooks) | shipped | [`design/toolchain/macros-phase1.md`](design/toolchain/macros-phase1.md) |
| `paideia-as-reflection` | Typed `Syntax` + `HygienicId` + `SyntaxWalker` elaborator surface | shipped | custom-assembler.md §R220 |
| `paideia-as-intrinsic` | AML / DMA / GPU / Vulkan intrinsic bindings | shipped | — |

### Stdlib and tooling binaries

| Crate | Role | Status | Entry docs |
|---|---|:-:|---|
| `paideia-as-stdlib` | `.pdx` stdlib sources (`Option<T>`, `Result<T,E>`, `Str`, …) | shipped | [`design/toolchain/stdlib-phase4.md`](design/toolchain/stdlib-phase4.md) |
| `paideia-as-doc` | `paideia-as doc` HTML doc generator | shipped | [`design/toolchain/tooling-phase4.md`](design/toolchain/tooling-phase4.md) |
| `paideia-as-test` | `paideia-as test` runner (discovers `#[test]`) | shipped | tooling-phase4.md |
| `paideia-fmt` | Source formatter — **COMPLETE as of v0.36.82** | shipped | [`design/toolchain/paideia-fmt-style-spec.md`](design/toolchain/paideia-fmt-style-spec.md) · [`design/toolchain/paideia-fmt-acceptance.md`](design/toolchain/paideia-fmt-acceptance.md) |
| `paideia-lsp` | Language Server Protocol server (tower-lsp) | shipped | [`design/toolchain/lsp-phase3.md`](design/toolchain/lsp-phase3.md) |

### Unicode (COMPLETE — UNI-16, v0.37.0)

| Crate | Role | Status | Entry docs |
|---|---|:-:|---|
| `paideia-as-unicode` | UTF-8 + NFC/NFD/NFKC/NFKD + case folding + segmentation + UAX #31 + UCD + width + fingerprint goldens | **COMPLETE as of v0.37.0** (UNI-0..UNI-16; §4 FROZEN at UNI-11) | [`crates/paideia-as-unicode/README.md`](crates/paideia-as-unicode/README.md) · [`design/toolchain/paideia-as-unicode-spec.md`](design/toolchain/paideia-as-unicode-spec.md) · [`design/toolchain/paideia-as-unicode-api-freeze.md`](design/toolchain/paideia-as-unicode-api-freeze.md) · [`design/toolchain/paideia-as-unicode-acceptance.md`](design/toolchain/paideia-as-unicode-acceptance.md) |

### Semantic shell (R221–R229)

| Crate | Role | Status | Entry docs |
|---|---|:-:|---|
| `paideia-as-shell-lex` | R221.M4 context-tracking lexer (Pipeline / Datalog / Lambda) | shipped | — |
| `paideia-as-shell-ast` | R221.M5–M7 unified AST + span-across-NFC parser | shipped | — |
| `paideia-as-shell-cmd` | R222.M1–M3 command-module functor surface | shipped | — |
| `paideia-as-shell-datalog` | R226.M1–M2 Datalog AST + seminaïve fixpoint evaluator | shipped | — |
| `paideia-as-shell-pds` | R227.M1 `.pds` script header pragma parser | shipped | — |
| `paideia-as-shell-hm` | R225.M1–M3 Hindley–Milner Algorithm W + Rémy rows | shipped | [`design/toolchain/rank-restricted-hm.md`](design/toolchain/rank-restricted-hm.md) |
| `paideia-as-shell-repl` | R229.M1 REPL turn pipeline | shipped | — |
| `paideia-as-shell-completion` | R228.M1 tab-completion substrate | shipped | — |

### Cryptography (post-quantum)

| Crate | Role | Status | Entry docs |
|---|---|:-:|---|
| `paideia-as-crypto` | Narrow crypto primitives for auth and at-rest key sealing | shipped | — |
| `paideia-pq-sign` | Hybrid signing CLI + library (Ed25519 + ML-DSA-65) | shipped | [`docs/release-signing.md`](docs/release-signing.md) · [`design/security/pq-trust-root.md`](design/security/pq-trust-root.md) |
| `paideia-satellite-runtime` | Link-line shim for satellite host tools; routes `.pdx` crypto FFI relocations | shipped | — |

### Non-crate asset trees

```
examples/        17 tutorial .pdx files; see examples/README.md
asm-reference/   Hand-written NASM references for files 13-17
tools/editor/    VS Code / Helix / Emacs / Neovim + tree-sitter grammar
tools/ddc/       DDC orchestrator, differ, allowlist, fixtures
design/          Toolchain + security + effects + encoding + stdlib design docs; see design/README.md
docs/            Operational guides (DDC, signing, determinism)
```

Companion binaries built by `cargo build --workspace --release`: `pax-introspect`, `paideia-link`, `paideia-lsp`, `paideia-fmt`, `paideia-pq-sign`, `ddc-diff`.

## Editor support

`paideia-as lsp` runs the language server. Drop-in configurations live under [`tools/editor/`](tools/editor/), one subdirectory per editor:

- [`tools/editor/vscode/`](tools/editor/vscode/) — VS Code extension (`package.json`, `client/`, `language-configuration.json`).
- [`tools/editor/helix/`](tools/editor/helix/) — `languages.toml` snippet + runtime queries.
- [`tools/editor/emacs/`](tools/editor/emacs/) — `paideia-mode.el` major mode + runtime.
- [`tools/editor/nvim/`](tools/editor/nvim/) — Neovim Lua config + runtime queries.

All four share the tree-sitter grammar under [`tools/editor/tree-sitter-paideia/`](tools/editor/tree-sitter-paideia/). See each editor's `README.md` for installation.

## Examples

The 17 files in [`examples/`](examples/) are a tutorial sequence: each isolates one feature, explains in-source what it is and how it lowers, and (for files 13–17) sits next to its hand-written NASM equivalent under [`asm-reference/algorithms/`](asm-reference/algorithms/). Read [`examples/README.md`](examples/README.md) for the table and status legend.

Highlights to start with:

- `01_hello_module.pdx` — module + structure + `let`.
- `02_functions.pdx` — the canonical `fn x -> x + 1` → `lea rax, [rdi+1]; ret`.
- `03_substructural_lattice.pdx` — the Ordered / Linear / Affine / Unrestricted classes.
- `10_pure_function.pdx` — what `!{}` actually commits to.
- `15_sum_array.pdx` — typed raw pointers + the SIB-form lowering shown above.

## Design documents

Every architectural and implementation decision lives under
[`design/`](design/). The [`design/README.md`](design/README.md)
index groups 60+ docs by subsystem (toolchain, compiler, effects,
encoding, security, stdlib, roadmap) with a Status tag per file
(AUTHORED / Landed / FROZEN / COMPLETE / Closed). Start there when
you want to understand *why* the toolchain works the way it does
rather than *how* it does.

## Version policy

`workspace.version` advances at phase close; git tags land at
acceptance-report landings (see
`feedback_paideia_as_version_discipline` in the project memory —
`workspace.version` + git tag + CHANGELOG entry move together, and
`find-paideia-as.sh` stays strict). Multi-softarch parallel
dispatches write per-primitive scratch CHANGELOGs under
`.plans/scratch/CHANGELOG-<name>.md`; main consolidates them and
bumps `workspace.version` at batch close
(`feedback_paideia_as_parallel_changelog`).

Current status: **`v0.37.0`** — paideia-fmt COMPLETE at FMT-17 and
paideia-as-unicode COMPLETE at UNI-16; both subjects now carry
only polish follow-ups, not regressions. All satellite crates
listed in the table above are consumed by at least one downstream
paideia-os component.

## Where to read next

- [`examples/README.md`](examples/README.md) — language surface tour, in tutorial order.
- [`design/README.md`](design/README.md) — index of every design doc under this repo.
- [`paideia-os/design/toolchain/custom-assembler.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/custom-assembler.md) — the master surface-language specification.
- [`paideia-os/design/toolchain/syntax-reference.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/syntax-reference.md) — normative lexical / grammar reference.
- [`design/toolchain/calling-convention.md`](design/toolchain/calling-convention.md) — register file, R15 handler table, R12 / R13 capability handles, UEFI thunk.
- [`design/toolchain/paideia-link.md`](design/toolchain/paideia-link.md) — PAX format and the four-phase linker contract.
- [`design/toolchain/paideia-fmt-style-spec.md`](design/toolchain/paideia-fmt-style-spec.md) — formatter style spec (FROZEN v1.0).
- [`crates/paideia-as-unicode/README.md`](crates/paideia-as-unicode/README.md) — Unicode substrate quick reference.
- [`docs/release-signing.md`](docs/release-signing.md) — hybrid PQ signing operational guide.
- [`docs/ddc.md`](docs/ddc.md) — Diverse Double Compilation operational guide.
- [`CHANGELOG.md`](CHANGELOG.md) — per-release notes.
- [`STATUS.md`](STATUS.md) — deep-dive on per-component status.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for PR discipline: one issue per PR, design-doc-first, PR-sizing rules, the local pre-push hook activation, and the squash-merge convention.

CI runs `cargo fmt`, `cargo clippy`, `cargo test --workspace`, plus a non-trivial post-commit DDC harness (`tools/ddc/run.sh` + `ddc-diff`) that builds the compiler twice under two host toolchain configurations and byte-compares the artefacts modulo an allowlist. Local `cargo test --workspace` runs the full test suite — currently around 1800 tests across the crate and harness workspace — in under a minute.

## License

MIT. See [`LICENSE`](LICENSE). `paideia-as` is part of the [PaideiaOS](https://github.com/paideia-os) organisation; the cross-OS source of truth for the toolchain design lives in the [`paideia-os/paideia-os`](https://github.com/paideia-os/paideia-os) repository under `design/toolchain/`.
