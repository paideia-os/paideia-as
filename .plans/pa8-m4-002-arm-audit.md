# PA8-m4-002: Arm Audit Report

## Summary

After PA8-m4-001 lands (lower.rs ExprData::Unsafe activation), the three PA7C-m2 reception
arms in emit_walker.rs become live. This document audits each arm to confirm it is reached
by tests and documents any deferred or reserved code paths.

## Arm 1: IrKind::RawInstruction in emit_block_body (lines 2129-2147)

**Status:** LIVE (after m4-001)

**Reachability:**
- Reached when emit_walker processes an Unsafe node's children in `emit_block_body`.
- The m4-001 lowering ensures each statement in the unsafe block becomes an IR child.
- Test: `lower_unsafe_tests.rs` unit tests verify the lowering; integration would exercise
  the emit path when a fixture containing unsafe blocks is built.

**Code path:**
1. `emit_block_body` iterates children of an Action/Unsafe node.
2. On encountering `IrKind::RawInstruction`, it looks up the instruction in `arena.instructions()`.
3. Clones the instruction and updates `self.state.instructions` and `estimated_offset`.
4. If not found in side-table, emits diagnostic `T0526`.

**Observations:**
- The diagnostic `T0526` is documented but never tested (no negative fixture).
- The arm is functionally correct; the eprintln! (line 2132) is debug output that should
  be cleaned in m6-001.

**Test coverage:**
- Existing `unsafe_walker_tests.rs` exercises the mnemonic resolution and instruction
  payload creation.
- New `lower_unsafe_tests.rs` exercises the lowering of ExprData::Unsafe.
- Integration test fixtures `pa8_lower_unsafe_*.pdx` would exercise the full emit path,
  but they are not yet hooked into `cargo test`; they require manual invocation or a
  `build_emit_pa8_lower_unsafe_*.rs` test harness.

---

## Arm 2: Let → RawInstruction RHS (emit_walker.rs 2083-2093 area, "Edit C")

**Status:** DEFERRED (not yet activated)

**Context:**
- PA7C-m2-002 introduced scratch-let binding support: `let lcr = 0x3FB; ... mov dx, lcr`.
- The scope of m4-001 is only unsafe blocks; the scratch-let feature is a separate elaborator
  path for resolving operand variables inside the unsafe block.
- This path would be activated when a `Let` RHS operand references an unresolved `Var`.

**Location:** Lines 2083-2093 (grep for "Edit C: Handle RawInstruction RHS")

**Note:** This arm is syntactically live but semantically deferred pending the full
  Operand::Var resolution pass (m4-001 scope covers the lowering, not the operand binding).
  Verify in m5-001 + m5-002 when the supervisor mnemonic dispatch and operand resolution
  land.

---

## Arm 3: Operand::Var resolution pass in cmd_build (cmd_build.rs area, not emit_walker.rs)

**Status:** DEFERRED (not in emit_walker.rs scope of this audit)

**Context:**
- PA7C-m2-003 introduced a pass that resolves Operand::Var names to register IDs.
- This is in the `cmd_build` workspace builder, not `emit_walker.rs`.
- Scope of this audit is emit_walker.rs only.

---

## Conclusion

**Summary:**
- Arm 1 (RawInstruction in emit_block_body): LIVE after m4-001, functionally correct,
  awaiting integration test fixture.
- Arm 2 (Let → RawInstruction RHS): DEFERRED, syntactically present but semantically
  not yet activated by m4-001 scope.
- Arm 3 (Operand::Var in cmd_build): Out of scope of this audit; verify in m5-001.

**Action items for m4-003:**
- Add integration fixture `pa8_lower_unsafe_let_scratch.pdx` that uses a scratch let
  binding inside unsafe block to exercise Arm 2 (pending operand resolution).

**Action items for m5-001:**
- Verify Arm 2 and Arm 3 are exercised once the full operand resolution lands.

---
