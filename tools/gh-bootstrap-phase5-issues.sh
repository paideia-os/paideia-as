#!/usr/bin/env bash
# Phase 5 GitHub Issue Bootstrap (Idempotent)
#
# Creates 38 Phase-5 issues against paideia-os/paideia-as per
# .plans/phase-5-build-emit-plan.md and .plans/phase-5-build-emit-softarch.md
#
# Idempotency: checks for existing milestone + issue title before creating.
# Maps each task (m1-001..m7-004) to GitHub issue number in .plans/phase-5-issue-map.tsv
#
# Run as: bash tools/gh-bootstrap-phase5-issues.sh
#

set -euo pipefail

REPO="paideia-os/paideia-as"
MAP_FILE=".plans/phase-5-issue-map.tsv"
THROTTLE_SLEEP=1  # seconds between creates

# Ensure map file exists with header
ensure_map() {
    if [[ ! -f "$MAP_FILE" ]]; then
        mkdir -p "$(dirname "$MAP_FILE")"
        echo -e "Task\tIssue\tTitle\tMilestone\tSize" > "$MAP_FILE"
    fi
}

# Check if label exists, create if not
ensure_label() {
    local label="$1"
    local color="$2"
    local description="$3"

    if gh label list --repo "$REPO" 2>/dev/null | grep -q "^$label"; then
        echo "[Label] $label already exists"
    else
        echo "[Label] Creating $label..."
        gh label create "$label" --repo "$REPO" --color "$color" --description "$description" 2>/dev/null || true
    fi
}

# Check if milestone exists, create if not
ensure_milestone() {
    local slug="$1"
    local title="$2"
    local description="$3"

    if gh milestone list --repo "$REPO" 2>/dev/null | grep -q "^$slug"; then
        echo "[Milestone] $slug already exists"
    else
        echo "[Milestone] Creating $slug..."
        gh milestone create "$slug" --repo "$REPO" --title "$title" --description "$description" 2>/dev/null || true
    fi
}

# Check if issue exists (by exact title), return issue number if found
find_existing_issue() {
    local title="$1"
    gh issue list --repo "$REPO" --state all --limit 1000 2>/dev/null | grep -F "$title" | head -1 | awk '{print $1}' || echo ""
}

# Record issue in map file
record_issue() {
    local task="$1"
    local issue="$2"
    local title="$3"
    local milestone="$4"
    local size="$5"

    if grep -q "^$task\t" "$MAP_FILE" 2>/dev/null; then
        # Update existing line
        sed -i.bak "s/^$task\t.*/$task\t$issue\t$title\t$milestone\t$size/" "$MAP_FILE" 2>/dev/null || true
        rm -f "$MAP_FILE.bak"
    else
        # Append new line
        echo -e "$task\t$issue\t$title\t$milestone\t$size" >> "$MAP_FILE"
    fi
}

# Create issue (idempotent)
create_issue() {
    local task="$1"
    local title="$2"
    local milestone="$3"
    local size="$4"
    local labels="$5"  # Comma-separated
    local body="$6"    # Full body text

    # Check if issue exists
    existing=$(find_existing_issue "$title")
    if [[ -n "$existing" ]]; then
        echo "[$task] Issue already exists: #$existing"
        record_issue "$task" "$existing" "$title" "$milestone" "$size"
        return 0
    fi

    echo "[$task] Creating issue: $title"
    issue=$(gh issue create --repo "$REPO" \
        --title "$title" \
        --milestone "$milestone" \
        --label "$labels" \
        --body "$body" 2>/dev/null | tail -1 | awk -F'/' '{print $NF}' || echo "")

    if [[ -z "$issue" ]]; then
        echo "[$task] WARNING: Could not create issue"
        return 1
    fi

    echo "[$task] Created as #$issue"
    record_issue "$task" "$issue" "$title" "$milestone" "$size"
    sleep "$THROTTLE_SLEEP"
}

# Step 1: Verify labels and milestones
echo "=========================================="
echo "STEP 1: Verify labels and milestones"
echo "=========================================="

ensure_label "phase:5" "5319E7" "Phase-5 deliverable per .plans/phase-5-build-emit-plan.md"
ensure_label "gated:downstream-paideia-os" "B60205" "Closure of this issue is part of the paideia-os Phase-1 unblock criterion"
ensure_label "area:emit-activation" "0E8086" "Cross-cutting work touching the elaborator → encoder → emitter glue"
ensure_label "area:boot-intrinsics" "D77B0E" "x86_64 instructions added specifically to support paideia-os boot code"

# Verify milestones exist
declare -a MILESTONES=(
    "phase-5-elab-lowering|Phase 5 — Elaborator per-construct lowering|Per-construct IR lowering for let/fn/unsafe-payload"
    "phase-5-encoder-boot-isa|Phase 5 — Encoder boot-ISA coverage|x86_64 encoder coverage for PaideiaOS Phase-1 boot ISA"
    "phase-5-unsafe-walker|Phase 5 — Unsafe-block payload walker|Walker that consumes unsafe { block: } AST → IR → bytes"
    "phase-5-static-data|Phase 5 — Static data surface|.data / .rodata emission for let : T = literal items"
    "phase-5-symbols-relocs|Phase 5 — Symbol export + relocations|Symbol export + cross-file relocations through the linker"
    "phase-5-end-to-end-smoke|Phase 5 — End-to-end smoke|A .pdx source assembles, links, QEMU-boots, writes x"
    "phase-5-docs-closure|Phase 5 — Documentation closure|Retrospective, STATUS.md, v0.5.0 tag, examples updates"
)

