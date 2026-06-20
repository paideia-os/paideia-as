# paideia-as

`paideia-as` is the custom x86_64 assembler and surface-language compiler for [PaideiaOS](https://github.com/paideia-os/paideia-os). It compiles `.pdx` source through a typed elaborator into ELF64, PAX (PaideiaOS-native), or PE/COFF (UEFI) objects. The surface language combines a **substructural type lattice** (Ordered / Linear / Affine / Unrestricted), **row-polymorphic algebraic effects with handlers**, **capability-based discipline**, and **post-quantum hybrid signing** of release artifacts.

## State of the world

**Phase 2 substrate complete.** Eleven milestones (m1–m11) closed across PRs #347–#471, tagged `v0.2.0`. Approximately 1614 workspace tests across 26+ crates and 23+ test harnesses; `cargo test --workspace` runs in under a minute. The toolchain is ready for decision-gate **G4** stamping subject to the operational items in [`docs/g4-prep.md`](docs/g4-prep.md).

CI workflows (`ci.yml`, `cross-build.yml`, `ddc.yml`, `release.yml`) are shipped but currently disabled at the org level pending GitHub Actions billing restoration. Local `cargo test --workspace` is the gate today.

See [`CHANGELOG.md`](CHANGELOG.md) for the full v0.2.0 release notes and [`STATUS.md`](STATUS.md) for per-milestone closure narratives.

## What you get

- **Four emit formats** (selected via `paideia-as build --emit <format>`):
  - `placeholder` — pipeline smoke target; writes a BLAKE3 hash of the lowered IR.
  - `elf64` — x86_64 SystemV ELF64 relocatable object (kernel-image target).
  - `pax` — PaideiaOS Architectural Executable: 96-byte header, 64-byte section-table descriptors, twelve vendor section content types, BLAKE3 content hash, PQ-signature slot.
  - `pe-coff` — Microsoft x64 / UEFI PE/COFF binary with `.reloc`, imports, and a SysV ↔ MS-x64 thunk.
- **`paideia-link`** — four-phase linker (parse / resolve / relocate / emit) over PAX inputs, with capability-binding resolution and `B1700`/`B1701` diagnostics.
- **`paideia-lsp`** — tower-lsp 0.20 server with eleven `textDocument/*` handlers: sync, diagnostics, hover, definition, references, completion, code actions, formatting, semantic tokens, inlay hints. Backed by an LRU parse cache and a hand-rolled Salsa-style query engine.
- **Tree-sitter grammar + four editor recipes** — VS Code, Helix, Emacs, Neovim. See [`tools/editor/`](tools/editor/).
- **`paideia-pq-sign`** — hybrid Ed25519 (RFC 8032 §7.1 KAT) + ML-DSA-65 (FIPS-204) signing with AND-verification semantics. 1984-byte public key, 3373-byte signature (≈ 3.4 KB). Includes a soft-HSM (Argon2id KDF + ChaCha20-Poly1305 AEAD; development-only).
- **DDC harness** — Diverse Double Compilation per Wheeler 2005 as the trusting-trust mitigation. Byte-level differ, allowlist, format-gate corpus, and `SOURCE_DATE_EPOCH` + `PDX_PATH_PREFIX_MAP` determinism contract.
- **Optimization pass catalog** — eleven passes (`O1500`–`O1512`): peephole, instruction scheduling, macro fusion, DSE, REX/EVEX tightening, branch hint, alignment, pool constants, tail-call elimination, loop unrolling, and catalog composition. See *Honesty about scaffolding* below.
- **DWARF 5** with three PaideiaOS vendor sections — `.debug.paideia.caps`, `.debug.paideia.effects`, `.debug.paideia.sig` — registered under vendor ID `paideia`.

## Quick start

```sh
# Build the CLI.
cargo build --release -p paideia-as

# Parse-check an example.
./target/release/paideia-as check examples/01_hello_module.pdx

# Emit a PAX object.
./target/release/paideia-as build --emit pax examples/01_hello_module.pdx -o /tmp/hello.pax

# Inspect what was produced.
./target/release/pax-introspect /tmp/hello.pax
```

The four `--emit` values are `placeholder`, `elf64`, `pax`, and `pe-coff`. With no `--emit` flag, `paideia-as build` writes a `<stem>.placeholder` smoke artifact next to the input.

## Repository layout

```
crates/                       Workspace crates (18 in total)
  paideia-as/                   CLI front end + build / check / sign dispatch
  paideia-as-{lexer,parser,ast} Front end
  paideia-as-{types,effects}    Substructural lattice + effect rows
  paideia-as-elaborator         Typed elaborator (reflection + walkers)
  paideia-as-ir                 Typed core IR + ANF + effect rewrite
  paideia-as-encoder            Shared x86_64 instruction encoder
  paideia-as-emitter-{elf,pax,pe}
                                Three backend emitters
  paideia-as-dwarf              DWARF 5 + vendor extensions
  paideia-as-diagnostics        SARIF + human + LSP rendering, catalog
  paideia-as-linker             paideia-link (PAX → executable PAX)
  paideia-lsp                   Language server
  paideia-fmt                   Minimum-viable formatter
  paideia-pq-sign               Hybrid PQ signing CLI + library
tests/                        Test harnesses (23+ workspace members)
  end-to-end, linearity-regression, effects-corpus, reflection-corpus,
  modules-multifile, pq-corpus, lsp-harness, pax-load-smoke, uefi-smoke,
  cross-build, opt-regression/*, migration-smoke/cap, e2e
tools/
  cross-build/                  NASM ↔ paideia-as ABI-parity smoke
  ddc/                          DDC orchestrator, differ, allowlist, fixtures
  editor/                       VS Code / Helix / Emacs / Neovim configs +
                                tree-sitter-paideia grammar
examples/                     17 tutorial `.pdx` files (see below)
asm-reference/                Hand-written NASM references for files 13–17
design/
  toolchain/                    Phase-2 outcome appendices (authoritative)
  security/                     PQ trust-root spec + phase-2 outcome
docs/                         Operational guides (DDC, determinism, signing, G4)
scripts/gdb/                  GDB Python helper for PaideiaOS-native debug info
.github/workflows/            CI workflows (currently disabled; see status above)
```

## Design documentation

Phase-2 outcome appendices live in this repo and are now authoritative for the toolchain pieces owned by `paideia-as`. The upstream `paideia-os/paideia-os/design/toolchain/` documents remain the source of truth for the cross-cutting OS-level specifications.

Local (authoritative for paideia-as):

| Document | Scope |
|---|---|
| [`design/toolchain/abi.md`](design/toolchain/abi.md) | Calling convention, register-file partitioning, ABI version |
| [`design/toolchain/bootstrap.md`](design/toolchain/bootstrap.md) | Dual stage-0 (NASM + GNU as) decision |
| [`design/toolchain/calling-convention.md`](design/toolchain/calling-convention.md) | R15 handler table, R12/R13 caps, UEFI thunk |
| [`design/toolchain/debug-info.md`](design/toolchain/debug-info.md) | DWARF 5 + `paideia` vendor sections |
| [`design/toolchain/macros-phase1.md`](design/toolchain/macros-phase1.md) | Pattern macros (phase 1) + reflection bridge (phase 2 outcome) |
| [`design/toolchain/optimization-passes.md`](design/toolchain/optimization-passes.md) | The 11-pass catalog (O1500–O1512) |
| [`design/toolchain/paideia-link.md`](design/toolchain/paideia-link.md) | PAX format + linker contract |
| [`design/toolchain/phase-transition-2.md`](design/toolchain/phase-transition-2.md) | Phase-2 retrospective + disposition table |
| [`design/security/pq-trust-root.md`](design/security/pq-trust-root.md) | Hybrid PQ signing + delegation scope |

Upstream (cross-OS source of truth):

- [`paideia-os/design/toolchain/custom-assembler.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/custom-assembler.md) — master spec
- [`paideia-os/design/toolchain/syntax-reference.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/syntax-reference.md) — normative lexical / grammar reference
- [`paideia-os/design/toolchain/editor-support.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/editor-support.md) — editor + LSP design
- [`paideia-os/design/toolchain/milestones.md`](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/milestones.md) — milestone plan

## Examples

[`examples/`](examples/) is a 17-file tutorial catalog. Each `.pdx` file isolates one feature and explains in the file itself what it is, why it exists, and how it lowers. Files 13–17 are paideia-as equivalents of the hand-written NASM algorithms under [`asm-reference/algorithms/`](asm-reference/algorithms/) (factorial, fibonacci, sum_array, memcpy, strlen) so the surface-language ↔ assembly mapping is reviewable side-by-side. See [`examples/README.md`](examples/README.md) for the full table.

## Building, testing, and verification

### Build

```sh
cargo build --release -p paideia-as
```

The CLI binary lands at `target/release/paideia-as`. Companion binaries (`pax-introspect`, `paideia-link`, `paideia-lsp`, `paideia-fmt`, `paideia-pq-sign`, `ddc-diff`) are built when the relevant crate is selected, or by `cargo build --workspace --release`.

### Test

```sh
cargo test --workspace
```

Runs ~1614 tests across the crate suites and 23+ harness members. Several harness tests are `#[ignore]`d behind probe-detected host requirements (`nasm`, `objdump`, OVMF firmware, `qemu-system-x86_64`) or behind Phase-3 IR work; the active set is the gate.

### DDC verification

```sh
bash tools/ddc/run.sh
./target/release/ddc-diff \
    tools/ddc/out/a/paideia-as \
    tools/ddc/out/b/paideia-as \
    tools/ddc/allowlist.toml
```

Exit codes: `0` match modulo allowlist, `1` unallowlisted divergence, `2` IO / usage error. The harness builds `paideia-as` twice under two host toolchain configurations, then byte-compares. See [`docs/ddc.md`](docs/ddc.md) for the operational guide and [`design/toolchain/bootstrap.md`](design/toolchain/bootstrap.md) for the dual stage-0 commitment.

For build determinism inputs (`SOURCE_DATE_EPOCH`, `PDX_PATH_PREFIX_MAP`) see [`docs/build-determinism.md`](docs/build-determinism.md). For release-artifact signing see [`docs/release-signing.md`](docs/release-signing.md).

## CI status

Four GitHub Actions workflows are versioned in [`.github/workflows/`](.github/workflows/) and parseable today:

| Workflow | Purpose | Activation |
|---|---|---|
| `ci.yml` | Push / PR fmt + clippy + test gate | disabled — billing |
| `cross-build.yml` | NASM ↔ paideia-as ABI-parity smoke | disabled — billing |
| `ddc.yml` | Nightly DDC, advisory, 30-day artifacts | disabled — billing |
| `release.yml` | Tag-triggered DDC hard-fail + sign | disabled — billing |

All four are disabled at the org level pending GitHub Actions billing restoration; they activate without code changes once billing is restored. Local `cargo test --workspace` is the gate today.

## Honesty about scaffolding

Several Phase 2 deliverables ship as architecturally complete scaffolds whose end-to-end activation depends on Phase 3 IR work:

- **m9 optimization passes** emit "would-fire" markers. Per-pass helpers (`schedule_block`, `dse_block`, `tco_blocker`, `is_unroll_safe`, …) are callable and unit-tested today. Flipping each pass to a real rewrite is a single PR once the kind-only IR exposes per-node x86_64 mnemonics.
- **m8 LSP semantics** (hover / definition / references / completion) currently use lexical stand-ins with `linear:` / `affine:` / `cap:` prefix recognition. The m8-008 query engine is in place; elaborator-driven per-position type queries land in Phase 3.
- **m1 walker diagnostics** run on real `.pdx` source through the CLI but mostly stay silent because the lowered IR is still kind-only. Diagnostics light up as m2 / m3 / m5 thread structured payloads through the IR — most do today; a small set remains gated.

Each scaffold carries a forward pointer in the source, in [`STATUS.md`](STATUS.md), and in [`design/toolchain/phase-transition-2.md`](design/toolchain/phase-transition-2.md) §2.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for PR discipline (one issue per PR, design-doc-first, sizing rules, pre-push hook activation). Phase 2 was driven primarily by an LLM-orchestrated autonomous loop (softarch → workerbee → debugger chain documented in `phase-transition-2.md` §3); Phase 3 is expected to mix manual and automated work.

## License

MIT. See [`LICENSE`](LICENSE). `paideia-as` is part of the [PaideiaOS](https://github.com/paideia-os) organisation.

## What's next

Phase 3 carries the deferrals catalogued in [`design/toolchain/phase-transition-2.md`](design/toolchain/phase-transition-2.md) §5: stage-0b GNU `as` entry-point source, per-node IR instruction payloads (to flip the m9 catalog from "would-fire" to real rewrites), elaborator-driven LSP semantics, hardware HSM integration, the PaideiaOS kernel link + QEMU boot test, NIST ACVP vectors for ML-DSA, row-polymorphic scope subsumption, per-rewrite peephole codes (`O1501` / `O1502` already reserved), remainder-loop generation for `#[unroll(n)]`, and signature timestamping / revocation.
