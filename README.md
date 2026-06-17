# paideia-as

The custom assembler for PaideiaOS. Rust implementation, Linux development hosts.

## Status

Phase-1 skeleton. The actual implementation will land progressively per the milestone plan documented in the main PaideiaOS repo.

## Design

The full design specification lives in the PaideiaOS monorepo:

- [Custom assembler design](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/custom-assembler.md) — top-level
- [Syntax reference](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/syntax-reference.md)
- [Calling convention](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/calling-convention.md)
- [Diagnostic catalog](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/diagnostics.md)
- [Optimization-pass catalog](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/optimization-passes.md)
- [PAX binary format + linker](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/paideia-link.md)
- [DWARF + vendor extensions](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/debug-info.md)
- [Editor support / LSP](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/editor-support.md)
- [Phase-1 macros (restricted)](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/macros-phase1.md)
- [Milestones](https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/milestones.md)

## Workspace layout

```
crates/
├── paideia-as            # CLI binary entry point
├── paideia-as-lexer      # context-aware lexer (pipeline / datalog / lambda)
├── paideia-as-parser     # recursive-descent parser
├── paideia-as-ast        # unified AST
├── paideia-as-types      # substructural lattice + Hindley-Milner inference
├── paideia-as-effects    # algebraic effect rows + handler inference
├── paideia-as-elaborator # typed elaborator (Idris/Lean reflection lineage)
├── paideia-as-ir         # typed core IR + ANF + effect-handler-rewrite passes
├── paideia-as-emitter-elf  # ELF64 backend (kernel images)
├── paideia-as-emitter-pax  # PAX-fragment backend (paideia-link consumer)
├── paideia-as-emitter-pe   # PE/COFF backend (UEFI loader)
├── paideia-as-dwarf      # DWARF 5 + PaideiaOS vendor extensions
├── paideia-as-diagnostics  # SARIF + human + LSP diagnostic emission
├── paideia-as-linker     # paideia-link + PAX assembly
├── paideia-lsp           # LSP server
├── paideia-fmt           # formatter
└── paideia-pq-sign       # PQ signing tool
```

## Build

```
cargo build         # whole workspace
cargo test          # run tests
cargo run -- check  # invoke CLI
```

## License

See [LICENSE](LICENSE).
