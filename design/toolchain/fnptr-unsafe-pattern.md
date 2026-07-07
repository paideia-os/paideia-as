# Function-Pointer Dispatch Unsafe Pattern (pa-r14-009)

## Motivation

paideia-os Phase 6+ requires trait-object-like dispatch for dynamically-bound operations:
- **VFS vops tables** (virtual file-system operations)
- **Driver dispatch** (device-driver operations)
- **UEFI protocols** (future phases)
- **Syscall dispatch** (paideia-os#536)

v0.14 does not have first-class, compile-time-typed function-pointer types — that capability ships in v0.17 (pa-r17-004, `fn_ptr_types`). Until then, **paideia-os drivers and kernel code can express dispatch manually via unsafe loads and register-indirect calls**, leveraging primitives landed in pa-r13-003 (#932).

This pattern documents the safe boundaries and invariants for that unsafe code.

## Pattern: Manual Load + Call

### Register-Indirect Call

Given a struct pointer in `rdi` and a function pointer at offset `+24`:

```asm
unsafe {
  mov rax, [rdi + 24]   // load vops[3] into rax (offset = index * 8)
  call rax              // register-indirect call (FF D0)
}
```

**Encoded**: `48 8B 47 18` (mov) + `FF D0` (call)

### Memory-Indirect Call (base + displacement)

Skip the register load; call directly from memory:

```asm
unsafe {
  call [rdi + 24]       // memory-indirect call (FF 57 18)
}
```

**Encoded**: `FF 57 18`

### Indexed Memory-Indirect Call (SIB)

For tables indexed by register:

```asm
unsafe {
  lea rax, [rip + _syscall_table]   // load table base
  call [rax + rdi*8]                // call [base + index*scale]
}
```

**Encoded for call**: `FF 14 F8` (rax base, rdi index, scale 8)

### Table Dispatch with RIP-Relative

For position-independent dispatch:

```asm
unsafe {
  lea rax, [rip + _dispatch_table]
  call [rip + rax*8]    // or call [rax + offset]
}
```

**Encoded**: `FF 15 <rel32>` (RIP-relative, requires PC32 relocation)

## Encoder Support (pa-r13-003)

The following paideia-as encoder functions emit these patterns:

- `call_reg64(buf, reg)` — register-indirect: `FF /2 mod=11`
- `call_mem_base_disp(buf, base, disp)` — memory base+disp: `FF /2 mod=<0|1|2>`
- `call_mem_sib_disp(buf, base, index, scale, disp)` — SIB indexed: `FF /2 SIB [disp]`
- `call_mem_rip_rel(buf, disp32)` — RIP-relative: `FF 15 <disp32>`

All forms emit opcode `FF` with `/2` register field (010 in ModR/M), distinguishing them from other `FF` opcodes (jmp, push, etc.).

## Safety Invariants

The compiler **cannot verify** the safety of these patterns at parse/check time. The programmer is responsible for:

1. **Function pointer validity**: The loaded pointer must be a valid, executable address.
   - No dangling pointers.
   - No null pointers (unless explicitly guarded).
   - No pointers to stale code (e.g., after module unload).

2. **Argument count and types**: The caller must match the function's signature.
   - Correct number of arguments in the right registers (`rdi`, `rsi`, `rdx`, `rcx`, `r8`, `r9`).
   - Correct sized arguments (64-bit vs 32-bit vs 16-bit vs 8-bit).
   - No type-safety check at the call site.

3. **ABI contract**: Caller-save vs callee-save registers.
   - Caller is responsible for saving any registers that the called function might clobber.
   - System V AMD64 ABI defines callee-save as `rbx`, `rsp`, `rbp`, `r12–r15`.
   - All others are caller-save.

4. **Return value semantics**: The return register (`rax` for 64-bit integer, `rdx:rax` for 128-bit, etc.) must match expectations.
   - No implicit conversion or validation.

## Examples in paideia-os

### VFS Vops Dispatch (Phase 6)

```c
struct vfs_vops {
  fn_ptr open;        // [vops + 0]
  fn_ptr close;       // [vops + 8]
  fn_ptr read;        // [vops + 16]
  fn_ptr write;       // [vops + 24]
};

unsafe {
  mov rax, [rdi + 24]  // load vops->write from [vops + 24]
  call rax
}
```

### Syscall Dispatch Table (paideia-os#536)

```asm
; rdi = syscall id (0..11)
lea rax, [rip + _syscall_table]
call [rax + rdi*8]
```

Relies on `_syscall_table` being a vector of 12 function pointers, indexed by ID.

### Driver Dispatch (Phase 7)

Generic driver call-out pattern:

```asm
; rdi = driver_context struct
; rax = operation ID (0=init, 1=probe, 2=attach, ...)
mov rax, [rdi + rax*8 + 16]  ; load driver_ops[rax] from struct+offset
call rax
```

## Optimized Pattern: Field-Access RIP-Relative (pa-r17-015)

As of v0.16, paideia-as elaborator supports an optimized calling pattern for module-level
vtables with field-access syntax. This feature **emits a single RIP-relative memory call**
instead of the two-instruction sequence (load + call-register).

### Syntax (Requires #1073 AST→IR Lowering)

```paideia
struct VTable { func: u64 }
let vops: VTable = ...
let result = (vops.func)(arg1, arg2);  // Optimized to single call [rip + vops + offset]
```

### Codegen (Elaborator Wiring: Complete)

The elaborator wiring for this pattern is **fully implemented and tested** (see `emit_indirect_call_via_mem_rip_sym`
in `emit_lambda.rs` and IR-level tests in `pa_r17_015_call_rip_rel.rs`).

Emitted sequence:
```asm
mov rdi, arg1           ; marshal argument 1
mov rsi, arg2           ; marshal argument 2
call [rip + vops + 0]   ; RIP-relative call to vops.func (FF 15 <rel32>)
ret
```

**Relocation**: `PcRel32` at byte offset +2, symbol: `vops`, addend: `field_offset - 4`.

### Current Limitation (#1073)

The `(vops.func)()` calling pattern currently **cannot fire from real source** because
the AST→IR lowering for FieldAccess-callee is not yet implemented (#1073). Once #1073 lands,
this optimization will automatically apply to field-access call patterns on module-level symbols.

Until then, the elaborator wiring is dormant but correct — verified by IR-level unit tests that
hand-synthesize the IR shape and confirm proper Call/MemRipRelSym emission.

## Future: First-Class fn-ptr Types (v0.17, pa-r17-004)

Once v0.17 ships, paideia-as will support typed function pointers:

```paideia
fn_ptr<(rdi: u64, rsi: u64) -> rax: u64> my_dispatch;

unsafe {
  call my_dispatch;  // Compiler verifies arg count and types
}
```

This will replace the manual unsafe patterns, enabling compile-time safety. Until then, the patterns in this document are the prescribed workaround for paideia-os kernel code requiring dynamic dispatch.

## Cross-References

- **pa-r13-003** (#932): Encoder primitives for indirect calls
- **pa-r17-004**: v0.17 feature gate for first-class `fn_ptr<...>` types
- **pa-r17-015** (#993): Optimized RIP-relative call for field-access vtable dispatch
  - Elaborator wiring: complete (v0.16)
  - AST→IR lowering: pending #1073
- **#1073**: AST→IR lowering for FieldAccess-callee (blocks pa-r17-015 from firing)
- **paideia-os#536**: Syscall dispatch table (real-world use case)
- **paideia-os#437**: VFS vops dispatch (Phase 6 planning)
