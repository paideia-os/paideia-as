# UEFI Loader Smoke Tests

## Overview

This crate implements a **UEFI loader smoke test harness** for PaideiaOS phase-2-m6-008. The harness scaffolds and gates a boot smoke test that exercises UEFI firmware + QEMU emulation.

## Scope

This harness covers **phase-2-m6-008 simulation** for the UEFI boot loader:

- **Environment check**: Probe the host for OVMF firmware + QEMU
- **PE/COFF structural build**: Emit a minimal but structurally-valid hello.efi programmatically
- **Boot smoke test** (currently `#[ignore]`'d): Spawn QEMU, boot the .efi, capture serial output

## Phase-2-m6-008 Honesty

A real boot smoke requires:

1. **Meaningful .efi code**: Currently, the PE emitter produces a structurally-valid binary but with zero real UEFI code. M6-009+ will wire the elaborator to emit actual Boot Services calls.
2. **OVMF firmware**: `/usr/share/OVMF/OVMF_CODE.fd` + `/usr/share/OVMF/OVMF_VARS.fd`. Not available in most CI runners.
3. **QEMU**: Not available in most CI runners.

**This is intentional.** The harness ships gated:
- Two tests (env-check + structural-build) are **active** and always run.
- One test (boot-and-print) is **`#[ignore]`'d** until m6-009+ ships real code.
- The harness fails gracefully if OVMF or QEMU are absent.

## Fixtures

The harness uses two test fixtures:

1. **`src/lib.rs` programmatic fixture**: `build_hello_efi()` emits a minimal PE/COFF binary directly via Rust (no source file).
   - Used by `hello_efi_builds_structurally_valid_pe` and `boot_and_print_under_ovmf` tests.
   - Delta baseline for v0.19 UEFI-ABI MVP (phase-2-m6-008).

2. **`fixtures/hello.pdx` source fixture** (NEW in pa-r19-013): 2-arg MS x64 identity function `fn(image, sys) -> image @abi("ms")`.
   - Compiled via paideia-as to PE/COFF using `build_hello_efi_via_paideia_as()`.
   - Used by `boot_and_print_paideia_compiled` test.
   - Validates end-to-end paideia-as → PE/COFF compilation pipeline (v0.19 milestone close).

## Tests

The harness includes four smoke tests (see `tests/smoke.rs`):

### 1. `env_check_describes_availability`

- Probes for OVMF + QEMU on the host.
- Always passes (diagnostic only).
- Prints whether boot test will run.

**Status**: ACTIVE (no `#[ignore]`)

### 2. `hello_efi_builds_structurally_valid_pe`

- Calls `build_hello_efi()` to emit a minimal .efi file.
- Validates:
  - File size ≥ 1 KB
  - Starts with "MZ" magic (DOS header)
  - Contains "PE\0\0" signature at offset 64
- Writes to temp directory; cleaned by OS on reboot.

**Status**: ACTIVE (no `#[ignore]`)

### 3. `boot_and_print_under_ovmf`

- Probes for OVMF + QEMU; skips if absent.
- Builds a hello.efi via `build_hello_efi()` (programmatic, no .pdx source).
- Creates a temporary FAT image with the .efi at `EFI/BOOT/BOOTX64.EFI`.
- Spawns QEMU with OVMF firmware + 30-second hard timeout.
- Captures serial output.
- Asserts output is non-empty.

**Status**: `#[ignore]`'d until m6-009+ ships real code.

### 4. `boot_and_print_paideia_compiled` (NEW in pa-r19-013)

- Probes for OVMF + QEMU; skips if absent.
- Compiles `fixtures/hello.pdx` (2-arg MS x64 identity function) via `cargo run --release -p paideia-as -- build --emit pe-coff ...`.
- Uses `build_hello_efi_via_paideia_as()` helper to invoke paideia-as compiler.
- Creates a temporary FAT image with the compiled .efi at `EFI/BOOT/BOOTX64.EFI`.
- Spawns QEMU with OVMF firmware + 30-second hard timeout.
- Captures serial output.
- Asserts output is non-empty.
- Validates v0.19 milestone closure: full paideia-as → PE/COFF pipeline end-to-end.

**Status**: `#[ignore]`'d (same gating as boot_and_print_under_ovmf).

## How to Run Locally

### Prerequisites

To run all tests, install:

```bash
# Ubuntu/Debian
sudo apt install ovmf qemu-system-x86

# macOS (Homebrew)
brew install qemu
# OVMF availability varies by Homebrew version; check /usr/local/Cellar/
```

### Running Tests

```bash
# Run the two active tests (env-check + structural-build)
cargo test -p paideia-uefi-smoke

# Show all test output
cargo test -p paideia-uefi-smoke -- --nocapture

# Run the ignored boot test (only if OVMF + QEMU are present)
cargo test -p paideia-uefi-smoke -- --ignored --nocapture
```

## Architecture

- **`src/lib.rs`**: Core harness functions
  - `UefiEnv::probe()`: Detect OVMF + QEMU on the host
  - `build_hello_efi(path)`: Emit a minimal structurally-valid PE/COFF .efi
  - `boot_and_capture_serial(env, efi_path)`: Spawn QEMU + capture output
- **`tests/smoke.rs`**: Three smoke tests (env-check, structural build, boot)

## Phase-2-m6-008 → m6-009 Roadmap

| Milestone | Responsibility | Status |
|-----------|-----------------|--------|
| m6-008 | Scaffolding + gating; env-check + structural build active | ✓ This PR |
| m6-009 | Elaborator threads real UEFI code into PE emitter | Future |
| m6-010+ | Boot test ungates; real UEFI interop tested end-to-end | Future |

## Future Work

- **m6-009+**: Wire elaborator to emit Boot Services calls (SetMode, OutputString, etc.)
- **m6-010+**: Ungating boot test when meaningful .efi ships
- **m6-015+**: Post-quantum signature validation on UEFI binaries
