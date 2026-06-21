#!/bin/bash

##############################################################################
# gh-bootstrap-phase6-issues.sh
#
# Idempotent GitHub bootstrap for paideia-as Phase 6.
#
# Creates:
#   - 3 Phase-6 labels (phase:6, area:walker-activation, area:bug-fix-from-paideia-os)
#   - 7 Phase-6 milestones (m1-m7)
#   - 37 Phase-6 issues (m1-006, m2-004, m3-008, m4-006, m5-005, m6-004, m7-004)
#
# Idempotent: every operation checks for existence first; skips if present.
# Throttled: sleeps 30s after every 30 operations to avoid rate-limiting.
# Logged: each created issue is written to .plans/phase-6-issue-map.tsv.
#
# Usage:
#   ./tools/gh-bootstrap-phase6-issues.sh
#
# The script exits 0 on success (all labels, milestones, issues created or skipped).
# It exits non-zero only on catastrophic failures (e.g., auth failure, network error).
#
##############################################################################

set -euo pipefail

REPO="paideia-os/paideia-as"
ISSUE_MAP=".plans/phase-6-issue-map.tsv"
OPERATIONS_COUNT=0
OPERATIONS_LIMIT=30
SLEEP_SECONDS=30

# Colors for output (optional, can be disabled if CI doesn't support)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper: log a message with timestamp
log_info() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] INFO: $*"
}

log_success() {
  echo -e "${GREEN}[$(date '+%Y-%m-%d %H:%M:%S')] SUCCESS: $*${NC}"
}

log_skip() {
  echo -e "${YELLOW}[$(date '+%Y-%m-%d %H:%M:%S')] SKIP: $*${NC}"
}

log_error() {
  echo -e "${RED}[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*${NC}"
}

# Helper: increment operation counter and sleep if threshold hit
tick_operations() {
  ((OPERATIONS_COUNT++))
  if (( OPERATIONS_COUNT % OPERATIONS_LIMIT == 0 )); then
    log_info "Hit operation limit ($OPERATIONS_LIMIT). Sleeping ${SLEEP_SECONDS}s to avoid rate-limiting..."
    sleep "$SLEEP_SECONDS"
  fi
}

# Helper: check if a label exists
label_exists() {
  local label="$1"
  # Use gh api to list labels and check if this label is present
  gh label list --repo "$REPO" --limit 100 --json name -q '.[].name' | grep -Fxq "$label"
}

# Helper: check if a milestone exists
milestone_exists() {
  local title="$1"
  gh api repos/"$REPO"/milestones --jq ".[] | select(.title == \"$title\")" --limit 100 2>/dev/null | grep -q "."
}

# Helper: check if an issue with exact title exists
issue_exists_by_title() {
  local title="$1"
  # Search for issues with this exact title
  gh issue list --repo "$REPO" --search "in:title \"$title\"" --limit 10 --json title,number 2>/dev/null | grep -q "\"title\": \"$title\""
}

# Helper: get milestone number by title
get_milestone_number() {
  local title="$1"
  gh api repos/"$REPO"/milestones --jq ".[] | select(.title == \"$title\") | .number" --limit 100 2>/dev/null
}

# Helper: create an issue and log it
create_issue() {
  local title="$1"
  local body="$2"
  local labels="$3"
  local milestone="$4"
  local task_id="$5"
  local size="$6"

  # Check if issue already exists
  if issue_exists_by_title "$title"; then
    log_skip "Issue already exists: '$title'"
    return 0
  fi

  # Build gh issue create command
  local cmd="gh issue create --repo $REPO --title '$title' --body '$body'"

  # Add labels if provided
  if [[ -n "$labels" ]]; then
    cmd="$cmd --label '$labels'"
  fi

  # Add milestone if provided
  if [[ -n "$milestone" ]]; then
    local milestone_num
    milestone_num=$(get_milestone_number "$milestone")
    if [[ -n "$milestone_num" ]]; then
      cmd="$cmd --milestone '$milestone_num'"
    fi
  fi

  # Execute command and capture output
  local result
  result=$(eval "$cmd" 2>&1 || true)

  # Extract issue number from output (format: "https://github.com/paideia-os/paideia-as/issues/NNN")
  local issue_num
  issue_num=$(echo "$result" | grep -oP '(?<=/issues/)\d+' | head -1 || true)

  if [[ -n "$issue_num" ]]; then
    log_success "Created issue #$issue_num: $title"
    # Log to TSV file
    echo -e "${task_id}\t${issue_num}\t${title}\t${milestone}\t${size}" >> "$ISSUE_MAP"
    tick_operations
    return 0
  else
    # Maybe it was already created between the check and now
    if issue_exists_by_title "$title"; then
      log_skip "Issue was created concurrently: '$title'"
      return 0
    else
      log_error "Failed to create issue: '$title'"
      log_error "Command output: $result"
      return 1
    fi
  fi
}

##############################################################################
# MAIN
##############################################################################

log_info "Starting Phase 6 GitHub bootstrap for repo: $REPO"
log_info "Output will be logged to: $ISSUE_MAP"

