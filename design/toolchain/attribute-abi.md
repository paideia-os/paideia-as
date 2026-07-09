# @abi calling-convention annotation

**Issue:** [#1006](https://github.com/PaideiaOS/PaideiaOS/issues/1006) — Kickoff of MS x64 ABI chain  
**Phase:** PA19-r19-001 (v0.19 UEFI-ABI)  
**Status:** MVP — Annotation parsed, validation gates in place; codegen deferred to #1011

## Syntax

```pdx
let f = fn(x: u64) -> u64 { ... } @abi("ms")
let g = fn(y: u32) -> u32 { ... } @abi("sysv")
let h = fn(z: u16) -> u16 { ... }           // No annotation: paideia default
```

Trailing symbol attribute on `let` bindings. Valid options: `"ms"` (Microsoft x64 ABI) and `"sysv"` (System V AMD64 ABI). Case-sensitive lowercase only. String argument required, enclosed in double quotes.

## Semantics

- **`@abi("ms")`** — Specifies Microsoft x64 calling convention for the function binding. Prologue/epilogue and register discipline are deferred to issue #1011. At present, the annotation is accepted at parse time but emission is gated with diagnostic **U1620** (deferred implementation). When #1011 ships, functions marked `@abi("ms")` will follow UEFI/Windows x64 ABI calling conventions: rcx, rdx, r8, r9 for integer arguments; callee-saved: rbx, rsp, rbp, rsi, rdi, r12-r15; caller-saved: rax, rcx, rdx, r8-r11.

- **`@abi("sysv")`** — Specifies System V AMD64 ABI (Unix/Linux calling convention). Currently the same path as unannotated bindings; explicit annotation is allowed for forward compatibility and intent clarity.

- **No annotation (default)** — Functions without `@abi` follow the paideia default calling convention (currently SysV-compatible). This is NOT the same as `@abi("sysv")` — there is semantic distinction for #1017 SysV↔MS bridge thunk decisions.

## Restrictions

### Non-lambda bindings (P0286)

`@abi` is only valid on function-shaped bindings (lambdas). Applying `@abi` to a non-function value triggers diagnostic **P0286**:

```pdx
let x : u64 = 42 @abi("ms")      // ERROR P0286: @abi on non-lambda

let payload : [u8; 8] = uninit @abi("ms")  // ERROR P0286
```

Rationale: calling conventions make sense only for callable entities.

### MS x64 codegen gate (U1620)

Codegen for `@abi("ms")` is not yet implemented (pending #1011). Any `@abi("ms")` lambda triggers diagnostic **U1620** at build time:

```pdx
let uefi_init : () -> () = fn() { } @abi("ms")  // ERROR U1620: not yet emittable
```

`@abi("sysv")` lambdas and unannotated lambdas continue to emit normally. U1620 exists to prevent silent miscompilation — the annotation is accepted at parse time, but the full ABI is deferred.

## Diagnostic codes

| Code | Severity | Context | Recovery |
|------|----------|---------|----------|
| **P0285** | Error | Parse-time: invalid @abi string (not "ms" or "sysv"), non-string argument, missing parens, empty string. | Supply a valid lowercase string ("ms" or "sysv"). Message lists valid options. |
| **P0286** | Error | Validation pass: @abi applied to non-lambda binding. | Apply @abi only to function-shaped bindings. |
| **U1620** | Error | Emit-time gate: @abi("ms") lambda bodies not yet emittable. | Defer to issue #1011 or use @abi("sysv") / no annotation. |

## Interaction with other attributes

Independent composition with @align, @ring, @link_section:

```pdx
// Valid: @abi can coexist with @align, @ring, @link_section
let f = fn(x: u64) -> u64 { x + 1 } @align(64) @abi("ms")

// Valid: order does not matter (all trailing)
let g = fn() {} @abi("sysv") @align(16)

// Invalid: @abi on non-lambda still triggers P0286 even with other attributes
let x = 42 @abi("ms") @align(8)  // ERROR P0286
```

## Roadmap

- **v0.19 (PA19-r19-001, this PR)** — Annotation parsing, AST + IR representation, P0285/P0286/U1620 gates.
- **v0.19 (PA19-r19-011, #1011)** — MS x64 prologue/epilogue emitter. Remove U1620 gate. Implement callee-saved discipline, parameter-passing rules, rsp alignment.
- **v0.19 (PA19-r19-017, #1017)** — SysV↔MS bridge thunks for interop. Thunk generation for calls crossing ABI boundaries (e.g., Rust-hosted tests calling into `@abi("ms")` UEFI code).

## MVP rationale

Why `"ms"` not `"ms_x64"`? Rust-idiomatic brevity. Current target is x86_64; if a future paideia target requires `"ms_arm64"` or other MS ABIs, the enum can be extended then (e.g., `pub enum CallingConvention { Ms, MsArm64, Sysv }`). At MVP, string-level disambiguation is unnecessary.

Why `None` on LetInfo means "paideia default", not `Some(Sysv)`? The distinction matters for #1017. Bridge thunk generation may need to know whether a function is explicitly SysV-annotated (`@abi("sysv")`) or implicitly paideia default (no annotation). Unannotated bindings are treated as "native paideia" in the thunk decision logic.

## Testing

- Parser: 9 unit tests in `parse_item/let_item.rs` (abi_ms_parses, abi_sysv_parses, abi_unknown_string_p0285, abi_uppercase_ms_p0285, abi_empty_string_p0285, abi_non_string_arg_p0285, abi_missing_parens_p0285, abi_duplicate_p0250, abi_with_align_and_link_section_all_accepted).
- IR: 1 unit test in `let_meta.rs` (let_info_with_abi_records_all_fields).
- Integration: 4 build-emit tests in `build_emit/abi_probe.rs` (abi_ms_lambda_emits_u1620, abi_sysv_lambda_builds_cleanly, abi_absent_lambda_builds_cleanly, abi_on_non_lambda_p0286).

## References

- AST: `crates/paideia-as-ast/src/items.rs` — CallingConvention enum, ItemData::Let.abi field.
- Parser: `crates/paideia-as-parser/src/parse_item/let_item.rs` — parse_abi_attr, LetSymbolAttrs struct.
- IR: `crates/paideia-as-ir/src/let_meta.rs` — CallingConvention enum, LetInfo.abi field, with_abi constructor.
- Build: `crates/paideia-as/src/cmd_build.rs` — P0286/U1620 validation passes.
- Diagnostics: `crates/paideia-as-diagnostics/catalog.toml` — P0285, P0286, U1620 entries.
