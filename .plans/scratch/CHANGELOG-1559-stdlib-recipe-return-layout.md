# paideia-as#1559 — stdlib-recipe return_record_layout participation + per-Symbol Slice C splice opt-out

Wave 51 (v0.36.76 → v0.36.77, 2026-09-28). Lands PAS-DEBT-B4-002-followup
Gaps B + C — the two plumbing pieces the Wave 46 blocker enumeration
(`.plans/scratch/CHANGELOG-1524-b4002-cpuid-retirement-attempt.md`)
identified as the joint blocker on any record-returning stdlib recipe
(and therefore on the retirement of PAS-DEBT-B4-002, `cpuid_leaf`).

Wave 51 is a plumbing wave: no historical recipe changes byte-shape, no
retirement lands here. A follow-up wave appends the first
`RecipeRecordReturn` entry (record-returning `cpuid_leaf`) and retires
PAS-DEBT-B4-002 alongside; that wave is self-contained (the elaborator
surgery is done).

## Scope

### Gap B — per-Symbol Slice-C splice opt-out

The Wave 45 (v0.36.71) landing of Slice C
(`emit_ret::emit_callee_sret_splice`) fires for every Lambda whose
`Symbol` carries a populated `return_record_layout`, appending the
callee-side sret store/load stream immediately after any user-body-shape
emission. For a Lambda whose body is a `RecordCons` (the user-code
corpus Slice D populates), this composes correctly: Slice D fills the
callee-local source buffer, Slice C copies from that buffer into the
sret target.

For a Lambda whose body is raw assembly (`unsafe { block: { ... } }`)
that itself writes the return record into the ABI-mandated
register(s) or sret buffer, Slice C's splice appends a duplicate
`sub rsp, padded` + copy-from-uninitialised-source + `mov rax, rdi`
sequence over those hand-written stores. This is the specific Wave 46
blocker on hand-written record-returning callees.

**Fix.** Add `Symbol::skip_sret_splice: bool` (default `false`, mirrors
the metadata-not-identity discipline of `return_record_layout`) plus
`Symbol::with_skip_sret_splice(bool)` builder. Gate
`emit_callee_sret_splice` with an early-return on the flag. Existing
constructors and the `resolve_names` rename pass default / carry the
field respectively; every user-code record-returning callee stays
byte-identical.

### Gap C — recipe participation in `Symbol::return_record_layout`

Slice B's caller-side site probe in `emit_call.rs` reads
`Symbol::return_record_layout` off `arena.symbols().lookup_by_name(&target_name)`.
The target name for a stdlib trait method call is
`"<Trait>::<method>"` (produced by `resolve_stdlib_trait_method`), but
no Symbol is inserted into `arena.symbols_mut()` for a recipe callee
today — the recipe is looked up via `lower_stdlib_method` at emit
time, not resolved through the symbol table. Consequently, the Slice B
probe silently sees scalar-return shape for every recipe call, and the
caller-side sret dance (stack slot + LEA + arg shift + slot release)
never fires for a record-returning recipe.

**Fix.** Extend `populate_return_record_layouts` (Slice A pass) with a
tail loop that enumerates
`stdlib_lowering::enumerate_record_return_recipes()` and injects a
synthetic `Symbol` per entry, keyed `"<trait_name>::<method_name>"`,
with `return_record_layout` and `skip_sret_splice` stamped from the
`RecipeRecordReturn`. The registry starts empty; a follow-up wave
appending the first entry then transparently exposes the layout to
Slice B without further plumbing.

## Files touched

### Core plumbing

* `crates/paideia-as-ir/src/symbol.rs`
  * New `Symbol::skip_sret_splice: bool` field with docblock.
  * Defaulted `false` in every constructor (`new`, `new_with_visibility`,
    `new_with_abi`).
  * New `Symbol::with_skip_sret_splice(bool)` builder.
  * Excluded from `PartialEq`/`Eq`/`Hash` (mirrors `return_record_layout`).
  * 4 new unit tests
    (`skip_sret_splice_defaults_to_false`,
     `with_skip_sret_splice_attaches_and_clears`,
     `skip_sret_splice_not_part_of_identity`,
     `builder_chain_layout_then_skip_preserves_both`).

* `crates/paideia-as-elaborator/src/stdlib_lowering/mod.rs`
  * New `LoweringRecipe::return_record_layout: Option<RecordLayout>` +
    `LoweringRecipe::skip_sret_splice: bool` fields with docblocks.
  * New `RecipeRecordReturn` struct + `enumerate_record_return_recipes`
    top-level function; empty today.
  * 2 new unit tests
    (`preexisting_recipes_default_new_record_return_fields_to_none_and_false`,
     `enumerate_record_return_recipes_starts_empty`).

* `crates/paideia-as-elaborator/src/stdlib_lowering/{barrierops,
  bitfieldops, bitmapops, bulkmemops, bytesops, checksumops, cpuidops,
  cryptoops/mod, mldsaops, mmioops, msrops, pauseops, percpuops,
  refcountops, testloopops, tlbops}.rs`
  * Bulk-added `return_record_layout: None,` +
    `skip_sret_splice: false,` fields after every
    `extern_target: …,` line (54 sites total). Byte-identical semantics.

* `crates/paideia-as-elaborator/src/return_record_layout_pass.rs`
  * Extended `populate_return_record_layouts` with a tail loop over
    `enumerate_record_return_recipes()`; injects synthetic Symbols
    keyed `"<trait>::<method>"` with sentinel `ir_node = u32::MAX`.
  * 2 new unit tests
    (`recipe_injector_no_op_when_registry_empty`,
     `recipe_synthetic_symbol_shape_matches_slice_b_probe`).