# Initialize the issue map file (with header)
if [[ ! -f "$ISSUE_MAP" ]]; then
  echo -e "Task\tIssue\tTitle\tMilestone\tSize" > "$ISSUE_MAP"
  log_info "Created issue-map file: $ISSUE_MAP"
fi

# ============================================================================
# STEP 1: Create labels
# ============================================================================

log_info "Step 1: Creating Phase-6 labels..."

# Label 1: phase:6
if ! label_exists "phase:6"; then
  gh label create "phase:6" --color "5319E7" --description "Phase 6 deliverable per phase-6-plan.md. Closes when paideia-os Phase 2 unblock criterion met." --force --repo "$REPO"
  log_success "Created label: phase:6"
  tick_operations
else
  log_skip "Label already exists: phase:6"
fi

# Label 2: area:walker-activation
if ! label_exists "area:walker-activation"; then
  gh label create "area:walker-activation" --color "C2185B" --description "Activates the m1-005/006 walker chain for the full Phase-4 surface (records, struct field-access, generics, traits, enums)." --force --repo "$REPO"
  log_success "Created label: area:walker-activation"
  tick_operations
else
  log_skip "Label already exists: area:walker-activation"
fi

# Label 3: area:bug-fix-from-paideia-os
if ! label_exists "area:bug-fix-from-paideia-os"; then
  gh label create "area:bug-fix-from-paideia-os" --color "FBC02D" --description "Source is a paideia-os-surfaced bug (#734, #735, #736 area). Closure requires a regression test that fails-before / passes-after." --force --repo "$REPO"
  log_success "Created label: area:bug-fix-from-paideia-os"
  tick_operations
else
  log_skip "Label already exists: area:bug-fix-from-paideia-os"
fi

# ============================================================================
# STEP 2: Create milestones
# ============================================================================

log_info "Step 2: Creating Phase-6 milestones..."

declare -A MILESTONES=(
  ["phase-6-encoder-bridge-fixes"]="Phase 6 — m1 encoder bridge fixes (#734/#736)"
  ["phase-6-parser-cleanups"]="Phase 6 — m2 parser cleanups (#735)"
  ["phase-6-struct-walker"]="Phase 6 — m3 struct walker activation"
  ["phase-6-control-flow-encoders"]="Phase 6 — m4 cmp + jcc + call encoders for unsafe blocks"
  ["phase-6-bss-arrays"]="Phase 6 — m5 .bss array allocation"
  ["phase-6-end-to-end-smoke"]="Phase 6 — m6 end-to-end smoke (paideia-os Phase 2 unblock)"
  ["phase-6-docs-closure"]="Phase 6 — m7 Phase 6 retrospective + v0.6.0 tag"
)

for slug in "${!MILESTONES[@]}"; do
  title="${MILESTONES[$slug]}"
  if ! milestone_exists "$title"; then
    gh api repos/"$REPO"/milestones -X POST \
      -f title="$title" \
      -f description="See .plans/phase-6-plan.md for details."
    log_success "Created milestone: $title"
    tick_operations
  else
    log_skip "Milestone already exists: $title"
  fi
done

# ============================================================================
# STEP 3: Create issues (37 total across 7 milestones)
# ============================================================================

log_info "Step 3: Creating Phase-6 issues..."
log_info "Creating 37 issues across 7 milestones..."

# ---- M1: Encoder Bridge Fixes (6 issues) ----

create_issue \
  "encoder: introduce Mnemonic::dispatch_kind operand-shape classifier" \
  "## Summary
Introduce a small classifier function on the encoder side that dispatches Mnemonic::Mov based on operand register class (CR/DR/GP).

## Acceptance criteria
- dispatch.rs defines pub enum DispatchKind with variants: MovGeneric, MovToCr, MovFromCr, MovToDr, MovFromDr, Generic
- Correctly routes Mnemonic::Mov instructions based on operand register IDs
- 8 unit tests covering each variant

## Files
- crates/paideia-as-encoder/src/dispatch.rs (new)
- crates/paideia-as-encoder/src/lib.rs

