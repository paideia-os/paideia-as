#!/bin/bash
# gh-bootstrap-pa7-completion.sh — Idempotent GitHub bootstrap for PA7-completion round
#
# Creates labels, milestone, and 21 issues from the osarch plan.
# Safe to re-run; all operations check existence first.
#
# Usage: ./tools/gh-bootstrap-pa7-completion.sh
#
# Dependencies: gh CLI, jq, standard Unix tools

set -e

REPO="paideia-os/paideia-as"
OSARCH_PLAN=".plans/pa7-completion-osarch-plan.md"
SOFTARCH_PLAN=".plans/pa7-completion-softarch-plan.md"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Step 0: Verify plans exist
if [ ! -f "$OSARCH_PLAN" ] || [ ! -f "$SOFTARCH_PLAN" ]; then
    log_error "Plan files not found. Expected:"
    log_error "  - $OSARCH_PLAN"
    log_error "  - $SOFTARCH_PLAN"
    exit 1
fi

log_info "Starting PA7-completion GitHub bootstrap for $REPO"

# Step 1: Create labels (idempotent via --force)
log_info "Step 1: Creating labels..."

gh label create "pa7-completion" \
    --color "5319E7" \
    --description "PA7-completion round (v0.7.0 target)." \
    --force --repo "$REPO"

gh label create "unblocks-paideia-os" \
    --color "FBC02D" \
    --description "Closure unquarantines specific paideia-os files." \
    --force --repo "$REPO"

gh label create "gap:byte-emit" \
    --color "D32F2F" \
    --description "PA7 byte-emit incompleteness (G1/G2/G3 family)." \
    --force --repo "$REPO"

gh label create "gap:parser-surface" \
    --color "1976D2" \
    --description "Parser/lexer/grammar additions (G4-G10 family)." \
    --force --repo "$REPO"

gh label create "gap:anticipated" \
    --color "388E3C" \
    --description "Anticipated future need; may defer to v0.8." \
    --force --repo "$REPO"

log_info "Labels created."

# Step 2: Create or verify milestone
log_info "Step 2: Creating milestone..."

MILESTONE_SLUG="pa7-completion"
MILESTONE_CHECK=$(gh api repos/paideia-os/paideia-as/milestones \
    --jq ".[] | select(.title == \"PA7 Completion (byte-emit + parser-surface + anticipated)\") | .number" 2>/dev/null || echo "")

if [ -z "$MILESTONE_CHECK" ]; then
    gh api repos/paideia-os/paideia-as/milestones \
        --input - <<EOF
{
  "title": "PA7 Completion (byte-emit + parser-surface + anticipated)",
  "description": "See .plans/pa7-completion-osarch-plan.md. Closes when all G1-G10 issues are closed and paideia-os unquarantine is verified."
}
EOF
    log_info "Milestone created."
else
    log_info "Milestone already exists (id: $MILESTONE_CHECK)."
fi

# Fetch the milestone number for use in issue creation
MILESTONE_NUM=$(gh api repos/paideia-os/paideia-as/milestones \
    --jq ".[] | select(.title == \"PA7 Completion (byte-emit + parser-surface + anticipated)\") | .number")

# Step 3: Create 21 issues
log_info "Step 3: Creating 21 issues..."

# Helper function to check if issue exists by title
issue_exists() {
    local title="$1"
    gh issue list --repo "$REPO" \
        --search "in:title \"$title\"" \
        --state open \
        --limit 1 \
        --json number \
        --jq 'length' 2>/dev/null || echo "0"
}

