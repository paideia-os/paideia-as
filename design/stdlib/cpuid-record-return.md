# CPUID record-return marshalling contract

**Status:** Scope note (v0.21 bundle, boot-intrinsics area).
**Date:** 2026-08-22.
**Origin issues:** paideia-as#1298 (this scope note), paideia-as#1283 (implementation).
**Downstream blockers:** paideia-os R18 M6 (hybrid tagging + topology walk), paideia-os R21 (XSAVE sizing via CPUID leaf 0x0D).
**Consumed by:** future `cpuid_leaf(...) -> CpuidRegs` typed intrinsic and `stdlib/pdx/cpuid.pdx` typed decoders (`cpuid_hybrid_class_from_1a`, `cpuid_topology_from_1f`, `cpuid_xsave_area_size`).

---

## 1. Why the zero-arity `cpuid` mnemonic is not enough

The zero-arity `Cpuid` mnemonic already lives in `MNEMONIC_TABLE` and round-trips through the encoder byte-for-byte (v0.21-006, closed). That mnemonic is the raw instruction form: it reads `EAX` / `ECX` and writes `EAX` / `EBX` / `ECX` / `EDX`, with no argument marshalling and no return value.

That is the correct primitive for hand-written assembly, but it is the wrong surface for high-level callers. paideia-os R18 M6 (hybrid P/E CPU tagging) and R21 (XSAVE area sizing) both want to call CPUID from `.pdx` sources and destructure the four returned 32-bit fields as a typed record — for example:

```pdx
let r: CpuidRegs = cpuid_leaf(0x1A_u32, 0x00_u32);
if (r.eax >> 24) & 0xFF == 0x40 { /* P-core */ }
```

Doing that requires a *typed intrinsic* whose recipe (a) marshals two `u32` arguments into `EAX` and `ECX`, (b) emits `cpuid`, and (c) returns the four 32-bit result registers to the caller as a `CpuidRegs { eax: u32, ebx: u32, ecx: u32, edx: u32 }` record.

Steps (a) and (b) fit the existing `SysVRegs` recipe pattern (see `crates/paideia-as-elaborator/src/emit_call.rs`, the `ArgConvention::SysVRegs` splice path). Step (c) does not: no recipe landed to date has returned a multi-field record. Every prior `SysVRegs` recipe returns a single scalar (unit, `u64` in `RAX`, or `Cap` in `R12`). That gap is what #1298 flags and what this note pins down before #1283 is implemented.

## 2. Record-return options under the PaideiaOS ABI

`design/toolchain/abi.md` §2.2 defines two rules that touch `CpuidRegs`:

| Rule | Register(s) | Notes |
|---|---|---|
| Tuple that fits in registers | `RAX + RDX` (integer half-widths packed) | Applies to aggregates ≤ 16 B. |
| Large struct (> 16 B) | Caller passes return pointer in `RDI`; callee writes through it and returns the pointer in `RAX`. | Applies to anything above the split boundary. |

`CpuidRegs` is exactly `4 × u32 = 16 B`. Formally, it falls under the *tuple-in-registers* rule: `{eax, ebx}` packed into `RAX` and `{ecx, edx}` packed into `RDX`. That is where a naive reading would stop.

The practical constraint is different. The v0.17 record subsystem materialises a record in memory and issues typed loads per field when a caller writes `r.ebx`. It has no lowering path today that (i) accepts a value split across `RAX + RDX`, (ii) unpacks each 64-bit half back into two 32-bit fields, and (iii) presents that as a field-addressable record to the destructurer. Adding that path is a general record-lowering feature, not a boot-intrinsic feature.

That leaves two shapes for the `cpuid_leaf` recipe. We record both explicitly so that #1283 can pick one without re-litigating.

### Option A — Caller-passed record pointer (recommended)

The caller allocates a `CpuidRegs`-shaped slot on its own frame (16 B, aligned per record layout). The ABI large-struct convention already reserves `RDI` for that pointer. The recipe then:

1. Move caller-arg 1 (`leaf: u32`) → `EAX`  (source register determined by SysV integer position 1 after the returned-pointer slot; see below).
2. Move caller-arg 2 (`subleaf: u32`) → `ECX`.
3. Emit `cpuid`.
4. Store `EAX` / `EBX` / `ECX` / `EDX` into `[RDI + 0]` / `[RDI + 4]` / `[RDI + 8]` / `[RDI + 12]`.
5. `mov RAX, RDI`; return.

Argument-register assignment when the callee expects a returned-pointer slot in `RDI`: the two `u32` arguments occupy the *next* two integer positions, i.e. `RSI` and `RDX`. The recipe therefore marshals `mov EAX, ESI` and `mov ECX, EDX` (32-bit moves clear the upper halves of `RAX` / `RCX`, which is exactly what CPUID reads).

**Why this is the recommendation.** It reuses the existing large-struct return convention verbatim — no new record-lowering path in the elaborator, no register-split unpacking, and it degrades cleanly if `CpuidRegs` ever grows beyond 16 B (adding hypothetical scratch or padding fields). The 16-B/64-B alignment cost is one caller-frame slot per call, which is negligible against the CPUID serialising cost.