* `crates/paideia-as-elaborator/src/emit_walker/emit_core.rs`
  * Gated `emit_callee_sret_splice` on `sym.skip_sret_splice` with a
    single early-return + docblock.

* `crates/paideia-as/src/cmd_build/resolve_names.rs`
  * Carry `sym.skip_sret_splice` across the rename via
    `.with_skip_sret_splice(...)`, mirroring the existing
    `with_return_record_layout` carry.

* `crates/paideia-as-elaborator/src/emit_walker_tests/sret_slice_c.rs`
  * 2 new emit-walker tests
    (`sysv_memory_callee_with_skip_sret_splice_omits_slice_c_splice`,
     `skip_sret_splice_is_per_callee_not_global`).

### Version + changelog

* `Cargo.toml` — workspace.version 0.36.76 → 0.36.77.
* `CHANGELOG.md` — new entry at top with cross-refs.

## Test count

10 new tests across 4 files (4 in `symbol.rs`, 2 in
`stdlib_lowering/mod.rs`, 2 in `return_record_layout_pass.rs`, 2 in
`emit_walker_tests/sret_slice_c.rs`). All exercise the new plumbing
in isolation; no existing test changes shape.

## PAS-DEBT-B4-002 (cpuid_leaf) retirement — status after Wave 51

**Now unblocked. Retirement can proceed as a single dedicated wave.**

The elaborator surgery half of the retirement is landed. What remains is
purely recipe-authoring work in `cpuidops.rs` plus a one-line registry
append. Sketch:

1. **Registry entry** — extend
   `stdlib_lowering::enumerate_record_return_recipes()` to return
   ```rust
   vec![RecipeRecordReturn {
       trait_name: "CpuidOps",
       method_name: "cpuid_leaf",
       layout: cpuid_regs_layout(), // 4×u32 → size 16, align 4
       skip_sret_splice: true,      // recipe packs directly into [rdi + 0/4/8/12]
   }]
   ```

2. **Recipe** — add a `"cpuid_leaf"` arm in
   `stdlib_lowering::cpuidops::try_lower` with `arg_convention =
   SysVRegs`, matching the RDI (sret pointer) / RSI (leaf) / RDX
   (subleaf) placement Slice B produces on the caller side:

   ```
   push rbx                 ; preserve callee-saved RBX
   mov  rax, rsi            ; leaf → EAX
   mov  rcx, rdx            ; subleaf → ECX
   cpuid                    ; EAX/EBX/ECX/EDX ← CPUID
   mov  [rdi + 0],  eax     ; ret.eax
   mov  [rdi + 4],  ebx     ; ret.ebx
   mov  [rdi + 8],  ecx     ; ret.ecx
   mov  [rdi + 12], edx     ; ret.edx
   pop  rbx                 ; restore RBX
   mov  rax, rdi            ; sret return value
   ```

3. **PDX trait** — extend `crates/paideia-as-stdlib/pdx/cpuid.pdx`
   with `fn cpuid_leaf(leaf: u32, subleaf: u32) -> CpuidRegs
   !{sysreg} @{paideia.sysreg};` and a `record CpuidRegs { eax: u32,
   ebx: u32, ecx: u32, edx: u32 }` declaration (or reference an
   equivalent named struct already registered).

4. **Retirement** — remove the deferral prose in
   `stdlib_lowering::cpuidops` module docblock (the whole "Gap A/B/C"
   subsection), rewrite the docblock as a normal recipe-family header,
   close PAS-DEBT-B4-002 in `STATUS.md` / the paideia-as debt catalog.

5. **Consumer migration** — audit call sites of `cpuid_leaf_ad` /
   `cpuid_leaf_bc` (paideia-os R18 M6 topology walker, XSAVE sizing,
   hybrid-tagging paths) and offer the record-returning replacement
   as a strict superset. The scalar-return pair can stay for a
   deprecation window; nothing forces removal on retirement day.

None of the above depends on further Slice A-D machinery — the
Wave 51 plumbing is sufficient. Estimated at one focused wave
(~200 LOC of recipe + ~40 LOC of registry + ~20 LOC of docblock
retirement + tests + trait extension).

## Gaps still remaining for full B4-002 retirement

**One.** The recipe-authoring wave itself (see the 5-step sketch above).
No further plumbing.

Adjacent, but *not* blocking B4-002 retirement:

* **Attribute-driven `#[skip_sret_splice]` on Let items.** The Wave 51
  gate reads `Symbol::skip_sret_splice`, and today only the recipe
  injector sets it (via `RecipeRecordReturn::skip_sret_splice`). A
  future Lambda-body callee written as raw assembly in .pdx would need
  either (a) a parser/lower pass reading a `#[skip_sret_splice]` item
  attribute onto `LetInfo::skip_sret_splice` → `Symbol` (the natural
  path — mirrors `@no_frame` in shape), or (b) a follow-up mechanism.
  Not needed for B4-002 (the retirement uses the recipe path, not a
  Lambda body); tracked as a general facility for hand-written stdlib
  wrappers that surface today only in the deferred docblock in
  `cpuidops.rs`.

* **`SymbolTable::lookup_by_ir_node` on the sentinel id.** The recipe-
  synthetic Symbol uses `IrNodeId::new(u32::MAX)` as an unreachable
  sentinel. A future consumer that scans `by_ir_node` for the sentinel
  would find every recipe Symbol. This is a design property, not a
  bug: no such consumer exists today, and if one is added it should
  filter by `symbol.kind == Function` and `ir_node != u32::MAX` (or
  the sentinel could be replaced with a distinct value if the space
  ever tightens).
