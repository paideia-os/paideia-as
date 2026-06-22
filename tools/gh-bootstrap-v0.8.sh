#!/bin/bash
# Bootstrap GitHub state for paideia-as v0.8 round.
# Creates 5 labels, 7 milestones, and 24 issues.
# Idempotent: checks existence before creating.

set -euo pipefail

REPO="paideia-os/paideia-as"
PLAN_OSARCH=".plans/v0.8-osarch-plan.md"
PLAN_SOFTARCH=".plans/v0.8-softarch-plan.md"

# Utility: check if a label exists
label_exists() {
  local name="$1"
  gh label list --repo "$REPO" --search "$name" --json name -q '.[].name' | grep -Fxq "$name" 2>/dev/null || return 1
}

# Utility: check if a milestone exists
milestone_exists() {
  local title="$1"
  gh api "repos/$REPO/milestones?state=all" --jq ".[] | select(.title == \"$title\") | .title" | grep -Fxq "$title" 2>/dev/null || return 1
}

# Utility: check if an issue with exact title exists
issue_exists() {
  local title="$1"
  gh issue list --repo "$REPO" --search "in:title \"$title\"" --limit 5 --json title -q '.[].title' | grep -Fxq "$title" 2>/dev/null || return 1
}

# Step 1: Create labels
echo "Step 1: Creating labels..."

labels=(
  "pa8:5319E7:v0.8 round bookmark (see .plans/v0.8-osarch-plan.md)"
  "v0.8:9C27B0:Version v0.8 filter"
  "regression-verify:E91E63:Verify-and-fix a reported regression"
  "correctness-sweep:FF9800:Width / cast / sub-register sweep"
  "activate-dead-code:4CAF50:Activate structural-readiness arms from PA7C-m2"
)

for label_def in "${labels[@]}"; do
  IFS=':' read -r name color desc <<<"$label_def"
  if label_exists "$name"; then
    echo "  ✓ Label '$name' already exists"
  else
    echo "  + Creating label '$name'..."
    gh label create "$name" --color "$color" --description "$desc" --repo "$REPO" --force
  fi
done

echo "Labels created. Sleeping 2s..."
sleep 2

# Step 2: Create milestones
echo ""
echo "Step 2: Creating milestones..."

milestones=(
  "pa8-m1-foundational:pa8-m1- Foundational (regression + st_value)"
  "pa8-m2-elaborator-gaps:pa8-m2- Elaborator gaps (if-as-final + array + pointer)"
  "pa8-m3-correctness-sweep:pa8-m3- Correctness sweep (width + cast + sub-reg)"
  "pa8-m4-dead-code-activation:pa8-m4- Dead-code activation (Unsafe lowering)"
  "pa8-m5-mnemonic-bridge:pa8-m5- Supervisor + memory operand"
  "pa8-m6-cleanup:pa8-m6- Cleanup (debug trace + audit)"
  "pa8-m7-closure:pa8-m7- Closure (smoke + unquarantine + tag)"
)

for milestone_def in "${milestones[@]}"; do
  IFS=':' read -r slug title <<<"$milestone_def"
  if milestone_exists "$title"; then
    echo "  ✓ Milestone '$title' already exists"
  else
    echo "  + Creating milestone '$title'..."
    gh api "repos/$REPO/milestones" -f "title=$title" -f "state=open" > /dev/null
  fi
done

echo "Milestones created. Sleeping 2s..."
sleep 2

# Step 3: Create issues
# This is the core of the bootstrap; we create 24 issues across 7 milestones.

echo ""
echo "Step 3: Creating 24 issues..."

# Helper: create a single issue
create_issue() {
  local title="$1"
  local body="$2"
  local milestone="$3"
  local labels="$4"

  if issue_exists "$title"; then
    echo "  ✓ Issue '$title' already exists"
    return
  fi

  echo "  + Creating: $title"
  gh issue create --repo "$REPO" \
    --title "$title" \
    --body "$body" \
    --milestone "$milestone" \
    --label "$labels"
}

# === M1: Foundational (3 issues) ===

create_issue \
  "PA8-m1-001: Verify the alleged m5-002 regression; bisect to the offending commit if real" \
  "## Summary
A workerbee report claims that PA7C-m5-002 introduced a regression that breaks checkpoint-1 builds. Steps: (a) build the workspace at v0.7.0 HEAD; (b) run ./tools/build.sh in PaideiaOS against checkpoint-1 source using a paideia-as binary built from v0.7.0; (c) if green, the regression claim is misdiagnosed — bump submodule pin from 4059d87 to v0.7.0; (d) if red, git-bisect the range to find the offending commit.