for ms in "${MILESTONES[@]}"; do
    IFS='|' read -r slug title desc <<< "$ms"
    ensure_milestone "$slug" "$title" "$desc"
done

# Step 2: Create 38 issues
echo ""
echo "=========================================="
echo "STEP 2: Create Phase 5 issues (38 total)"
echo "=========================================="

ensure_map

# M1 — Elaborator per-construct lowering (5 issues)
create_issue "m1-001" \
    "elaborator: EmitWalker skeleton + EmitPassState side-table writer" \
    "phase-5-elab-lowering" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature" \
    "## Summary
Introduce a new walker, \`EmitWalker\`, whose job is to populate \`InstructionSideTable\` for the three Phase-5 lowering shapes. Owns the entry into the emit-side of the pipeline; per-construct logic lands in m1-002..004.

## Acceptance criteria
- [ ] \`crates/paideia-as-elaborator/src/emit_walker.rs\` defines \`pub struct EmitWalker\`
- [ ] \`EmitPassState\` exposes \`instructions\`, \`current_function\`, \`current_offset\`
- [ ] Exported from \`lib.rs\` alongside other walkers
- [ ] Unit test: walking empty \`IrArena\` produces zero diagnostics
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
\`crates/paideia-as-elaborator/src/emit_walker.rs\`, \`crates/paideia-as-elaborator/src/lib.rs\`

## Dependencies
none

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §3 m1-001."

create_issue "m1-002" \
    "elaborator: EmitWalker lowers IrKind::Let(Literal) for let : u64 = imm" \
    "phase-5-elab-lowering" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature" \
    "## Summary
When the walker enters an \`IrKind::Let\` whose body is an \`IrKind::Literal\` of integer type, emit the canonical \`mov reg64, imm\` \`Instruction\` into \`InstructionSideTable\`.

## Acceptance criteria
- [ ] Walker recognises \`Let → Literal\` shape
- [ ] Emits \`Mov\` instruction with \`Reg(RAX)\` and appropriate immediate
- [ ] Unit test: \`let answer : u64 = 42\` → \`48 c7 c0 2a 00 00 00\`
- [ ] Unit test: \`let magic : u64 = 0xCAFE_F00D_DEAD_BEEF\` → \`48 b8 ef be ad de 0d f0 fe ca\`
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
\`crates/paideia-as-elaborator/src/emit_walker.rs\`

## Dependencies
m1-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §3 m1-002."

create_issue "m1-003" \
    "elaborator: EmitWalker lowers IrKind::Lambda body for fn (x) -> x + N" \
    "phase-5-elab-lowering" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature" \
    "## Summary
When the walker enters an \`IrKind::Lambda\` matching the \`Var + Literal\` shape (the \`add_one\` exemplar), emit \`lea rax, [rdi + N] ; ret\` into \`InstructionSideTable\`.

## Acceptance criteria
- [ ] Recognises \`Lambda → App(+, Var(arg0), Literal(n))\` shape
- [ ] Emits \`Lea\` and \`Ret\` instructions
- [ ] Unit test: identity → \`48 89 f8 c3\`
- [ ] Unit test: double → \`48 8d 04 3f c3\`
- [ ] Unit test: add_one → \`48 8d 47 01 c3\`
- [ ] Records \`lambda_node_id → first_instruction_offset\` in \`function_offsets\`
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
\`crates/paideia-as-elaborator/src/emit_walker.rs\`

## Dependencies
m1-002

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §3 m1-003."

create_issue "m1-004" \
    "elaborator: EmitWalker handles IrKind::Unsafe — delegate to UnsafeWalker (m3)" \
    "phase-5-elab-lowering" \
    "XS" \
    "phase:5,area:elaborator,area:emit-activation,type:feature" \
    "## Summary
When the walker enters an \`IrKind::Unsafe\`, record the node ID into \`pending_unsafe_blocks\` for m3's \`UnsafeWalker\` to resolve later.

## Acceptance criteria
- [ ] On \`enter_node\` for \`IrKind::Unsafe\`, appends to \`pending_unsafe_blocks\`
- [ ] \`EmitPassState::take_pending_unsafe()\` drains and returns vector
- [ ] Unit test: two unsafe nodes recorded in order
- [ ] Walker does not inspect block contents
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-elaborator/src/emit_walker.rs\`

## Dependencies
m1-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §3 m1-004."

create_issue "m1-005" \
    "elaborator: chain EmitWalker into cmd_build and propagate diagnostics" \
    "phase-5-elab-lowering" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Activate \`EmitWalker\` in the \`paideia-as build\` pipeline alongside the existing walkers. Diagnostics route into \`walker_sink\`.

## Acceptance criteria
- [ ] \`cmd_build.rs\` allocates \`EmitWalker\` and walks it via existing \`WalkerCtx\`
- [ ] Populated \`InstructionSideTable\` survives into emit step
- [ ] Empty \`.pdx\` produces zero diagnostics
- [ ] \`examples/01_hello.pdx\` gets 4 side-table entries
- [ ] \`examples/02_functions.pdx\` gets ~8 entries
- [ ] \`cmd_check.rs\` NOT modified
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
\`crates/paideia-as/src/cmd_build.rs\`, \`crates/paideia-as-elaborator/src/lib.rs\`

## Dependencies
m1-002, m1-003, m1-004

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §3 m1-005."

# M2 — Encoder boot-ISA coverage (10 issues)
create_issue "m2-001" \
    "ir + encoder: extend Mnemonic with privileged-ISA variants + bridge stub" \
    "phase-5-encoder-boot-isa" \
    "S" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature,gated:downstream-paideia-os" \
    "## Summary
Extend \`paideia_as_ir::Mnemonic\` with 20 privileged + system-ISA mnemonics needed by PaideiaOS Phase-1. Add \`Err(EncodeError::Unsupported)\` stubs in encoder bridge.

## Acceptance criteria
- [ ] \`Mnemonic\` enum: \`Lgdt, Lidt, MovCr, MovDr, Wrmsr, Rdmsr, In, Out, Iret, Iretq, Sysret, Swapgs, Cpuid, Cli, Sti, Hlt, Int, Nop, RepStosq, FarJmp\`
- [ ] Size remains ≤ 4 bytes
- [ ] \`encode_instruction.rs\` dispatches each to unsupported stub
- [ ] Round-trip test for \`Nop\`
- [ ] Existing 10 encoders unchanged; tests pass
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-ir/src/instruction.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
none

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-001."

# M2-002 through M2-010 (9 more issues)
create_issue "m2-002" \
    "encoder: zero-operand control + sync instructions (cli, sti, hlt, nop, swapgs, cpuid)" \
    "phase-5-encoder-boot-isa" \
    "XS" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature" \
    "## Summary
Encode six zero-operand instructions: \`cli\`, \`sti\`, \`hlt\`, \`nop\`, \`swapgs\`, \`cpuid\`.

## Acceptance criteria
- [ ] \`cli\` → \`FA\` (1 byte)
- [ ] \`sti\` → \`FB\`, \`hlt\` → \`F4\`, \`nop\` → \`90\`
- [ ] \`swapgs\` → \`0F 01 F8\`, \`cpuid\` → \`0F A2\`
- [ ] Six round-trip tests via iced-x86
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#10

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-002."

create_issue "m2-003" \
    "encoder: I/O port instructions (in al/ax dx, out dx al/ax)" \
    "phase-5-encoder-boot-isa" \
    "S" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature" \
    "## Summary
Encode four \`in\`/\`out\` forms for UART 16550 init.

## Acceptance criteria
- [ ] \`in al,dx\` → \`EC\`, \`in ax,dx\` → \`66 ED\`, \`in eax,dx\` → \`ED\`
- [ ] \`out dx,al\` → \`EE\`, \`out dx,ax\` → \`66 EF\`, \`out dx,eax\` → \`EF\`
- [ ] Width parameter on \`Mnemonic\` selects encoding
- [ ] Six round-trip tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#6

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-003."

create_issue "m2-004" \
    "encoder: MSR access (wrmsr, rdmsr) + Mnemonic::Int N" \
    "phase-5-encoder-boot-isa" \
    "XS" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature" \
    "## Summary
Encode MSR-access forms (\`wrmsr\`, \`rdmsr\`) and software-interrupt (\`int N\`).

## Acceptance criteria
- [ ] \`wrmsr\` → \`0F 30\`, \`rdmsr\` → \`0F 32\`
- [ ] \`int N\` → \`CD <imm8>\`
- [ ] Three round-trip tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#3

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-004."

create_issue "m2-005" \
    "encoder: control-register MOV (mov cr*, reg, mov reg, cr*)" \
    "phase-5-encoder-boot-isa" \
    "S" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature,gated:downstream-paideia-os" \
    "## Summary
Encode CR-register access forms for long-mode entry: \`mov cr0/2/3/4/8, reg\` and reverse.

## Acceptance criteria
- [ ] \`mov cr0,rax\` → \`0F 22 C0\`, etc.
- [ ] CR0..CR4 + CR8 supported
- [ ] Operand validation: 64-bit GPR only
- [ ] 12 round-trip tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#3

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-005."

create_issue "m2-006" \
    "encoder: debug-register MOV (mov dr*, reg, mov reg, dr*)" \
    "phase-5-encoder-boot-isa" \
    "XS" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature" \
    "## Summary
Encode DR-register access forms for future debug subsystems.

## Acceptance criteria
- [ ] \`mov dr0,rax\` → \`0F 23 C0\`, \`mov dr7,rax\` → \`0F 23 F8\`
- [ ] DR0..DR7 supported
- [ ] 16 round-trip tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
XS

## Unblocks paideia-os
none

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-006."

create_issue "m2-007" \
    "encoder: descriptor-table load (lgdt [mem], lidt [mem])" \
    "phase-5-encoder-boot-isa" \
    "XS" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature,gated:downstream-paideia-os" \
    "## Summary
Encode descriptor-table load forms: \`lgdt [mem]\` and \`lidt [mem]\`.

## Acceptance criteria
- [ ] \`lgdt [rdi]\` → \`0F 01 17\`, \`lgdt [rdi+8]\` → \`0F 01 57 08\`
- [ ] \`lidt [rdi]\` → \`0F 01 1F\`, \`lidt [rdi+16]\` → \`0F 01 5F 10\`
- [ ] Operand: \`MemSib { base, index: None, scale, disp }\`
- [ ] Six unit tests with disp=0/8/-128
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-007."

create_issue "m2-008" \
    "encoder: interrupt-return + system-return (iret, iretq, sysret)" \
    "phase-5-encoder-boot-isa" \
    "XS" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature,gated:downstream-paideia-os" \
    "## Summary
Encode return-from-privileged-context forms.

## Acceptance criteria
- [ ] \`iret\` → \`CF\`, \`iretq\` → \`48 CF\`, \`sysret\` → \`48 0F 07\`
- [ ] Three round-trip tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#6

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-008."

create_issue "m2-009" \
    "encoder: rep stosq for .bss zeroing (P1-005)" \
    "phase-5-encoder-boot-isa" \
    "XS" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature" \
    "## Summary
Encode \`rep stosq\` for .bss zeroing.

## Acceptance criteria
- [ ] \`rep stosq\` → \`F3 48 AB\` (3 bytes)
- [ ] One round-trip test
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#5

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-009."

create_issue "m2-010" \
    "encoder: far-jmp m16:64 for the 32→64 mode transition (P1-003)" \
    "phase-5-encoder-boot-isa" \
    "S" \
    "phase:5,area:encoder,area:boot-intrinsics,type:feature,gated:downstream-paideia-os" \
    "## Summary
Encode far-jmp form for long-mode entry sequence.

## Acceptance criteria
- [ ] \`jmp far [rdi]\` → \`48 FF 2F\`, \`jmp far [rip+offset32]\` → \`48 FF 2D <disp32>\`
- [ ] Dispatch via \`Mnemonic::FarJmp\`
- [ ] Three round-trip tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-encoder/src/encode.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`

