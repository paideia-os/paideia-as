# Lambda arity > 6: SysV stack-spill design

**Issue:** #1326 — P0276 lambda arity cap prevents >6-arg System-V ABI signatures
**Status:** Design (2026-08-22)
**Target:** v0.22.0 (minor bump from current v0.21.0)
**Companion doc:** `design/toolchain/calling-convention.md`

---

## 1. Motivation

paideia-as caps lambda arity at 6 primitive `u64` parameters (SysV
integer-arg registers `rdi, rsi, rdx, rcx, r8, r9`). The parser emits
**P0276** at the 7th parameter of a `fn (...) -> body` lambda; the
elaborator emits **T0521** ("SysV ABI: max 6 arguments supported") at call
sites with >6 args. Both are hard refusals, not deferrals.

On 2026-08-22 this blocked paideia-os R51.M2 (`nvme_ns_dual_kind_mint`,
9 args). The workaround — pack args behind a struct pointer — is fragile
and duplicates work that every real ABI-shaped surface (POSIX-style
`ioctl`, driver DMA descriptor construction, ACPI `Method` invocations)
would otherwise ask for directly.

The infrastructure for stack-passed integer args **already exists**:
- `paideia_as_ir::abi::map_args` computes `ArgSlot::Stack { offset }`
  for SysV positions ≥ 6 (`crates/paideia-as-ir/src/abi.rs:225–259`,
  covered by unit tests at lines 356–394).
- Issue #1277 (v0.21-001) landed the parallel MS x64 caller-side stack
  path for args ≥ 4, at ~220 LOC in `emit_call.rs` plus a fixture and
  test. See commit `0555b93`.

The remaining work is (a) removing the parser refusal, (b) mirroring the
#1277 MS caller-side pattern for SysV, (c) teaching the callee-side to
read stack-passed args, and (d) plumbing a `StackSlot` variant through
`BindingHome` + `resolve_var_operands`.

---

## 2. Where the 6-cap lives today

Six enforcement sites, four of which must change:

| # | File | Line | What | Action |
|---|------|------|------|--------|
| 1 | `crates/paideia-as-diagnostics/catalog.toml` | 1314–1322 | P0276 catalog entry (severity=error, "Lambda has more than 6 parameters") | **Rewrite** brief + description → codegen-conditional guidance; retain code |
| 2 | `crates/paideia-as-parser/src/parse_lambda.rs` | 81–103 | `parse_lambda_fn` refuses `params.len() > 6` after each group | **Delete** the arity check; params flow into `ExprData::Lambda` unmodified |
| 3 | `crates/paideia-as-parser/src/parse_lambda.rs` | 154–207 | `parse_lambda_pipe` does **not** enforce the cap — >6 pipe-form params reach codegen today | Audit-only: confirm no accidental parity break |
| 4 | `crates/paideia-as-elaborator/src/emit_call.rs` | 620–628 | Caller-side SysV `arg_idx >= arg_regs.len()` branch emits T0521 | **Replace** with stack-spill emission (mirror MS path at 494–620) |
| 5 | `crates/paideia-as-elaborator/src/emit_visit_lambda.rs` | 53–78 | `param_index_to_reg_for_abi(cc, idx)` returns `None` for SysV idx ≥ 6 | **Extend** callee-side registration to install a `BindingHome::StackSlot(off)` when `idx ≥ 6` |
| 6 | `crates/paideia-as-elaborator/src/emit_walker_tests/layouts_calls.rs` | ~2041 | Test asserts T0521 fires with "max 6" message | **Retire** with the codegen path (or rewrite as regression against the new SysV stack path) |
| 7 | `crates/paideia-as-elaborator/src/emit_lambda.rs` | 238, 338, 425, 532, 579–584 | Special-shape lambda emitters (`emit_arith_lambda`, indirect-call marshalling) hard-code `[RDI, RSI, RDX, RCX, R8, R9]` as `arg_regs` | Audit-only: none of these shapes admits >6 params (identity/bitnot/cast/double/binop are ≤2-arg), so leaving them 6-limited is correct — the widening flows through `visit_lambda`'s general path |

