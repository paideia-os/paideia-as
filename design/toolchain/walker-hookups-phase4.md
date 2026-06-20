# Walker hookups (Phase 4 m1)

**Status:** Phase 4 m1 closure appendix.
**Scope:** Documents the per-kind walker surface, populate-path pattern, the m3-007 would-fire → real-rewrite flip, and the LSP-side activation.

## 0. Why m1 came after m7/m9/m10/m8/m11

The PaideiaOS-aware ordering put m7 (records/enums) → m9 (generics) → m10 (allocator) → m8 (strings/loops) → m11 (stdlib) → m1 (walker hookups) → m2/m3/m4-m6/m12-14. m1 came later than the original Phase 4 plan's default because PaideiaOS subsystem code is gated more by surface-language gaps (records, allocator, stdlib) than by elaborator-side walker quality.

Today m1's job is to make the m9/m10/m11 stdlib code actually elaborate end-to-end through the LSP, and to flip the remaining 4 m3-007 would-fire optimisation passes to real rewrites.

## 1. The walker surface (m1-001..004)

Phase 3 m4 shipped the **lookup paths** (PositionIndex / NameResolutionTable / InstructionSideTable side-tables). m1-001..004 close the **walker-side hooks** that populate those tables during traversal.

### 1.1 Call introspection (m1-001)

`paideia-as-ir::call_meta::CallSideTable` (new) maps `IrNodeId → CallMeta { callee_name, arg_count, is_intrinsic }`. The IR lowerer records this metadata when creating Call nodes; the m2-003 populate path reads `CallMeta.is_intrinsic` to detect intrinsic calls and synthesise their Instruction payload.

For the m1-004 intrinsic catalog (index_*, ptr_sub*, etc.), `synthesise_intrinsic_instruction` emits the canonical Mnemonic + Operand + EncodingHint per intrinsic name. Today's coverage:

- `index_u64` → `Mov RAX, [RDI + RSI*8]` (opcode 0x8B, operand_size 8).
- `index_u64_set` → `Mov [RDI + RSI*8], RAX` (opcode 0x89, operand_size 8).
- `ptr_sub_bytes_u64` → `Sub RAX, RDI` (opcode 0x29, operand_size 8).
- Other intrinsics emit a stub Mov (m2-004 register-allocator integration target).

### 1.2 Match arm surface (m1-002)

`IrWalker` gains `enter_match_arm(idx, ctx)` / `exit_match_arm(idx, ctx)`. The walk driver visits the scrutinee, then enters each arm with arm-local scope, then exits.

LinearityWalker hooks the arm-enter/exit to snapshot the use-count state. After all arms walk, `check_multi_arm_consume()` detects affine bindings consumed across multiple arms — the long-deferred S0904 firing from Phase 3 m7-002.

### 1.3 Handle (effect-handler) surface (m1-003)

Same pattern as Match: `enter_handler_clause(idx, ctx)` / `exit_handler_clause(idx, ctx)`. The handler body + per-clause bodies each get their own scope. EffectRowWalker records (handler_id, effect_row_consumed) into the m3-007 HandlerSideTable on traversal (today only the lowerer populated it).

### 1.4 Branch (conditional) surface (m1-004)

`enter_branch_then(ctx)` / `exit_branch_then(ctx)` + `enter_branch_else(ctx)` / `exit_branch_else(ctx)`. PositionIndex populates per-arm so type/effect info from one branch doesn't leak to the other.

The Phase 3 m3-005 tailcall.rs "recursion check blocked on per-branch walker visibility" comment **lifts** at m1-004 — recursion checks can now use per-branch context.

## 2. Walker-side side-table population (m1-005 / m1-006)

### 2.1 PositionIndex inserts (m1-005)

Each walker (linearity, effect-row, capability, type) inserts a PositionEntry at every visited span. The walker context gains a `PositionIndexWriter` reference (interior-mutable via RefCell to avoid forced lifetime games).

Per-walker field ownership:
- LinearityWalker fills `lin_class`.
- EffectRowWalker fills `effect_row_id`.
- CapWalker fills `cap_set_id`.
- Type walker (future) fills `type_id`.

Each pass accumulates onto the same PositionEntry. After all walkers complete, the LSP queries see all four fields populated.

### 2.2 NameResolutionTable inserts (m1-006)

Symmetric: a `NameResolutionTableWriter` trait + `NameResolutionPassState` struct. The name-resolution walker pre/post visits Let nodes (definitions) and Var nodes (uses), recording `(use_span → def_span)` pairs.

LSP `definition_at_via_elaboration` and `references_at_via_elaboration` (from Phase 3 m4-003) now query the populated table instead of returning empty. Cross-document references gate on the elaborator's import-resolution chokepoint — single-document references activate at m1-006.

## 3. m3-007 would-fire → real-rewrite flip (m1-007..010)

Phase 3 m3-007 shipped 4 passes as documented "would-fire" diagnostics pending walker chokepoints. Phase 4 m1 closes the chokepoints; m1-007..010 flips each pass to real rewrites:

### 3.1 macro-fusion (m1-007 → O1504)