## Dependencies
m2-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#3

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §4 m2-010."

# M3 — Unsafe-block payload walker (5 issues)
create_issue "m3-001" \
    "ir + ast: persist StmtInstruction mnemonic + operand AST shape through lowering" \
    "phase-5-unsafe-walker" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Introduce \`IrKind::RawInstruction\` to preserve back-pointer to originating AST node for unsafe blocks.

## Acceptance criteria
- [ ] \`IrKind::RawInstruction\` added
- [ ] \`lower.rs\`: \`StmtInstruction → IrKind::RawInstruction\` (not \`Action\`)
- [ ] Round-trip via \`ast_to_ir\` map works
- [ ] Unit test: \`mov rax,1\` produces \`RawInstruction\`
- [ ] Existing tests updated
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-ir/src/node.rs\`, \`crates/paideia-as-elaborator/src/lower.rs\`

## Dependencies
none

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §5 m3-001."

create_issue "m3-002" \
    "elaborator: operand parser for the unsafe-block surface" \
    "phase-5-unsafe-walker" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Build \`parse_operand_from_ast\` that parses AST operand subtrees into \`Operand\`.

## Acceptance criteria
- [ ] Handles registers: \`rax\`, \`rdi\`, \`r15\`
- [ ] Handles memory: \`[rdi]\`, \`[rdi+8]\`, \`[rdi+rsi*4]\`
- [ ] Handles immediates: \`0x12345678\`
- [ ] 12 unit tests covering operand shapes + 4 error paths
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-elaborator/src/unsafe_walker.rs\`, \`crates/paideia-as-elaborator/src/lib.rs\`

## Dependencies
m3-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §5 m3-002."

create_issue "m3-003" \
    "elaborator: mnemonic-name → Mnemonic enum resolver" \
    "phase-5-unsafe-walker" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Build \`resolve_mnemonic\` that maps source strings to \`Mnemonic\` enum.

## Acceptance criteria
- [ ] Case-insensitive: \`mov\`, \`MOV\` both work
- [ ] All 8 Jcc forms map correctly
- [ ] All m2-001 mnemonics supported
- [ ] Unknown mnemonic emits U1605 diagnostic
- [ ] 30+ unit tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-elaborator/src/unsafe_walker.rs\`, \`crates/paideia-as-diagnostics/catalog.toml\`

## Dependencies
m2-001, m3-002

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §5 m3-003."

create_issue "m3-004" \
    "elaborator: UnsafeWalker consumes pending blocks, emits Instruction entries" \
    "phase-5-unsafe-walker" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Implement \`UnsafeWalker::run\` that walks pending unsafe blocks and emits instructions.

## Acceptance criteria
- [ ] Iterates pending IDs; finds children via reverse \`ast_to_ir\` lookup
- [ ] Emits one \`Instruction\` per \`StmtInstruction\`
- [ ] Unknown mnemonic: emits U1605, skips instruction
- [ ] Operand error: emits U1606, skips instruction
- [ ] Three integration tests under \`tests/unsafe_walker/\`
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-elaborator/src/unsafe_walker.rs\`, \`crates/paideia-as-elaborator/tests/unsafe_walker/\`, \`crates/paideia-as-diagnostics/catalog.toml\`

## Dependencies
m3-001, m3-002, m3-003

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §5 m3-004."

create_issue "m3-005" \
    "cli: cmd_build calls UnsafeWalker::run after EmitWalker" \
    "phase-5-unsafe-walker" \
    "XS" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Wire \`UnsafeWalker::run\` into build pipeline after \`EmitWalker\`.

## Acceptance criteria
- [ ] \`cmd_build.rs\` calls \`EmitPassState::take_pending_unsafe()\` then \`UnsafeWalker::run\`
- [ ] End-to-end test: 3-instruction unsafe block produces \`InstructionSideTable\` with 3 entries
- [ ] Disassembled bytes match expected sequence
- [ ] \`cmd_check.rs\` NOT modified
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as/src/cmd_build.rs\`, \`crates/paideia-as/tests/build_unsafe.rs\`

## Dependencies
m1-005, m3-004

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §5 m3-005."

# M4 — Initialised static data surface (4 issues)
create_issue "m4-001" \
    "parser: [T; N] fixed-array type parses without P0100" \
    "phase-5-static-data" \
    "S" \
    "phase:5,area:parser,type:feature,gated:downstream-paideia-os" \
    "## Summary
Extend \`parse_type.rs\` to accept \`[T; N]\` syntax for fixed-size arrays.

## Acceptance criteria
- [ ] \`let bytes : [u8; 16] = [...]\` parses
- [ ] \`let table : [u64; 5] = [...]\` parses
- [ ] Nested arrays \`[[u8; 4]; 4]\` parse
- [ ] Length expr parsed as primary expression
- [ ] 6 unit tests: valid + invalid forms
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-parser/src/parse_type.rs\`, \`crates/paideia-as-ast/src/types.rs\`

## Dependencies
none

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §6 m4-001."

create_issue "m4-002" \
    "parser: array literal [expr, expr, ...] initialisers" \
    "phase-5-static-data" \
    "S" \
    "phase:5,area:parser,type:feature,gated:downstream-paideia-os" \
    "## Summary
Extend parser to accept array-literal initialisers in expression position.

## Acceptance criteria
- [ ] \`let xs : [u64; 3] = [1, 2, 3]\` parses
- [ ] \`let bytes : [u8; 5] = [0xCF, 0x9A, ...]\` parses
- [ ] Empty array requires type annotation; emits P0210 without one
- [ ] Trailing comma accepted
- [ ] \`ExprData::ArrayLit(Vec<NodeId>)\` created
- [ ] 6 unit tests
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-parser/src/parse_primary.rs\`, \`crates/paideia-as-ast/src/exprs.rs\`, \`crates/paideia-as-diagnostics/catalog.toml\`

## Dependencies
m4-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §6 m4-002."

create_issue "m4-003" \
    "emitter-elf: .rodata + .data section population from elaborator" \
    "phase-5-static-data" \
    "S" \
    "phase:5,area:emitter-elf,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Introduce \`DataSideTable\` holding byte sequences for literals. Extend \`EmitWalker\` to populate for module-level \`let\` items.

## Acceptance criteria
- [ ] \`DataSideTable\` in \`crates/paideia-as-ir/src/data.rs\`
- [ ] \`IrArena::data() / data_mut()\` accessors
- [ ] \`EmitWalker\` recognises module-level \`Let\` with \`Literal\`/\`ArrayLit\` body
- [ ] Bytes little-endian-packed per type
- [ ] ELF writer's \`add_rodata_bytes\` / \`add_data_bytes\` exist
- [ ] 6 unit tests + 1 integration test (16-byte GDT descriptor)
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-ir/src/data.rs\`, \`crates/paideia-as-ir/src/arena.rs\`, \`crates/paideia-as-ir/src/lib.rs\`, \`crates/paideia-as-elaborator/src/emit_walker.rs\`, \`crates/paideia-as-emitter-elf/src/sections.rs\`, \`crates/paideia-as-emitter-elf/src/writer.rs\`

## Dependencies
m1-002, m4-002

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §6 m4-003."

create_issue "m4-004" \
    "emitter-elf: relocation linking .text references to .rodata data symbols" \
    "phase-5-static-data" \
    "S" \
    "phase:5,area:emitter-elf,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
When \`Operand::SymbolRef\` references a data symbol, create \`R_X86_64_PC32\` relocation in \`.rela.text\`.

## Acceptance criteria
- [ ] Encoder writes placeholder displacement; returns reloc site
- [ ] Emitter inserts relocation at displacement byte offset
- [ ] Relocation's symbol points to \`STT_OBJECT\` data symbol
- [ ] \`readelf -r <object>\` shows relocation
- [ ] \`ld\` resolves displacement to final data address
- [ ] 2 integration tests (same-file + cross-function references)
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-emitter-elf/src/relocs.rs\`, \`crates/paideia-as-emitter-elf/src/lower.rs\`

## Dependencies
m4-003, m5-002

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §6 m4-004."

# M5 — Symbol export + cross-file relocations (5 issues)
create_issue "m5-001" \
    "ir: top-level binding symbol table" \
    "phase-5-symbols-relocs" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Introduce \`SymbolTable\` side-table holding \`Symbol { name, kind, ir_node, global }\`. Populate for module-level \`let\` items.

## Acceptance criteria
- [ ] \`SymbolTable\` in \`crates/paideia-as-ir/src/symbol.rs\`
- [ ] \`insert\`, \`lookup_by_name\`, \`iter\` methods
- [ ] \`EmitWalker\` populates on module-level \`Let\`
- [ ] \`_start\` name auto-flagged \`global: true\`, marked entry-point
- [ ] 3 unit tests: Object, Function, entry-point
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-ir/src/symbol.rs\`, \`crates/paideia-as-ir/src/arena.rs\`, \`crates/paideia-as-ir/src/lib.rs\`, \`crates/paideia-as-elaborator/src/emit_walker.rs\`

## Dependencies
m1-005

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §7 m5-001."

create_issue "m5-002" \
    "ir: Operand::SymbolRef(String) for unresolved symbol references in instructions" \
    "phase-5-symbols-relocs" \
    "S" \
    "phase:5,area:encoder,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Add \`Operand::SymbolRef { name, addend }\` for unresolved symbol references. Encoder produces reloc sites.

## Acceptance criteria
- [ ] \`Operand::SymbolRef { name, addend }\` added
- [ ] Parser produces it for identifiers not resolving to register/immediate
- [ ] Encoder recognises \`[SymbolRef(...)]\` shapes
- [ ] Encoder writes placeholder 4-byte displacement
- [ ] \`encode_instruction\` returns \`EncodeOutput { reloc_sites }\`
- [ ] 4 unit tests: lea, lgdt, mov, call
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-ir/src/instruction.rs\`, \`crates/paideia-as-encoder/src/encode_instruction.rs\`, \`crates/paideia-as-encoder/src/encode.rs\`

## Dependencies
m2-001

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §7 m5-002."

create_issue "m5-003" \
    "emitter-elf: real symbol-table emission from SymbolTable" \
    "phase-5-symbols-relocs" \
    "S" \
    "phase:5,area:emitter-elf,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Replace hard-coded \`add_one\` symbol with \`SymbolTable\`-driven loop. Emit \`STT_FUNC\` / \`STT_OBJECT\` as appropriate.

## Acceptance criteria
- [ ] \`build_elf_object\` iterates \`SymbolTable::iter()\`
- [ ] Function symbol value: byte offset of first instruction
- [ ] Data symbol value: byte offset in .rodata/.data
- [ ] \`readelf -s <object>\` shows symbols with correct types/sizes
- [ ] \`_start\` symbol with \`STB_GLOBAL\`
- [ ] 3 integration tests: single, multiple, mixed symbols
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as/src/cmd_build.rs\`, \`crates/paideia-as-emitter-elf/src/symtab.rs\`, \`crates/paideia-as-emitter-elf/src/writer.rs\`

## Dependencies
m5-001, m1-005, m4-003

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §7 m5-003."

create_issue "m5-004" \
    "emitter-elf: undefined-symbol entries for cross-file references" \
    "phase-5-symbols-relocs" \
    "S" \
    "phase:5,area:emitter-elf,area:emit-activation,type:feature" \
    "## Summary
When \`SymbolRef\` names unknown symbol, create undefined-symbol entry (\`SHN_UNDEF\`) for linker resolution.

## Acceptance criteria
- [ ] Emitter calls \`add_undefined_symbol(name)\` when symbol not in local table
- [ ] Relocation uses undefined-symbol index
- [ ] \`readelf -s <object>\` shows undefined symbol with \`NOTYPE\`, \`SHN_UNDEF\`
- [ ] Cross-file linking works: \`ld a.o b.o -o linked\`
- [ ] \`objdump -d linked\` shows displacement resolved
- [ ] Test fixtures under \`tests/cross_file/\`
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as-emitter-elf/src/symtab.rs\`, \`crates/paideia-as-emitter-elf/src/relocs.rs\`

## Dependencies
m5-002, m5-003

## Estimated size
S

## Unblocks paideia-os
none

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §7 m5-004."

create_issue "m5-005" \
    "cli: cmd_build writes the real InstructionSideTable body into .text" \
    "phase-5-symbols-relocs" \
    "S" \
    "phase:5,area:elaborator,area:emit-activation,type:feature,gated:downstream-paideia-os" \
    "## Summary
Replace \`lower_add_one\` placeholder with loop that iterates \`InstructionSideTable\`, encodes each instruction, writes to .text buffer.

## Acceptance criteria
- [ ] \`build_elf_object\` iterates \`InstructionSideTable::iter()\` in IR-node order
- [ ] Calls \`encode_instruction\` per entry; accumulates bytes
- [ ] Records byte offsets for each instruction
- [ ] Per-function ranges from \`function_offsets\` + next function
- [ ] Accumulates reloc sites for m4-004 / m5-004 consumption
- [ ] \`lower_add_one\` DELETED (or repurposed as test fixture only)
- [ ] Empty .pdx emits valid .o with empty .text
- [ ] \`examples/02_functions.pdx\` produces ~16-byte .text
- [ ] \`objdump -d\` matches snapshot
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as/src/cmd_build.rs\`, \`crates/paideia-as-emitter-elf/src/lower.rs\`

## Dependencies
m1-005, m2-001, m3-005, m4-003, m5-003, m5-004

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §7 m5-005."

# M6 — End-to-end smoke (5 issues)
create_issue "m6-001" \
    "fixtures: tests/build-emit/uart_smoke.pdx source" \
    "phase-5-end-to-end-smoke" \
    "XS" \
    "phase:5,area:elaborator,type:feature,gated:downstream-paideia-os" \
    "## Summary
Author minimal \`.pdx\` declaring \`_start\` and using unsafe block to write byte to COM1 (port 0x3F8) and halt.

## Acceptance criteria
- [ ] \`tests/build-emit/uart_smoke.pdx\` exists
- [ ] \`paideia-as check uart_smoke.pdx\` exits 0
- [ ] Under 30 lines with comments
- [ ] \`uart_smoke.expected_bytes.txt\` records expected .text bytes
- [ ] cargo test --workspace green

## Files
\`tests/build-emit/uart_smoke.pdx\`, \`tests/build-emit/uart_smoke.expected_bytes.txt\`

## Dependencies
m3-005, m5-003

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §8 m6-001."

create_issue "m6-002" \
    "fixtures: tests/build-emit/link.ld and tools/run-smoke.sh driver" \
    "phase-5-end-to-end-smoke" \
    "XS" \
    "phase:5,area:elaborator,type:feature,gated:downstream-paideia-os" \
    "## Summary
Minimal linker script (1 MiB, ENTRY(_start)) and shell script to build, link, run QEMU, assert serial output.

## Acceptance criteria
- [ ] \`link.ld\`: OUTPUT_FORMAT, ENTRY(_start), .text at 0x100000
- [ ] \`tools/run-smoke.sh\`: takes .pdx, builds, links, runs QEMU ≤5s, greps output
- [ ] Exit 0 on success, 1 on failure, 77 on no QEMU
- [ ] cargo test --workspace green

## Files
\`tests/build-emit/link.ld\`, \`tools/run-smoke.sh\`

## Dependencies
m6-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §8 m6-002."

create_issue "m6-003" \
    "tests: byte-sequence assertion for uart_smoke.pdx" \
    "phase-5-end-to-end-smoke" \
    "S" \
    "phase:5,area:elaborator,type:test,gated:downstream-paideia-os" \
    "## Summary
Rust integration test: build \`uart_smoke.pdx\`, extract .text bytes, assert byte-for-byte match against snapshot.

## Acceptance criteria
- [ ] \`crates/paideia-as/tests/build_emit_smoke.rs\` exists
- [ ] Invokes \`cmd_build\` on \`uart_smoke.pdx\`, asserts exit 0
- [ ] Extracts .text via \`object\` crate, asserts match against snapshot
- [ ] Asserts \`_start\` symbol present with correct \`st_size\`
- [ ] Deterministic per \`det.rs::build_timestamp()\`
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as/tests/build_emit_smoke.rs\`

## Dependencies
m6-001, m5-005

## Estimated size
S

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §8 m6-003."

create_issue "m6-004" \
    "tests: QEMU smoke under cargo test --test qemu_smoke (gated)" \
    "phase-5-end-to-end-smoke" \
    "XS" \
    "phase:5,area:elaborator,type:test,gated:downstream-paideia-os" \
    "## Summary
Rust integration test shells out to \`tools/run-smoke.sh\`, asserts exit 0. Auto-skipped if QEMU not on PATH.

## Acceptance criteria
- [ ] \`crates/paideia-as/tests/qemu_smoke.rs\` exists
- [ ] With QEMU: test passes within 30s
- [ ] Without QEMU: test auto-skipped with message
- [ ] Nix flake's devShell includes qemu
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as/tests/qemu_smoke.rs\`, \`flake.nix\` (if needed)

## Dependencies
m6-002, m6-003

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §8 m6-004."

create_issue "m6-005" \
    "cli + tests: add_one regression — 02_functions.pdx::add_one byte-identical" \
    "phase-5-end-to-end-smoke" \
    "XS" \
    "phase:5,area:elaborator,type:test,gated:downstream-paideia-os" \
    "## Summary
Confirm \`fn x -> x+1\` lowers to \`48 8d 47 01 c3\`. m5-005 deletes \`lower_add_one\`; verify walker chain reproduces same bytes.

## Acceptance criteria
- [ ] \`cargo test --test build_emit_smoke -- add_one_byte_identical\` passes
- [ ] Invokes \`cmd_build\` on \`examples/02_functions.pdx\`
- [ ] Finds \`add_one\` symbol, extracts 5 bytes
- [ ] Asserts \`[0x48, 0x8d, 0x47, 0x01, 0xc3]\`
- [ ] Three other functions produce expected bytes
- [ ] **PaideiaOS Phase-1 unblock declared** in commit message
- [ ] \`STATUS.md\` references unblock
- [ ] cargo test --workspace green

## Files
\`crates/paideia-as/tests/build_emit_smoke.rs\`, \`STATUS.md\`

## Dependencies
m6-003, m5-005

## Estimated size
XS

## Unblocks paideia-os
paideia-os/paideia-os#1

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §8 m6-005."

# M7 — Documentation + closure (4 issues)
create_issue "m7-001" \
    "docs: design/toolchain/phase-transition-5.md retrospective" \
    "phase-5-docs-closure" \
    "XS" \
    "phase:5,area:docs,type:docs" \
    "## Summary
Author Phase 5 retrospective: scope, per-milestone outcomes, carryover, what didn't ship, what we got right, what we'd change, Phase-5→Phase-6 carryover.

## Acceptance criteria
- [ ] \`design/toolchain/phase-transition-5.md\` exists, < 250 lines
- [ ] §0 scope, §1 carryover, §2 honest list, §3 right calls, §4 changes, §5 Phase-6 carryover
- [ ] Phase-6 carryover: original Phase-5 self-hosting (T1/T2/T3)
- [ ] Honest list: records, generics, traits, etc. deferred
- [ ] cargo test --workspace green

## Files
\`design/toolchain/phase-transition-5.md\`

## Dependencies
m6-005

## Estimated size
XS

## Unblocks paideia-os
none

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §9 m7-001."

create_issue "m7-002" \
    "docs: STATUS.md Phase 5 closure section" \
    "phase-5-docs-closure" \
    "XS" \
    "phase:5,area:docs,type:docs" \
    "## Summary
Prepend Phase 5 closure section to STATUS.md listing milestones, issues, test count delta.

## Acceptance criteria
- [ ] STATUS.md gains \"Phase 5 closure (m1-m7)\" section above Phase 4
- [ ] Each milestone: one-line summary + issue ID list
- [ ] Test totals table grows Phase-5-close row
- [ ] \"Where to look next\" adds \`phase-transition-5.md\`
- [ ] cargo test --workspace green

## Files
\`STATUS.md\`

## Dependencies
m7-001

## Estimated size
XS

## Unblocks paideia-os
none

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §9 m7-002."

create_issue "m7-003" \
    "release: v0.5.0 tag + CHANGELOG Phase 5 section" \
    "phase-5-docs-closure" \
    "XS" \
    "phase:5,area:docs,type:release" \
    "## Summary
Bump version 0.4.0→0.5.0, author CHANGELOG listing build-emit activation + new capabilities.

## Acceptance criteria
- [ ] \`Cargo.toml\` workspace \`version = \"0.5.0\"\`
- [ ] \`CHANGELOG.md\` §0.5.0 with date, new capabilities, deferred items
- [ ] \`git tag v0.5.0\` created + pushed
- [ ] \`cargo build --workspace\` clean post-bump
- [ ] \`.plans/issue-map.tsv\` maps m1-m7 issues to commits
- [ ] cargo test --workspace green

## Files
\`Cargo.toml\`, \`CHANGELOG.md\`, \`.plans/issue-map.tsv\`

## Dependencies
m7-002

## Estimated size
XS

## Unblocks paideia-os
none

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §9 m7-003."

create_issue "m7-004" \
    "examples: build-clean parity for the build-emit subset" \
    "phase-5-docs-closure" \
    "S" \
    "phase:5,area:docs,type:docs" \
    "## Summary
Confirm \`01_hello.pdx\`, \`02_functions.pdx\`, \`15_unsafe.pdx\` all build (not just check) to non-empty .text. Add per-example status table to README.

## Acceptance criteria
- [ ] Three examples build to valid .o with non-empty .text
- [ ] \`examples/README.md\` gains status table: \"check\" (all pass), \"build\" (3 pass, 17 deferred)
- [ ] 17 deferred examples have one-line build-block reason
- [ ] \`tests/examples-corpus.rs\` exercises \`paideia-as build\` on 3 build-clean examples
- [ ] cargo test --workspace green

## Files
\`examples/01_hello.pdx\`, \`examples/02_functions.pdx\`, \`examples/15_unsafe.pdx\`, \`examples/README.md\`, \`tests/examples-corpus.rs\`

## Dependencies
m6-005

## Estimated size
S

## Unblocks paideia-os
none

## Cross-repo escalation source
none

See .plans/phase-5-build-emit-plan.md §9 m7-004."

# Step 3: Verify issue counts
echo ""
echo "=========================================="
echo "STEP 3: Verify issue counts"
echo "=========================================="

phase5_count=$(gh issue list --repo "$REPO" --state open --label "phase:5" --limit 1000 2>/dev/null | wc -l || echo "0")
echo "Total Phase-5 issues (open): $phase5_count"

echo ""
echo "Per-milestone counts (open):"
for ms in phase-5-elab-lowering phase-5-encoder-boot-isa phase-5-unsafe-walker phase-5-static-data phase-5-symbols-relocs phase-5-end-to-end-smoke phase-5-docs-closure; do
    count=$(gh issue list --repo "$REPO" --state open --milestone "$ms" --limit 1000 2>/dev/null | wc -l || echo "0")
    echo "  $ms: $count"
done

echo ""
echo "=========================================="
echo "BOOTSTRAP COMPLETE"
echo "=========================================="
echo "Issue map recorded in: $MAP_FILE"
echo ""
echo "First 3 m1 issues:"
gh issue list --repo "$REPO" --state open --milestone "phase-5-elab-lowering" --limit 3 2>/dev/null | awk '{print "  #" $1 ": " $2}' || echo "  (unable to list; check gh CLI)"

echo ""
echo "Script is re-runnable and idempotent."