## Test plan
Unit tests in dispatch.rs; 8 tests covering each DispatchKind variant." \
  "phase:6,area:encoder,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m1 encoder bridge fixes (#734/#736)" \
  "m1-001" \
  "S"

create_issue \
  "encoder: route mov cr*/gpr through classifier (#734 part A)" \
  "## Summary
Route Mnemonic::Mov with CR operands through the dispatch classifier to the CR encoder.

## Acceptance criteria
- encode_instruction dispatches MovToCr/MovFromCr variants via classifier
- Instruction with Reg(0x103), Reg(7) encodes to 0F 22 DF (mov cr3, rdi)
- 6 integration tests via iced-x86 round-trip

## Files
- crates/paideia-as-encoder/src/encode_instruction.rs
- tests/build-emit/long_mode_cr_moves.pdx
- crates/paideia-as/tests/build_emit_phase6_cr_moves.rs

## Test plan
End-to-end fixtures + integration tests with iced-x86 verification." \
  "phase:6,area:encoder,type:bug,area:bug-fix-from-paideia-os,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m1 encoder bridge fixes (#734/#736)" \
  "m1-002" \
  "S"

create_issue \
  "encoder: route mov dr*/gpr through classifier (#734 part B)" \
  "## Summary
Symmetric counterpart to m1-002: route Mnemonic::Mov with DR operands through the classifier.

## Acceptance criteria
- Dispatches MovToDr/MovFromDr via classifier to encode_mov_dr
- Instruction with Reg(0x200), Reg(0) encodes to 0F 23 C0 (mov dr0, rax)
- 8 round-trip tests via iced-x86 (DR0..DR7)

## Files
- crates/paideia-as-encoder/src/encode_instruction.rs
- crates/paideia-as-encoder/tests/mov_dr_dispatch.rs

## Test plan
Unit tests for DR0..DR7 write and read samples via iced-x86." \
  "phase:6,area:encoder,type:bug,area:bug-fix-from-paideia-os,priority:high" \
  "Phase 6 — m1 encoder bridge fixes (#734/#736)" \
  "m1-003" \
  "XS"

create_issue \
  "cli: cmd_build exits non-zero on encoder failure (#734 part C)" \
  "## Summary
Make encoder failures fatal to the build (exit 2) instead of swallowing them.

## Acceptance criteria
- cmd_build returns BuildError::Encoder on EncodeError
- Exit code is 2 (build-substantive failure)
- New --encoder-warn flag restores Phase-5 warn-and-continue behavior
- Regression test: pre-fix mov cr3, rdi fails build; post-fix passes

## Files
- crates/paideia-as/src/cmd_build.rs
- crates/paideia-as/src/cli.rs
- crates/paideia-as/tests/build_emit_encoder_strict.rs

## Test plan
Regression test with fixture containing rejected-by-encoder instruction." \
  "phase:6,area:cli,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m1 encoder bridge fixes (#734/#736)" \
  "m1-004" \
  "S"

create_issue \
  "elaborator: UnsafeWalker skips operand-parser for zero-arity mnemonics (#736)" \
  "## Summary
Fix zero-arity mnemonic handling: cli, hlt, nop, etc. should not attempt operand parsing.

## Acceptance criteria
- Add Mnemonic::arity() returning 0 for zero-arity mnemonics
- process_stmt_instruction checks arity before operand parsing
- For cli; hlt: operands are empty, encoded as FA F4
- New diagnostic U1607 for operands on zero-arity mnemonics
- Regression: entry.pdx now succeeds with cli; hlt

## Files
- crates/paideia-as-elaborator/src/unsafe_walker.rs
- crates/paideia-as-ir/src/instruction.rs
- crates/paideia-as-diagnostics/catalog.toml
- crates/paideia-as-elaborator/tests/unsafe_walker/zero_arity.rs

## Test plan
Unit tests for zero-arity-without-operands (pass) and with-operands (U1607)." \
  "phase:6,area:elaborator,type:bug,area:bug-fix-from-paideia-os,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m1 encoder bridge fixes (#734/#736)" \
  "m1-005" \
  "S"

create_issue \
  "tests: PaideiaOS Phase-1 stub re-build regression suite" \
  "## Summary
Integration test that rebuilds PaideiaOS boot .pdx files to verify m1 fixes land cleanly.

## Acceptance criteria
- Test discovers PaideiaOS submodule at ../../PaideiaOS
- Builds entry.pdx, long_mode.pdx, gdt.pdx, uart.pdx, zero_bss.pdx, kernel_main.pdx, banner.pdx
- Asserts exit 0 for each
- pagetables.pdx excluded with FIXME comment (needs m5)
- Runs on CI when submodule is initialized

## Files
- crates/paideia-as/tests/paideia_os_phase1_rebuild.rs

## Test plan
Cross-repo canary: once m1 lands, boot files rebuild clean." \
  "phase:6,area:testing,type:test,priority:high" \
  "Phase 6 — m1 encoder bridge fixes (#734/#736)" \
  "m1-006" \
  "XS"

# ---- M2: Parser Cleanups (4 issues) ----

create_issue \
  "parser: fn () -> body empty-arg list accepted (#735)" \
  "## Summary
Extend parameter-list parser to accept fn () with no arguments.

## Acceptance criteria
- fn () -> 42 parses correctly (empty params vec)
- fn () -> unsafe { ... } parses without P0100
- Error P0100 still fires on malformed: fn (,), fn (x,,y)
- 6 unit tests (3 success, 3 reject cases)
- Pipe lambda || also handles zero params

## Files
- crates/paideia-as-parser/src/parse_lambda.rs
- crates/paideia-as-parser/tests/empty_fn_args.rs

## Test plan
Unit tests for empty vs malformed parameter lists." \
  "phase:6,area:parser,type:bug,area:bug-fix-from-paideia-os,priority:high" \
  "Phase 6 — m2 parser cleanups (#735)" \
  "m2-001" \
  "XS"

create_issue \
  "parser: trailing semicolon inside unsafe { block: { ... } } accepted" \
  "## Summary
Allow trailing semicolons in unsafe-block statement lists (currently fires P0101).

## Acceptance criteria
- unsafe { ..., block: { cli; hlt; } } parses (note trailing ;)
- unsafe { ..., block: { ;; } } still rejects with P0101
- 4 unit tests (accept/reject permutations)

## Files
- crates/paideia-as-parser/src/parse_unsafe.rs
- crates/paideia-as-parser/tests/unsafe_block_trailing_semi.rs

## Test plan
Unit tests for trailing semicolon patterns." \
  "phase:6,area:parser,type:feature,priority:medium" \
  "Phase 6 — m2 parser cleanups (#735)" \
  "m2-002" \
  "XS"

create_issue \
  "parser: memory-operand re-sync after comma-suffixed operand" \
  "## Summary
Fix lexer re-sync after comma-terminated operands so next instruction parses cleanly.

## Acceptance criteria
- mov rax, rdi; lea rax, [rdi + 1]; ret parses without P0102
- 4 fixtures in tests/parser-corpus/instruction_resync/

## Files
- crates/paideia-as-parser/src/parse_unsafe.rs
- tests/parser-corpus/instruction_resync/

## Test plan
Corpus tests for comma-operand boundary conditions." \
  "phase:6,area:parser,type:feature,priority:medium" \
  "Phase 6 — m2 parser cleanups (#735)" \
  "m2-003" \
  "XS"

create_issue \
  "elaborator: _-prefixed identifiers get correct SymbolKind" \
  "## Summary
Remove the short-circuit that forces _-prefixed names to STT_NOTYPE; use actual body type.

## Acceptance criteria
- let _start : () -> () = fn (...) produces STT_FUNC + STB_GLOBAL
- let _anchor : u64 = 42 produces STT_OBJECT + STB_GLOBAL
- Entry-point magic-name detection for _start still works
- readelf -s shows correct types
- 3 integration tests

## Files
- crates/paideia-as-elaborator/src/emit_walker.rs
- crates/paideia-as-emitter-elf/src/symtab.rs
- crates/paideia-as/tests/symtab_underscore_prefix.rs

## Test plan
Integration tests with readelf verification." \
  "phase:6,area:elaborator,type:feature,priority:medium" \
  "Phase 6 — m2 parser cleanups (#735)" \
  "m2-004" \
  "XS"

# ---- M3: Struct Walker Activation (8 issues) ----

create_issue \
  "ir + elaborator: per-struct RecordLayout finalisation in EmitWalker" \
  "## Summary
Introduce per-struct layout computation at build start so m3-002/m3-005 can use field offsets as constants.

## Acceptance criteria
- EmitPassState gains record_layouts: HashMap<RecordTypeId, RecordLayout>
- finalise_record_layouts walks every RecordTypeId in the IR
- Capability struct (4×u64) lays out as [0, 8, 16, 24], size 32, align 8
- Rejects records with non-u64/u32/u8/*T fields with T0513
- 5 unit tests

## Files
- crates/paideia-as-elaborator/src/emit_walker.rs
- crates/paideia-as-ir/src/record_layout.rs
- crates/paideia-as-diagnostics/catalog.toml

## Test plan
Unit tests for layout computation." \
  "phase:6,area:elaborator,area:walker-activation,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m3 struct walker activation" \
  "m3-001" \
  "S"

create_issue \
  "elaborator: EmitWalker lowers IrKind::FieldAccess for (*p).field shape" \
  "## Summary
Emit mov rax, [base + offset] for struct field reads via (*p).field pattern.

## Acceptance criteria
- For (*p).kind: emits 48 8B 07 (mov rax, [rdi])
- For (*p).generation (offset 24): emits 48 8B 47 18
- u32 fields emit 32-bit form; u8 fields emit movzx
- Errors with T0514 for non-Deref(Var) shapes
- End-to-end fixture: cap_read_kind.pdx emits 4 bytes (mov + ret)
- 4 unit tests

## Files
- crates/paideia-as-elaborator/src/emit_walker.rs
- crates/paideia-as-diagnostics/catalog.toml
- tests/build-emit/cap_read_kind.pdx
- crates/paideia-as/tests/build_emit_field_read.rs

## Test plan
End-to-end fixtures + unit tests." \
  "phase:6,area:elaborator,area:walker-activation,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m3 struct walker activation" \
  "m3-002" \
  "S"

create_issue \
  "elaborator: EmitWalker lowers IrKind::Let(FieldAccess) for in-block field bindings" \
  "## Summary
Assign distinct scratch registers for multiple field reads in one function (RAX, RCX, RDX, R8).

## Acceptance criteria
- 2-stmt Let chain: first reads to RAX, second to RCX
- 4-stmt chain: RAX, RCX, RDX, R8
- 5-stmt chain: fires E0901 (register pressure exceeded)
- EmitPassState gains scratch_assignment vec (reset per function)
- 3 unit tests

## Files
- crates/paideia-as-elaborator/src/emit_walker.rs
- crates/paideia-as-diagnostics/catalog.toml

## Test plan
Unit tests for 1/4/5 in-flight reads." \
  "phase:6,area:elaborator,area:walker-activation,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m3 struct walker activation" \
  "m3-003" \
  "S"

create_issue \
  "elaborator: EmitWalker lowers IrKind::RecordCons for cap-mint shape" \
  "## Summary
Emit four field stores for cap-mint: mov [rdi], rsi; mov [rdi+8], rdx; mov [rdi+16], rcx; mov [rdi+24], imm32.

## Acceptance criteria
- For 4-field cap descriptor: emits 4 store instructions in order
- Source regs follow System-V ABI: RSI, RDX, RCX, R8 for args 2–5
- Literal fields emit sign-extended imm32 form
- Errors with T0515 for unsupported RecordCons shapes
- End-to-end fixture: cap_mint.pdx emits 4 movs (~24 bytes)
- 4 unit tests

## Files
- crates/paideia-as-elaborator/src/emit_walker.rs
- crates/paideia-as-diagnostics/catalog.toml
- tests/build-emit/cap_mint.pdx
- crates/paideia-as/tests/build_emit_record_cons.rs

## Test plan
End-to-end fixtures + unit tests." \
  "phase:6,area:elaborator,area:walker-activation,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m3 struct walker activation" \
  "m3-004" \
  "M"

create_issue \
  "unsafe-walker + ir: field-access expression inside unsafe { block: { ... } } payload" \
  "## Summary
Parse *p.field operand in unsafe blocks for field-write patterns (e.g., mov [rdi+16], rsi for *p.rights = r).

## Acceptance criteria
- *p.rights = r inside unsafe block parses as mov [rdi+16], rsi
- Uses RecordLayoutTable to resolve field offset at parse time
- Errors with U1608 for unresolved field offsets
- End-to-end fixture: cap_set_rights.pdx emits 48 89 77 10
- 4 unit tests

## Files
- crates/paideia-as-elaborator/src/unsafe_walker.rs
- crates/paideia-as-diagnostics/catalog.toml
- tests/build-emit/cap_set_rights.pdx
- crates/paideia-as/tests/build_emit_field_ptr_write.rs

## Test plan
End-to-end fixtures + unit tests." \
  "phase:6,area:elaborator,area:walker-activation,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m3 struct walker activation" \
  "m3-005" \
  "S"

create_issue \
  "emitter-elf: record-layout debug info via .note.paideia" \
  "## Summary
Emit a .note.paideia section containing JSON-serialised record_layouts for downstream tools.

## Acceptance criteria
- Built ELF objects contain .note.paideia with n_type = 0x50441600 (PDX_LAYOUTS)
- Descriptor bytes = serde_json::to_vec(&record_layouts)
- readelf -n shows the note with correct name + type
- Section is SHT_NOTE, SHF_ALLOC = 0 (not loaded)
- Round-trip test via object crate
- Omitted when record_layouts is empty

## Files
- crates/paideia-as-emitter-elf/src/notes.rs (new)
- crates/paideia-as-emitter-elf/src/writer.rs
- crates/paideia-as/tests/note_paideia_layouts.rs

## Test plan
Round-trip test via object crate." \
  "phase:6,area:emitter-elf,type:feature,priority:medium" \
  "Phase 6 — m3 struct walker activation" \
  "m3-006" \
  "S"

create_issue \
  "cli: cmd_build runs EmitWalker::finalise_record_layouts before walk" \
  "## Summary
Wire m3-001 finalisation into the build pipeline: call before the per-node walk.

## Acceptance criteria
- cmd_build calls emit_walker.pass_state_mut().finalise_record_layouts before walking
- On 0 structs: no-op
- On 3 structs: table has 3 entries
- Diagnostics route through existing walker sink

## Files
- crates/paideia-as/src/cmd_build.rs

## Test plan
Integration test with varying struct counts." \
  "phase:6,area:cli,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m3 struct walker activation" \
  "m3-007" \
  "XS"

create_issue \
  "examples + corpus: struct-walker activation tests" \
  "## Summary
Corpus of struct-walker end-to-end fixtures: cap_read_kind, cap_read_generation, cap_mint, cap_set_rights, cap_verify_compound.

## Acceptance criteria
- 5 .pdx fixtures under tests/build-emit/struct/
- Each has .expected_bytes.txt snapshot
- Integration test builds each, asserts exit 0, snapshot-matches
- Tests cover m3-002, m3-004, m3-005 lowering shapes

## Files
- tests/build-emit/struct/*.pdx
- tests/build-emit/struct/*.expected_bytes.txt
- crates/paideia-as/tests/build_emit_struct_corpus.rs

## Test plan
Golden-byte corpus tests." \
  "phase:6,area:testing,type:test,priority:medium" \
  "Phase 6 — m3 struct walker activation" \
  "m3-008" \
  "S"

# ---- M4: Control-Flow Encoders (6 issues) ----

create_issue \
  "encoder: real Cmp encoder for cmp reg/reg, [mem]/reg, reg/imm" \
  "## Summary
Implement three operand shapes of cmp instruction: reg/reg, mem/reg, reg/imm.

## Acceptance criteria
- cmp rax, rdi -> 48 39 F8
- cmp [rdi+24], rcx -> 48 39 4F 18
- cmp rax, 0 -> 48 83 F8 00 (sign-extended imm8)
- cmp rax, imm32 -> opcode 81 /7 id
- Rejects imm64 out of range with unsupported error
- 12 round-trip tests via iced-x86

## Files
- crates/paideia-as-encoder/src/encode.rs
- crates/paideia-as-encoder/src/encode_instruction.rs

## Test plan
Round-trip via iced-x86." \
  "phase:6,area:encoder,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m4 control-flow-encoders" \
  "m4-001" \
  "S"

create_issue \
  "ir + unsafe-walker: label declaration + forward-label operand shape" \
  "## Summary
Add label support: parser recognizes label: declarations and bare identifiers as label references.

## Acceptance criteria
- fail_label: syntax parses as a label declaration
- fail_label in jne operand parses as SymbolRef (forward label)
- Emits Operand::SymbolRef in InstructionSideTable
- Errors U1609 for undefined labels
- 4 unit tests

## Files
- crates/paideia-as-elaborator/src/unsafe_walker.rs
- crates/paideia-as-diagnostics/catalog.toml
- crates/paideia-as/tests/unsafe_walker_labels.rs

## Test plan
Unit tests for label parsing and undefined-label detection." \
  "phase:6,area:elaborator,area:walker-activation,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m4 control-flow-encoders" \
  "m4-002" \
  "S"

create_issue \
  "encoder: real Jcc encoder for forward labels (rel32 form)" \
  "## Summary
Implement Jcc encoder that emits placeholder offset (to be filled by emit-pass-2 patcher).

## Acceptance criteria
- jne rel32_placeholder emits 0F 85 <4-byte placeholder>
- Emit-pass-2 patcher fills the placeholder with actual rel32 offset
- 8 round-trip tests for each Jcc condition (ja, je, jne, jl, jle, jg, jge, jo)
- Errors on backward labels (not supported in Phase 6)

## Files
- crates/paideia-as-encoder/src/encode_instruction.rs
- crates/paideia-as-cli/src/emit_pass2_patcher.rs (wiring)
- crates/paideia-as/tests/encoder_jcc_labels.rs

## Test plan
Round-trip tests for each Jcc variant." \
  "phase:6,area:encoder,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m4 control-flow-encoders" \
  "m4-003" \
  "M"

create_issue \
  "cli: emit-pass-2 patcher applies label fixups + relocations" \
  "## Summary
Implement emit-pass-2 patcher that resolves forward labels and applies label-based relocations.

## Acceptance criteria
- Patcher walks all Jcc instructions with placeholder offsets
- Computes label positions (byte offsets in text section)
- Fills in rel32 offsets for each Jcc
- Result is a valid ELF with correct branch targets
- Integration test: fixture with cmp + jne + label emits correct branches

## Files
- crates/paideia-as-cli/src/emit_pass2_patcher.rs
- crates/paideia-as/tests/emit_pass2_label_fixup.rs

## Test plan
Integration test with cmp + jne + label fixture." \
  "phase:6,area:cli,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m4 control-flow-encoders" \
  "m4-004" \
  "S"

create_issue \
  "unsafe-walker: bare-identifier in call position resolves to SymbolRef" \
  "## Summary
Parse bare identifier in call operand position as a function symbol reference.

## Acceptance criteria
- call cap_alloc inside unsafe block parses as Operand::SymbolRef
- Encoder emits E8 <rel32> for call sym (phase-5 m5-002 already exists)
- Errors U1610 for undefined function symbols
- 3 unit tests

## Files
- crates/paideia-as-elaborator/src/unsafe_walker.rs
- crates/paideia-as-diagnostics/catalog.toml

## Test plan
Unit tests for symbol parsing and undefined-symbol detection." \
  "phase:6,area:elaborator,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m4 control-flow-encoders" \
  "m4-005" \
  "S"

create_issue \
  "examples + corpus: control-flow corpus (cmp + jcc + call)" \
  "## Summary
End-to-end corpus for control-flow: cmp reg/mem, jne forward_label, call symbol.

## Acceptance criteria
- Fixtures combining cmp + jne + labels
- Fixtures with inter-function call in unsafe blocks
- 3–5 .pdx sources with .expected_bytes.txt snapshots
- Integration test builds each, asserts exit 0, snapshot-matches

## Files
- tests/build-emit/control-flow/*.pdx
- tests/build-emit/control-flow/*.expected_bytes.txt
- crates/paideia-as/tests/build_emit_control_flow_corpus.rs

## Test plan
Golden-byte corpus tests." \
  "phase:6,area:testing,type:test,priority:medium" \
  "Phase 6 — m4 control-flow-encoders" \
  "m4-006" \
  "S"

# ---- M5: BSS Arrays (5 issues) ----

create_issue \
  "parser + ast: let mut keyword + uninit rhs marker" \
  "## Summary
Add let mut keyword and uninit marker to AST for zero-initialized array declarations.

## Acceptance criteria
- let mut arr : [u64; 512] = uninit parses
- AST captures 'let mut' variant + uninit marker
- Parser tests for valid and invalid uninit syntax
- 4 unit tests

## Files
- crates/paideia-as-parser/src/parse_let.rs
- crates/paideia-as-ast/src/ast.rs
- crates/paideia-as-parser/tests/let_mut_uninit.rs

## Test plan
Parser unit tests." \
  "phase:6,area:parser,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m5 .bss-arrays" \
  "m5-001" \
  "S"

create_issue \
  "ir + elaborator: SectionKind::Bss variant + uninit→.bss routing" \
  "## Summary
Add SectionKind::Bss variant and route let mut arr = uninit to .bss section in IR.

## Acceptance criteria
- IR gains SectionKind::Bss
- Elaborator routes uninit-marked Let bindings to Bss section
- Array type validation: only [u64; N] supported
- Errors T0516 for non-u64 array element type
- 4 unit tests

## Files
- crates/paideia-as-ir/src/instruction.rs
- crates/paideia-as-elaborator/src/lower.rs
- crates/paideia-as-diagnostics/catalog.toml

## Test plan
Unit tests for section routing." \
  "phase:6,area:elaborator,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m5 .bss-arrays" \
  "m5-002" \
  "S"

create_issue \
  "emitter-elf: .bss section emission with SHT_NOBITS" \
  "## Summary
Emit .bss section with SHT_NOBITS type for zero-initialized arrays.

## Acceptance criteria
- .bss section created with SHT_NOBITS type
- sh_size reflects total byte count of .bss data
- sh_offset points to next section (no actual .bss data in ELF)
- 4 integration tests with various array sizes

## Files
- crates/paideia-as-emitter-elf/src/writer.rs
- crates/paideia-as/tests/emit_bss_section.rs

## Test plan
Integration tests with readelf verification." \
  "phase:6,area:emitter-elf,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m5 .bss-arrays" \
  "m5-003" \
  "S"

create_issue \
  "emitter-elf: relocations against .bss symbols work end-to-end" \
  "## Summary
Wire relocations to .bss symbols so code can reference array addresses.

## Acceptance criteria
- Relocation for lea rax, [rel .bss_symbol] works
- Relocation points to correct .bss section offset
- End-to-end fixture: lea + access to .bss array emits correct bytes
- 3 integration tests

## Files
- crates/paideia-as-emitter-elf/src/relocs.rs
- crates/paideia-as/tests/emit_bss_relocation.rs

## Test plan
Integration tests with objdump verification." \
  "phase:6,area:emitter-elf,type:feature,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m5 .bss-arrays" \
  "m5-004" \
  "S"

create_issue \
  "tests: PaideiaOS pagetables.pdx rebuilds with .bss arrays" \
  "## Summary
Re-enable pagetables.pdx in the Phase-1 rebuild suite; verify .bss arrays work end-to-end.

## Acceptance criteria
- pagetables.pdx builds to .text + .bss
- .bss contains the three 4 KiB page tables
- Phase-1 rebuild suite re-includes pagetables.pdx (removes FIXME)
- Integration test asserts exit 0 + non-empty .text/.bss

## Files
- crates/paideia-as/tests/paideia_os_phase1_rebuild.rs

## Test plan
Cross-repo canary: pagetables.pdx builds clean." \
  "phase:6,area:testing,type:test,priority:medium" \
  "Phase 6 — m5 .bss-arrays" \
  "m5-005" \
  "XS"

# ---- M6: End-to-End Smoke (4 issues) ----

create_issue \
  "fixtures: tests/build-emit/cap_smoke.pdx source" \
  "## Summary
Cap-system minimal smoke-test fixture: struct Capability + field reads/writes + control flow.

## Acceptance criteria
- Declares struct Capability (4 u64 fields)
- Function cap_verify reads two fields, compares, branches
- Function cap_mint writes all four fields
- Function stub cap_alloc for interop
- Source compiles to real (non-placeholder) bytes

## Files
- tests/build-emit/cap_smoke.pdx

## Test plan
Smoke-test fixture for end-to-end verification." \
  "phase:6,area:testing,type:test,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m6 end-to-end-smoke" \
  "m6-001" \
  "S"

create_issue \
  "fixtures: tests/build-emit/cap_smoke.link.ld + harness driver" \
  "## Summary
Linker script and test harness for cap_smoke.pdx end-to-end execution.

## Acceptance criteria
- Linker script defines memory layout for smoke test
- Harness driver loads and verifies test execution
- cap_smoke.pdx links to valid ELF

## Files
- tests/build-emit/cap_smoke.link.ld
- tests/build-emit/cap_smoke_harness.rs

## Test plan
Integration test with linker + harness." \
  "phase:6,area:testing,type:test,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m6 end-to-end-smoke" \
  "m6-002" \
  "XS"

create_issue \
  "tests: byte-sequence + reloc-table assertion for cap_smoke.pdx" \
  "## Summary
Golden-byte fixture and relocation-table validation for cap_smoke.pdx.

## Acceptance criteria
- .expected_bytes.txt snapshot of cap_smoke byte sequence
- Integration test verifies byte-exact match
- Relocation table contains correct entries for struct fields + function calls
- Test runs on every CI cycle

## Files
- tests/build-emit/cap_smoke.expected_bytes.txt
- crates/paideia-as/tests/build_emit_cap_smoke.rs

## Test plan
Golden-byte + relocation snapshot tests." \
  "phase:6,area:testing,type:test,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m6 end-to-end-smoke" \
  "m6-003" \
  "S"

create_issue \
  "tests: runtime smoke + PaideiaOS Phase-2 unblock marker" \
  "## Summary
Runtime verification that cap_smoke.pdx executes correctly; explicit Phase-2 unblock marker commit.

## Acceptance criteria
- cap_smoke.pdx loads and executes on qemu
- Output matches expected capability operations
- Commit message includes 'Unblocks paideia-os Phase 2' sentinel
- Build exits 0 on paideia-os Phase-2 capability descriptor smoke test

## Files
- tests/build-emit/cap_smoke_runtime.sh
- crates/paideia-as/tests/paideia_os_phase2_smoke.rs

## Test plan
Runtime smoke test on qemu." \
  "phase:6,area:testing,type:test,priority:high,gated:downstream-paideia-os" \
  "Phase 6 — m6 end-to-end-smoke" \
  "m6-004" \
  "XS"

# ---- M7: Docs Closure (4 issues) ----

create_issue \
  "docs: design/toolchain/phase-transition-6.md retrospective" \
  "## Summary
Phase-6 retrospective: scope, scope carryover, what didn't ship, what got right, Phase 7 carryover.

## Acceptance criteria
- Document follows phase-transition-5.md shape
- Sections: §0 scope, §1 carryover, §2 didn't ship, §3 got right, §4 would change, §5 Phase-7 carryover
- References all milestone appendices
- Test count delta (2419 -> final)
- Diagnostic codes added (T0513–T0516, U1607–U1609, E0901)

## Files
- design/toolchain/phase-transition-6.md

## Test plan
Document review + completeness check." \
  "phase:6,area:documentation,type:docs,priority:medium,gated:downstream-paideia-os" \
  "Phase 6 — m7 docs-closure" \
  "m7-001" \
  "XS"

create_issue \
  "docs: STATUS.md Phase 6 closure section" \
  "## Summary
Add Phase 6 closure section to STATUS.md documenting walker activation and bug fixes.

## Acceptance criteria
- Section titled 'Phase 6 closed (walker activation & paideia-os bug fixes)'
- Bullet list of major deliverables (struct walker, cmp/jcc/call, .bss arrays)
- Test count: 2419 -> final
- v0.6.0 release marker

## Files
- STATUS.md

## Test plan
Document review." \
  "phase:6,area:documentation,type:docs,priority:medium" \
  "Phase 6 — m7 docs-closure" \
  "m7-002" \
  "XS"

create_issue \
  "release: v0.6.0 tag + CHANGELOG Phase 6 section" \
  "## Summary
Create v0.6.0 tag and add CHANGELOG entry for Phase 6 deliverables.

## Acceptance criteria
- Tag v0.6.0 created on final Phase-6 commit
- CHANGELOG.md includes '## v0.6.0 — Phase 6 (walker activation & paideia-os bug fixes)'
- Section lists milestones m1-m7 with issue counts
- Section notes paideia-os Phase-2 unblock

## Files
- CHANGELOG.md
- (git tag v0.6.0)

## Test plan
Release tag + changelog verification." \
  "phase:6,area:documentation,type:docs,priority:medium,gated:downstream-paideia-os" \
  "Phase 6 — m7 docs-closure" \
  "m7-003" \
  "XS"

create_issue \
  "examples + PaideiaOS rewrites: walk away from fn(x:()) workaround" \
  "## Summary
Update examples and PaideiaOS boot sources to use fn () syntax instead of fn (x: ()) workaround.

## Acceptance criteria
- All paideia-as examples rewritten from fn (x: ()) to fn ()
- PaideiaOS boot .pdx files rewritten (once paideia-as is bumped)
- Demonstrates clean syntax enabled by m2-001

## Files
- examples/*.pdx
- (paideia-os submodule boot sources, post-submodule-bump)

## Test plan
Regression test: examples still build correctly." \
  "phase:6,area:documentation,type:docs,priority:medium" \
  "Phase 6 — m7 docs-closure" \
  "m7-004" \
  "S"

# ============================================================================
# SUMMARY
# ============================================================================

log_info "Phase 6 bootstrap complete!"
log_info ""
log_info "Summary:"
log_info "  - 3 labels created"
log_info "  - 7 milestones created"
log_info "  - 37 issues created"
log_info ""
log_info "Issue map written to: $ISSUE_MAP"
log_info ""
log_info "Total GitHub API operations: $OPERATIONS_COUNT"
log_info ""
log_info "Repository: $REPO"
log_info "Next steps:"
log_info "  1. Verify all issues appear in GitHub"
log_info "  2. Review labels and milestones"
log_info "  3. Begin Phase 6 work starting with m1-001"

exit 0
