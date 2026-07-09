# Compile-Time Embed Primitives (`@guid`, `@include_bytes`, `@include_str`, `@include_bytes_as_str`)

**Status**: v0.19 (UEFI-ABI Milestone)

**Issues**: #1012 (InlineBytes primitive), #1013 (`@include_bytes`), #1014 (`@include_str` + `@include_bytes_as_str`)

## Overview

Paideia supports four compile-time embed directives that read data at parse time and emit it directly into `.rodata` (or `.data` for mutable bindings):

| Directive | Payload | Type | Validation |
|---|---|---|---|
| `@guid("...")` | 16 bytes, UEFI mixed-endian | `[u8; 16]` | GUID string format |
| `@include_bytes("path")` | File bytes, verbatim | `[u8; N]` | none |
| `@include_str("path")` | File bytes, verbatim | `str` | UTF-8 well-formedness |
| `@include_bytes_as_str("path")` | File bytes, verbatim | `str` | none |

The byte-typed directives (`@guid`, `@include_bytes`) share the `ExprData::InlineBytes(Vec<u8>)` AST variant and route through `IrKind::InlineBytes`. The str-typed directives (`@include_str`, `@include_bytes_as_str`) share the `ExprData::InlineStr(Vec<u8>)` AST variant and route through `IrKind::StringLiteral`, reusing the same rodata inlining + interning path as regular string literals.

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
- Allows `..` traversal (see path-resolution section below).
- Rejects absolute paths, empty paths, non-regular targets.
- Rejects files > 16 MiB.
- Emits P0279 (path/IO) or P0280 (oversize).

### `@include_str`

```pdx
let banner : str = @include_str("data/banner.txt")
```

- Reads a file the same way `@include_bytes` does, then **validates UTF-8** before allocating the AST node.
- On invalid UTF-8, emits **P0281** with the byte offset of the first invalid sequence (from `Utf8Error::valid_up_to()`).
- Empty file is valid UTF-8 (`str::from_utf8(&[]) == Ok("")`); accepted with no diagnostic.
- BOM handling: **UTF-8 BOM (`EF BB BF`) is included verbatim**. Not stripped, not rejected. Rationale: matches Rust's `include_str!` and Zig's `@embedFile`; kernel/UEFI blobs may require exact-byte fidelity; UTF-8 BOM is itself valid UTF-8 so `from_utf8` passes.
- Shares P0279 (path/IO) and P0280 (oversize) with `@include_bytes`.

### `@include_bytes_as_str`

```pdx
let raw_utf8 : str = @include_bytes_as_str("data/precomputed.utf8")
```

- Reads a file the same way `@include_bytes` does, produces a `str`-typed payload, but **skips UTF-8 validation**.
- Intended for cases where the file is guaranteed valid UTF-8 by construction (build-script output, tooling artifact) and the validation cost is worth skipping.
- Shares P0279/P0280 with the other directives.
- Does NOT emit P0281 even on non-UTF-8 content — the caller asserts validity.
- **Unsafe-context gating for this directive is a deferred follow-up** (paideia-as does not yet have unsafe-context enforcement machinery workspace-wide). Until that lands, callers assume responsibility informally.

## Path Resolution

Applies to `@include_bytes`, `@include_str`, and `@include_bytes_as_str`:

1. Extract `source_dir` from the input `.pdx` file's parent directory (set by `cmd_build.rs` and `cmd_check.rs` via `Parser::with_source_dir(...)`).
2. Resolve the relative path against `source_dir`.
3. Fall back to CWD if `source_dir` is not set (test-only path).
4. Check metadata: existence, is-regular-file, readable.
5. Check size against the 16 MiB cap BEFORE reading (reject-before-allocate discipline).

`..` traversal is **allowed** without an escape check. Rationale: compile-time embeds are inside the same trust boundary as source imports — the parser is already free to read the source file, so reading a sibling data file is not a privilege escalation. Absolute paths remain rejected as a syntactic (not semantic) safeguard: keeps builds hermetic and reproducible.

## Type Annotation Guard (T0558)

For **byte-typed** directives (`@guid`, `@include_bytes`), the declared `[u8; N]` type annotation must match the actual payload length:

```pdx
let payload : [u8; 8] = @include_bytes("file.bin")  // OK when file is 8 bytes
let payload : [u8; 7] = @include_bytes("file.bin")  // Error T0558: declared 7, got 8
```

If `N != bytes.len()`, the elaborator emits **T0558** and skips the data entry, preventing silent truncation or padding.

T0558 does **not** apply to str-typed directives. `@include_str` and `@include_bytes_as_str` produce `str`-typed values; if a user writes `let X : [u8; N] = @include_str(...)`, the existing `IrKind::StringLiteral` → `[u8; N]` inlining path applies its own truncate/pad semantics (a deliberate feature for embedding text into fixed-size buffers).

## Lowering and Emission

**Byte path** (`@guid`, `@include_bytes`):
- AST: `ExprData::InlineBytes(Vec<u8>)`
- IR: `IrKind::InlineBytes` with the payload in the `literal_bytes` side-table
- Emit: `cmd_build.rs` InlineBytes dispatch branch, T0558 guard, then `DataEntry` into `.rodata` (or `.data` for mutable bindings), 1-byte alignment

**Str path** (`@include_str`, `@include_bytes_as_str`):
- AST: `ExprData::InlineStr(Vec<u8>)` — stored as bytes, not `String`, so `@include_bytes_as_str` cannot violate Rust's `String` UTF-8 invariant
- IR: `IrKind::StringLiteral` with the payload in `literal_bytes` (reuses the regular-string-literal side-table)
- Emit: reuses the existing `IrKind::StringLiteral` emit path in `cmd_build.rs` — `let X : [u8; N] = ...` inlines into rodata; other bindings intern under `__str_<hash>`

## Diagnostic Codes

- **P0278**: Malformed `@guid` literal (format, length, invalid dashes, non-hex).
- **P0279**: `@include_bytes` / `@include_str` / `@include_bytes_as_str` path or IO error (not found, permission denied, not a regular file, empty path, absolute path, missing paren, missing string literal).
- **P0280**: File exceeds the 16 MiB compile-time cap.
- **P0281**: `@include_str` — file is not valid UTF-8. Message includes the byte offset of the first invalid sequence.
- **T0558**: InlineBytes size mismatch (declared `[u8; N]` != actual bytes). Byte-typed directives only.

## Future Work

- **pa-r19-008-followup**: `--include-dir` search list (multiple resolution paths).
- **pa-r19-009-followup**: unsafe-context gating for `@include_bytes_as_str` once workspace-wide unsafe enforcement lands.
- Security audit: evaluate `..` traversal risk vs. use cases.
