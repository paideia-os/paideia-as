# PAX Load Smoke Tests

## Overview

This crate implements a **mock PaideiaOS supervisor** that simulates loading PAX (PaideiaOS Architectural Executable) files without executing any code. It validates the harness's ability to:

- Load and parse PAX files from disk
- Extract and parse capability/effect/export/import/symbol sections
- Dispatch to named exports (symbolic, no execution)
- Identify entry-point symbols (phase-2-m12 definition: Global + Default visibility)

## Scope

This harness covers **phase-2-m12 simulation** for the mock supervisor:

- Reads PAX metadata (headers, section table, capability descriptors)
- Parses structured sections into Rust types
- Validates entry-point dispatch paths symbolically
- Does **not** execute any code; dispatch is metadata validation only

## Tests

The harness includes six smoke tests (see `tests/smoke.rs`):

1. **`mock_supervisor_loads_hello_world`** (AC 1)
   - Programmatically builds a minimal PAX with Executable flag
   - Loads from temp file
   - Asserts header is present and Executable flag is set

2. **`mock_supervisor_dispatch_to_hello_main`** (AC 1)
   - Loads hello-world PAX
   - Calls `.dispatch()` with the hash of "hello_main" export
   - Expects `Some(CapDescriptor)` matching the export

3. **`mock_supervisor_consumes_paideia_caps`** (AC 2)
   - Loads hello-world PAX
   - Calls `.cap_binding_sites(0)`
   - Asserts 1 MmioMemCap entry with Linear linearity class

4. **`mock_supervisor_parsed_bindings_snapshot`** (AC 3)
   - Loads hello-world PAX
   - Validates section count (5), cap count (1), export count (1), symbol count (1), effect row count (1)
   - Checks BLAKE3 name hash on export matches "hello_main"

5. **`mock_supervisor_rejects_non_pax`**
   - Attempts to load file with invalid magic
   - Expects `LoadError::NotPax`

6. **`mock_supervisor_returns_entry_point_symbol`**
   - Loads hello-world PAX
   - Calls `.entry_point(0)`
   - Asserts entry point has Global binding and Default visibility

## Test Fixtures

PAX test fixtures are **built programmatically** in the test harness using the public API of `paideia-as-emitter-pax`. No pre-built `.pdx` source files are required.

The `build_hello_world_pax()` function constructs a minimal PAX with:
- `.code` section (16 bytes of placeholder)
- `.symtab` section (1 symbol: "hello_main" Global/Default)
- `.paideia.caps` section (1 entry: MmioMemCap)
- `.exports` section (1 entry: "hello_main")
- `.paideia.effects` section (1 entry: empty effects row)

All offsets, sizes, and BLAKE3 hashes are computed correctly at build time.

## Architecture Coverage

- **x86-64** (Architecture::X86_64) PAX files
- Relocatable and Executable header flags
- All standard section types (code, symtab, caps, effects, exports, imports)

## Future Work

- Phase-2-m13+: Add tests for relocatable objects
- Phase-2-m14+: Add tests for multi-symbol entry-point selection
- Phase-2-m15+: Add post-quantum signature validation
