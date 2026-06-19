# Cross-Build Smoke Tests

Integration tests for the cross-build infrastructure that verify NASM and paideia-as emit identical instruction sequences.

## Tests

### `uefi_loader_cross_build_succeeds`

Runs `tools/cross-build/tools/cross-build.sh tools/cross-build/fixtures/uefi_loader/` and verifies exit code 0. This test requires:

- `nasm` (NASM assembler)
- `objdump` (GNU binutils, part of the m1-013 tooling stack)
- A working paideia-as build (`cargo run -p paideia-as`)

To run locally with the UEFI loader fixture:

```bash
cargo test --test uefi-loader -- --ignored --nocapture
```

### `cross_build_script_exists`

Verifies that `tools/cross-build/tools/cross-build.sh` exists. Always enabled.

### `uefi_loader_fixture_files_present`

Verifies that the UEFI loader fixture files (`module.asm`, `module.pdx`, `module.expect-mnemonics.txt`) all exist. Always enabled.

## Fixtures

Each fixture directory under `tools/cross-build/fixtures/` requires:

- `module.asm`: NASM source code
- `module.pdx`: paideia-as source code
- `module.expect-mnemonics.txt`: expected instruction sequence (one mnemonic per line)
- `README.md`: fixture documentation

## Phase-2-m6-009 Notes

The UEFI loader fixture is a placeholder matching the `add_one` pattern for phase-2 compatibility. Real UEFI loader semantics (EFI_HANDLE, EFI_SYSTEM_TABLE, ConOut->OutputString) will be added when m6-010+ wires full codegen from the elaborator.