### Option B — Return in `RAX + RDX` (deferred)

The recipe emits the raw `cpuid`, then packs `{EAX, EBX}` into `RAX` (low half, high half) and `{ECX, EDX}` into `RDX`. This is ABI-legal per §2.2 (tuple-in-registers), but it requires the record subsystem to grow a lowering path that recognises a value returned in a register pair and materialises it into a field-addressable temporary. That is a record-subsystem enhancement, not a boot-intrinsic enhancement, and it is out of scope for #1283. If we ever want a broader class of ≤ 16 B records to return in registers we should open a dedicated ticket for the record-subsystem work and adopt Option B uniformly.

### Not-considered — Callee-stack return

A shape where the recipe allocates on the callee stack and returns a pointer into that frame is unsound: the returned pointer would dangle after the callee returns. It is mentioned only to be ruled out.

## 3. Concrete recipe shape for #1283

```
; SysVRegs recipe: cpuid_leaf(leaf: u32, subleaf: u32) -> *CpuidRegs
;
; Register discipline on entry (per §2.2 large-struct rule, Option A):
;   RDI = caller-provided record pointer (16 B slot, record-layout aligned)
;   RSI = leaf     (u32, low 32 bits meaningful)
;   RDX = subleaf  (u32, low 32 bits meaningful)
;
; Clobbers: RAX, RBX, RCX, RDX  (all caller-saved; RBX preserved by save/restore).
; Preserves: RBP, R12-R15, RDI.

    push    rbx                 ; RBX is callee-saved; CPUID writes EBX.
    mov     eax, esi            ; EAX <- leaf   (zero-extends into RAX)
    mov     ecx, edx            ; ECX <- subleaf
    cpuid
    mov     dword ptr [rdi +  0], eax
    mov     dword ptr [rdi +  4], ebx
    mov     dword ptr [rdi +  8], ecx
    mov     dword ptr [rdi + 12], edx
    mov     rax, rdi            ; return the record pointer
    pop     rbx
    ret
```

Field offsets (`eax @ 0`, `ebx @ 4`, `ecx @ 8`, `edx @ 12`) match natural declaration order for `CpuidRegs { eax, ebx, ecx, edx }` under the default record layout (packed 4-byte fields, 16-byte total, 4-byte alignment). If Phase-4 records ever reorder or pad these fields the recipe must be regenerated from the record's `field_offset` table, not from a hand-written constant — that is a check for the #1283 implementation.

## 4. What #1283 must deliver

The scope-note issue (#1298) resolves as documentation. Actual code changes land under #1283 and are constrained by this note:

- Recipe registered in the `SysVRegs` recipe table with the shape above.
- Recipe emission code path that:
  - Reads the caller's return-pointer register (`RDI`) from the frame's returned-record slot, not from a hard-coded literal.
  - Uses `field_offset(CpuidRegs, ebx / ecx / edx)` for the store offsets (record layout is the source of truth).
- `stdlib/pdx/cpuid.pdx` typed decoders that never touch the raw mnemonic:
  - `cpuid_hybrid_class_from_1a() -> HybridClass` (P | E | LP-E | Unknown, from `EAX[31:24]` of leaf `0x1A`).
  - `cpuid_topology_from_1f() -> TopologyLevels` (walks subleaves of `0x1F` until it sees a zero-type terminator).
  - `cpuid_xsave_area_size() -> u32` (returns `ECX` of leaf `0x0D` subleaf `0x00`).
- QEMU `-cpu host` fixture that witnesses P-core class on Raptor Lake and asserts a non-zero XSAVE size.

## 5. Boundary conditions and non-goals

- **`RBX` preservation.** `RBX` is callee-saved in the PaideiaOS ABI and CPUID writes `EBX`; the recipe must save and restore it (see §3). Every review of the recipe should re-check this line — it is the easiest single thing to break.
- **Serialising semantics.** `cpuid` is a serialising instruction. The recipe does not need explicit fences around it. Callers that use CPUID as a serialising barrier for reasons unrelated to the returned values (e.g. before `rdtsc`) should call a dedicated barrier intrinsic, not `cpuid_leaf`.
- **Not for hypercall or MSR paths.** This recipe covers only CPUID. `rdmsr` / `wrmsr` already have SysVRegs recipes with single-scalar returns and are unaffected. VMX / SVM entry paths have their own register discipline and are out of scope.
- **Not a general record-return contract.** This note deliberately does not generalise Option A to all ≤ 16 B records. If we later want the record subsystem to consume a `RAX + RDX` pair (Option B), open a dedicated record-lowering ticket and revisit — do not sneak it in through a boot-intrinsic patch.

## 6. Cross-references

- `design/toolchain/abi.md` §2.2 (Return values) — the ABI rules that Options A and B derive from.
- `design/toolchain/calling-convention.md` — companion document; register semantics.
- `crates/paideia-as-elaborator/src/emit_call.rs`, `ArgConvention::SysVRegs` splice path — the existing SysVRegs recipe machinery this intrinsic plugs into.
- paideia-os `design/roadmap/r18-plus-bare-metal.md` §5, T14 G4 — the downstream consumer that motivates the typed shape.
