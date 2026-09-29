# PAS-DEBT-B4-002 retirement — Wave 54 (v0.36.78, paideia-as#1524)

**Status: Path A recipe machinery + tests LANDED. Issue #1524 stays OPEN
pending Slice E follow-up.**

## What landed

1. **Registry entry** (`crates/paideia-as-elaborator/src/stdlib_lowering/
   mod.rs`): `enumerate_record_return_recipes()` now returns one
   `RecipeRecordReturn` — `CpuidOps::cpuid_leaf` with the field-exact
   `CpuidRegs` layout (4 × u32 at offsets 0/4/8/12, size 16 B, align
   4 B) and `skip_sret_splice = true`. Consumed by
   `populate_return_record_layouts` (Wave 53 plumbing) which injects a
   synthetic `Symbol` keyed `"CpuidOps::cpuid_leaf"` — the caller-side
   Slice-B site probe in `emit_call.rs` reads its `return_record_layout`
   the same way it reads a user-code Lambda's.

2. **Lowering arm** (`crates/paideia-as-elaborator/src/stdlib_lowering/
   cpuidops.rs::try_lower`): 9-instruction recipe under
   `"cpuid_leaf"`:

   ```
   push rbx                    ; 53                   (1 B)
   mov rax, rsi                ; 48 89 F0             (3 B)  — leaf → EAX
   mov rcx, rdx                ; 48 89 D1             (3 B)  — subleaf → ECX
   cpuid                       ; 0F A2                (2 B)
   mov_d [rdi + 0], eax        ; 89 07                (2 B)  — CpuidRegs.eax
   mov_d [rdi + 4], ebx        ; 89 5F 04             (3 B)  — CpuidRegs.ebx
   mov_d [rdi + 8], ecx        ; 89 4F 08             (3 B)  — CpuidRegs.ecx
   mov_d [rdi + 12], edx       ; 89 57 0C             (3 B)  — CpuidRegs.edx
   pop rbx                     ; 5B                   (1 B)
   ```

   Total: **21 bytes**. Full hex:
   `53 48 89 F0 48 89 D1 0F A2 89 07 89 5F 04 89 4F 08 89 57 0C 5B`

   Arg convention: `SysVRegs` (leaf in RSI, subleaf in RDX after Slice B
   sret-shift; sret buffer pointer in RDI). Recipe carries
   `return_record_layout = Some(CpuidRegs)` and `skip_sret_splice = true`.

3. **pdx surface** (`crates/paideia-as-stdlib/pdx/cpuid.pdx`):
   `struct CpuidRegs { eax: u32, ebx: u32, ecx: u32, edx: u32 }`
   declared at the top; `fn cpuid_leaf(leaf: u32, subleaf: u32) ->
   CpuidRegs !{sysreg} @{paideia.sysreg}` added to `trait CpuidOps`
   alongside the retained AD/BC pair. Field names + order MUST stay
   field-exact with the recipe's registered `RecordLayout.field_names`.

