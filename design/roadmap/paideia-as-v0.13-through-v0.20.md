# paideia-as strategic roadmap: v0.13 through v0.20

**Status:** Strategic multi-release plan. osarch deliverable.
**Date:** 2026-07-04.
**Vantage:** paideia-as v0.12.0 (submodule commit `978fa56`); paideia-os R14B in flight.
**Consumers:** paideia-os Phases 5-14 (storage, drivers, network, CoW FS, WASM VM, semantic terminal, UEFI, SMP+PQ, self-hosting).
**Author:** osarch.
**Scope:** Feature contract only. Tactical issue breakdown is the softarch companion; GitHub milestones/issues will be filed after consolidation.

---

## 0. Preamble — how to read this document

This roadmap is the *covenant* between paideia-as (the toolchain) and paideia-os (the consumer) for the seven-release arc that carries paideia-os from Phase 5 (storage substrate) to Phase 14 (self-hosting). It defines *what each release enables* rather than how each feature is built. Where "how" matters, it points to the tactical companion under `.plans/` (softarch owns tactical breakdown).

The document is deliberately expansive. Every prospective consumer of paideia-as within paideia-os over the next 18-30 months is enumerated with the release that unblocks it. Where a paideia-as feature is speculative — a design gap not yet felt because paideia-os has not reached that phase — the roadmap flags it as **speculative** and identifies the paideia-os round that will confirm or refute the need.

Absolute references throughout use `/home/snunez/Development/PaideiaOS/` roots so cross-repo tracing is unambiguous.

---

## 1. North star — what "paideia-as complete" means

paideia-as is complete when **every idiom paideia-os needs across Phases 1-14 is expressible in `.pdx` source without falling back to an `unsafe` block hand-rolled with byte layouts, and without hand-writing a `.S` GAS source for anything downstream of the very first PVH boot stub**. Concretely:

