# PaideiaOS x86_64 ABI

**Status:** Specification v1.0 (Phase 2)  
**Date:** 2026-06-18  
**Canonical machine-readable definition:** `src/toolchain/abi/abi.pdx` (this repo)  
**ABI Version:** 1  
**Consumed by:** NASM (via macro generator), paideia-as (directly)

---

## Overview

This document specifies the PaideiaOS x86_64 ABI — the interface contract between independently-assembled modules that may be built by either NASM (legacy) or paideia-as (canonical). Both toolchains must emit binaries that conform to this specification and must consume a machine-readable definition (`src/toolchain/abi/abi.pdx`) to validate cross-build compatibility (per `design/02-development-environment.md` §8.2 — the cross-build smoke test).

**Why a separate ABI document in paideia-as?**  
The kernel's calling convention lives in `PaideiaOS/design/toolchain/calling-convention.md`. This document complements it by defining the *toolchain-level artifact format* and the *versioning policy* — items relevant to how both NASM and paideia-as emit objects and how linkers (paideia-link, ld.lld) consume them. Consult calling-convention.md for register semantics; consult this document for ELF metadata layout, ABI versioning, and object-file structure.

---

## 0. Versioning

### 0.1 ABI_VERSION constant

The ABI version is a 32-bit unsigned integer. Current canonical version: **1**.

Every paideia-as-emitted ELF64 object shall carry the ABI version in a read-only section (§5.3). The cross-build smoke test (dev-env §8.2) compares the version of NASM-built and paideia-as-built modules to confirm compatibility before linking.

### 0.2 Semantic versioning policy

The ABI follows semantic versioning:

| Condition | Action | Example |
|---|---|---|
| **Additive change** — new diagnostic codes, new PaideiaOS-specific section types, new flag bits | **Minor bump** (1.0 → 1.1) | Add `.paideia.caps2` without removing `.paideia.caps`. |
| **Compatible extension** — new hardware-feature annotation in DWARF extensions (LAM, etc.) | **Minor bump** | New DWARF vendor extension code; old readers silently ignore. |
| **Breaking change** — modification to register partition, calling-convention reordering, removal of a section type | **Major bump** (1.0 → 2.0) | R12/R13 reordered; all Phase-2 objects must be rebuilt. |

Pre-phase-3 versions (1.x during phase 2): the project coordinates ABI changes via the phase-2 milestones; a major bump triggers re-migration of all Phase-2 subsystems. Phase-3+ versions are gated by formal release notes and the steering committee.

### 0.3 Compatibility guarantee

An object built with ABI version N is linkable with objects built with ABI version N. Version skew (N vs. N+1 minor) is up to the linker (paideia-link) to detect and warn.

---

## 1. Register file partitioning

Canonical source: `design/toolchain/calling-convention.md` §1. This section summarizes; refer to the calling-convention document for full semantics and rationale.

### 1.1 General-purpose registers

| Band | Registers | Discipline | Use | Caller/Callee saved |
|---|---|---|---|---|
| **General** | RAX, RBX, R8–R11 | unrestricted | Scratch, return values, computation | RAX/RCX/RDX/R8–R11 caller-saved; RBX callee-saved |
| **Argument** | RDI, RSI, RDX, RCX | unrestricted | First 4 integer/pointer arguments | Caller-saved |
| **Reserved** | RSP, RBP, RIP | reserved | Stack, frame, instruction pointers | Callee-saved (architectural) |

### 1.2 Capability registers

| Register | Type | Discipline | Use | Saved |
|---|---|---|---|---|
| **R12** | linear capability handle (LAM-tagged) | linear | First capability argument | Callee-saved |
| **R13** | linear capability handle (LAM-tagged) | linear | Second capability argument | Callee-saved |

Both are callee-saved; a caller retains its capability across function calls without spilling (per calling-convention.md §7.1).

### 1.3 Effect environment registers

| Register | Type | Discipline | Use | Saved |
|---|---|---|---|---|
| **R14** | handler-stack pointer | affine | Reserved for future effect-chain linking | Callee-saved |
| **R15** | handler-table base address | affine | Root of active effect-environment; used for 2-instruction effect dispatch | Callee-saved |

### 1.4 Vector registers