The AST (`ExprData::Lambda { params: Vec<NodeId>, .. }` at
`crates/paideia-as-ast/src/exprs.rs:72–82`) is arbitrary-arity; the
type/effect system does not encode an arity cap either. **The cap is
purely surface-level and codegen-level**, never type-level. That is what
makes this a codegen extension rather than a language-surface widening.

---

## 3. Language surface: is anything type-level?

No. Concretely:

- `ExprData::Lambda.params: Vec<NodeId>` accepts any count.
- The type interner represents function types as `Fn(&[TypeId], TypeId)`
  with no length bound (grep `paideia-as-types` for `Ty::Fn` / `Fn(`).
- The effect system attaches an effect row to the return type but never
  inspects arity.
- Pipe-form (`|a, b, c, ...|`) already parses arbitrary counts today and
  reaches codegen; only `fn`-style bounces off P0276 at parse.

So the language grammar has no >6-param prohibition; the parser has
been rejecting shapes the grammar admits, matching a codegen limitation
that itself is not fundamental — just unimplemented for SysV.

The design keeps the parser purely syntactic and moves all
capacity-related enforcement into codegen, where it belongs.

---

## 4. SysV stack-spill design

### 4.1 Layout (per SysV AMD64 §3.2.3)

Args 0..5 pass in `rdi, rsi, rdx, rcx, r8, r9`. Args 6..N pass on the
stack, above the return address:

```
                             ↑ higher addresses
   [rsp + 8 + 8*(i-6)]  ←  arg i  (i ≥ 6, callee view after entry)
   ...
   [rsp + 16]           ←  arg 8
   [rsp + 8]            ←  arg 7  (first stack arg)
   [rsp + 0]            ←  return address (pushed by CALL)
                             ↓ lower addresses (RSP grows down)
```

- **Caller view before CALL:** the caller writes arg 7 to `[rsp + 0]`,
  arg 8 to `[rsp + 8]`, ..., after reserving `sub rsp, N` bytes where
  `N = 8 * stack_arg_count + pad`.
- **Callee view after CALL:** the CALL push shifts each caller offset up
  by 8. Callee reads arg 7 from `[rsp + 8]` (or `[rbp + 16]` when a
  frame-pointer prologue emits, because `push rbp; mov rbp, rsp` adds
  another 8-byte offset — see §4.4).

### 4.2 Alignment

SysV requires `RSP ≡ 0 (mod 16)` at the CALL. Entry `RSP ≡ 8 (mod 16)`
(the caller-supplied return address puts us 8 bytes into the alignment
cycle). Every push moves RSP by 8, flipping parity.

The rule the caller must maintain at CALL:

```
bridge_pushes + scratch_pushes + sysv_bump + 8 (return addr) ≡ 0 (mod 16)
                                            ─── the CALL push
```

For a pure-SysV → SysV call (no bridge), the existing paideia-as SysV
path pre-#1326 relies on the callee's own `push rbp` to consume the
odd-parity slot. With stack args added:

- `stack_arg_bytes = 8 * (arg_count - 6)` — always a multiple of 8.
- `stack_arg_pad = 8` **iff** `stack_arg_count` is odd, else `0`.

`stack_arg_pad` keeps `sub rsp, (stack_arg_bytes + stack_arg_pad) ≡ 0
(mod 16)`. This is the direct analogue of `MS_CALL_STACK_BUMP_ODD_PAD`
(`abi.rs:107–111`) and `SYSV_CALL_ALIGN_PAD` (`abi.rs:113–119`).

Concretely for 7-, 8-, 9-arg calls:
- 7 args: 1 stack arg, bytes = 8, pad = 8, bump = 16.
- 8 args: 2 stack args, bytes = 16, pad = 0, bump = 16.
- 9 args: 3 stack args, bytes = 24, pad = 8, bump = 32.