1. **Boot substrate**: PVH entry + long-mode transition + GDT/IDT bring-up expressible in `.pdx`. (Achieved in principle by v0.12.0 encoder surface + `[bits=32]` mode; boot_stub.S migration is pending #900 cross-module symbol export.)
2. **Kernel core**: capability handlers, scheduler, IPC, memory manager, syscall trampoline — all in `.pdx`. (Achieved by R14B on top of v0.12.0; small encoder gaps caught in this session drive v0.13.)
3. **Drivers**: MMIO-mapped, DMA-driven device drivers with function-pointer dispatch tables (vops). Requires v0.13 (`call [mem]`, `call reg`) + v0.14 (first-class function-pointer types) + v0.15 (bulk memory-move primitives).
4. **Network stack**: userspace TCP/QUIC with byte-level packet parsing, endian conversion, checksums, and protocol dispatch. Requires v0.16 (narrow-width memory operands, rotates, packed records).
5. **Filesystem (CoW, capability-encoded)**: bitmap allocation, atomic refcount updates, content-addressed indexing. Requires v0.13 (`bts/btr/btc`, `lock adc`) already in scope.
6. **Userspace runtime (WASM/VM)**: interpreter dispatch on a large opcode table, dynamic code generation with paideia-as embedded as a library at runtime. Requires v0.14 (dispatch tables) + v0.17 (runtime-embeddable encoder as a linkable library, no filesystem I/O required).
7. **Semantic terminal**: strongly typed command objects, hash-mapped registries, discriminated-union command results. Requires v0.19 (data structure primitives, hash tables in `paideia-stdlib`).
8. **UEFI real hardware**: UEFI protocol tables, GUID handling, MS x64 calling convention at UEFI boundaries. Requires v0.17 (MS x64 ABI annotation, GUID literals, UEFI protocol type helpers).
9. **SMP + PQ trust root**: full multicore with AP boot code paths, `cmpxchg16b`, `pause` hint, per-CPU state validated at compile time. PQ boot verification wired into the emitter, not the runtime. Requires v0.18.
10. **Self-hosting**: paideia-as (all Tier 1 + Tier 2 + Tier 3 crates) compiles under paideia-os. Deterministic builds. Requires v0.20 (portability polish, deterministic emission, host-target parity smoke).

The **capability contract** at each release is the enumerated set of paideia-os idioms it makes expressible; §4 lays this out per-release.

---

## 2. Release milestone map

Eight releases. v0.13 through v0.20. Codenames chosen to make the strategic theme unambiguous.

### 2.1 v0.13 — "Encoder gap catch-up"

- **Codename:** GAP-CATCH.
- **Goal:** Close every encoder + directive gap caught during paideia-os R13/R14B substrate work so that storage/driver critical path stops accumulating in-tree workarounds.
- **Deliverables:**
  - **Encoder correctness fixes:**
    - #927 narrow-width `mov r8/r16/r32, [mem]` — currently emits REX.W as if r64. Fix REX.W selection to depend on operand width; add width-tagged variants for `mov r8/r16/r32, [base+disp]`, `[base+index*scale+disp]`, `[rip+sym]`, and the symmetric store forms.
    - #928 REX.B on SIB with r8-r15 base — dropped high bit collapses r12→rsp aliasing, r8→rax aliasing, etc. Fix REX.B emission across every SIB emitter (`emit_indexed_load`, `emit_indexed_store`, and the direct `[base + disp]` fallthrough).
    - #929 `call [mem]` and `call reg` — memory-indirect and register-indirect `call` currently unencodable. Add `FF /2` for `call r/m64` and the register form; symmetric to existing `jmp r/m64` primitives.
  - **Encoder additions (unfiled workarounds retired):**
    - `ud2` (0F 0B) — deliberate undefined-instruction trap; used in unreachable arms.
    - `dec r64/imm form` — currently emulated via `sub r64, 1`.
    - `test r64, imm32` — currently emulated via `and rax, rax; jz` sequence.
    - `cld` (FC), `std` (FD) — direction flag control for string ops.
    - `rep_movsb` (F3 A4), `rep_movsq` (F3 REX.W A5), `rep_stosb` (F3 AA), `rep_stosq` (F3 REX.W AB) — bulk memory ops; critical for DMA buffer setup.
    - `bts r/m64, r64` / `bts r/m64, imm8` (0F BA /5) — bit test-and-set.
    - `btr r/m64, r64` / `btr r/m64, imm8` (0F BA /6) — bit test-and-reset.
    - `btc r/m64, r64` / `btc r/m64, imm8` (0F BA /7) — bit test-and-complement.
    - `or r64, imm64` — currently limited to imm32 sign-extended forms; needed for full-mask constants.
    - `lock adc [mem], r64` / `lock inc [mem]` / `lock dec [mem]` / `lock add [mem], imm` — atomic refcount primitives.
    - `xadd [mem], r64` — atomic fetch-and-add.
    - `crc32 r64, r/m64` (F2 0F 38 F1) — hardware CRC32C for network checksums (available since Nehalem; paideia-os pillar 1 baseline).
  - **Directive additions:**
    - `@include_bytes("path")` — file-system-backed byte literal; embeds raw bytes as `.rodata` at declared alignment.
    - `@embed_align(N)` optional postfix — control alignment of embedded blobs.
    - `@debug_break` / `@panic_here` — emit `int3` / `ud2` with source position metadata (SARIF).
  - **Diagnostic tightening:**
    - Promote silent-wrong emissions (like #927 REX.W drop) into hard errors during elaboration.
    - Add byte-exact operand-width mismatch diagnostic (T0533).
- **Exit criteria:** paideia-os idioms that become expressible:
  - Byte-level PCIe MMIO reads/writes (narrow-width forms correct).
  - Register-indirect virtual dispatch (function pointer stashed in register, `call rax`).
  - Bulk DMA buffer setup via `rep movsq`.
  - Bitmap allocator using `bts`/`btr` and CRC32C for content-addressed filesystem paths.
  - Atomic refcount increment via `lock inc [mem]`.
  - No paideia-os handler needs a `@include_bytes` fallback into `.S` for embedded blobs (ACPI tables, PQ public key blobs).
- **Prereqs:** v0.12.0 (current).
- **Approximate issue count:** 22-28. Broken down: ~10 correctness fixes, ~14 new instructions/directives, ~4 diagnostic upgrades.

### 2.2 v0.14 — "Function pointers"

- **Codename:** FP-DISPATCH.
- **Goal:** First-class function-pointer types in the `.pdx` type system. Idiomatic vops tables (VFS, driver dispatch, WASM opcode tables) without falling into `unsafe { call rax }`.
- **Deliverables:**
  - **Language surface:**
    - `fn(T1, T2) -> R` function-pointer type. Callable syntactically as `ptr(arg1, arg2)`.
    - Function-pointer literal syntax: `&function_name` produces a `fn(...) -> R` value.
    - `fn(...)` in record fields — vops tables: `record VfsVops { open: fn(&Path) -> Handle, read: fn(Handle, Buf) -> Size, ... }`.
    - Effect + capability annotations on function-pointer types: `fn(Cap) -> Result [io, +mem]` — matches the invariants the effect system enforces on direct calls.
  - **IR:**
    - `IrKind::FnPtrLit`, `IrKind::IndirectCall`.
    - Callee register selection for indirect call (RAX by default in native ABI, per calling-convention.md §3).
  - **Encoder:**
    - `call reg` (v0.13 delivered) is the substrate; v0.14 wires the type-checked language surface on top.
  - **Effect + capability checking:**
    - A function-pointer whose type carries `+mem` effect can be assigned only from a function that declares `+mem` effect.
    - Linear-capability arguments through function pointers respect linearity — a `fn(Cap)` invocation consumes the capability just like a direct call.
  - **paideia-stdlib support:**
    - Callback / continuation types formalized: `type Callback[T] = fn(T) -> Unit`.
    - `VopsTable[N]` helper for building N-slot dispatch tables at compile time.
- **Exit criteria:**
  - VFS vops tables in `.pdx` without `unsafe`.
  - PCIe driver hierarchy with virtual dispatch (open, read, write, ioctl per device) fully typed.
  - WASM opcode table (256 entries) expressible as a `VopsTable[256]` of `fn(&Frame) -> Trap`.
  - Effect regression: an `fn(...)` that leaks its capability argument fails to type-check.
- **Prereqs:** v0.13 (needs `call reg` and `call [mem]`).
- **Approximate issue count:** 18-22.

### 2.3 v0.15 — "MMIO + DMA + memory-model idioms"

- **Codename:** DMA-IDIOM.
- **Goal:** Idiomatic device-driver expression. MMIO reads/writes with explicit ordering; DMA buffer setup with ownership-tracked lifetimes; ring-buffer construction as a first-class stdlib primitive.
- **Deliverables:**
  - **Language surface:**
    - `mmio` region declaration: `mmio DevRegs @0xFE00_0000 for 0x1000 { CTRL: u32 @0x00, STATUS: u32 @0x04, ... }`. Compiler emits address-typed accessors that carry the `+mmio` effect.
    - `dma_buffer[N]` type: statically sized, contiguous, cache-line aligned; carries `+dma_owned` effect; lifetime is linear (can be handed off to a device, must be reclaimed).
    - `barrier` / `fence` intrinsics with explicit semantic tag: `barrier(load_load)`, `barrier(store_store)`, `barrier(full)` — lowered to `lfence` / `sfence` / `mfence`. (Semantics inherited from calling-convention.md §5.)
  - **Encoder:**
    - `lfence` (0F AE E8), `sfence` (0F AE F8) — completeness (mfence already exists).
    - `clflush` (0F AE /7), `clflushopt` (66 0F AE /7), `clwb` (66 0F AE /6) — cache-line control for DMA coherence.
    - `movnti r/m64, r64` (0F C3 /r) — non-temporal store; used for DMA descriptor rings.
    - `pause` (F3 90) — spin-wait hint; needed here so ring-buffer wait loops can express it (real SMP use is v0.18).
  - **Effect system:**
    - New effect: `+mmio`. Every load/store through an `mmio` region carries it. A function whose signature omits `+mmio` cannot access an mmio-typed value.
    - New effect: `+dma`. `dma_buffer` operations require this effect.
    - `+mmio` and `+dma` are mutually incompatible with pure code (they cannot be inferred out).
  - **paideia-stdlib:**
    - `SpscRing[T, N]` — single-producer/single-consumer ring built on non-temporal stores + fences.
    - `MpmcRing[T, N]` — deferred to v0.18 (needs cmpxchg16b for wait-free).
- **Exit criteria:**
  - AHCI driver in `.pdx` end-to-end (config space walk → command list → FIS ring).
  - NVMe driver: SQ/CQ pair as `SpscRing[NvmeSqe, 64]` / `SpscRing[NvmeCqe, 64]`.
  - PCIe root-complex bring-up with mmio regions typed.
  - Regression: a driver reading an MMIO register without `+mmio` fails to type-check.
- **Prereqs:** v0.13 (rep_movsq for bulk moves), v0.14 (function-pointer dispatch for vops).
- **Approximate issue count:** 24-30. (Heavier due to effect-system extension.)

### 2.4 v0.16 — "Network stack + packed-record byte parsing"

- **Codename:** PACK-PARSE.
- **Goal:** Byte-level protocol parsing without hand-rolled shift/mask sequences. Idiomatic Ethernet/IP/TCP/QUIC frame handling.
- **Deliverables:**
  - **Language surface:**
    - `packed record` type: no padding between fields; fields must be scalars or arrays; layout is byte-exact per declaration order. `packed record EthernetHeader { dst: [u8; 6], src: [u8; 6], ethertype: u16be }`.
    - Endian-tagged scalar types: `u16be`, `u32be`, `u64be` (big-endian on the wire; native-endian in registers via `bswap`).
    - `bswap` intrinsic surfaced as the endian conversion helper. (0F C8+rd already encodable via `swap` sequences; formalize as `bswap` mnemonic.)
    - Reading a `packed record` from a `[u8]` slice: `let hdr : EthernetHeader = parse::<EthernetHeader>(&frame[0..14])?` — bounds-checked, endian-normalized.
  - **Encoder additions:**
    - `bswap r32/r64` (0F C8+rd) — byte swap.
    - `movbe r64, [mem]` / `movbe [mem], r64` (F0 38 F0/F1) — load/store big-endian (Nehalem+; paideia-os baseline).
    - `rol r/m64, imm8` / `ror r/m64, imm8` — bit rotations (checksum inner loops).
    - `popcnt r64, r/m64` (F3 0F B8) — population count (bitmap density, IP option parsing).
    - `lzcnt` / `tzcnt` — leading/trailing zero count (already needed for bitmap allocator, may land in v0.13 patch).
  - **paideia-stdlib:**
    - `NetBuffer` — zero-copy packet handle wrapping a `dma_buffer` slice.
    - `ip_checksum(&NetBuffer) -> u16` — ones-complement checksum using `adcx`/`adox` (if available) or plain `adc`. (Requires `adcx` (66 0F 38 F6) / `adox` (F3 0F 38 F6) — ADX extension, Broadwell+; paideia-os baseline.)
    - `crc32c(&[u8]) -> u32` — hardware CRC32C (from v0.13).
  - **Diagnostics:**
    - Endian-mismatch as a type error (assigning `u16be` to `u16` requires an explicit `.to_native()` conversion).
    - Packed-record overlap warning if `@offset(N)` annotations collide.
- **Exit criteria:**
  - Full Ethernet/IP/TCP header parsing in `.pdx`, no hand-rolled shifts.
  - QUIC packet number decoding using variable-length integer parsing helpers.
  - TCP checksum + IP checksum both idiomatic (single `ip_checksum()` call, correct on random test vectors).
  - Regression: mixing `u16be` and `u16` without conversion is a type error.
- **Prereqs:** v0.13 (narrow-width mov correctness for byte-level access), v0.15 (dma_buffer, NetBuffer wrapping).
- **Approximate issue count:** 22-26.

### 2.5 v0.17 — "MS x64 ABI + UEFI"

- **Codename:** UEFI-ABI.
- **Goal:** UEFI protocols and MS x64 calling convention as first-class boundary primitives. Real-hardware boot path is expressible in `.pdx`.
- **Deliverables:**
  - **Language surface:**
    - `@abi("ms_x64")` function attribute. Alternative annotation forms: `@abi("sysv")` (default), `@abi("paideia_native")` (the current calling-convention.md scheme).
    - MS x64 register allocation: RCX/RDX/R8/R9 as first four integer args (vs. RDI/RSI/RDX/RCX in SysV). XMM0-XMM3 for first four floats. Shadow space (32 bytes) allocated by caller.
    - Elaborator + emitter generate the correct prologue/epilogue per ABI attribute.
    - `guid` scalar type: 16-byte, little-endian first field, matches Windows/UEFI wire format. Literal syntax `guid"12345678-1234-1234-1234-123456789ABC"`.
    - `uefi_protocol` declaration: named protocol with GUID + vops-like function-pointer table. Emits a `Protocol` type usable via `bs->LocateProtocol(&PROTO_GUID, ...)`.
  - **paideia-stdlib:**
    - `paideia-stdlib::uefi` module: `SystemTable`, `BootServices`, `RuntimeServices`, `SimpleFileSystemProtocol`, `GraphicsOutputProtocol`, etc. Function-pointer-table records with GUIDs baked in.
    - `paideia-stdlib::gpt` — GPT partition-table parsing (packed records from v0.16).
  - **Encoder:**
    - No new instructions required (the SysV → MS x64 shift is register allocation + stack discipline, not new opcodes). Verify the shadow-space stack discipline is emitted correctly and round-trips through iced-x86.
  - **Emitter:**
    - PE/COFF emitter (`paideia-as-emitter-pe`) gains UEFI application subsystem (IMAGE_SUBSYSTEM_EFI_APPLICATION = 10). Verify against `edk2` build outputs.
    - Support both PE32+ (UEFI) and existing ELF64 (kernel) simultaneously from a single build tree.
- **Exit criteria:**
  - A UEFI application "hello world" written entirely in `.pdx`, PE32+ output, boots under OVMF (QEMU firmware).
  - GetMemoryMap + ExitBootServices + typed handoff struct construction all in `.pdx`.
  - GOP framebuffer init via `LocateProtocol` — no `unsafe` blocks.
  - MS x64 ABI callee-saved register discipline verified via a round-trip test that calls into and returns from a `@abi("ms_x64")` function.
- **Prereqs:** v0.14 (function pointers for protocol vops), v0.15 (mmio for framebuffer), v0.16 (packed records for GPT / boot config tables).
- **Approximate issue count:** 26-32. (PE emitter extension is heavy.)

### 2.6 v0.18 — "SMP + PQ trust root"

- **Codename:** SMP-PQ.
- **Goal:** True multicore expressibility. Compile-time-integrated PQ trust root — signatures verified as part of the emitter pipeline, keys carried in typed regions.
- **Deliverables:**
  - **Encoder additions:**
    - `cmpxchg16b [mem]` (REX.W 0F C7 /1) — 128-bit CAS; wait-free MPMC queues, per-CPU descriptor rotation.
    - `wbinvd` (0F 09), `invd` (0F 08) — cache management (rare; needed for AP boot / MTRR reconfiguration).
    - `monitor` (0F 01 C8), `mwait` (0F 01 C9), `umonitor` (F3 0F AE /6), `umwait` (F2 0F AE /6), `tpause` (66 0F AE /6) — WAITPKG idle primitives.
    - `wrpkru` / `rdpkru` — MPK / PKU support (already speculatively in feature-inventory C10).
    - `lock cmpxchg16b` — the locked form for the wait-free primitives.
  - **Language surface:**
    - Per-CPU state declaration: `percpu let mut current_thread : &Tcb = null` — compiler wires GS-relative addressing automatically. (Requires the elaborator to translate access to `[gs:offset]`.)
    - `atomic<T>` scalar wrapper. Operations: `load(order)`, `store(v, order)`, `compare_exchange(old, new, success, failure)`, `fetch_add(v, order)`, etc. `order` is one of `relaxed`, `acquire`, `release`, `acq_rel`, `seq_cst`. Compiler lowers to LOCK-prefixed instructions with correct fence semantics.
    - Wait-free `MpmcRing[T, N]` in paideia-stdlib (built on cmpxchg16b).
    - `spinlock`, `rwlock`, `seqlock` in paideia-stdlib.
  - **PQ integration:**
    - `paideia-pq-sign` (already at v0.12.0) gains ACVP vector validation (was `#525` open by design; close it or split into a lasting suite).
    - Emitter `paideia-as-emitter-pax` binds `paideia-pq-sign::sign_pax_hash` into the release-artifact path: every PAX object is signed with a hybrid ed25519 + ML-DSA-65 signature by default. `--unsigned` flag for developer builds.
    - Boot-time verification: paideia-os's boot substrate embeds a `.paideia.trust_root` section with the vendor public key; the runtime verifier reads it before running any signed PAX segment. Verifier code itself must be Tier-1 pinned and compilable without deferred features.
    - New `attest` module in `paideia-pq-sign`: bind TPM 2.0 PCRs to signature metadata for measured-boot attestation (paideia-os C11 dependency).
  - **AP boot support:**
    - `.pdx` module attribute `#![bits=16]` for real-mode AP trampolines. Round-trip on iced-x86 (or equivalent 16-bit decoder — iced-x86 covers 16-bit).
    - Selector encoding + far-jump idioms for the 16→32→64 chain in AP bring-up.
- **Exit criteria:**
  - AP boot in `.pdx` — the trampoline code from real mode to long mode is authored in `.pdx`, not hand-written in `.S`.
  - Per-CPU runqueues in `.pdx` using `percpu let mut` — no `unsafe { mov gs:[offset], rax }` in application code.
  - Wait-free MPMC queue benchmark landed in paideia-os performance corpus.
  - Every artifact in a `paideia-os` release is PQ-signed; the boot substrate refuses to load an unsigned or badly-signed artifact.
- **Prereqs:** v0.13 (basic atomic primitives), v0.14 (function-pointer dispatch for scheduler class virtualization), v0.15 (memory-model fences).
- **Approximate issue count:** 30-36. (Widest release — encoder, effect system, stdlib, PQ integration, and AP boot all touched.)

### 2.7 v0.19 — "Data structures + semantic terminal"

- **Codename:** SEMANTIC-DS.
- **Goal:** Rich data structure primitives so the semantic terminal (paideia-os Phase 11) is idiomatically expressible.
- **Deliverables:**
  - **Language surface:**
    - Records and discriminated unions (enums) already exist (Phase 4 m7). v0.19 rounds them out:
      - Generic records / generic enums with monomorphization (partially in Phase 6 m2; needs the last mile for stdlib containers).
      - `impl RecordType { fn ... }` inherent methods — currently only trait dispatch is documented; direct methods let stdlib types like `HashMap<K, V>` expose their API cleanly.
      - Pattern-matching on enums with exhaustiveness checking already exists; extend with or-patterns and binding patterns activation (Phase 4 m7 laid groundwork).
    - `trait` dispatch idioms for terminal-object polymorphism: `trait SemanticObject { fn describe(&self) -> String; fn schema(&self) -> Schema; }`.
    - Iterator / stream protocol formalization: `trait Iter[T] { fn next(&mut self) -> Option[T]; }`. Semantic-terminal pipelines chain via `.filter().map().collect()`.
  - **paideia-stdlib:**
    - `HashMap[K, V]` — open-addressing, robin-hood hashing. Hash trait: `trait Hash { fn hash(&self, hasher: &mut Hasher); }`. Default hasher: SipHash-1-3 (DoS-resistant; used by Rust std).
    - `BTreeMap[K, V]` — ordered map, needed for TCP flow state indexed by 5-tuple.
    - `Vec[T]`, `String`, `Option[T]`, `Result[T, E]` — full method surface parity with the informal set already used.
    - `Regex` — the semantic terminal needs it. Uses derivative-based DFA construction (predictable memory).
    - `Serde`-equivalent — needed for the semantic terminal's typed marshaling. Roll it forward from the paideia-as-diagnostics SARIF work.
  - **Encoder:**
    - No new instructions. This release is language / stdlib-heavy, encoder-light.
    - Possible small additions if paideia-stdlib benchmarks require them (e.g. `pdep`/`pext` from BMI2 for hash finalization).
- **Exit criteria:**
  - The paideia-os semantic terminal's core primitives (`type Command`, `type Session`, pipeline composition) are authored in `.pdx`, not `.S`.
  - `HashMap`, `BTreeMap`, `Vec`, `String` used pervasively across paideia-os userspace with no need for hand-rolled equivalents.
  - Regex-based command matching in the terminal works on realistic input.
- **Prereqs:** v0.13-v0.14 (encoder + function pointers), v0.15 (dma_buffer + effects — semantic terminal is a user of stdlib containers).
- **Approximate issue count:** 28-34. (Heavy stdlib work.)

### 2.8 v0.20 — "Self-hosting + portability"

- **Codename:** SELF-HOST.
- **Goal:** paideia-as compiles itself under paideia-os. Deterministic emission. Tier 3 crate ports complete.
- **Deliverables:**
  - **Portability:**
    - Every Rust crate under `crates/` has a corresponding `.pdx` twin, per the tier plan in `design/toolchain/self-hosting-phase5-plan.md`. Tier 1 (~30k LoC) and Tier 2 (~40k LoC) already have partial ports by v0.19; v0.20 completes Tier 3 (~10k LoC: emitters + linker) and closes the loop.
    - Deferred crates (`paideia-lsp`, `paideia-pq-sign`) get first-cut `.pdx` versions. LSP requires an async runtime — the `paideia-stdlib` async surface must ship in v0.19 to unblock this.
  - **Deterministic emission:**
    - Every byte of every emitted ELF64 / PAX / PE32+ is a deterministic function of the input `.pdx` sources + declared toolchain version. No embedded timestamps, no host-path leakage in DWARF, no unordered hash iteration.
    - Round-trip determinism smoke: `paideia-as build foo.pdx` twice produces byte-identical outputs.
  - **Host-target parity:**
    - The Rust-hosted paideia-as (compiled for `x86_64-unknown-linux-gnu`) and the paideia-os-hosted paideia-as (compiled for `x86_64-paideia-os-native`) produce byte-identical outputs given identical inputs.
    - Bootstrap chain documented: how a paideia-os image can rebuild paideia-as from scratch given only `.pdx` sources.
  - **Missing encoder work if any:**
    - Whatever the self-hosting effort surfaces. Expect small fixes; the encoder should already be mature by v0.18.
    - Speculative: `xgetbv` / `xsetbv` (0F 01 D0/D1) for XCR0 manipulation — needed if the self-hosted compiler runs on hardware that boots with different XCR0 defaults than QEMU.
  - **Emitter:**
    - `paideia-as-emitter-pax` gains a self-verifying mode: it produces PAX outputs and immediately re-parses them, cross-checking. Catches silent emitter drift.
  - **CI:**
    - paideia-as CI grows a "self-host smoke" stage: build paideia-as under paideia-as (Rust-hosted stage 1 → Rust-hosted stage 2 → assert stage-2 output matches stage-1 output for its own compilation).
- **Exit criteria:**
  - `paideia-as` compiled on paideia-os produces byte-identical outputs to the Rust-hosted paideia-as, for a corpus including paideia-as's own source and paideia-os's kernel.
  - Self-hosting cycle is closed: paideia-as can rebuild itself from `.pdx` sources on a running paideia-os instance.
  - Documented bootstrap chain in `design/toolchain/self-hosting-completion.md`.
- **Prereqs:** every previous release. Especially v0.14 (function pointers used pervasively in emitters), v0.18 (SMP for parallel elaboration), v0.19 (stdlib data structures).
- **Approximate issue count:** 40-50. (The largest release — this is where the ambition lives.)

---

## 3. Dependency graph

### 3.1 Strict ordering

The following pairs are strictly ordered (later depends on earlier):

- v0.13 → v0.14. Function-pointer dispatch requires the underlying `call reg` / `call [mem]` fix from v0.13.
- v0.13 → v0.16. Narrow-width memory operands (#927) are the substrate for byte-level packet parsing.
- v0.14 → v0.15. DMA / MMIO idioms depend on function-pointer dispatch for driver vops.
- v0.14 → v0.17. UEFI protocol tables are function-pointer records.
- v0.15 → v0.16. NetBuffer wraps dma_buffer.
- v0.13, v0.14, v0.15 → v0.18. SMP + PQ pulls together atomics (v0.13), dispatch (v0.14), and memory model (v0.15).
- Everything → v0.20. Self-hosting demands the full surface.

### 3.2 Parallelizable

The following releases can be parallelized once their strict prereqs are met:

- v0.16 (network) and v0.17 (UEFI) are parallel after v0.15. They touch disjoint areas.
- v0.19 (data structures) can start any time after v0.14 — its encoder needs are minimal. In practice it will overlap with v0.17 or v0.18.
- v0.18 (SMP + PQ) can start after v0.15. It doesn't block on v0.16 or v0.17.

### 3.3 Critical path

The critical path from v0.12.0 to self-hosting (v0.20):

```
v0.12.0 → v0.13 → v0.14 → v0.15 → v0.18 → v0.20
                                 ↘
                                   v0.19 → v0.20
```

v0.16 (network) and v0.17 (UEFI) are off the critical path for self-hosting but on the critical path for paideia-os Phases 8 and 12 respectively.

### 3.4 ASCII graph

```
                            ┌──────────┐
                            │  v0.12.0 │  (current)
                            └────┬─────┘
                                 │
                          ┌──────▼──────┐
                          │   v0.13     │  GAP-CATCH
                          │  (encoder)  │
                          └─────┬───────┘
                                │
                          ┌─────▼───────┐
                          │   v0.14     │  FP-DISPATCH
                          │ (fn ptrs)   │
                          └─────┬───────┘
                                │
                          ┌─────▼───────┐
                          │   v0.15     │  DMA-IDIOM
                          │ (mmio/dma)  │
                          └──┬──────┬───┘
                             │      │
             ┌───────────────┼──────┴──────────────┐
             │               │                     │
       ┌─────▼─────┐   ┌────▼──────┐        ┌──────▼──────┐
       │  v0.16    │   │  v0.17    │        │   v0.18     │  SMP-PQ
       │(net stack)│   │ (UEFI)    │        │(SMP + PQ)   │
       └───────────┘   └───────────┘        └──────┬──────┘
                                                   │
                                            ┌──────▼──────┐
                                            │   v0.19     │  SEMANTIC-DS
                                            │(data struct)│
                                            └──────┬──────┘
                                                   │
                                            ┌──────▼──────┐
                                            │   v0.20     │  SELF-HOST
                                            │(self-host)  │
                                            └─────────────┘
```

---

## 4. paideia-os consumer matrix

The following matrix maps each paideia-os phase (5-14) to the paideia-as release(s) it depends on.

| paideia-os phase                | Required paideia-as release | Why                                                                                       |
|---------------------------------|-----------------------------|-------------------------------------------------------------------------------------------|
| **P5-P6 Storage substrate**     | v0.13 + v0.14 + v0.15       | PCIe MMIO reads, function-pointer vops (VFS), `rep movsq` for DMA, ring buffers.          |
| **P7 Drivers + NIC**            | v0.14 + v0.15               | Driver hierarchy + function-pointer dispatch + typed MMIO regions + DMA rings.            |
| **P8 Network stack**            | v0.16                       | Packed-record packet parsing, endian conversion, checksums, crc32c, dispatch tables.      |
| **P9 CoW capability FS**        | v0.13 + v0.19               | Bitmap allocation (bts/btr from v0.13), refcount atomics; content-addressed via crc32c; BTreeMap for inode indexing from v0.19. |
| **P10 WASM/VM userspace runtime** | v0.14 + v0.17             | Opcode-table dispatch (v0.14); paideia-as embeddable as a runtime library for JIT (targeted in v0.17 encoder-as-library refactor). |
| **P11 Semantic terminal**       | v0.19                       | HashMap, BTreeMap, iterators, traits — full stdlib surface for typed objects.             |
| **P12 UEFI real hardware**      | v0.17                       | MS x64 ABI, GUID literals, UEFI protocol declarations, PE32+ output.                      |
| **P13 SMP + PQ trust root**     | v0.18                       | cmpxchg16b, per-CPU state, atomics with orderings, PAX signature verification path.       |
| **P14 Self-hosting**            | v0.20                       | Full Tier 1-3 crate ports, deterministic emission, host-target parity.                    |

### 4.1 Notes on P10 (WASM/VM runtime)

paideia-os Phase 10 wants userspace WASM / VM interpreters and eventually a JIT. The interpreter falls out of v0.14 (dispatch tables). The JIT is a distinct capability: it requires paideia-as's encoder crate (`paideia-as-encoder`) to be embeddable as a library — no filesystem dependency, no CLI shell-out. This is called out as a v0.17 sub-deliverable ("encoder-as-library"): factor the encoder into a pure library crate consumable from `.pdx` code inside a running paideia-os. The Rust-hosted paideia-as already has this factoring implicitly; v0.17 formalizes it and gives it a stable API.

### 4.2 Notes on P9 (CoW capability FS)

Content-addressed storage benefits from CRC32C (v0.13 crc32 instruction) and, for cryptographic content addressing, BLAKE3 (`paideia-stdlib::hash::blake3`). BLAKE3 was already flagged in the Phase 6 G8 gate as a stdlib item; the earliest it lands is Phase 7's Tier 2 activation (v0.19 for the mature form). Filesystems can start with CRC32C and swap in BLAKE3 later.

---

## 5. Language-level extensions — the plan beyond opcodes

The most consequential extensions in this roadmap are not new instructions but new language surfaces. Enumerated:

### 5.1 Function pointer types (v0.14)

- Type: `fn(T1, T2, ..., Tn) -> R` with optional `[effect, +cap]` annotation.
- Literal: `&name` produces a `fn(...)` value from a named function.
- Callable: `ptr(arg1, arg2)` — indirect call.
- Storable: record fields, arrays, module-level `let mut`.
- Enforces linearity through function pointers: consuming a linear capability via a function pointer is a consume.

### 5.2 Records and enums, matured (v0.19)

- Already exist as of Phase 4 m7. v0.19 completes:
  - Generic monomorphization matured (partial in Phase 6 m2).
  - Inherent methods (`impl T { fn ... }`) as sibling to trait impls.
  - Or-patterns + binding patterns fully activated (elaborator hookup).
  - Exhaustiveness checking hardened.

### 5.3 MS x64 calling convention annotation (v0.17)

- `@abi("ms_x64")` on function declarations.
- Elaborator and emitter select register bank + shadow-space + callee-saved discipline per attribute.
- Round-trip test: a Rust-hosted `@abi("ms_x64")` function called via MSVC-emitted caller returns correctly.

### 5.4 Better module system (v0.15 / v0.17)

- Cross-module constant imports (#900 in paideia-as; already flagged as a Phase 15 carryover blocker for boot_stub migration).
- Public constants (`pub const N : u64 = 4096`) importable across modules.
- Import syntax: `use kernel::mm::PAGE_SIZE`.
- Guaranteed by v0.15 (needed for `mmio` region declarations that share address constants across driver modules).

### 5.5 Data structure primitives (v0.19)

- `HashMap[K, V]`, `BTreeMap[K, V]`, `Vec[T]`, `String` — in `paideia-stdlib`.
- Trait-based extensibility: `trait Hash`, `trait Ord`, `trait Iter[T]`.
- No new language surface required beyond generics + traits (already exist from Phase 4).

### 5.6 Directives (v0.13)

- `@include_bytes("path")`.
- `@align(N)` (already exists post-PA10-006y).
- `@debug_break` / `@panic_here` — source-positioned trap emission.
- `@abi("...")` (v0.17).
- `@embed_align(N)` postfix (v0.13).

### 5.7 Effect system extensions (v0.15, v0.18)

- `+mmio` (v0.15) — device-register access.
- `+dma` (v0.15) — DMA buffer ownership.
- `+atomic(ordering)` (v0.18) — memory-ordering annotation.
- `+irq_disabled` (v0.18) — critical section marker; conflicts with `+wait`.

### 5.8 Per-CPU state (v0.18)

- `percpu let mut X : T = init` — compiler wires GS-relative addressing.
- Access via normal syntax; compiler emits `mov reg, [gs:offset]`.
- Init runs per-CPU at boot.

### 5.9 Packed records + endian scalars (v0.16)

- `packed record Name { field: type, ... }` — no padding.
- `u16be`, `u32be`, `u64be`, `u16le`, `u32le`, `u64le` — endian-tagged scalars.
- Conversion methods: `.to_native()`, `.from_native()`.
- Parse combinators: `parse::<T>(&[u8]) -> Result[T, ParseError]`.

---

## 6. Self-hosting outlook

Self-hosting requires:

1. **All Tier 1 crates in `.pdx`** (lexer, parser, AST, diagnostics). Groundwork in Phase 6-7 of paideia-as (see `design/toolchain/self-hosting-phase5-plan.md`). Fully landed by v0.19.
2. **All Tier 2 crates in `.pdx`** (types, effects, IR, elaborator, dwarf, encoder, linker). Landed incrementally through v0.19; the elaborator (19k LoC) is the biggest single port and gates the whole cycle.
3. **All Tier 3 crates in `.pdx`** (emitter-elf, emitter-pax, emitter-pe). Deferred to v0.20.
4. **paideia-stdlib complete** — SmallVec, Unicode XID, serde-equivalent, BLAKE3, LRU cache (from the G8 gate), plus HashMap, BTreeMap, Regex, and the full container / iterator surface.
5. **Deterministic emission** — no host-path leakage, no timestamp embedding, ordered hash iteration.
6. **Host-target parity** — byte-identical output from Rust-hosted paideia-as and paideia-os-hosted paideia-as, for a self-compilation corpus.
7. **A running paideia-os with a filesystem** — Phase 5-6 completion.

The path is: paideia-os Phases 5-9 land while paideia-as ships v0.13-v0.19. Once paideia-os has a running FS, terminal, and can invoke external programs, v0.20 executes.

**Bootstrap contract.** The bootstrap chain is:
- Stage 0: Rust host builds paideia-as (Rust-hosted). This is the existing state.
- Stage 1: Rust-hosted paideia-as compiles `.pdx` versions of all crates → produces a paideia-as-native binary.
- Stage 2: The paideia-as-native binary compiles the same `.pdx` sources → produces a Stage 2 binary.
- Stage 3: Verify Stage 1 and Stage 2 are byte-identical.

Once Stage 3 verifies, paideia-as is self-hosted. From that point, the Rust host is optional — it stays as a redundant bootstrap path for auditability but is not required for ongoing development.

---

## 7. Testing / verification discipline

### 7.1 iced-x86 round-trip corpus

Every new instruction added in v0.13-v0.20 must ship with:
1. Byte-exact tests (`assert_eq!(encode(...), [0x48, 0x89, ...])`) for a minimum of 3 register / addressing-mode variants.
2. iced-x86 round-trip (decode + re-encode + verify) for the same variants.
3. Negative-shape tests (wrong operand count, unsupported operand type) returning the canonical elaborator diagnostic.

### 7.2 Fuzz layer

Introduce at v0.15 (when memory operand encoding gets richer):
- `cargo fuzz` target that generates random `.pdx` inputs of a curated grammar subset, elaborates to IR, encodes, then round-trips through iced-x86.
- Regression corpus: every historical encoder bug (issues #876, #877, #909, #910, #911, #912, #913, #914-924, #927-929) becomes a corpus entry.

### 7.3 Golden output regression

Introduce at v0.16 (packet parsing):
- Golden byte-for-byte outputs for a corpus of representative `.pdx` sources.
- Any encoder change must justify a golden update or a bugfix.

### 7.4 Self-emit smoke (v0.20 prerequisite)

Introduce at v0.18:
- paideia-as (Rust-hosted) compiles the Tier 1 `.pdx` crates and confirms the output binary re-parses cleanly and passes its own test suite.
- This is not full self-hosting (the Rust host still runs the compiler), but it exercises every code path a self-hosted compiler will touch.

### 7.5 SMP stress (v0.18)

- Multi-thread torture tests exercising `atomic<T>` primitives on 8-16 core hardware. QEMU KVM required (TCG is deterministic and hides races).

### 7.6 PQ ACVP corpus (v0.18)

- Close #525 by landing NIST ACVP validation vectors for ML-DSA-65.
- Add ACVP vectors for ML-KEM-768 if paideia-os pulls in the KEM crate.
- Automate ACVP corpus refresh (cron / scheduled) — vectors get updated as NIST evolves.

---

## 8. PQ crypto integration outlook

The `paideia-pq-sign` crate is at ~4.5k LoC (per `design/toolchain/self-hosting-phase5-plan.md`) with a mature FIPS-204 ML-DSA-65 + Ed25519 hybrid implementation. The following work is queued:

### 8.1 v0.13 — quiet round

- No PQ changes. Focus on encoder correctness.

### 8.2 v0.14-v0.16 — quiet

- PQ crate is stable. Only bug fixes if surfaced.

### 8.3 v0.17 — release-artifact integration

- The PE/COFF emitter (added in v0.17 for UEFI) must sign its outputs. Extend `paideia-pq-sign::pax` to also sign PE32+ images: a PKCS#7-style signature block (borrowed structure) inside a WIN_CERTIFICATE table, but using ML-DSA-65 instead of RSA. This is not standards-track UEFI signing — UEFI Secure Boot demands RSA / ECDSA. Paideia-os's boot substrate has its own verifier (v0.18) that reads the ML-DSA signature; the UEFI firmware's own signature check is left to the vendor path.
- Alternatively: dual-sign PE images (ML-DSA for paideia-os verifier + ECDSA for UEFI Secure Boot compatibility). Track as a v0.17 decision gate.

### 8.4 v0.18 — full PQ integration

- **Boot-time verifier**: paideia-os's boot substrate embeds the vendor public key and rejects unsigned or badly-signed PAX segments.
- **ACVP vectors**: close #525. Land NIST ACVP validation as a permanent CI stage.
- **TPM attestation**: `paideia-pq-sign::attest` binds ML-DSA signatures to TPM 2.0 PCRs. paideia-os C11 (measured boot) consumes this.
- **HSM path**: `paideia-pq-sign::soft_hsm` is dev-only. Production HSM support (via YubiHSM or generic PKCS#11) is a follow-up.

### 8.5 Beyond v0.20 — speculative

- SLH-DSA (SPHINCS+, FIPS-205) as a signing backend option — stateless hash-based, quantum-safe, no reliance on ML-DSA's lattice assumptions surviving. Adds ~30k LoC to `paideia-pq-sign`.
- ML-KEM-768 for key encapsulation — needed if paideia-os wants a PQ-KEX handshake in-kernel or in userspace TLS.
- Hybrid X25519 + ML-KEM-768 handshake per draft-ietf-tls-hybrid-design — landed as the paideia-os TLS default in Phase 8+.
- Formal verification of hybrid combiner — challenging; punt to research literature.

---

## 9. Risk map

### 9.1 Highest-risk items

1. **v0.17 PE emitter maturity**. `paideia-as-emitter-pe` today is 2.5k LoC per the tier plan. Extending it to full UEFI application subsystem (PE32+, correct section layout, relocation types, MSVC-compatible debug info) is significant. Mitigation: build against edk2 test fixtures early; land subsystem support before UEFI protocol work.

2. **v0.18 SMP correctness**. Wait-free MPMC on `cmpxchg16b` is subtle. Mitigation: pull in academic corpus (Michael-Scott, Feldman-Dechev queues) as reference; land under a fuzz + torture test regime from day one.

3. **v0.20 host-target parity**. Deterministic emission is easy to specify but hard to enforce (any non-portable Rust idiom in the Tier 1-3 crates — HashMap iteration order, filesystem readdir order, `std::env::current_dir` in DWARF — will bite). Mitigation: introduce determinism smoke as a CI stage from v0.15 onward; every emitter test asserts byte-for-byte reproducibility across two runs.

4. **v0.14 function-pointer effect propagation**. Effect polymorphism through function pointers is a well-known type-system pothole (see: Rust's `Fn`/`FnMut`/`FnOnce` split; Koka's effect-polymorphic signatures). Mitigation: keep effects on function-pointer types monomorphic (a `fn(...) [+mem]` is a different type from `fn(...) [pure]`); require explicit coercion. Revisit in v0.19 if constraints hurt.

### 9.2 Breaking changes

The following changes are anticipated to break existing `.pdx` consumers:

- **v0.13**: silently-wrong emissions (like #927) become hard errors. paideia-os code that relied on the incorrect REX.W behavior for narrow-width memory ops must be updated. Track as a paideia-os R14/R15 cleanup pass — every affected fixture explicitly re-emitted.
- **v0.15**: `+mmio` and `+dma` effect introduction. Existing hand-rolled MMIO code (paideia-os `kind_dev.pdx` OP_MAP_MMIO) doesn't declare these effects. Migration: paideia-as elaborator emits a warning (not error) for one full paideia-os phase; then converts to hard error in v0.17.
- **v0.16**: `u16be` / endian scalars. Existing paideia-os code that treats network fields as `u16` compiles but is semantically wrong (silent byteswap missing). Migration: introduce endian scalars as opt-in; encourage adoption via new stdlib types; do not error on the un-annotated form.
- **v0.18**: `atomic<T>` as the recommended interface for shared state. Hand-rolled `lock cmpxchg` still works but is discouraged.
- **v0.20**: none anticipated — determinism polish, not surface change.

### 9.3 Stability guarantees paideia-os depends on

- **Native calling convention** (calling-convention.md v1.0). Register banding (R12/R13 capability, R14/R15 effect, RDI-RCX args) is frozen. Any change is a v2 ABI (major bump). Paideia-os is engineered against v1.
- **PAX v1 wire format**. PAX section layout, signature location, effect encoding. Frozen. Any change is v2 major.
- **ABI_VERSION = 1**. Frozen through v0.20. If v2 is ever needed (SMP + effect polymorphism might push it), paideia-os gets one full phase notice to migrate.
- **Reserved-word set**. Adding new keywords to the lexer breaks `.pdx` sources that use those identifiers. Paideia-as's reserved-word policy (`design/toolchain/reserved-word-policy.md`) limits growth. New keywords proposed in this roadmap: `packed`, `mmio`, `dma_buffer`, `percpu`, `atomic`, `uefi_protocol`, `guid`, `u16be`/`u32be`/etc. Roll them out one release at a time; audit paideia-os corpus before each addition.

### 9.4 Speculative risks

- **paideia-stdlib scale**. Building HashMap, BTreeMap, Regex, Serde-equivalent in `.pdx` is 5-10k LoC each. Cumulative Tier 0 stdlib may hit 30-50k LoC. Watch for churn; land incrementally.
- **`.pdx` compiler self-elaboration bottleneck**. The elaborator is 19k LoC of Rust; the `.pdx` port may run slow at first. Optimization pass work (`optimization-passes.md`) is currently light; may need investment in v0.20.

---

## 10. Definition of done — full roadmap

The roadmap is complete when paideia-os hits Phase 14 (self-hosting). Paideia-os phase completions certify paideia-as releases as follows:

| paideia-os phase          | Certifies paideia-as release |
|---------------------------|------------------------------|
| P5-P6 (storage)           | v0.13 + v0.14                |
| P7 (drivers)              | v0.15                        |
| P8 (network)              | v0.16                        |
| P9 (CoW FS)               | v0.13 encoder gaps           |
| P10 (WASM/VM)             | v0.14 + v0.17 encoder-as-library refactor |
| P11 (semantic terminal)   | v0.19                        |
| P12 (UEFI real hardware)  | v0.17                        |
| P13 (SMP + PQ)            | v0.18                        |
| P14 (self-hosting)        | v0.20                        |

Certification means: paideia-os phase X consumes the release without needing a `.S` fallback, a manual byte-array embedding, or an `unsafe` workaround for a capability that should have been idiomatic. A paideia-as release is not certified until every paideia-os fixture that depends on it compiles and runs.

**Global DoD**: after v0.20, paideia-as-on-paideia-os compiles paideia-as's own sources byte-identically to the Rust-hosted version, and the paideia-os terminal can invoke `pas build myprogram.pdx` and see it work.

---

## 11. Appendix — release-count sanity

Total issues across v0.13-v0.20, using midpoints of the ranges above:

| Release | Approx. issues |
|---------|----------------|
| v0.13   | 25             |
| v0.14   | 20             |
| v0.15   | 27             |
| v0.16   | 24             |
| v0.17   | 29             |
| v0.18   | 33             |
| v0.19   | 31             |
| v0.20   | 45             |
| **Total** | **~234**      |

At the paideia-as 927-issue-per-12-release historical rate (~77 issues per release from Phase 1-15), 234 issues across 8 releases is *below* trend. That suggests one of two things: (a) the roadmap under-specifies (softarch's tactical breakdown should push these numbers up on subsequent revisions), or (b) the releases are larger in per-issue scope (each issue is a bigger unit of work). Both are plausible; softarch reconciles when it does the tactical decomposition.

## 12. Appendix — open questions for softarch consolidation

1. **`.pdx` `[bits=16]` for AP boot (v0.18)** — does iced-x86 fully cover 16-bit round-trip? Confirm before scoping.
2. **PQ signing of PE images (v0.17)** — dual-sign vs. single-sign? Requires paideia-os security council decision.
3. **Encoder-as-library (v0.17)** — does the refactor add per-release cost or is it a one-time carve? Softarch estimates.
4. **`+irq_disabled` effect (v0.18)** — is it worth introducing before Phase 13 SMP? Or is it a Phase 13 activation?
5. **`Regex` design (v0.19)** — DFA-based (memory predictable, exponential blowup) vs. NFA-based (linear memory, slower). Decision gate before v0.19 open.
6. **UEFI Secure Boot (v0.17)** — do we ship a vendor key ECDSA co-sign, or is it out-of-scope for the paideia-as boundary? Requires vendor-relations decision.
7. **HSM production path (v0.18)** — YubiHSM vs. generic PKCS#11 vs. TPM-native? Investigation gate.
8. **paideia-lsp async runtime (v0.20)** — how much of tokio's surface must paideia-stdlib mimic? Or is a smaller async primitive set (per-event-loop) enough?

Softarch owns tactical breakdown of these questions when it files the v0.13 issue set.

---

**End of roadmap.**