| Band | Registers | Caller/Callee saved | Use |
|---|---|---|---|
| **Scratch SIMD** | ZMM0–ZMM15 | Caller-saved | Floating-point, SIMD arguments (ZMM0–ZMM3), and computation |
| **Reserved SIMD** | ZMM16–ZMM31 | Callee-saved | Reserved for vectorized PQ-crypto state; non-PQ callees must preserve |

K-mask registers (K0–K7) are caller-saved; treated as scratch.

---

## 2. Calling convention

Canonical source: `design/toolchain/calling-convention.md` §2–§7.

### 2.1 Argument passing

#### Integer / pointer arguments
- **Positions 1–4:** RDI, RSI, RDX, RCX (in declaration order).
- **Position 5+:** stack, 8-byte aligned, in declaration order.

#### Capability arguments
- **Position 1:** R12 (linear).
- **Position 2:** R13 (linear).
- **Position 3+:** stack with capability-tagged stack-slot annotation (consumed by paideia-link for capability-binding metadata).

#### Floating-point / vector arguments
- **Positions 1–4:** ZMM0–ZMM3 (in declaration order).
- **Position 5+:** stack, 64-byte aligned, in declaration order.

#### Mixed argument orders
A function `(a: Cap, b: u64, c: Cap, d: f64)` places:
- `a` in R12 (cap position 1)
- `b` in RDI (integer position 1)
- `c` in R13 (cap position 2)
- `d` in ZMM0 (float position 1)

### 2.2 Return values

| Type | Register |
|---|---|
| Integer / pointer | RAX |
| Capability | R12 |
| Floating-point / vector | ZMM0 |
| Tuple (fits in registers) | RAX + RDX (integers); RAX + R12 (mixed); ZMM0 + ZMM1 (floats) |
| Large struct (> 16 bytes) | Caller provides return pointer in RDI; callee writes through it; returns the pointer in RAX |

### 2.3 Caller-saved registers

A caller must save (if needed across a call):
- RAX, RCX, RDX, RSI, RDI, R8–R11 (general-purpose)
- ZMM0–ZMM15 (vector)
- K0–K7 (mask)
- RFLAGS (condition flags)

### 2.4 Callee-saved registers

A callee must preserve:
- RBX, RBP, R12, R13, R14, R15 (general-purpose + capability + effect)
- RSP (stack pointer, always preserved by `ret`)
- ZMM16–ZMM31 (vector)

### 2.5 Stack alignment