### 4.3 Caller-side emission (emit_call.rs)

Replace the T0521 refusal at `emit_call.rs:620–628` with a stack-emit
branch that mirrors the MS path at `emit_call.rs:494–620`. Reuse the
existing helpers:

- `emit_mov_stack_slot_reg(id, disp, src_reg)` at `emit_call.rs:1040–1058`
  → `mov [rsp + disp], src_reg`, encodes `48 89 ... 24 disp8/disp32`.
- `emit_mov_stack_slot_imm(id, disp, value)` at `emit_call.rs:1067–1086`
  → `mov qword ptr [rsp + disp], imm32` (encoder narrows if value fits;
  otherwise the encoder emits `Unsupported`, surfaced as a build
  failure rather than silent miscompile).

Both helpers are ABI-agnostic — they emit an RSP-relative store — so no
new encoder work.

New caller-side computation (paralleling the `ms_stack_arg_*` bindings
introduced by #1277 at `emit_call.rs:270–276`):

```rust
// SysV stack passing for arg 7+.
let sysv_stack_arg_count: usize = if callee_abi == CallingConvention::Sysv {
    arg_ids.len().saturating_sub(abi::ARG_REGS.len())  // len == 6
} else {
    0
};
let sysv_stack_arg_bytes: u32 = (sysv_stack_arg_count as u32) * 8;
let sysv_stack_arg_pad:   u32 = if sysv_stack_arg_count % 2 == 1 { 8 } else { 0 };
```

The prelude bump for a plain paideia → SysV call becomes:

```rust
let sysv_bump: u32 = sysv_stack_arg_bytes + sysv_stack_arg_pad
                   + (existing #1195 pad for cross-ABI bridge_saves parity);
```

The existing `#1195` pad (`SYSV_CALL_ALIGN_PAD`) applies only when
`bridge_saves` is non-empty (paideia → explicit-SysV bridging); it
composes additively with `sysv_stack_arg_bytes + sysv_stack_arg_pad`
because both addends preserve `mod 16` invariants independently.

The arg-marshalling loop's stack-arg branch computes:

```rust
let stack_off: i32 = 8 * (arg_idx as i32 - abi::ARG_REGS.len() as i32);
                     // arg 6 → 0, arg 7 → 8, arg 8 → 16, ...
                     // (NO shadow-space offset — SysV differs from MS here)
```

then dispatches on `arg_node.kind`:
- `IrKind::Literal` → `emit_mov_stack_slot_imm(id, stack_off, value)`.
- `IrKind::Var` with a `LocalBindingTable` entry (register-homed) →
  `emit_mov_stack_slot_reg(id, stack_off, src_reg)`.
- `IrKind::Var` for a module-level `Object` constant → RIP-relative
  load into R10 (scratch), then store (identical pattern to MS at
  `emit_call.rs:541–566`).
- `IrKind::EnumCons` (nullary variant) → `emit_mov_stack_slot_imm(id,
  stack_off, variant_index as i64)`.
- Other kinds → **T0521** with "arg N: kind not yet supported for
  SysV stack passing" (matches the MS diagnostic wording).

**Emission-order note (copied from #1277's rationale):** arg 6+ writes
run strictly after arg 0..5 register MOVs, so `first_id` has already
been consumed by bridge_saves and/or the prelude `sub rsp, sysv_bump`.
Always allocate a fresh id for stack-arg stores.

**Postlude:** the existing `add rsp, ms_bump` emission logic
(`emit_call.rs:395–460`, approximate) must gain a symmetric SysV branch:
`add rsp, sysv_bump` after CALL. If the current code already unifies
"add back whatever was subtracted" via a single accumulator, this is
a one-line change; if it forks on `callee_abi`, add the SysV arm.

### 4.4 Callee-side intake (emit_visit_lambda.rs)

Extend `param_index_to_reg_for_abi` to a richer return type:

```rust
pub(crate) enum ParamHome {
    Reg(RegId),
    StackSlot { off_from_rbp: i32, off_from_rsp_at_entry: i32 },
}
```

For SysV `idx ≥ 6`:
- Frame-pointer prologue emitted (the common case): callee reads
  `[rbp + 16 + 8*(idx-6)]`. `push rbp; mov rbp, rsp` inserts one push
  (8 bytes) above the return address, so param 7 sits at `rbp+16`,
  param 8 at `rbp+24`, ...
- No prologue (`@no_frame` or unsafe-bodied): `[rsp + 8 + 8*(idx-6)]` at
  function entry. Because arbitrary user code can bump RSP in an unsafe
  body, we adopt the invariant that `@no_frame` + arity>6 is
  **disallowed** and emit a new diagnostic **B1708 `no_frame` incompatible
  with stack-passed params** (warning promoted to error under a
  `#[deny(...)]`-style attribute, or unconditionally an error for v0.22 —
  see §11). This avoids threading RSP-drift tracking through
  UnsafeWalker for a use case that has no consumer.

Extend `LocalBindingTable::BindingHome` at
`crates/paideia-as-elaborator/src/local_binding_table.rs:28–38`:

```rust
pub enum BindingHome {
    Reg(RegId),
    RegPair(RegId, RegId),
    EnvSlot(i32),          // [R14 + offset]  (existing; closure captures)
    Closure(RegId),
    StackSlot(i32),        // NEW: [RBP + offset]  (SysV stack-passed params)
}
```

- Insertion API: `insert_stack(name, off_from_rbp: i32)` — parallel to
  `insert_env`. Called by `register_nested_lambda_params` when
  `idx >= arg_regs.len()`.
- Anchor: RBP-relative because the frame-pointer prologue is emitted
  for every non-unsafe, non-`@no_frame` lambda — which is the exact
  set that permits >6 params (see B1708 above).

Extend `resolve_var_operands` at
`crates/paideia-as/src/resolve_var_operands.rs:98–121`:

```rust
BindingHome::StackSlot(off) => {
    *operand = Operand::MemSib {
        base: abi::RBP,
        index: None,
        scale: Scale::X1,
        disp: off,
    };
}
```

`live_regs()` in `LocalBindingTable` (used by `emit_call.rs:257–265`
to compute `scratch_save_set`) already returns only registers held by
`Reg` / `RegPair` bindings; `StackSlot` will be a natural no-op.

### 4.5 The 6-arg fast path stays untouched

Every code path introduced above is gated on
`arg_ids.len() > ARG_REGS.len()` (caller) or
`idx >= ARG_REGS.len()` (callee registration). For ≤6-param lambdas:

- `sysv_stack_arg_count == 0` ⇒ `sysv_bump` is either 0 (pure SysV) or
  the existing `SYSV_CALL_ALIGN_PAD` (bridge case). Byte-identical to
  today's emission.
- `register_nested_lambda_params` never installs `StackSlot`.
- `BindingHome::StackSlot` never appears in `resolve_var_operands`.
- No new `sub rsp` / `add rsp` sequence emits.

The zero-overhead pledge is upheld by construction.

---

## 5. IR / pipeline touchpoints

| Layer | File | Change |
|---|---|---|
| Parser | `parse_lambda.rs` | Delete `params.len() > 6` guard in `parse_lambda_fn`. |
| Parser (diagnostics) | `parser.rs` diagnostic emit sites | Verify no other `P0276.emit` uses. |
| Diagnostics catalog | `catalog.toml` | Rewrite P0276 brief/description; keep code allocated so old snapshots still resolve helpUri. Optionally add B1708 (see §4.4). |
| IR ABI | `abi.rs` | No change (map_args already correct). |
| Elaborator (call site) | `emit_call.rs` | Add `sysv_stack_arg_*` bindings; extend arg-marshalling loop's `arg_idx >= arg_regs.len()` branch; extend prelude `sub rsp, ...` and postlude `add rsp, ...`. |
| Elaborator (callee) | `emit_visit_lambda.rs` | Extend `param_index_to_reg_for_abi` return semantics; extend `register_nested_lambda_params` to install `StackSlot` bindings. |
| Local bindings | `local_binding_table.rs` | Add `BindingHome::StackSlot(i32)` variant + `insert_stack(name, off)`. |
| Var resolver | `paideia-as/src/resolve_var_operands.rs` | Add `StackSlot` arm → `MemSib[RBP + off]`. |
| Live-reg tracking | `local_binding_table.rs::live_regs()` | Verify `StackSlot` produces no register (should be automatic). |
| Scratch-mat / T0521 path | `emit_call.rs` non-Var arg branch (currently at the `EnumCons`/`Object` const cases) | Extend the same fanout for stack targets. |
| Encoder | `encode_instruction.rs` | **No change.** `mov [rsp + disp], reg64` and `mov [rsp + disp], imm32` are already used by MS path. `mov reg64, [rbp + disp]` (positive disp) is a standard encoding. |
| Cross-module symbol resolution | `symbols::lookup_by_name` in `emit_call.rs:224` | No change; arg count is not part of the symbol identity. |

---

## 6. Test corpus

Under `tests/build-emit/` (fixtures) and
`crates/paideia-as/tests/build_emit/` (test drivers):

| Fixture (`.pdx`) | Test driver | Assertion |
|---|---|---|
| `flat_seven_returns_seventh.pdx` | `pa_r22_1326_flat_seven.rs` | Symbol emits; body reads `[rbp+16]` into RAX and returns. |
| `flat_eight_returns_eighth.pdx` | `pa_r22_1326_flat_eight.rs` | Body reads `[rbp+24]` into RAX. |
| `flat_nine_returns_ninth.pdx` | `pa_r22_1326_flat_nine.rs` | Body reads `[rbp+32]` into RAX; nvme-ns_dual_kind_mint shape. |
| `flat_seven_caller_all_var.pdx` | `pa_r22_1326_caller_var.rs` | Caller emits `sub rsp, 16` + `mov [rsp], src`; postlude `add rsp, 16`. Alignment check on the 7-arg (odd) pad. |
| `flat_eight_caller_all_var.pdx` | `pa_r22_1326_caller_var8.rs` | Even count: `sub rsp, 16`, no pad; two stores at `[rsp]`, `[rsp+8]`. |
| `flat_nine_caller_lit_var_lit.pdx` | `pa_r22_1326_caller_mixed.rs` | Mix of Literal / Var / EnumCons at positions 7..9; verifies `emit_mov_stack_slot_imm` vs `_reg` selection. |
| `flat_seven_caller_object_const.pdx` | `pa_r22_1326_caller_obj.rs` | Position-7 arg is a module-level `Object` constant — verifies RIP-relative load into R10, then store. |
| `pa_r22_1326_unsafe_seven.rs` | (uses inline unsafe body + `@no_frame`) | Asserts new B1708 fires and no code is emitted. |
| `pa_r22_1326_cross_module_seven.rs` | Two-file: caller in module A, 7-arg callee in module B | End-to-end: linker resolves symbol, callee returns pos-7 value. |
| `pa_r22_1326_bridge_seven.rs` | paideia caller → `@abi("sysv")` 7-arg callee | Verifies `bridge_saves` composition with `sysv_stack_arg_bytes + pad`; alignment invariant preserved. |
| `pa_r22_1326_p0276_removed.rs` | Regression fixture at `tests/build-emit/flat_seven_rejects.pdx` (existing) | Rewrite: instead of asserting P0276 fires, assert the build succeeds. Retain the old fixture; retire the assertion. |

The existing negative test at
`crates/paideia-as/tests/build_emit/pa_r17_005_flat_multi_param.rs:262–295`
(`flat_seven_rejects`) inverts: assert success and check the emitted
body length is nonzero + symbol table contains the name.

The existing SysV-max-6 assertion at
`crates/paideia-as-elaborator/src/emit_walker_tests/layouts_calls.rs:2041`
retires (or gets rewritten to assert the new stack-emit shape).

---

## 7. Encoder gap check

Instructions the new codegen emits, and their existing coverage:

| Sequence | Existing coverage |
|---|---|
| `mov qword ptr [rsp + disp8], reg64` | Used by MS path (`emit_mov_stack_slot_reg` @ `emit_call.rs:1040`); encoder handles via generic `Mov r/m64, r64` with SIB (`RSP` base forces SIB byte). |
| `mov qword ptr [rsp + disp32], reg64` | Same encoder path; disp32 form triggered when `disp > 127`. Needed for 9+ arg calls with scratch pushes bumping the effective disp above 127. |
| `mov qword ptr [rsp + disp], imm32` (sign-extended) | Used by MS path (`emit_mov_stack_slot_imm` @ `emit_call.rs:1067`); encoder is `48 C7 /0`. Values outside i32 range currently trip `Unsupported` — surface as build failure, not silent miscompile. |
| `mov reg64, qword ptr [rbp + disp8]` | Standard SysV frame-pointer read; already exercised anywhere `resolve_var_operands` produces `MemSib{base:RBP}`. Grep the encoder tests for `[rbp + ` — many hits. |
| `mov reg64, qword ptr [rbp + disp32]` | Same encoder, disp32 form. Comfortable for callees with prologue + arg 30+ (well beyond 9). |
| `sub rsp, imm32` / `add rsp, imm32` | Emitted today by MS prelude/postlude (`emit_call.rs:345,405`), closure prologue (`emit_visit_lambda.rs:384–410`), and the frame-pointer path — all cross-covered. |

**No new encoder patterns.** If a future test fires
`EncodeError::Unsupported` for a wide `mov [rsp + big_disp], imm64`, that
is the correct behaviour under the current design (spec matches #1277's
policy — see `emit_mov_stack_slot_imm`'s doc comment).

---

## 8. Phased implementation

The phases below are commits in one branch (`fix/1326-sysv-stack-spill`);
each phase is buildable and testable in isolation and ships behind the
same code path — no feature flag.

| Phase | Scope | Files touched | Lines added / removed (est.) |
|---|---|---|---|
| **1. Parser opens the door** | Delete P0276 guard in `parse_lambda_fn`. Rewrite P0276 catalog brief to "reserved; codegen may still refuse >6 args on some paths" (keep the code allocated for backwards diagnostic reference). Invert `flat_seven_rejects.rs` to assert success (or move the assertion under a `#[cfg(feature = "arity-cap-legacy")]` guard). Add 7-arg AST-only round-trip test if the AST test crate can synthesise it without codegen. | `parse_lambda.rs`, `catalog.toml`, `pa_r17_005_flat_multi_param.rs` | +20 / -60 |
| **2. Caller-side SysV stack marshalling** | Mirror MS #1277 path in `emit_call.rs`: `sysv_stack_arg_*` bindings, extended `sysv_bump`, arg-marshalling loop stack-arg branch (Literal + Var register-homed cases first). Extend postlude to `add rsp, sysv_bump`. Alignment invariant proof written in a comment block. New fixtures + tests for 7- and 8-arg calls with Var-only args. | `emit_call.rs`, 2 fixtures, 2 test drivers | +250 / -15 |
| **3. Callee-side SysV stack intake** | `BindingHome::StackSlot(i32)` variant + `insert_stack`. Extend `param_index_to_reg_for_abi` (or introduce a `ParamHome` return). Extend `register_nested_lambda_params` to install `StackSlot` at idx ≥ 6. Extend `resolve_var_operands` to lower `StackSlot` → `MemSib[RBP+off]`. Introduce B1708 diagnostic for `@no_frame` + arity>6. Fixtures for 7-, 8-, 9-arg lambdas that read positional args and return one. | `local_binding_table.rs`, `emit_visit_lambda.rs`, `paideia-as/src/resolve_var_operands.rs`, `catalog.toml`, 3 fixtures, 3 test drivers | +280 / -10 |
| **4. Encoder-emission ordering + wide-disp verification** | Add a golden-byte test that a 9-arg call with 3 caller scratch pushes still produces the correct `mov [rsp + 24]` disp8 vs disp32 encoding. Wire an encoder round-trip check (via `iced-x86` decoder in a dev-dep) confirming caller-side stores land at the right RSP offsets after every push. No production code changes expected. | 1 fixture, 1 test driver | +120 / 0 |
| **5. Cross-cutting scratch-materialisation + T0521 replacement** | Extend the arg-marshalling non-Var branch (module-level `Object` constant via RIP-relative load, `EnumCons` nullary literal, other kinds) to the stack-target case. Replace the old T0521 "max 6 arguments" arm with position-specific "kind not yet supported for SysV stack passing" diagnostics (matches MS wording at `emit_call.rs:530–618`). Add regression tests for `Object`-const + `EnumCons` at positions 7+. Add the cross-ABI (paideia → `@abi("sysv")`) fixture asserting `bridge_saves` + `sysv_stack_arg_bytes + pad` compose correctly. | `emit_call.rs`, `emit_walker_tests/layouts_calls.rs`, 3 fixtures, 3 test drivers | +200 / -30 |
| **6. Test corpus + doc + version bump** | Cross-module 7-arg call fixture. `unsafe { }` + arity-7 interaction test asserting B1708. Rewrite `crates/paideia-as-elaborator/src/emit_walker_tests/layouts_calls.rs:2041` block. Update `design/toolchain/calling-convention.md` to cross-link this doc. Add `CHANGELOG.md` v0.22.0 entry. Bump `workspace.version = "0.22.0"` in `Cargo.toml`. Tag `v0.22.0`. | `Cargo.toml`, `CHANGELOG.md`, `design/toolchain/calling-convention.md`, 2 fixtures, 2 test drivers | +150 / -5 |

**Total estimate:** ~1000 lines added, ~120 lines removed, across 6
commits. Cross-check: #1277 landed the analogous MS path at ~400 lines
in one commit; SysV is comparable but larger because it also touches the
callee-side + `BindingHome` extension (MS #1277 was caller-only).

---

## 9. paideia-os re-integration (follow-up, not part of this fix)

After v0.22.0 lands and the submodule bumps in paideia-os:

1. Revert paideia-os commit `cb6291e` (the 9→6 arg refactor of
   `nvme_ns_dual_kind_mint`) back to the original 9-arg shape.
2. Verify R51.M2 rebuilds cleanly with `nvme_ns_dual_kind_mint(a, b, c,
   d, e, f, g, h, i)`.
3. Audit `paideia-os` for other struct-pointer packing done as a
   workaround for the arity cap — grep for TODOs referencing #1326 or
   P0276, and for helper structs whose only purpose is arg-count
   compaction. Convert back where clarity improves.

Note this **only** as a follow-up on issue #1326 (in the paideia-os
issue tracker), not as a blocker for the paideia-as fix landing.

---

## 10. Risks and rejected alternatives

**Rejected: RSP-relative param anchoring.**
Would let unsafe-bodied lambdas with `@no_frame` accept >6 params.
Requires tracking every push/sub/add across the function body — the
UnsafeWalker would need a symbolic RSP delta, which is a
disproportionate lift for a use case with no consumer. Instead, B1708
forbids the combination.

**Rejected: passing >6 args in extra callee-save regs (RBX, R12-R15).**
Breaks SysV compliance — external code entering paideia via the SysV
bridge cannot pack args that way. Also collides with the effect (R14/R15)
and capability (R12/R13) bands documented in
`design/toolchain/calling-convention.md §1`.

**Rejected: implicit struct-pack transformation.**
Compiler-invisible pointer indirection. Regresses register pressure at
the callee (must LOAD each field before using it), fights the type
system's transparency guarantees, and — critically — cannot ABI-bridge
to external C code that expects the raw SysV shape.

**Risk: register clobber inside caller arg-marshalling.**
When arg 7+ sources a register-homed `Var` and that register is one of
the caller-save scratch registers that may be clobbered by the
arg-register MOVs preceding this store, the current MS pattern's
scratch-save-set logic must extend to the stack-arg source registers
too. Concretely: `live_regs()` (which drives `scratch_save_set` at
`emit_call.rs:255`) already tracks every live register-homed binding —
if the stack-arg source is register-homed, it is already in `live` and
already saved. This is the reason the MS path Just Works and the SysV
path will too. Adding an explicit regression test in phase 5 covers
the composition.

**Risk: closure-body lambdas with arity>6.**
Closure frames (`closure_frame_meta`, `emit_visit_lambda.rs:384`) emit
their own `sub rsp, total_size` prologue before the body. This runs
after any frame-pointer `push rbp; mov rbp, rsp`, so RBP is still valid
for stack-param addressing. Add one fixture in phase 6 to lock this in.

---

## 11. Diagnostic changes

**P0276 (existing, rewrite):**
- Old brief: "lambda arity exceeds System-V register-passed cap (6)".
- New brief: "reserved — historical arity cap now handled by codegen".
- New description: "As of v0.22.0 the parser no longer enforces the
  6-parameter cap; lambdas with >6 parameters lower to the SysV
  stack-passing convention (paideia-as#1326). This code is retained
  for backwards-compatible diagnostic references and may be re-issued
  if a future codegen path re-imposes a cap. See B1708 for the
  `@no_frame` interaction."
- Keep `since = "0.17.0"`; add a `deprecated_since` field or a note in
  the description (depends on catalog schema — audit at implementation
  time).

**B1708 (new):**
- Category: B (build), Severity: Error, Number: 1708.
- Title: "`@no_frame` incompatible with >6-parameter lambda"
- Description: "A lambda declared with `@no_frame` cannot accept more
  than 6 parameters, because the callee-side stack-passing convention
  reads them at `[rbp + N]` offsets that require the frame-pointer
  prologue (`push rbp; mov rbp, rsp`) to be present. Remove `@no_frame`,
  or split the function into curried groups so each takes ≤6 params."
- since = "0.22.0".

**T0521 (existing, narrow):**
The "SysV ABI: max 6 arguments supported" arm at `emit_call.rs:623–628`
is retired. Any remaining T0521 firings under the SysV path now report
kind-specific messages ("SysV stack arg N: EnumCons missing metadata",
etc.) matching the MS wording — no catalog change.

---

## 12. Version discipline

- Current: `workspace.version = "0.21.0"` (`Cargo.toml:62`).
- Proposed: **`0.22.0`** (minor bump).
- Justification: additive language capability (previously-rejected
  programs now compile); no source-level breakage; no ABI change to
  ≤6-param lambdas. Semver-minor.
- Coordinated moves per project memory:
  1. `Cargo.toml` `version = "0.22.0"`.
  2. `CHANGELOG.md` new `## v0.22.0` section — one bullet per commit
     phase (compact, per memory `feedback_compact_commit_messages.md`).
  3. `git tag v0.22.0` on the merge commit.
  4. Update paideia-os submodule pin.
- `tools/find-paideia-as.sh` continues to be strict — no leeway for the
  transitional window.

---

## 13. Cross-references

- `design/toolchain/calling-convention.md` — canonical register
  discipline (this doc updates §1's "Argument" band description in
  phase 6).
- `crates/paideia-as-ir/src/abi.rs:225–259` — `map_args` for SysV
  already computes stack offsets correctly. This doc reuses that
  authority; do not duplicate the mapping in the elaborator.
- Issue #1277 / commit `0555b93` — MS x64 caller-side stack-passing
  reference implementation. Every SysV caller-side change in this doc
  is a direct analogue.
- paideia-os commit `cb6291e` — the workaround that motivated this fix
  (nvme_ns_dual_kind_mint 9→6 arg refactor); revert in follow-up.
