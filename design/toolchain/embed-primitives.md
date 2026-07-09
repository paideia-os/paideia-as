# Compile-Time Embed Primitives (`@guid`, `@include_bytes`)

**Status**: v0.19 (UEFI-ABI Milestone)

**Issue**: #1012 (InlineBytes), #1013 (include_bytes)

## Overview

Paideia supports compile-time file embedding via the `@include_bytes` directive and GUID literal encoding via `@guid`. Both produce `ExprInlineBytes` AST nodes that carry raw byte payloads embedded directly into the emitted object file.

## Syntax

### `@guid`

```pdx
let my_guid : [u8; 16] = @guid("12345678-1234-1234-1234-123456789abc")
```

- Parses a GUID string in RFC 4122 format (8-4-4-4-12 hexadecimal groups).
- Encodes into 16 bytes using UEFI mixed-endian byte order:
  - Data1 (first 8 hex): u32 little-endian
  - Data2 (second 4 hex): u16 little-endian
  - Data3 (third 4 hex): u16 little-endian
  - Data4 (remaining 12 hex): 8 bytes in raw order
- Emits P0278 on malformed GUID strings.

### `@include_bytes`

```pdx
let payload : [u8; 8] = @include_bytes("data/file.bin")
```

- Reads a file relative to the source `.pdx` file's directory.
- Embeds the exact file contents as a byte array.
- Allows relative paths using `..` traversal. **Rationale:** compile-time embeds are inside the same trust boundary as source imports — the parser is already free to read the source file, so reading a sibling data file is not a privilege escalation.
- Rejects absolute paths (must use relative paths only).
- Rejects empty paths.
- Rejects files > 16 MiB to prevent accidental large-file embeds.
- Emits P0279 for path errors (not found, permission denied, not a file, empty path, absolute path).
- Emits P0280 for oversized files (> 16 MiB).

## Type Annotation Guard (T0558)

Both `@guid` and `@include_bytes` must match their declared `[u8; N]` type annotation:

```pdx
let payload : [u8; 8] = @include_bytes("file.bin")  // OK: file is 8 bytes
let payload : [u8; 7] = @include_bytes("file.bin")  // Error T0558: declared 7, got 8
```

If the declared length `N` and actual byte count differ, the elaborator emits **T0558** and skips the data entry. This prevents silent data truncation or padding mismatches.

### Rationale

- `@guid` always produces exactly 16 bytes; the annotation must be `[u8; 16]`.
- `@include_bytes` must match the actual file size; if the file changes, the type annotation must be updated to match.
- T0558 catches bugs where file updates invalidate hardcoded array lengths.

## Path Resolution

Path resolution follows these rules:

1. Extract the `source_dir` from the input `.pdx` file's parent directory (set by `cmd_build.rs` and `cmd_check.rs`).
2. Resolve the relative path against `source_dir`.
3. Fall back to CWD if `source_dir` is not set (for tests or CLI-only invocations).
4. Check metadata: existence, is-regular-file, readable.
5. Check size: reject > 16 MiB without reading.

## Lowering and Emission

Both inline bytes expressions lower to `IrKind::InlineBytes` nodes carrying the byte payload in the `literal_bytes` side-table.

At emit time (`cmd_build.rs`):
1. Locate the Let binding's `[u8; N]` type annotation.
2. Call `declared_array_len_from_type()` to extract `N`.
3. If `N != bytes.len()`, emit T0558 and skip (preventing silent data mismatch).
4. Otherwise, emit a `DataEntry::new_rodata()` or `DataEntry::new_data()` with:
   - Symbol name derived from the binding name.
   - Byte payload as-is (1-byte alignment, 8-byte if relocs present).

## Future Work

- **#1014**: `@include_str` for UTF-8 text embedding.
- **pa-r19-008**: `--include-dir` search list (multiple resolution paths).
- Security audit: evaluate `..` traversal risk vs. use cases.

## Diagnostic Codes

- **P0278**: Malformed GUID literal (format, length, invalid dashes, non-hex).
- **P0279**: `@include_bytes` path error (not found, permission denied, not a file, empty path, absolute path).
- **P0280**: `@include_bytes` file too large (> 16 MiB).
- **T0558**: InlineBytes size mismatch (declared `[u8; N]` != actual bytes).