- **Alignment:** 64-byte aligned at every `call` instruction (versus System V's 16-byte).
- **Rationale:** 64-byte alignment accommodates ZMM stores without unaligned-access penalties.
- **No red zone:** Unlike System V (128-byte red zone), PaideiaOS does not permit stack usage below RSP. Exception/interrupt entry would corrupt the red zone on PaideiaOS-native frames.

### 2.6 Stack frame layout

```
   high address
   ┌─────────────────────────────────┐
   │ Caller's arguments on stack     │  (positions 5+)
   ├─────────────────────────────────┤
   │ Return address                  │  pushed by CALL
   ├─────────────────────────────────┤  ← RSP at entry
   │ Saved RBP (if frame-pointer)    │
   ├─────────────────────────────────┤  ← RBP
   │ Saved callee-saved registers    │
   │   RBX, R12, R13, R14, R15       │
   │   ZMM16..ZMM31 (if used)        │
   ├─────────────────────────────────┤
   │ Local variables                 │
   ├─────────────────────────────────┤
   │ Outgoing argument area          │
   ├─────────────────────────────────┤  ← RSP, 64-byte aligned before call
   │ (no red zone)                   │
   ▼  low address
```

---

## 3. PaideiaOS extensions

Canonical sources: calling-convention.md §4–§5; custom-assembler.md §8.

### 3.1 Handler table at R15 (effect dispatch)

R15 holds the address of the active effect-handler table. The table structure (calling-convention.md §4.1) is per-AS and per-thread:

```
R15 → ┌──────────────────────────────────┐
      │ effect_table_id : u32            │
      │ size            : u32            │
      │ effect_0_handler: u64            │
      │ effect_0_state  : u64            │
      │ ...                              │
      └──────────────────────────────────┘
```

Effect dispatch is two instructions:
```asm
mov rax, [r15 + handler_offset]  ; offset is a compile-time constant
call rax
```

### 3.2 Capability registers (R12, R13)

R12 and R13 carry LAM-tagged 64-bit capability handles (per `design/capabilities/linearity-and-tags.md`). High 15 bits are tag bits; low 49 bits are pointer.

The substructural type system enforces linear consumption of capabilities on entry/exit. R12/R13 are callee-saved, so a caller retains its capability across calls without spilling (calling-convention.md §7.1).

### 3.3 Sigil instructions

Sigil instructions (e.g., `cli`, `sti`) are unsafe. They appear only in `unsafe` blocks (custom-assembler.md §9) with explicit effect and capability declarations. paideia-as emits audit trail entries for every sigil use.

---

## 4. ABI version policy

### 4.1 When to bump

- **Minor bump (1.0 → 1.1):** Additive changes only. New section types, new diagnostic codes, new DWARF vendor extensions. Old readers continue to work; they ignore new sections.
- **Major bump (1.0 → 2.0):** Breaking changes. Register reordering, removal of sections, calling-convention changes. All existing objects become incompatible and must be rebuilt.

### 4.2 Migration during phase 2

Phase-2 is permitted to bump the minor version (e.g., 1.0 → 1.1) if new PaideiaOS-specific sections are added. The cross-build smoke test (dev-env §8.2) verifies that NASM and paideia-as remain in sync.

A major-version bump during phase 2 requires:
1. Re-migration of all Phase-2 subsystems (per custom-assembler.md §10.2).
2. Steering-committee approval (design-decision gate).
3. Updated milestones.md and coordination via the phase-2 plan.

### 4.3 Phase 3 and beyond

Post-phase-3, ABI changes follow the project's release-note process. Semver is binding.

---

## 5. Object-file requirements

### 5.1 ELF64 header conventions

All paideia-as-emitted and NASM-emitted (phase-2 legacy) modules are ELF64 objects:

- **ELF magic:** `0x7f`, `'E'`, `'L'`, `'F'`
- **Class:** 64-bit (ELFCLASS64 = 2)
- **Data:** little-endian (ELFDATA2LSB = 1)
- **OS/ABI:** UNIX System V (ELFOSABI_SYSV = 0) — standard for PaideiaOS
- **Architecture:** x86-64 (e_machine = EM_X86_64 = 62)

### 5.2 PaideiaOS-specific sections

All sections are optional except where noted. The linker (paideia-link) processes them to build the PAX manifest (paideia-link.md).

| Section | Type | Flags | Purpose | Phase-2 required? |
|---|---|---|---|---|
| `.text` | SHT_PROGBITS | SHF_ALLOC + SHF_EXECINSTR | Executable code (machine bytes) | Yes |
| `.rodata` | SHT_PROGBITS | SHF_ALLOC | Read-only data | Yes |
| `.data` | SHT_PROGBITS | SHF_ALLOC + SHF_WRITE | Initialized data | Yes |
| `.bss` | SHT_NOBITS | SHF_ALLOC + SHF_WRITE | Zero-initialized data | Yes |
| `.paideia.caps` | SHT_PROGBITS | 0 | Capability-binding sites (list of offsets, capability types, parent info) | Yes |
| `.paideia.effects` | SHT_PROGBITS | 0 | Function-level effect-row annotations (consumed by LSP, audit tools) | No (advisory) |
| `.paideia.unsafe` | SHT_PROGBITS | 0 | Audit catalog of `unsafe` blocks | No (advisory) |
| `.paideia.opt-passes` | SHT_PROGBITS | 0 | Record of which optimization passes ran | No (advisory) |
| `.paideia.lin` | SHT_PROGBITS | 0 | Linearity-check witness data (E14 regression corpus integration) | No (advisory) |
| `.debug_*` | SHT_PROGBITS | 0 | DWARF debug sections (all standard) | Recommended |
| `.debug_paideia.caps` | SHT_PROGBITS | 0 | DWARF vendor extension: capability type info | No (phase-2 stub) |
| `.debug_paideia.effects` | SHT_PROGBITS | 0 | DWARF vendor extension: effect-row annotations | No (phase-2 stub) |
| `.debug_paideia.sig` | SHT_PROGBITS | 0 | DWARF vendor extension: functor signatures, sharing constraints | No (phase-2 stub) |
| `.symtab` | SHT_SYMTAB | 0 | Symbol table (standard ELF) | Yes |
| `.strtab` | SHT_STRTAB | 0 | String table (standard ELF) | Yes |
| `.shstrtab` | SHT_STRTAB | 0 | Section header string table (standard ELF) | Yes |
| `.rel.text` | SHT_REL | 0 | Relocations in `.text` | Yes |

### 5.3 ABI version stamp

Every paideia-as-emitted ELF64 object shall include an ABI-version constant in a new, mandatory section:

**Section name:** `.paideia.abi_version`  
**Type:** SHT_PROGBITS (read-only data)  
**Flags:** SHF_ALLOC (loaded into memory for accessibility by verification tooling)  
**Content:**
```c
struct {
  u32 abi_version;  // = 1 (phase 2)
  u32 reserved;     // = 0 (reserved for future use)
} abi_info;
```

Byte order: little-endian (per ELF header).

The cross-build smoke test compares this constant in NASM- and paideia-as-emitted objects before linking.

NASM-emitted legacy objects (phase 2) shall emit the same section with the same format. A Makefile macro or build-script invocation handles this; the macro is part of the paideia-as reference (§6.2).

---

## 6. Cross-host bring-up

### 6.1 NASM consumer expectations

NASM (legacy, phase 2 only) shall:
1. Accept a machine-readable ABI constant from `src/toolchain/abi/abi.pdx` (via a macro generator or script).
2. Emit `.paideia.abi_version` section with ABI_VERSION = 1.
3. Follow the register partition (§1) and calling-convention (§2) exactly.
4. Emit `.paideia.caps` section with capability-binding sites if the module uses capabilities.

NASM modules are linked alongside paideia-as modules during phase 2 coexistence. The cross-build smoke test verifies that a NASM-built and paideia-as-built module of the same source have compatible calling conventions (per dev-env §8.2, design clarification item S6).

### 6.2 paideia-as consumer expectations

paideia-as shall:
1. Parse `src/toolchain/abi/abi.pdx` at build time (via `paideia-as check`).
2. Validate the ABI_VERSION constant.
3. Emit all required and advisory sections (§5.2) with correct format.
4. Emit `.paideia.abi_version` on every object.
5. Report per-function adherence to calling convention (R12/R13 linearity, R14/R15 affinity, stack alignment) via the elaborator's type/effect system.
6. Implement the effect-dispatch mechanism (R15 + handler table, §3.1) via the effect-handler-rewrite IR pass (custom-assembler.md §6.5).

### 6.3 Cross-build smoke test

The smoke test (dev-env §8.2) is a single integration test run per phase-2 subsystem migration:

1. Compile subsystem X with NASM (legacy).
2. Compile the same source with paideia-as (canonical).
3. Link both objects against the rest of the kernel.
4. Verify that the linked kernel boots and the subsystem's entry point is reachable and functional (behavioral test).
5. Compare ABI_VERSION in `.paideia.abi_version` sections (design clarification item S6 — byte-for-byte vs. semantic diff).

The test infrastructure lives in `.plans/m1-013.md` (cross-build smoke test, the next phase-1 issue after this one).

---

## 7. References

- **Calling convention:** `design/toolchain/calling-convention.md` — System V bridge, effect environment, capability discipline, stack layout.
- **Custom assembler:** `design/toolchain/custom-assembler.md` — IR pipeline, effect-handler rewrite (§6.5), ABI versioning policy (§16), emitter backends (§12).
- **OS requirements:** `design/02-development-environment.md` — phase-2 toolchain requirements, DDC, cross-build smoke test (§8.2), feature-masked CI (§10.5).
- **Phase-2 requirements audit:** `design/.plans/phase-2/os-requirements.md` — T1 (ABI document), T2 (cross-build smoke test), design-clarification items S6.
- **Capabilities & linearity:** `design/capabilities/linearity-and-tags.md` — LAM-tagged handles, substructural discipline on R12/R13.

---

*End of document.*