# Helper function to create an issue with proper formatting
create_issue() {
    local task_id="$1"
    local title="$2"
    local summary="$3"
    local acceptance="$4"
    local files="$5"
    local deps="$6"
    local size="$7"
    local unblocks_files="$8"
    local gap_family="$9"

    # Full title for checking
    local full_title="${task_id}: ${title}"

    # Check if issue already exists
    if [ "$(issue_exists "$full_title")" != "0" ]; then
        log_warn "Issue already exists: $full_title"
        return
    fi

    # Build labels based on gap family
    local labels="pa7-completion,type:feature,size:${size}"

    if [ "$gap_family" = "byte-emit" ]; then
        labels="${labels},gap:byte-emit"
    elif [ "$gap_family" = "parser-surface" ]; then
        labels="${labels},gap:parser-surface"
    elif [ "$gap_family" = "anticipated" ]; then
        labels="${labels},gap:anticipated"
    fi

    # Add unblocks-paideia-os label if there are files to unblock
    if [ -n "$unblocks_files" ]; then
        labels="${labels},unblocks-paideia-os"
    fi

    # Create issue body
    local body="$summary

## Acceptance criteria
$acceptance

## Files
$files

## Dependencies
$deps

## Estimated size
$size

## Milestone
pa7-completion

## Unblocks paideia-os file(s)
${unblocks_files:-none}

## Surfaced by
paideia-os@d155100 (https://github.com/paideia-os/paideia-os/commit/d155100)

## Definition of done
Observable test, not just \"compiles\"."

    # Create the issue
    gh issue create \
        --repo "$REPO" \
        --title "$full_title" \
        --body "$body" \
        --label "$labels" \
        --milestone "$MILESTONE_NUM" \
        --assignee snunezcr

    log_info "Created: $full_title"
}

# M1 issues (Symbol export + PLT32 reloc offset — G1 + G2)

create_issue "PA7C-m1-001" "cmd_build: walk the SymbolTable using binding names, kill the add_one fallback" \
    "In cmd_build.rs, the SymbolKind::Function arm needs to use binding names instead of synthetic names. Fix: (a) populate SymbolEntry::name from binding name at IR construction time; (b) delete the synthetic-add_one fallback. After both changes, readelf shows one STT_FUNC per top-level let-fn with the binding's actual name." \
    "- Every top-level let NAME : T = fn (...) -> BODY produces exactly one SymbolEntry { name: \"NAME\", kind: SymbolKind::Function, st_value, st_size }
- The fallback branch at cmd_build.rs:786..788 is removed
- New diagnostic B0007 (\"no exported symbols\") is added
- Integration test builds a 3-function source and asserts symbol table contains exactly those three names
- Regression: pre-existing tests assuming add_one fallback are rewritten" \
    "crates/paideia-as-elaborator/src/lower.rs
crates/paideia-as/src/cmd_build.rs
crates/paideia-as-diagnostics/catalog.toml
crates/paideia-as/tests/build_emit_pa7c_symbol_export.rs" \
    "none" "S" \
    "kernel_main.pdx, int/exceptions.pdx, int/idt.pdx, mm/pt_walk.pdx" "byte-emit"

create_issue "PA7C-m1-002" "emitter-elf: assert symbol-name uniqueness and non-overlapping ranges" \
    "A defensive check that catches future regressions of m1-001. Before finalize() emits the symbol-table section, assert (a) every symbol range lies inside its section; (b) no two ranges overlap; (c) symbol names are unique." \
    "- ElfWriter::finalize runs three checks before writing symbol table
- Synthetic tests for duplicate names, overlapping ranges, out-of-bounds
- cmd_build catches EmitterError::SymbolLayoutInvalid and reports as B0008" \
    "crates/paideia-as-emitter-elf/src/writer.rs
crates/paideia-as-emitter-elf/src/lib.rs
crates/paideia-as/src/cmd_build.rs
crates/paideia-as-diagnostics/catalog.toml" \
    "PA7C-m1-001" "XS" "" "byte-emit"

create_issue "PA7C-m1-003" "encoder + walker: collapse byte-position counters into single source of truth" \
    "Today byte position is tracked in two places: (a) CodeBuffer::bytes.len() in encoder; (b) EmitWalker::current_offset. These drift on PA7 multi-stmt unsafe paths. Fix: InstructionSideTable entry gains byte_offset_in_text field populated during encoding. All reloc-offset arithmetic reads from this slot. The walker's current_offset becomes an advisory estimate." \
    "- InstructionSideTable gains pub byte_offset_in_text: Option<u32>
- RelocSite::byte_offset computed as instruction.byte_offset_in_text.unwrap() + 1
- EmitWalker::current_offset renamed to estimated_offset with doc comment
- assert_eq! at end of build path checks estimated_offset == buf.bytes.len()
- Regression fixture with four unsafe blocks with interleaved calls
- Six unit tests covering interleave patterns" \
    "crates/paideia-as-ir/src/instruction.rs
crates/paideia-as/src/cmd_build.rs
crates/paideia-as-encoder/src/encode_instruction.rs
crates/paideia-as-elaborator/src/emit_walker.rs
crates/paideia-as-diagnostics/catalog.toml
tools/run-pa7c-reloc-regression.sh
crates/paideia-as-elaborator/tests/emit_walker/byte_offset.rs" \
    "PA7C-m1-002" "M" \
    "kernel_main.pdx, int/exceptions.pdx, int/idt.pdx, mm/pt_walk.pdx" "byte-emit"

create_issue "PA7C-m1-004" "tests: PLT32 round-trip via iced-x86 disassembly and ld rejection witness" \
    "A standalone test crate that builds .o via cmd_build, loads via object crate, and for every relocation, disassembles surrounding bytes via iced-x86 to confirm reloc offset lands inside call rel32 immediate. Constructs minimal companion .o declaring target symbol and links via ld." \
    "- New test file crates/paideia-as/tests/build_emit_pa7c_plt32_witness.rs
- Covers ≥ 8 instruction shapes
- For each: asserts reloc offset is after E8, iced-x86 disassembles correctly, ld -r exits 0
- Test gated on ld availability (skipped on macOS/Windows)" \
    "crates/paideia-as/tests/build_emit_pa7c_plt32_witness.rs
crates/paideia-as/tests/fixtures/pa7c_plt32/*.pdx
crates/paideia-as/tests/fixtures/pa7c_plt32/partner.S" \
    "PA7C-m1-003" "S" "" "byte-emit"

# M2 issues (Unsafe-block body bridging — G3)

create_issue "PA7C-m2-001" "emit_walker: recognise IrKind::RawInstruction inside Action and forward to side-table" \
    "In emit_block_body, the IrKind::Action arm has a TODO. Fix: inspect the Action's child node; if RawInstruction, forward to side-table and bump current_offset. The encoder's estimated_size_for provides the conservative upper bound." \
    "- emit_block_body's IrKind::Action arm inspects child and inserts Instruction on RawInstruction
- New helper Mnemonic::estimated_size(operands) returns conservative upper-bound size
- Regression fixture: pa7c_unsafe_body_outb.pdx with mov/out sequence
- All 7 fixtures build with byte-exact .text matching iced-x86 disassembly
- 5 unit tests in emit_walker/unsafe_body.rs" \
    "crates/paideia-as-elaborator/src/emit_walker.rs
crates/paideia-as-ir/src/instruction.rs
crates/paideia-as-elaborator/tests/emit_walker/unsafe_body.rs
7 new .pdx fixtures" \
    "none" "M" \
    "kernel_main.pdx, int/exceptions.pdx, int/idt.pdx, mm/pt_walk.pdx" "byte-emit"

create_issue "PA7C-m2-002" "emit_walker: recognise IrKind::Let with RawInstruction RHS and propagate dest reg" \
    "Sister to m2-001 for IrKind::Let arm. Inspect Let's RHS; if RawInstruction { mnemonic: Mov, operands: [Imm64] }, allocate scratch reg and insert Instruction. Handles let binding patterns in unsafe blocks." \
    "- Let arm inspects RHS and on Mov+Imm64 emits Instruction
- New diagnostic U1612 for invalid let-binding RHS
- state.scratch_assignment records (IrNodeId, RegId) pairs
- Regression: pa7c_unsafe_body_let_scratch.pdx with let chain
- 3 unit tests covering single let, three-let chain, register-pressure" \
    "crates/paideia-as-elaborator/src/emit_walker.rs
crates/paideia-as-diagnostics/catalog.toml
crates/paideia-as-elaborator/tests/emit_walker/unsafe_body_let.rs
1 new fixture" \
    "PA7C-m2-001" "S" "" "byte-emit"

create_issue "PA7C-m2-003" "emit_walker: Var(name) inside RawInstruction operands resolves to scratch reg" \
    "When unsafe-block instruction operands contain bare identifier, the Operand::Var(name) needs to resolve to scratch reg allocated in m2-002. A new operand-translation pass in cmd_build replaces Var(name) with Reg(scratch_for(name))." \
    "- New operand-translation pass in cmd_build or encoder/dispatch.rs
- Replaces Operand::Var(name) with Operand::Reg using m2-002 map
- Unresolved names emit U1613
- Regression: pa7c_unsafe_body_var_resolve.pdx with let+var pattern
- 2 unit tests: resolved-var and unresolved-var" \
    "crates/paideia-as/src/cmd_build.rs or crates/paideia-as-encoder/src/dispatch.rs
crates/paideia-as-diagnostics/catalog.toml
crates/paideia-as-elaborator/tests/emit_walker/unsafe_body_var.rs
1 new fixture" \
    "PA7C-m2-001, PA7C-m2-002" "S" "" "byte-emit"

create_issue "PA7C-m2-004" "tests: PaideiaOS R1.5/R2.5 four-file re-build regression suite" \
    "The cross-repo canary for m1 + m2. An integration test that, if PaideiaOS is present, copies the four gap-list files into a temp dir, runs cmd_build on each, links via ld, and asserts exit 0 + non-empty .text." \
    "- crates/paideia-as/tests/paideia_os_r1_5_r2_5_rebuild.rs discovers PaideiaOS or skips
- Builds 4 files; asserts each .o has ≥ 1 STT_FUNC symbol
- Links 4 .o's + stub_partner.S via ld -e _start; asserts exit 0
- Four files explicitly named with gap comments" \
    "crates/paideia-as/tests/paideia_os_r1_5_r2_5_rebuild.rs
crates/paideia-as/tests/fixtures/pa7c_link.ld
crates/paideia-as/tests/fixtures/stub_partner.S" \
    "PA7C-m1-001, PA7C-m1-003, PA7C-m2-001, PA7C-m2-002, PA7C-m2-003" "XS" \
    "kernel_main.pdx, int/exceptions.pdx, int/idt.pdx, mm/pt_walk.pdx" "byte-emit"

# M3 issues (Parser papercuts — G5 + G6 + G9)

create_issue "PA7C-m3-001" "lexer: free handle as a user identifier" \
    "Today handle is reserved but PA7 surface does not use it as keyword. PaideiaOS needs it as parameter name. Remove handle from keyword list (delete three entries from token.rs and KwHandle variant). If we reserve it in future, add it back then." \
    "- TokenKind::KwHandle removed from token.rs
- Three occurrences (variant, keyword_kind arm, reserved-words list) deleted
- Reserved-words test passes; list shrunk from 69 to 68
- New test: handle lexes as Ident
- New doc design/toolchain/reserved-word-policy.md" \
    "crates/paideia-as-lexer/src/token.rs
crates/paideia-as-lexer/tests/handle_identifier.rs
design/toolchain/reserved-word-policy.md" \
    "none" "XS" "" "parser-surface"

create_issue "PA7C-m3-002" "parser: make -> optional before { ... } body in fn-literal grammar" \
    "fn () { stmt; expr } rejects today; fn () -> { stmt; expr } parses. The -> is redundant when body is a block. Fix: after parameter list, peek at next token; if LBrace, accept body directly. Existing -> { } form continues to parse." \
    "- Fn-literal production accepts both fn (...) { body } and fn (...) -> { body }
- fn (...) -> Type { body } (explicit return type) continues to parse
- fn (...) -> body_expr (non-block body) requires ->
- 4 new parser tests: arrow-elided block, arrow-present block, arrow-elided + explicit-type (rejected), arrow-present + non-block
- Round-trip through paideia-fmt" \
    "crates/paideia-as-parser/src/parse_handler.rs
crates/paideia-as-parser/tests/fn_literal_arrow_elision.rs
crates/paideia-fmt/src/settings.rs" \
    "none" "S" "" "parser-surface"

create_issue "PA7C-m3-003" "parser: unit-typed blocks accept trailing-semi without requiring ()" \
    "if x < N { a[i] = b; head = i; } rejects with P0158. This is correct for value-position blocks but wrong for statement-position. Fix: at P0158 emit site, check if block is in unit-typed position. If yes, synthesise final IrKind::Unit and accept. If no, emit P0158." \
    "- New helper Parser::expect_block_kind(expected: BlockKind) distinguishes Value from Statement
- Statement-position blocks synthesise final Unit; no P0158 fires
- Value-position blocks emit P0158 as today
- 8 parser tests: statement-position if, value-position if, nested if/else, while-body, for-body, loop-body, void-return fn-body, value-return fn-body
- Regression: quarantined files re-parse cleanly" \
    "crates/paideia-as-parser/src/parse_control.rs
crates/paideia-as-parser/src/parser.rs
crates/paideia-as-parser/tests/block_kind.rs" \
    "none" "S" "" "parser-surface"

# M4 issues (Expression surface — G4 + G7 + G8)

create_issue "PA7C-m4-001" "lexer + parser: unary bitwise NOT prefix ~" \
    "Today there is no Tilde token; ~ is not lexed at all. Fix: add TokenKind::Tilde to lexer, add prefix parselet in parser that consumes Tilde and lowers to IrKind::UnaryOp { op: BitNot }. Add encoder case for Mnemonic::Not on [Reg]." \
    "- TokenKind::Tilde exists in paideia-as-lexer; ~ lexes as Tilde
- Expression grammar has prefix parselet at same precedence as unary -/!
- ~x parses to Expr::UnaryOp(BitNot, x) and lowers to IrKind::UnaryOp { op: BitNot }
- Encoder emits F7 D0 (not rax) with REX prefix as needed
- Capability-verifier fixture: pa7c_cap_verify_bitnot.pdx
- 4 unit tests at lexer/parser/IR/encoder layers" \
    "crates/paideia-as-lexer/src/token.rs
crates/paideia-as-parser/src/precedence.rs
crates/paideia-as-parser/src/parser.rs
crates/paideia-as-ir/src/
crates/paideia-as-encoder/src/encode_instruction.rs
4 test files" \
    "none" "S" \
    "cap/verify.pdx (or similar capability-verifier usage)" "parser-surface"

create_issue "PA7C-m4-002" "parser + IR: EXPR as TYPE cast operator" \
    "x as u32 rejects today. Grep confirms KwAs exists but production is incomplete. Fix: add postfix parselet at precedence below multiplicative ops that consumes EXPR as TYPE, parses type via parse_type, lowers to IrKind::Cast { target_ty, arg }." \
    "- x as u32 parses to Expr::Cast(x_id, Type::U32)
- Lowers to IrKind::Cast { target_ty: IntWidth::U32, arg }
- Encoder emits right instruction per (src_width, dst_width): widening signed (movsx), widening unsigned (movzx), narrowing (mov sub-reg), same-width (no-op)
- Cast to *T from u64 accepted (pointer-from-integer)
- Cast from *T to u64 accepted (pointer-to-integer)
- PT-walk fixture: pa7c_pt_walk_cast.pdx with (va as u64 >> 12) as u32 pattern
- 4 parser tests + 12 encoder tests + 1 end-to-end fixture" \
    "crates/paideia-as-parser/src/parser.rs
crates/paideia-as-parser/src/precedence.rs
crates/paideia-as-ir/src/
crates/paideia-as-encoder/src/encode_instruction.rs
test files" \
    "none" "S" \
    "mm/pt_walk.pdx" "parser-surface"

create_issue "PA7C-m4-003" "encoder: thread IntWidth from IR through DispatchKind to RegId size" \
    "AST + parser accept u8/u16/u32/i32 but build pipeline is u64-only. A let x : u32 = 42 binding silently falls out because DispatchKind::classify treats every Mov as 64-bit. Fix: extend DispatchKind to carry Option<IntWidth> discriminator from IR's TypeSideTable; encoder reads discriminator and emits right REX + opcode-size." \
    "- DispatchKind gains width: Option<IntWidth>
- cmd_build populates width from TypeSideTable
- encode_mov reads width and emits right opcode-size per SDM (B0 for mov al, imm8; etc.)
- 16 unit tests: 4 widths × 4 operand shapes
- Slab fixture: pa7c_slab_u32_index.pdx with u32 index emitted as B8 NN 00 00 00
- Regression: encode_mov tests updated" \
    "crates/paideia-as-encoder/src/dispatch.rs
crates/paideia-as-encoder/src/encode_instruction.rs
crates/paideia-as/src/cmd_build.rs
16 test files" \
    "none" "S" \
    "cap/slab.pdx, ipc files using u32 slot indices" "parser-surface"

create_issue "PA7C-m4-004" "tests: round-trip-via-iced-x86 for the m4 expression surface" \
    "A parametrised test file that covers ≥ 20 source/disassembly pairs for ~x, x as T, and sized-int operations. Encodes .text, disassembles via iced-x86, asserts disassembly matches canonical string." \
    "- crates/paideia-as/tests/build_emit_pa7c_expr_surface.rs covers ≥ 20 pairs
- Each pair is one source line + expected disassembly string
- Test parametrised via rstest or hand-rolled vec-driven test" \
    "crates/paideia-as/tests/build_emit_pa7c_expr_surface.rs" \
    "PA7C-m4-001, PA7C-m4-002, PA7C-m4-003" "XS" "" "parser-surface"

# M5 issues (L-value surface — G10)

create_issue "PA7C-m5-001" "parser + IR + encoder: array-index l-value a[i] = expr" \
    "Assignment-expression grammar parses LHS via parse_expr, then checks that parsed expression is a place. Today accepts only Var. Fix: extend place classifier to accept Index(base, idx) and lower assignment to Store { addr: compute(base, idx, elem_size), value, ty: elem_ty }. Encoder emits mov [base + idx * scale], reg." \
    "- a[i] = b parses to Expr::Assign(Expr::Index(a, i), b)
- Lowers to IrKind::Store { addr, value, ty }
- Encoder emits 48 89 04 F7 for mov [rdi + rsi*8], rax (u64 elements)
- For u32: 89 04 B7; for u8: 88 04 37
- Slab fixture: pa7c_slab_freelist_store.pdx with free_list[idx] = free_head
- IPC fixture: pa7c_ipc_ring_store.pdx with ring[head & mask] = msg
- 6 unit tests (3 element widths × 2 register shapes)" \
    "crates/paideia-as-parser/src/parser.rs
crates/paideia-as-ir/src/
crates/paideia-as-encoder/src/encode_instruction.rs
2 fixtures, 6 test files" \
    "PA7C-m3-003" "M" \
    "cap/slab.pdx, ipc/{slots,allocator,dispatch,mpsc_lock,destroy_channel,channel}.pdx, ipi/tlb_shootdown.pdx, sched/enqueue.pdx" "parser-surface"

create_issue "PA7C-m5-002" "parser + IR + encoder: pointer-deref l-value *p = expr and (*p).f = expr" \
    "Companion to m5-001 for pointer-deref l-values. Extend place classifier to accept Deref(ptr) and FieldAccess(Deref(ptr), field). IR lowering reuses Store. Encoder composes with Phase-6 m3 struct walker for field offset lookup." \
    "- *p = expr parses + lowers + encodes as mov [r], rax (3 bytes including REX)
- (*p).field = expr encodes as mov [r + offset], rax (3-6 bytes)
- Channel fixture: pa7c_channel_head_store.pdx with (*ch).head = new_head at offset 16
- destroy_channel fixture: pa7c_channel_destroy.pdx with 3-field zero-initialisation
- 4 unit tests + 2 fixtures" \
    "crates/paideia-as-parser/src/parser.rs
crates/paideia-as-ir/src/
crates/paideia-as-encoder/src/encode_instruction.rs
crates/paideia-as-elaborator/src/emit_walker.rs
test files, 2 fixtures" \
    "PA7C-m5-001" "S" \
    "ipc/channel.pdx, ipc/destroy_channel.pdx" "parser-surface"

# M6 issues (End-to-end smoke + PaideiaOS unquarantine)

create_issue "PA7C-m6-001" "e2e: boot_orchestration_v2 smoke test + paideia-os submodule pin verification" \
    "The cross-repo verification point. After G1+G2+G3+G9+G10 close, verify the paideia-os unquarantine ritual produces a bootable kernel.elf without tools/stubs.S workarounds. The test discovers paideia-os via PAIDEIA_OS_PATH or relative path, runs ./tools/build.sh, and asserts kernel.elf is non-empty." \
    "- boot_orchestration_v2 fixture passes all smoke checks
- Cross-repo test (e2e_paideia_os_pa7_unquarantine.rs or similar) discovers paideia-os
- Runs ./tools/build.sh in paideia-os and asserts kernel.elf produced
- All 13 quarantined files have been unquarantined (test-time check via git ls-files)" \
    "crates/paideia-as/tests/e2e_paideia_os_pa7_unquarantine.rs
tools/boot_orchestration_v2_fixture" \
    "PA7C-m1-001, PA7C-m1-003, PA7C-m2-001, PA7C-m3-003, PA7C-m5-001, PA7C-m5-002" "S" \
    "all 13 quarantined files (kernel_main.pdx, int/{exceptions,idt}.pdx, mm/{pt_walk,tlb_shootdown}.pdx, core/{cap/{handle,invoke,verify,slab},ipc/{slots,allocator,dispatch,mpsc_lock,destroy_channel,channel},ipi/tlb_shootdown,sched/enqueue}.pdx)" "byte-emit"

create_issue "PA7C-m6-002" "PA7-completion round: close issue and verify test count growth" \
    "Formal round-close criterion. Verifies all G1-G10 issues closed, all 13 files unquarantined, tools/stubs.S deleted, and workspace test count strictly grew from PA7 baseline (2651)." \
    "- All G1-G10 issues marked closed
- gh issue list --label pa7-completion --label gap:byte-emit --state open = 0
- gh issue list --label pa7-completion --label gap:parser-surface --state open = 0
- paideia-os: ls .quarantine/src/kernel/ | wc -l = 0
- paideia-os: test ! -f tools/stubs.S
- cargo test --workspace: test count > 2651
- CHANGELOG.md updated with v0.7.0 section" \
    "CHANGELOG.md
Cargo.toml (workspace.version bump to 0.7.0)" \
    "all prior issues" "XS" "" "byte-emit"

create_issue "PA7C-m6-003" "design: phase-transition-7.md retrospective" \
    "Round-close documentation ritual. Captures scope, carryover disposition, what didn't ship (G11-G15), what got right, what would change, and closure note. References per-issue design notes from §10." \
    "- design/toolchain/phase-transition-7.md written per §10.2 template
- References each G-issue's design doc
- Sections: scope, carryover, didn't ship, got right, would change, closing note" \
    "design/toolchain/phase-transition-7.md" \
    "all prior issues" "S" "" "byte-emit"

create_issue "PA7C-m6-004" "tag v0.7.0 and bump paideia-os submodule" \
    "Final round-close ceremony. Tags paideia-as v0.7.0, bumps paideia-os submodule pin to v0.7.0, and resumes paideia-os R6.5+/D7+ autonomous loop." \
    "- paideia-as workspace.version 0.6.0 -> 0.7.0
- git tag v0.7.0 created and pushed
- paideia-os submodule pin bumped to v0.7.0
- paideia-os resumed on R6.5+/D7+ backlog" \
    "Cargo.toml (version bump)
.git/refs/tags/v0.7.0" \
    "PA7C-m6-001, PA7C-m6-002, PA7C-m6-003" "XS" "" "byte-emit"

# Throttle after 21 operations
log_info "Created 21 issues. Throttling for 30 seconds..."
sleep 30

# Step 4: Verify creation
log_info "Step 4: Verifying issue creation..."

ISSUE_COUNT=$(gh issue list --repo "$REPO" --milestone pa7-completion --state open \
    --json number --jq 'length')

log_info "Total open issues in pa7-completion milestone: $ISSUE_COUNT (expected: 21)"

# Fetch and report first 5 issues
log_info "First 5 issues in milestone:"
gh issue list --repo "$REPO" --milestone pa7-completion --state open \
    --json number,title \
    --jq 'sort_by(.number) | .[0:5] | .[] | "  #\(.number): \(.title)"'

# Final status
if [ "$ISSUE_COUNT" -eq 21 ]; then
    log_info "SUCCESS: All 21 issues created."
else
    log_warn "Issue count is $ISSUE_COUNT (expected 21). Some issues may have failed to create or already existed."
fi

log_info "GitHub bootstrap complete. Now save the issue map and commit."