4. **Tests** (5 new, all in-tree):
   * `enumerate_record_return_recipes_registers_cpuid_leaf` — pins
     registry contents (replaces Wave 51's `starts_empty` test).
   * `cpuid_leaf_recipe_carries_record_return_metadata` — pins the
     recipe-level metadata (layout populated + skip_sret_splice true).
   * `cpuid_leaf_recipe_emits_expected_bytes` — byte-exact assertion
     on the 21-byte splice.
   * `cpuid_leaf_recipe_layout_matches_registry` — crosschecks the
     recipe layout against the registry entry (two sites must agree).
   * `cpuid_leaf_ad_still_lowers_and_byte_shape_is_pinned` +
     `cpuid_leaf_bc_still_lowers_and_byte_shape_is_pinned` —
     regression fingerprints on the pre-existing AD/BC arms.

5. **Docblock refresh**: `cpuidops.rs` header retitled from Wave 46
   "retirement STILL DEFERRED" to Wave 54 "Path A landed". Historical
   Gap A/B/C prose retained below the new banner as provenance.

6. **Version bump**: workspace.version 0.36.77 → 0.36.78 + top-of-
   file CHANGELOG.md entry documenting the wave.

## What did NOT land — Slice E follow-up

The recipe is compile-recognised end-to-end (elaborator sees it, emits
the correct splice bytes, stamps the correct layout on the synthetic
Symbol). However, **`populate_return_record_cons_slots` (Slice C)
does not allocate persistent sret slots for recipe callees**, because
it reads `return_record_layout_table` (keyed by user-code Let
IrNodeIds — recipes have no Let).

The failure mode is a **16-byte stack imbalance** across every
`CpuidOps::cpuid_leaf` call site:

1. `aggregate_shape = SysvSret { padded_slot: 16 }` (correctly
   resolved via the synthetic Symbol).
2. `persistent_slot = None` (no `caller_sret_slot_table` entry).
3. Caller-side transient prelude fires at `emit_call.rs` line 665:
   `sub rsp, 16; lea rdi, [rsp+0]`.
4. Arg-marshalling shifts leaf → RSI, subleaf → RDX.
5. Recipe splice at `emit_call.rs` line 1358 emits the 9 instructions
   above, then `return` at line 1405 (existing SysVRegs recipe path).
6. The `if aggregate_shape.needs_sret() && persistent_slot.is_none()
   { add rsp, padded_slot }` release at line 1459 **never fires** —
   it sits after the CALL, which the recipe path skipped.

Net: RSP left 16 B below the caller's expected level for the rest of
the function body. Any subsequent RBP-relative frame access is
correct (RBP is stable), but pushes/calls miscount.

**Fix scope (Slice E)** — a scoped elaborator wave, NOT part of Wave 54:

  * Extend `populate_return_record_cons_slots` in
    `crates/paideia-as-elaborator/src/return_record_cons_pass.rs` to
    also iterate `enumerate_record_return_recipes()`, adding each
    recipe callee to `callee_info` (keyed by
    `"<trait_name>::<method_name>"`). The rest of the pass — walking
    Lambdas for descendant Apps, allocating per-caller persistent
    slots via `caller_sret_slot_table_mut()`, summing the per-caller
    total in `caller_sret_frame_bump_table_mut()` — works unchanged.

  * With that in place, `persistent_slot` becomes `Some(slot)` for
    every `CpuidOps::cpuid_leaf` App site whose enclosing caller is
    not `@no_frame`. `emit_call.rs` line 665 takes the persistent-
    slot branch (`lea rdi, [rbp + slot.rbp_disp]` — no `sub rsp`),
    the recipe splices, `return` at line 1405 — RSP stays balanced,
    the sret buffer lives at `[rbp + slot.rbp_disp]` for the caller
    to read from.

  * Estimated as a single-file follow-up (~30 lines in
    `return_record_cons_pass.rs`, plus one App-site test).

**Test-wise** — Wave 54's byte-exact tests DO NOT flush the stack-
imbalance bug because they test the recipe in isolation (no caller-
side wiring). A Slice E fixture with an actual `let regs =
cpuid_leaf(0x01, 0); regs.eax` snippet is the right regression.

## Wave 54 answers the retirement's remaining questions

* **"Can the recipe machinery land?"** — Yes. It's landed here.
* **"Does the Wave 53 plumbing carry a real payload?"** — Yes. The
  synthetic Symbol injection now has a real entry to inject; every
  Wave 53 test that hypothesised a `CpuidOps::cpuid_leaf` entry
  now sees the real one.
* **"Do the old AD/BC recipes stay clean?"** — Yes. Both continue
  to compile to byte-identical instruction streams (regression
  tests pin the length + tail-fingerprint). No `#[deprecated]`
  attribute is applied — the stdlib_lowering machinery has no
  hook to surface such an attribute today, so a doc-only note in
  `cpuid.pdx` marks the AD/BC pair as "prefer cpuid_leaf" without
  breaking existing consumers.
* **"Can #1524 close?"** — Not yet. Slice E is the last mile.

## Cross-refs

* `CHANGELOG-1524-b4002-cpuid-sret.md` — original Wave 35 blocker
  enumeration (blockers 1-5 there are now resolved).
* `CHANGELOG-1524-b4002-cpuid-retirement-attempt.md` — Wave 46
  Gap A/B/C analysis (all three now cleared: A by #1558, B/C by #1559).
* `crates/paideia-as-elaborator/src/stdlib_lowering/cpuidops.rs` —
  the updated module docblock has cross-refs to all four waves.
* `crates/paideia-as-elaborator/src/return_record_layout_pass.rs` —
  the recipe-registry loop that consumes the new entry.
* `crates/paideia-as-elaborator/src/return_record_cons_pass.rs` —
  where Slice E's extension lands.