`macro_fusion::apply` iterates InstructionSideTable for adjacent (Cmp, Jcc) pairs. For each pair, sets `Cmp.encoding_hint` to flag macro-fusion (opcode 0x3B + operand_size 255 marker). Emits O1504 with "rewrote N sites".

The actual fusion-prefix byte emission at encode time consumes the EncodingHint flag in the m2-002 encoder bridge.

### 3.2 branch-hint (m1-008 → O1507)

`branch_hint::apply` iterates Jcc nodes; sets EncodingHint flag for branch-hint prefix (0x3E taken / 0x2E not-taken). Emits O1507 with "rewrote N sites".

### 3.3 align (m1-009 → O1508)

`align::apply` reads m8-006's LoopMetaTable for loop-entry markers. Each Loop node gets an alignment marker on its body. Emits O1508 with "rewrote N sites".

### 3.4 pool-constants (m1-010 → O1509)

`pool_constants::apply` detects repeated Imm64 operands (≥2 occurrences). New `ConstantPoolTable` interns deduplicated constants with stable insertion-order offsets. Emits O1509 with "rewrote N sites".

Actual PC-relative load emission + paideia-link `.rodata` section threading is m2 emit-stage follow-up.

## 4. Pass-catalog regression (m1-011)

`tests/opt-regression/`'s 4 per-pass test files (macro_fusion / branch_hint / align / pool_constants) updated to assert "rewrote N sites" instead of "would-fire". Sentinel markers retired from test file headers.

Status table after m1 closure:

| Pass            | Real-rewrite shipped at | Diagnostic |
|-----------------|--------------------------|------------|
| peephole        | Phase 3 m3-001           | O1501/02   |
| schedule        | Phase 3 m3-002           | O1503      |
| dse             | Phase 3 m3-003           | O1505      |
| encode-tight    | Phase 3 m3-004           | (encoder)  |
| tailcall        | Phase 3 m3-005           | O1510      |
| macro-fusion    | **Phase 4 m1-007**       | O1504      |
| branch-hint     | **Phase 4 m1-008**       | O1507      |
| align           | **Phase 4 m1-009**       | O1508      |
| pool-constants  | **Phase 4 m1-010**       | O1509      |
| unroll          | (still would-fire)       | O1511      |

9/10 passes ship real rewrites. unroll remains would-fire pending the m8-006/m1-009 IR-level loop work being threaded through `is_unroll_safe`'s body-duplication path (m3-006 closure follow-up).

## 5. Phase-4-m1 honesty

What's shipped and what remains:

**Shipped**:
- Walker hooks for Call / Match / Handle / Branch (m1-001..004).
- Walker-side PositionIndex + NameResolutionTable population (m1-005 / m1-006).
- 4-pass m3-007 real-rewrite flip with diagnostic emission (m1-007..010).
- Regression suite update (m1-011).

**Honest deferrals**:
- LSP handler test activation (gated by elaborator per-document invocation; documented in m1-005/006 scope notes).
- Cross-document references via NameResolutionTable (gates on elaborator import resolution).
- Unroll body duplication (m3-006 closure follow-up).
- Real prefix-byte emission for macro-fusion / branch-hint (gates on encoder-side EncodingHint consumption).
- PC-relative load emission for pool-constants (gates on m2 emit-stage + paideia-link `.rodata` threading).

These deferrals are documented per-issue and don't block PaideiaOS m1 (kernel-banner-via-capability-smoke) work.

## 6. Diagnostic catalog (no new codes)

m1 introduces no new diagnostic codes. The 4 flipped passes use the m3-007 codes (O1504/07/08/09) reserved in Phase 3.

## 7. Forward links

- **m2 encoder-real-rewrites**: closes the encoder-side consumption of EncodingHint flags (macro-fusion / branch-hint prefix emission, align directive insertion, pool-constants PC-relative loads). Activates the m1-007..010 markers at code-generation time.
- **m3 runtime integrations**: cryptoki + yubihsm + reqwest. Independent of m1.
- **m4-m6 borrow stack**: enables `&mut Self` for stdlib methods; activates the linear-discipline cleanup deferred from m10/m11.
- **m12 tooling**: paideia-as test / fmt / doc CLI subcommands.
- **PaideiaOS m1**: the first kernel subsystem written in paideia-as uses the m11 stdlib + the m1-activated LSP.

## 8. Cross-reference to per-node-ir-payload-phase3.md

The Phase 3 `per-node-ir-payload-phase3.md` §7 "Deferred to Phase 3 m3" listed:

- Per-mnemonic populate-path expansion (Call / Jmp / Jcc / Add / Sub / Cmp / Lea / RepMovsb beyond the m2-003 Load/Store seed).

m1-001 (Call introspection) + m1-002 (Match arms) + m1-003 (Handle clauses) + m1-004 (Branch then/else) close that gap. The populate path now reaches every IR kind that has walker-visible structure.

**Update to `per-node-ir-payload-phase3.md`**: append a "Phase 4 cross-link" pointer in §7 noting that the populate-path expansion lands at Phase 4 m1-001..004.