## Acceptance criteria
- A script \`tools/verify-pa7c-m5-002-regression.sh\` exists that checks out paideia-as at v0.7.0, builds it, and runs ./tools/build.sh in PaideiaOS
- Script stdout includes 'REGRESSION CONFIRMED' or 'NO REGRESSION' as the last line
- On NO REGRESSION path: submodule pin bumped to v0.7.0 with message 'pa8-m1-001: bump paideia-as submodule to v0.7.0 (regression claim closed as misdiagnosed)'
- On REGRESSION CONFIRMED path: bisect output captured to .plans/pa8-m1-001-bisect.log; new issue PA8-m1-001a filed
- Decision recorded in .plans/pa8-m1-001-decision.md (~20 lines)

## Files created / modified
tools/verify-pa7c-m5-002-regression.sh (new, ~80 lines), .plans/pa8-m1-001-decision.md (new)

## Dependencies
none

## Estimated size
S

## Milestone
pa8-m1-foundational

## Unblocks paideia-os file(s)
(cross-repo submodule gate; verify before m7)

## Surfaced by
paideia-as@3a380bb (PA7-completion m5-002)

## Definition of done
Observable test: none required; done when decision recorded in .plans/pa8-m1-001-decision.md" \
  "pa8-m1- Foundational (regression + st_value)" \
  "pa8,v0.8,regression-verify,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m1-002: cmd_build: thread function_offsets into per-symbol st_value via PA7C-m1-001 symbol walk" \
  "## Summary
PA7C-m1-001 made every top-level \`let NAME : T = fn (...)\` produce a SymbolEntry with the binding's real name. PA7C-m2-001 populated function_offsets: HashMap<u32, u32> (NodeId → byte offset in .text). cmd_build.rs:767..783 reads function_offsets to compute per-symbol size but never writes the offset itself into SymbolEntry::st_value. Every exported symbol still has st_value: 0. Fix: where cmd_build constructs each SymbolEntry, set st_value = offset_in_text.

## Acceptance criteria
- Each STT_FUNC SymbolEntry emitted has st_value = function_offsets[node_id] and st_size = next_offset - st_value
- Existing PA7C-m1-002 emitter assertion continues to pass
- New integration test build_emit_pa8_st_value.rs: builds 3-function source, asserts fn_b.st_value > fn_a.st_value, contiguous packing
- New test build_emit_pa8_call_st_value.rs: builds two .o files, links via ld -r, asserts two call instructions resolve to different offsets (not offset 0)
- Four PA7C-m1-001 symbol-name tests continue to pass

## Files created / modified
crates/paideia-as/src/cmd_build.rs, crates/paideia-as/tests/build_emit_pa8_st_value.rs (new), crates/paideia-as/tests/build_emit_pa8_call_st_value.rs (new), fixtures

## Dependencies
none

## Estimated size
S

## Milestone
pa8-m1-foundational

## Unblocks paideia-os file(s)
(structural prerequisite for every checkpoint-2 file with external function calls)

## Surfaced by
paideia-os@dfac617 (checkpoint-1 unquarantine commit)

## Definition of done
Test: crates::paideia_as::tests::build_emit_pa8_st_value::test_contiguous_packing_three_functions (fails before, passes after)" \
  "pa8-m1- Foundational (regression + st_value)" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m1-003: emit_walker: assert function_offsets is populated for every top-level lambda (defence)" \
  "## Summary
Defensive complement to m1-002. After EmitWalker::run, iterate workspace's top-level let-fn bindings and assert each has an entry in function_offsets. Missing entry triggers new diagnostic B0010.

## Acceptance criteria
- Post-EmitWalker::run check in cmd_build iterates top-level let-fn bindings; missing function_offsets entry triggers B0010
- New diagnostic B0010 added to catalog.toml
- Negative test: fixture that intentionally elides function_offsets.insert triggers B0010
- Positive test: every existing build_emit_* test passes

## Files created / modified
crates/paideia-as/src/cmd_build.rs, crates/paideia-as-diagnostics/catalog.toml (B0010), crates/paideia-as/tests/build_emit_pa8_b0010.rs (new), SARIF regen

## Dependencies
m1-002

## Estimated size
XS

## Milestone
pa8-m1-foundational

## Unblocks paideia-os file(s)
(structural prerequisite)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as::tests::build_emit_pa8_b0010::test_missing_offset_triggers_b0010 (fails before, passes after)" \
  "pa8-m1- Foundational (regression + st_value)" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature"

sleep 2

# === M2: Elaborator gaps (4 issues) ===

create_issue \
  "PA8-m2-001: emit_block_body: accept Branch as the final expression of unit-typed blocks" \
  "## Summary
PA7C-m3-003 made statement-position blocks accept a trailing ; by introducing BlockKind::Statement. emit_walker's emit_block_body was not updated to handle the case where the final expression of a unit-typed block is itself a Branch. Concretely, slab_alloc in cap/slab.pdx:95..103 has an if-expression whose arms are unit-typed. Fix: in emit_block_body, when the last NodeId in the block is IrKind::Branch, recursively call emit_block_body on each arm with the same BlockKind as the enclosing block.

## Acceptance criteria
- emit_block_body recognises IrKind::Branch as the last element and dispatches per-arm
- For each arm, the arm body is elaborated; arm-end jumps stitched to block-end label
- Fixture pa8_if_as_final_expr.pdx builds; disassembly shows cmp + jcc + then-block + jmp + else-block + ret
- Negative test: value-position if (let x = if ...) continues to work
- 4 unit tests: simple if-as-tail, if-else with let-in-else, nested if-as-tail, if-as-tail with side effects

## Files created / modified
crates/paideia-as-elaborator/src/emit_walker.rs, tests/build-emit/pa8_if_as_final_expr.pdx (new), crates/paideia-as-elaborator/tests/emit_walker/branch_as_tail.rs (new)

## Dependencies
none

## Estimated size
M

## Milestone
pa8-m2-elaborator-gaps

## Unblocks paideia-os file(s)
cap/slab.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::emit_walker::branch_as_tail::test_slab_alloc_if_as_tail (fails before, passes after)" \
  "pa8-m2- Elaborator gaps (if-as-final + array + pointer)" \
  "pa8,v0.8,gap:parser-surface,area:elaborator,type:feature"

create_issue \
  "PA8-m2-002: parser + IR + cmd_build: module-level constant array-literal initialisers" \
  "## Summary
let mut free_list : [u64; 256] = [1, 2, 3, ..., 256] parses today but the initialiser does not materialise into .data. IR shape is dropped at lowering. Fix is three-part: (a) new IR shape IrKind::ArrayLit { elems: Vec<NodeId> }; (b) lower.rs arm for ExprData::ArrayLit at module level; (c) cmd_build pass that emits DataEntry.

## Acceptance criteria
- IrKind::ArrayLit { elems: Vec<NodeId> } exists in crates/paideia-as-ir/src/instruction.rs
- lower.rs routes ExprData::ArrayLit at module level to IrKind::ArrayLit
- cmd_build emits DataEntry in Section::Data for mut bindings; Section::Rodata for immutable; Section::Bss for uninitialised
- Fixture pa8_array_literal_data.pdx with [u64; 8] = [1..8] builds; readelf -x .data shows packed bytes
- u32 variant produces 4-byte elements
- Element overflow diagnostic T0540 when element exceeds width
- Rodata variant for immutable array
- Bss preservation for uninitialised array
- 6 unit tests + 2 integration tests

## Files created / modified
crates/paideia-as-ir/src/instruction.rs (ArrayLit), crates/paideia-as-elaborator/src/lower.rs (ExprData::ArrayLit), crates/paideia-as/src/cmd_build.rs (Data emission), crates/paideia-as-diagnostics/catalog.toml (T0540), 2 fixtures, 8 test files

## Dependencies
none

## Estimated size
M

## Milestone
pa8-m2-elaborator-gaps

## Unblocks paideia-os file(s)
cap/slab.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::lower::array_lit::test_array_lit_u64_to_data_section (fails before, passes after)" \
  "pa8-m2- Elaborator gaps (if-as-final + array + pointer)" \
  "pa8,v0.8,gap:parser-surface,area:elaborator,type:feature"

create_issue \
  "PA8-m2-003: lower.rs + cmd_build: pointer operand binding in module-level let initialiser" \
  "## Summary
channel.pdx declares let mut channel_pools : [[Channel; 32]; 1] = [ [ Channel { ... }, ... ] ]. The elaborator's record-literal lowering was wired for value-position, not module-level static initialisers. Fix: extend lowering to handle ExprData::RecordLit at module level by walking fields and encoding each to Section::Data.

## Acceptance criteria
- Investigation step: read channel.pdx, dispatch.pdx, allocator.pdx, mpsc_lock.pdx; identify concrete shapes; record in .plans/pa8-m2-003-investigation.md
- Record literals at module level supported: let mut ch : Channel = Channel { ... }
- Nested record + array literals: let mut pools : [[Channel; 32]; 1] = [ [ Channel { ... }; 32 ] ]
- [ Channel { ... }; 32 ] repeat-N syntax recognised and unrolled to bytes
- Fixture pa8_record_lit_module_data.pdx: 4 Channel literals → .data section 96 bytes
- Diagnostic T0541 when non-constant in module-level initialiser
- 5 unit tests + 2 integration tests

## Files created / modified
crates/paideia-as-elaborator/src/lower.rs (RecordLit), crates/paideia-as/src/cmd_build.rs (RecordLit Data), crates/paideia-as-diagnostics/catalog.toml (T0541), 2 fixtures, 7 test files, .plans/pa8-m2-003-investigation.md (new)

## Dependencies
m2-002

## Estimated size
M

## Milestone
pa8-m2-elaborator-gaps

## Unblocks paideia-os file(s)
ipc/channel.pdx, ipc/dispatch.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::lower::record_lit::test_record_lit_module_level_nested (fails before, passes after)" \
  "pa8-m2- Elaborator gaps (if-as-final + array + pointer)" \
  "pa8,v0.8,gap:parser-surface,area:elaborator,type:feature"

create_issue \
  "PA8-m2-004: tests: PaideiaOS checkpoint-2 elaborator regression suite (3-file canary)" \
  "## Summary
Cross-repo canary for m2. Integration test copies slab.pdx, channel.pdx, enqueue.pdx from PaideiaOS/.quarantine into temp dir, runs cmd_build on each, asserts (a) exit 0; (b) expected STT_FUNC symbols; (c) .data section contains expected initialiser bytes; (d) slab_alloc if-as-tail encoded correctly.

## Acceptance criteria
- crates/paideia-as/tests/paideia_os_checkpoint2_m2_canary.rs discovers PaideiaOS or skips
- Builds 3 files; asserts each has ≥1 STT_FUNC symbol with st_value > 0
- .data of slab.pdx contains 256 u64 values 1..256 packed little-endian
- .data of channel.pdx contains channel_pools layout
- slab_alloc disassembly shows cmp + jcc + then-block + jmp + else-block + ret
- Three files explicitly named with // gap: V2/V3/V4 comments

## Files created / modified
crates/paideia-as/tests/paideia_os_checkpoint2_m2_canary.rs (new)

## Dependencies
m2-001, m2-002, m2-003

## Estimated size
XS

## Milestone
pa8-m2-elaborator-gaps

## Unblocks paideia-os file(s)
cap/slab.pdx, ipc/channel.pdx, sched/enqueue.pdx (composite)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as::tests::paideia_os_checkpoint2_m2_canary::test_m2_canary_slab_alloc (fails before, passes after)" \
  "pa8-m2- Elaborator gaps (if-as-final + array + pointer)" \
  "pa8,v0.8,gap:parser-surface,area:elaborator,type:feature,unblocks-paideia-os"

sleep 2

# === M3: Correctness sweep (5 issues) ===

create_issue \
  "PA8-m3-001: emit_walker: width-thread the 11 peer Mov sites through MovSized" \
  "## Summary
PA7C-m4-003 wired MovSized into visit_let_literal only. The remaining 11 Mov sites in emit_walker.rs still emit unconditional 64-bit forms. Fix: at each emission point, read destination NodeId's TypeSideTable entry, resolve to IntWidth, select Mnemonic::MovSized { width } when width is known and not W64.

## Acceptance criteria
- All 11 Mov sites consult TypeSideTable and emit MovSized when non-W64 width determinable
- New helper EmitWalker::width_for(node_id) consolidates width-resolution logic
- Coverage matrix: 11 sites × 4 widths = 44 combinations, all have unit tests via rstest
- Fixture pa8_slab_u32_index.pdx: let cap_idx : u32 = slot_id emits 5-byte 32-bit mov, not 10-byte 64-bit
- Fixture pa8_ipc_u16_seqnum.pdx: let seq : u16 = next_seq emits 4-byte 16-bit mov
- Regression review: existing tests asserting 64-bit Mov updated; grep -c 'Mnemonic::Mov\\b' → 0-1 at end

## Files created / modified
crates/paideia-as-elaborator/src/emit_walker.rs (11 sites + width_for helper), 2 fixtures, crates/paideia-as-elaborator/tests/emit_walker/width_threading.rs (new, parametrised)

## Dependencies
none

## Estimated size
M

## Milestone
pa8-m3-correctness-sweep

## Unblocks paideia-os file(s)
cap/slab.pdx, ipc/channel.pdx, ipc/destroy_channel.pdx, sched/enqueue.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::emit_walker::width_threading::test_movzx_w32_to_w8 (fails before, passes after)" \
  "pa8-m3- Correctness sweep (width + cast + sub-reg)" \
  "pa8,v0.8,correctness-sweep,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m3-002: encoder: cast lowering by (src_width, dst_width, signedness) per SDM dispatch" \
  "## Summary
PA7C-m4-002 added ExprData::Cast + IrKind::Cast with a single encoder lowering that emits movsxd for ALL casts. Correct dispatch per SDM: widening signed → movsx{b,w}/movsxd; widening unsigned → movzx{b,w}/mov eax; narrowing → mov sub-register; same-width → no-op. Fix: extend IrKind::Cast to carry (src_width, dst_width, signed); encoder dispatches on triple.

## Acceptance criteria
- IrKind::Cast carries src: IntWidth, dst: IntWidth, signed: bool
- lower.rs populates from elaborator's type-resolved types
- Encoder dispatches on (src, dst, signed) triple; 32 combinations × 1 test = 32 unit tests
- Pointer cast no-op: let p : *u8 = addr as *u8 emits zero bytes
- Fixture pa8_pt_walk_cast_corrected.pdx: (va as u64 >> 12) as u32 & 0x1FF emits mov eax, eax or sub-register mov, not movsxd
- PA7C-m4-002 tests reviewed; movsxd assertions updated

## Files created / modified
crates/paideia-as-ir/src/instruction.rs (Cast extension), crates/paideia-as-elaborator/src/lower.rs (Cast lowering), crates/paideia-as-encoder/src/encode_instruction.rs (Cast dispatch), parametrised test file crates/paideia-as-encoder/tests/cast_dispatch.rs (new)

## Dependencies
m3-001

## Estimated size
M

## Milestone
pa8-m3-correctness-sweep

## Unblocks paideia-os file(s)
core/mm/pt_walk.pdx, cap/slab.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_encoder::tests::cast_dispatch::test_u32_to_u64_unsigned_widening (fails before, passes after)" \
  "pa8-m3- Correctness sweep (width + cast + sub-reg)" \
  "pa8,v0.8,correctness-sweep,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m3-003: encoder: sub-register encoder width-awareness for the MOV family" \
  "## Summary
Sub-register names (al, ax, eax, r8b, r9w, etc.) map to the same underlying RegId regardless of width. Encoder for mov al, 0x80 emits 64-bit mov. Fix: extend Instruction operand model with operand_width: Option<IntWidth> per-operand; encoder's MOV path reads hint and selects right opcode: mov r8, imm8 → B0+reg; mov r16, imm16 → 66 B8+reg; etc.

## Acceptance criteria
- MovSized { width } dispatches to right opcode for (reg-imm, reg-reg, reg-mem, mem-reg) × 4 widths = 16 cases
- Existing 4 reg-imm cases continue to pass
- unsafe-walker mnemonic resolver: mov al, 0x80 → Instruction { MovSized { W8 }, [Reg(rax), Imm8(0x80)] }
- Fixture pa8_uart_init_subreg.pdx: unsafe { mov al, 0x80; mov dx, 0x3FB; out dx, al } emits B0 80 66 BA FB 03 EE
- Fixture pa8_r8b_imm.pdx: mov r8b, 0x12 emits 41 B0 12
- Fixture pa8_r9w_imm.pdx: mov r9w, 0x1234 emits 66 41 B9 34 12
- Regression: 64-bit Mov tests pass; sub-register fixtures new
- 16 unit tests + 3 integration fixtures

## Files created / modified
crates/paideia-as-ir/src/instruction.rs (operand-shape clarification), crates/paideia-as-encoder/src/encode_instruction.rs::encode_mov_sized (4-form fan-out), crates/paideia-as-elaborator/src/unsafe_walker.rs (sub-reg name → (RegId, width)), 3 fixtures, 16 test files

## Dependencies
m3-001

## Estimated size
M

## Milestone
pa8-m3-correctness-sweep

## Unblocks paideia-os file(s)
cap/slab.pdx, ipi/tlb_shootdown.pdx, ipc/dispatch.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_encoder::tests::sub_register_mov::test_mov_r8b_imm8 (fails before, passes after)" \
  "pa8-m3- Correctness sweep (width + cast + sub-reg)" \
  "pa8,v0.8,correctness-sweep,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m3-004: tests: round-trip-via-iced-x86 for the m3 corrected encoder surface" \
  "## Summary
Parametrised regression suite for m3 encoder corrections. For each (mnemonic, operand-shape, width, signedness) tuple, build 4-line .pdx, encode, disassemble via iced-x86, assert disassembly matches canonical string.

## Acceptance criteria
- crates/paideia-as-encoder/tests/round_trip_pa8_m3.rs covers ≥60 source/disassembly pairs (44 from m3-001 + 12 from m3-002 + ~4 from m3-003)
- Each pair: one source line + one expected disassembly string
- Parametrised or hand-rolled vec
- Test runs in <1 second

## Files created / modified
crates/paideia-as-encoder/tests/round_trip_pa8_m3.rs (new)

## Dependencies
m3-001, m3-002, m3-003

## Estimated size
XS

## Milestone
pa8-m3-correctness-sweep

## Unblocks paideia-os file(s)
none (regression test)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_encoder::tests::round_trip_pa8_m3::test_all_combinations (fails before, passes after)" \
  "pa8-m3- Correctness sweep (width + cast + sub-reg)" \
  "pa8,v0.8,correctness-sweep,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m3-005: cross-repo: PaideiaOS R6.5 checkpoint-1 byte-identical regression sweep" \
  "## Summary
Defensive integration test: build unquarantined checkpoint-1 files (R6.5 files plus banner.pdx, uart.pdx) at pa8-m3-close paideia-as binary, compare .text against v0.7.0-built versions, assert divergence is intended. Manifest under tests/regression/pa8_m3_intended_divergence.json lists permitted divergences.

## Acceptance criteria
- crates/paideia-as/tests/pa8_m3_checkpoint1_byte_compat.rs exists
- Manifest tests/regression/pa8_m3_intended_divergence.json lists every intended divergence with V-item comment
- Test runs against PaideiaOS if present; skips otherwise
- On unintended divergence, test fails with diff against v0.7.0 reference
- Manifest checked into repo (~50 lines JSON)

## Files created / modified
crates/paideia-as/tests/pa8_m3_checkpoint1_byte_compat.rs (new), tests/regression/pa8_m3_intended_divergence.json (new), tests/regression/pa8_m3_v070_reference_bytes/ directory (~50 KB)

## Dependencies
m3-001, m3-002, m3-003

## Estimated size
S

## Milestone
pa8-m3-correctness-sweep

## Unblocks paideia-os file(s)
none (regression test)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as::tests::pa8_m3_checkpoint1_byte_compat::test_no_unintended_divergence (fails before, passes after)" \
  "pa8-m3- Correctness sweep (width + cast + sub-reg)" \
  "pa8,v0.8,correctness-sweep,gap:byte-emit,area:elaborator,type:feature"

sleep 2

# === M4: Lower Unsafe activation (3 issues) ===

create_issue \
  "PA8-m4-001: lower.rs: lower ExprData::Unsafe block body to IrKind::RawInstruction children" \
  "## Summary
lower.rs:416..417 maps NodeKind::ExprUnsafe to IrKind::Unsafe but the block body is dropped. Fix: iterate ast.children(node_id), match each child's NodeKind against unsafe-asm-shape set (Mov, In, Out, Cli, Sti, Hlt, Wrmsr, Rdmsr, Iret, Iretq, Lgdt, Lidt, Invlpg, Swapgs, Int, Rdtsc, MovCr, MovDr), emit IrKind::RawInstruction as IR child of Unsafe parent.

## Acceptance criteria
- lower.rs::lower_expr_unsafe walks block body and emits one RawInstruction per recognised asm statement
- Non-recognised statements lower to normal IR shape; still children of Unsafe parent
- PA7C-m2 arms in emit_walker now activate: RawInstruction routed per PA7C-m2-001
- Fixture pa8_lower_unsafe_basic.pdx: let f : () -> () = fn () -> unsafe { cli; hlt } builds to .text containing FA F4
- PA7C uart_smoke fixture passes byte-identically
- Fixture pa8_lower_unsafe_three_stmt.pdx: three-instruction sequence
- Negative test: malformed unsafe block (not_a_mnemonic) emits U1605
- 6 unit tests at lower.rs + emit_walker

## Files created / modified
crates/paideia-as-elaborator/src/lower.rs (ExprUnsafe arm), 2 fixtures, crates/paideia-as-elaborator/tests/lower/unsafe_block.rs (new)

## Dependencies
m2 (elaborator-gap closures inform asm-shape recognition)

## Estimated size
M

## Milestone
pa8-m4-dead-code-activation

## Unblocks paideia-os file(s)
(structural prerequisite for all 9 quarantined files with unsafe blocks)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::lower::unsafe_block::test_cli_hlt_sequence (fails before, passes after)" \
  "pa8-m4- Dead-code activation (Unsafe lowering)" \
  "pa8,v0.8,activate-dead-code,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m4-002: emit_walker: deprecate the dead-code arms or document them as live" \
  "## Summary
Companion to m4-001. Once V9 lands, three PA7C-m2 arms become live. Audit each: (a) confirm reached by at least one test; (b) ensure helper code paths populated + consumed correctly; (c) remove #[allow(dead_code)] annotations. If still unreachable post-m4-001, document architectural reason as comment.

## Acceptance criteria
- Every PA7C-m2 arm (grep -n 'PA7C-m2') reached by at least one test
- No #[allow(dead_code)] on PA7C-m2 helpers (unless documented as reserved)
- scratch_assignment Vec populated by PA7C-m2-002 asserted non-empty by fixture debug print or test
- Round-trip witness: Operand::Var resolution exercised by pa8_lower_unsafe_let_scratch.pdx
- Audit findings committed to .plans/pa8-m4-002-arm-audit.md (~30 lines)

## Files created / modified
crates/paideia-as-elaborator/src/emit_walker.rs (annotation cleanup), .plans/pa8-m4-002-arm-audit.md (new), 1 fixture

## Dependencies
m4-001

## Estimated size
S

## Milestone
pa8-m4-dead-code-activation

## Unblocks paideia-os file(s)
none (cleanup)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::emit_walker::unsafe_arms::test_operand_var_resolution (fails before, passes after)" \
  "pa8-m4- Dead-code activation (Unsafe lowering)" \
  "pa8,v0.8,activate-dead-code,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m4-003: tests: PaideiaOS unsafe-block-heavy file regression" \
  "## Summary
Cross-repo canary that builds the most unsafe-block-heavy quarantined file (ipi/tlb_shootdown.pdx) at post-m4 paideia-as and asserts (a) build exit 0; (b) .text contains real instructions (not placeholder mov rax, rax); (c) each unsafe block contributes expected byte count.

## Acceptance criteria
- crates/paideia-as/tests/paideia_os_tlb_shootdown_lower.rs discovers PaideiaOS or skips
- Builds .quarantine/src/kernel/core/ipi/tlb_shootdown.pdx; asserts exit 0
- Asserts .text byte count exceeds placeholder lower bound (3 × 3 = 9)
- Records actual byte count in .plans/pa8-m4-003-tlb-baseline.md

## Files created / modified
crates/paideia-as/tests/paideia_os_tlb_shootdown_lower.rs (new), .plans/pa8-m4-003-tlb-baseline.md (new)

## Dependencies
m4-001

## Estimated size
XS

## Milestone
pa8-m4-dead-code-activation

## Unblocks paideia-os file(s)
ipi/tlb_shootdown.pdx (composite with m5-002)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as::tests::paideia_os_tlb_shootdown_lower::test_three_unsafe_blocks_encoded (fails before, passes after)" \
  "pa8-m4- Dead-code activation (Unsafe lowering)" \
  "pa8,v0.8,activate-dead-code,gap:byte-emit,area:elaborator,type:feature,unblocks-paideia-os"

sleep 2

# === M5: Supervisor + memory operand (4 issues) ===

create_issue \
  "PA8-m5-001: unsafe_walker: complete the per-mnemonic dispatch table to reach every existing encoder" \
  "## Summary
Encoders for every supervisor mnemonic exist. unsafe_walker's mnemonic-name resolver is incomplete: PA7C-m3 added basic set but not full long-mode supervisor surface. Today, wrmsr/lgdt/invlpg fall through to silent-drop. Fix: extend mnemonic resolver to recognise every name in encoder dispatch table. Hand-typed table: 'wrmsr' => Some(Mnemonic::Wrmsr), etc.

## Acceptance criteria
- Mnemonic resolver recognises: cli, sti, hlt, nop, swapgs, cpuid, in, out, wrmsr, rdmsr, int, mov cr0..cr8, mov dr0..dr7, lgdt, lidt, iret, iretq, sysret, syscall, rep stosq, jmp far, invlpg, rdtsc, rdtscp
- Unit test per mnemonic asserting resolver returns right Mnemonic
- End-to-end fixture per mnemonic with existing encoder: pa8_super_<mnemonic>.pdx builds 1-line unsafe block, asserts encoded bytes
- invlpg added to unsafe-walker dispatch AND encoder if absent (SDM: 0F 01 38+r/m)
- rdtsc added similarly (SDM: 0F 31)
- U1606 diagnostic count drops by ≥5 (newly-routed mnemonics replace silent-drop)
- 20+ unit tests + 20+ fixtures

## Files created / modified
crates/paideia-as-elaborator/src/unsafe_walker.rs (mnemonic table), crates/paideia-as-encoder/src/encode_instruction.rs (invlpg + rdtsc if absent), 20+ fixtures, parametrised test file

## Dependencies
m4-001

## Estimated size
M

## Milestone
pa8-m5-mnemonic-bridge

## Unblocks paideia-os file(s)
ipi/tlb_shootdown.pdx (invlpg), structural prerequisite for R7 LAPIC work

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_elaborator::tests::unsafe_walker::mnemonic_dispatch::test_invlpg_dispatch (fails before, passes after)" \
  "pa8-m5- Supervisor + memory operand" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature,unblocks-paideia-os"

create_issue \
  "PA8-m5-002: encoder + IR: general memory operand mov [base + disp] for both load and store" \
  "## Summary
Today mov rax, [r10 + 24] works only for specific paths (struct-field, array indexing). General form (any base RegId, immediate displacement) for both load/store not exposed at IR level. Fix: introduce IrKind::MemLoad { base, disp, width } and IrKind::MemStore { base, disp, value, width }; add encoder paths emitting canonical SIB-less [base + disp] form per SDM ModRM table.

## Acceptance criteria
- IrKind::MemLoad and IrKind::MemStore exist; PA7C-m5-002's Store either deprecated or thin wrapper
- Encoder emits 48 8B 47 10 for mov rax, [rdi + 16] (disp8)
- Encoder emits 48 8B 87 00 01 00 00 for mov rax, [rdi + 256] (disp32)
- Encoder emits 48 89 47 10 for mov [rdi + 16], rax (disp8)
- Width variants (u8/u16/u32) compose with m3-003 sub-register
- Negative disp: mov rax, [rdi - 8] emits 48 8B 47 F8
- Fixture pa8_channel_head_load.pdx: let h : u64 = (*ch).head emits 48 8B 47 10
- Fixture pa8_channel_head_store.pdx: (*ch).head = new_head emits 48 89 47 10
- 12 unit tests (3 widths × 2 directions × 2 disp sizes) + 2 fixtures

## Files created / modified
crates/paideia-as-ir/src/instruction.rs (MemLoad/MemStore), crates/paideia-as-elaborator/src/lower.rs (lowering), crates/paideia-as-encoder/src/encode_instruction.rs (encode_mem_load, encode_mem_store), 2 fixtures, 12 test files

## Dependencies
m3 (MemLoad/MemStore compose with m3-003 width-aware encoding)

## Estimated size
M

## Milestone
pa8-m5-mnemonic-bridge

## Unblocks paideia-os file(s)
ipc/channel.pdx, ipc/destroy_channel.pdx, sched/enqueue.pdx

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_encoder::tests::mem_operand::test_mem_load_disp8_u64 (fails before, passes after)" \
  "pa8-m5- Supervisor + memory operand" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature,unblocks-paideia-os"

create_issue \
  "PA8-m5-003: tests: PaideiaOS LAPIC/IPI byte-sequence regression" \
  "## Summary
Cross-repo witness that supervisor mnemonics + memory operands are end-to-end correct. Builds existing R6.5 supervisor files (idt.pdx, exceptions.pdx, mm/pt_walk.pdx) at pa8-m5 and asserts placeholder mov rax, rax sequences are replaced by real bytes.

## Acceptance criteria
- crates/paideia-as/tests/paideia_os_supervisor_post_m5.rs exists
- Builds idt.pdx, exceptions.pdx, mm/pt_walk.pdx; asserts .text byte count exceeds pre-m5 baseline
- Disassembles via iced-x86; asserts presence of lgdt/lidt/wrmsr/iretq per file

## Files created / modified
crates/paideia-as/tests/paideia_os_supervisor_post_m5.rs (new), .plans/pa8-m5-003-pre-baseline.md (new)

## Dependencies
m5-001

## Estimated size
XS

## Milestone
pa8-m5-mnemonic-bridge

## Unblocks paideia-os file(s)
none (regression test)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as::tests::paideia_os_supervisor_post_m5::test_idt_contains_lgdt (fails before, passes after)" \
  "pa8-m5- Supervisor + memory operand" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m5-004: tests: round-trip-via-iced-x86 for m5 (memops + supervisor surface)" \
  "## Summary
Parametrised regression suite for m5 surface. Each (mnemonic, operands) tuple builds 1-line fixture, encodes, disassembles, asserts disassembly matches canonical string.

## Acceptance criteria
- crates/paideia-as-encoder/tests/round_trip_pa8_m5.rs covers ≥40 source/disassembly pairs (20 mnemonics + 12 memory operand + 8 width variants)
- Each pair: source + expected disassembly
- Test runs in <1 second

## Files created / modified
crates/paideia-as-encoder/tests/round_trip_pa8_m5.rs (new)

## Dependencies
m5-001, m5-002

## Estimated size
XS

## Milestone
pa8-m5-mnemonic-bridge

## Unblocks paideia-os file(s)
none (regression test)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as_encoder::tests::round_trip_pa8_m5::test_all_combinations (fails before, passes after)" \
  "pa8-m5- Supervisor + memory operand" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature"

sleep 2

# === M6: Cleanup (2 issues) ===

create_issue \
  "PA8-m6-001: emit_walker: remove or gate the debug eprintln traces" \
  "## Summary
PA7C-m4 left many eprintln calls in emit_walker.rs for debugging. These produce stderr noise during every test run. Fix: delete them (the assertions and tests catch the conditions they probed).

## Acceptance criteria
- grep -c 'eprintln!' crates/paideia-as-elaborator/src/emit_walker.rs matches v0.6.0 baseline (0 or 1)
- Any eprintln left has explicit comment justifying why
- Workspace test stderr output shrinks by ≥200 lines
- All existing tests continue to pass byte-identically

## Files created / modified
crates/paideia-as-elaborator/src/emit_walker.rs

## Dependencies
none

## Estimated size
XS

## Milestone
pa8-m6-cleanup

## Unblocks paideia-os file(s)
none (cleanup)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: none required; done when eprintln cleanup verified" \
  "pa8-m6- Cleanup (debug trace + audit)" \
  "pa8,v0.8,gap:anticipated,area:elaborator,type:feature"

create_issue \
  "PA8-m6-002: diagnostics + workspace audit: catalog entries + SARIF regen + test-count audit" \
  "## Summary
Compose catalog updates for new diagnostics (B0010 from m1-003, T0540 from m2-002, T0541 from m2-003, any U16XX from m4-001/m5-001) and regenerate SARIF. Verify workspace test count crossed 2900 (from 2760 at v0.7.0).

## Acceptance criteria
- Every new diagnostic has catalog entry in crates/paideia-as-diagnostics/catalog.toml with description, fix, severity
- SARIF snapshot regenerated and committed
- Workspace test count recorded in .plans/pa8-m6-002-test-count.md with breakdown
- Count reflected in v0.8.0 CHANGELOG entry

## Files created / modified
crates/paideia-as-diagnostics/catalog.toml, crates/paideia-as-diagnostics/data/catalog.sarif, .plans/pa8-m6-002-test-count.md (new)

## Dependencies
m1, m2, m3, m4, m5 (catalog updates accumulate)

## Estimated size
XS

## Milestone
pa8-m6-cleanup

## Unblocks paideia-os file(s)
none (bookkeeping)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: none required; done when count recorded in .plans/pa8-m6-002-test-count.md" \
  "pa8-m6- Cleanup (debug trace + audit)" \
  "pa8,v0.8,gap:anticipated,area:elaborator,type:feature"

sleep 2

# === M7: Closure (3 issues) ===

create_issue \
  "PA8-m7-001: fixture: checkpoint2_orchestration.pdx exercises V2-V11 end-to-end" \
  "## Summary
Successor to PA7C-m6-001's boot_orchestration_v2.pdx. Single .pdx file exercising every gap closed in m1-m5: if-as-tail, ArrayLit .data, pointer-operand, width-threaded mov, corrected cast, sub-register encoding, real st_value, activated unsafe, supervisor mnemonics, memory operand load. Compiles + links + boots under QEMU, prints CHK2_OK\\n.

## Acceptance criteria
- Fixture pa8_checkpoint2_orchestration.pdx (~150 lines) exists
- paideia-as build produces ELF with ≥5 symbols, expected .data bytes, correct .text, expected PLT32 entries
- Link via ld produces pa8_checkpoint2.elf
- QEMU smoke via tools/run-pa8-checkpoint2-smoke.sh boots ELF, asserts stdout contains CHK2_OK\\n within 5s
- Smoke integrated into crates/paideia-as/tests/build_emit_pa8_checkpoint2.rs, gated on qemu-system-x86_64

## Files created / modified
tests/build-emit/pa8_checkpoint2_orchestration.pdx, tests/build-emit/pa8_checkpoint2_link.ld, tools/run-pa8-checkpoint2-smoke.sh, crates/paideia-as/tests/build_emit_pa8_checkpoint2.rs

## Dependencies
m1, m2, m3, m4, m5, m6

## Estimated size
S

## Milestone
pa8-m7-closure

## Unblocks paideia-os file(s)
none (integration fixture)

## Surfaced by
paideia-os@dfac617

## Definition of done
Test: crates::paideia_as::tests::build_emit_pa8_checkpoint2::test_checkpoint2_qemu_smoke (fails before, passes after)" \
  "pa8-m7- Closure (smoke + unquarantine + tag)" \
  "pa8,v0.8,gap:byte-emit,area:elaborator,type:feature"

create_issue \
  "PA8-m7-002: cross-repo: PaideiaOS 9-file unquarantine + kernel.elf re-build + QEMU smoke" \
  "## Summary
Cross-repo unquarantine pass. All 9 files in PaideiaOS/.quarantine/src/kernel/ git mv back to src/kernel/<path>. Run ./tools/build.sh, verify exit 0. Verify .quarantine/ empty. Run tools/run-smoke.sh, assert UART banner appears.

## Acceptance criteria
- All 9 files moved from .quarantine back to src/kernel/
- ./tools/build.sh in PaideiaOS exits 0, produces build/kernel.elf
- tools/stubs.S remains absent
- readelf -s build/kernel.elf | grep -E 'FUNC|OBJECT' shows ≥50 symbols
- Every symbol has st_value != 0
- nm -u build/kernel.elf shows zero undefined symbols
- tools/run-smoke.sh exits 0, stdout shows UART banner
- Commit message lists every unquarantined file with unblocking V-item

## Files created / modified
PaideiaOS-side: 9 files moved, submodule pin bumped. No paideia-as-side files.

## Dependencies
m7-001, m1-001, m1-002, m2, m3, m4, m5

## Estimated size
S

## Milestone
pa8-m7-closure

## Unblocks paideia-os file(s)
ALL 9 (final unquarantine batch)

## Surfaced by
paideia-os@dfac617

## Definition of done
Checkpoint: ./tools/build.sh produces kernel.elf; tools/run-smoke.sh outputs UART banner" \
  "pa8-m7- Closure (smoke + unquarantine + tag)" \
  "pa8,v0.8,area:elaborator,type:feature,unblocks-paideia-os"

create_issue \
  "PA8-m7-003: closure: STATUS.md + v0.8.0 tag + CHANGELOG + retrospective + submodule pin" \
  "## Summary
Round-closure marker. Cargo.toml workspace.version 0.7.0 → 0.8.0. CHANGELOG.md gains v0.8.0 entry (~40 lines per v0.7.0 template). STATUS.md updated with m1-m7 closure. Tag v0.8.0 pushed. Retrospective design/toolchain/phase-transition-pa8.md documents findings: gap inventory vs surfaced; 9-file unquarantine results; carryover to v0.9.

## Acceptance criteria
- Cargo.toml workspace.version = '0.8.0'
- CHANGELOG.md v0.8.0 entry ~40 lines, includes actual test count
- STATUS.md updated with m1-m7 closure markers
- Tag v0.8.0 exists on main
- Retrospective design/toolchain/phase-transition-pa8.md exists, ~150 lines: (1) gap inventory vs surfaced; (2) regression decision; (3) unquarantine findings; (4) encoder sweep lessons; (5) v0.9 carryover
- PaideiaOS submodule pin bumped to v0.8.0; bump + unquarantine moves in single PaideiaOS PR

## Files created / modified
Cargo.toml, CHANGELOG.md, STATUS.md, design/toolchain/phase-transition-pa8.md, PaideiaOS-side submodule bump

## Dependencies
m7-002

## Estimated size
S

## Milestone
pa8-m7-closure

## Unblocks paideia-os file(s)
none (round closure)

## Surfaced by
paideia-os@dfac617

## Definition of done
Checkpoint: v0.8.0 tag pushed; PaideiaOS submodule bumped; R-phase resume begins" \
  "pa8-m7- Closure (smoke + unquarantine + tag)" \
  "pa8,v0.8,area:elaborator,type:feature"

echo ""
echo "Step 3 complete. All 24 issues created (or verified as already existing)."

# Step 4: Verify issue count
echo ""
echo "Step 4: Verifying issue count..."

count=$(gh issue list --repo "$REPO" --label "pa8" --state open --json number -q 'length')
echo "Total open issues with label 'pa8': $count (expected 24)"

if [ "$count" -eq 24 ]; then
  echo "✓ All 24 issues confirmed."

  # Get first 3 issue numbers
  first_three=$(gh issue list --repo "$REPO" --label "pa8" --state open --json number -q '.[0:3] | .[].number')
  echo "First 3 issue numbers: $first_three"

  # Count per milestone
  echo ""
  echo "Per-milestone counts:"
  milestones=(
    "pa8-m1-foundational"
    "pa8-m2- Elaborator gaps (if-as-final + array + pointer)"
    "pa8-m3- Correctness sweep (width + cast + sub-reg)"
    "pa8-m4- Dead-code activation (Unsafe lowering)"
    "pa8-m5- Supervisor + memory operand"
    "pa8-m6- Cleanup (debug trace + audit)"
    "pa8-m7- Closure (smoke + unquarantine + tag)"
  )

  total=0
  for m in "${milestones[@]}"; do
    c=$(gh api "repos/$REPO/milestones?state=all" --jq ".[] | select(.title | contains(\"$m\")) | .open_issues" 2>/dev/null || echo "0")
    echo "  $m: $c"
    total=$((total + c))
  done
  echo "Total: $total"
else
  echo "✗ Issue count mismatch. Expected 24, got $count"
  exit 1
fi

echo ""
echo "Bootstrap complete!"
