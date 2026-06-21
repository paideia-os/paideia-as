#!/usr/bin/env python3
"""
Phase 5 GitHub issue bootstrapper - idempotent issue creation script.
Reads task definitions and creates GitHub issues defensively (checks for existing).
"""

import subprocess
import json
import sys
from pathlib import Path

REPO = "paideia-os/paideia-as"
ISSUE_MAP_PATH = Path(".plans/phase-5-issue-map.tsv")

# Phase 5 task definitions
TASKS = [
    # m1 - elaborator lowering
    {
        "id": "m1-001",
        "title": "elaborator: EmitWalker skeleton + EmitPassState side-table writer",
        "milestone": "phase-5-elab-lowering",
        "labels": "phase:5,area:emit-activation,type:feature,size:S,priority:critical",
        "body": """## Summary
Introduce a new walker, `EmitWalker`, whose job is to populate `InstructionSideTable` for the three Phase-5 lowering shapes. Owns the entry into the emit-side of the pipeline; per-construct logic lands in m1-002..004.

## Acceptance criteria
- [ ] `crates/paideia-as-elaborator/src/emit_walker.rs` defines `pub struct EmitWalker { pass_state: EmitPassState }`
- [ ] `EmitPassState` exposes `instructions: &mut InstructionSideTable` plus `current_function: Option<IrNodeId>` and `current_offset: u64`
- [ ] `impl IrWalker for EmitWalker` provides `enter_node` / `exit_node` stubs matching populate-path patterns
- [ ] The walker is exported from `paideia-as-elaborator/src/lib.rs` alongside `LinearityWalker`, `EffectRowWalker`, `CapWalker`
- [ ] One unit test confirms walking an empty `IrArena` produces no side effects and zero diagnostics
- [ ] cargo test --workspace green
- [ ] Test count strictly grew
- [ ] SARIF snapshot regenerated if catalog.toml touched

## Files
`crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-elaborator/src/lib.rs`

## Dependencies
none (uses existing Phase-4 m1 walker convention)

## Estimated size
S

## Unblocks paideia-os
paideia-os#1 (indirectly; m6 is the direct unblock)

## Cross-repo escalation source
none (Phase-5-internal follow-up to Phase 4 m1-005)
""",
    },
    # m1-002
    {
        "id": "m1-002",
        "title": "elaborator: EmitWalker lowers IrKind::Let(Literal) for let : u64 = imm",
        "milestone": "phase-5-elab-lowering",
        "labels": "phase:5,area:emit-activation,type:feature,size:S,priority:critical",
        "body": """## Summary
When the walker enters an `IrKind::Let` whose body is an `IrKind::Literal` of integer type, it emits the canonical `mov reg64, imm` `Instruction` into `InstructionSideTable` keyed by the let node's IrNodeId. Phase-5 simplification: the target register is always `RAX` for top-level `let` items.

## Acceptance criteria
- [ ] The walker recognises the `Let → Literal` shape via IR inspection
- [ ] For literals fitting in i32, emits `Instruction { mnemonic: Mov, operands: [Reg(RegId(0)), Imm64(value as i64)], encoding_hint: None }`
- [ ] For literals > i32 range, emits the same with full 64-bit immediate
- [ ] `Instruction` is inserted via `InstructionSideTable::insert(let_node_id, instruction)`
- [ ] Unit test: `let answer : u64 = 42` emits exactly one entry, encodes to `48 c7 c0 2a 00 00 00`
- [ ] Unit test: `let magic : u64 = 0xCAFE_F00D_DEAD_BEEF` emits `48 b8 ef be ad de 0d f0 fe ca`
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
`crates/paideia-as-elaborator/src/emit_walker.rs`

## Dependencies
m1-001

## Estimated size
S

## Unblocks paideia-os
paideia-os#1 (indirectly; m6 is the direct unblock)

## Cross-repo escalation source
none
""",
    },
    # m1-003
    {
        "id": "m1-003",
        "title": "elaborator: EmitWalker lowers IrKind::Lambda body for fn (x) -> x + N",
        "milestone": "phase-5-elab-lowering",
        "labels": "phase:5,area:emit-activation,type:feature,size:S,priority:critical",
        "body": """## Summary
When the walker enters an `IrKind::Lambda` whose body matches the `Var + Literal` shape (the `add_one` exemplar), it emits `lea rax, [rdi + N] ; ret` into `InstructionSideTable`. Phase-5 simplification: only single-parameter `fn (x : u64) -> x + N` shape is wired here.

## Acceptance criteria
- [ ] The walker recognises the `Lambda → App(+, Var(arg0), Literal(n))` shape
- [ ] Emits two `Instruction` entries into `InstructionSideTable`: `Lea` and `Ret`
- [ ] For body `fn (x : u64) -> x` (identity), emits `mov rax, rdi ; ret` instead
- [ ] For body `fn (x : u64) -> x + x` (double), emits `lea rax, [rdi + rdi*1] ; ret`
- [ ] Three unit tests: `add_one` → `48 8d 47 01 c3`; identity → `48 89 f8 c3`; double → `48 8d 04 3f c3`
- [ ] The walker records `(lambda_node_id → first_instruction_offset)` in `EmitPassState.function_offsets`
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
`crates/paideia-as-elaborator/src/emit_walker.rs`, possibly adjustments to `crates/paideia-as-encoder/src/encode_instruction.rs`

## Dependencies
m1-002

## Estimated size
S

## Unblocks paideia-os
paideia-os#1 (indirectly; m6 is the direct unblock)

## Cross-repo escalation source
none
""",
    },
    # m1-004
    {
        "id": "m1-004",
        "title": "elaborator: EmitWalker handles IrKind::Unsafe — delegate to UnsafeWalker (m3)",
        "milestone": "phase-5-elab-lowering",
        "labels": "phase:5,area:emit-activation,type:feature,size:XS,priority:critical",
        "body": """## Summary
When the walker enters an `IrKind::Unsafe`, it does not emit anything itself — instead, it records the node ID into `EmitPassState.pending_unsafe_blocks: Vec<IrNodeId>` so m3's `UnsafeWalker` can resolve the block's parsed instruction stream in a follow-up pass.

## Acceptance criteria
- [ ] On `enter_node` for `IrKind::Unsafe`, the walker appends `node_id` to `EmitPassState.pending_unsafe_blocks`
- [ ] `EmitPassState::take_pending_unsafe()` drains and returns the vector
- [ ] Unit test: walking an IR with two `IrKind::Unsafe` nodes records both IDs in declaration order
- [ ] The walker does not attempt to inspect the unsafe block's contents
- [ ] cargo test --workspace green

## Files
`crates/paideia-as-elaborator/src/emit_walker.rs`

## Dependencies
m1-001

## Estimated size
XS

## Unblocks paideia-os
paideia-os#1 (indirectly)

## Cross-repo escalation source
none
""",
    },
    # m1-005
    {
        "id": "m1-005",
        "title": "elaborator: chain EmitWalker into cmd_build and propagate diagnostics",
        "milestone": "phase-5-elab-lowering",
        "labels": "phase:5,area:emit-activation,type:feature,size:S,priority:critical",
        "body": """## Summary
Activate `EmitWalker` in the `paideia-as build` pipeline alongside the existing `LinearityWalker` / `EffectRowWalker` / `CapWalker` chain in `crates/paideia-as/src/cmd_build.rs`. Diagnostics from `EmitWalker` route into the same `walker_sink: VecSink`.

## Acceptance criteria
- [ ] `cmd_build.rs` allocates an `EmitWalker` and walks it over `lowering.ir`
- [ ] The walker's populated `InstructionSideTable` survives into the emit step
- [ ] On an empty `.pdx` source the chain produces zero diagnostics and an empty `InstructionSideTable`
- [ ] On `examples/01_hello.pdx` (4 `let` bindings) the table gets 4 entries
- [ ] On `examples/02_functions.pdx` (4 functions) the table gets ~8 entries
- [ ] The check subcommand path is NOT modified — `EmitWalker` is build-only
- [ ] cargo test --workspace green
- [ ] Test count strictly grew

## Files
`crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as-elaborator/src/lib.rs`

## Dependencies
m1-002, m1-003, m1-004

## Estimated size
S

## Unblocks paideia-os
paideia-os#1 (indirectly)

## Cross-repo escalation source
none
""",
    },
]

def run_cmd(cmd):
    """Run a shell command and return stdout."""
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=30)
        return result.stdout.strip(), result.returncode
    except Exception as e:
        print(f"Error running command: {cmd}", file=sys.stderr)
        print(f"Exception: {e}", file=sys.stderr)
        return "", 1

def issue_exists(title):
    """Check if an issue with the given title already exists."""
    cmd = f'gh issue list --repo {REPO} --state all --search "in:title \\"{title}\\"" --limit 5 2>/dev/null | grep -F "{title}" | head -1'
    output, _ = run_cmd(cmd)
    return bool(output)

def get_issue_number(title):
    """Get the issue number for a given title."""
    cmd = f'gh issue list --repo {REPO} --state all | grep "{title}" | awk "{{print $1}}" | head -1'
    output, _ = run_cmd(cmd)
    return output if output else None

def create_issue(task_id, title, body, labels, milestone):
    """Create a GitHub issue."""
    # Check if exists
    if issue_exists(title):
        existing = get_issue_number(title)
        print(f"  ⊘ {task_id}: {title} (already exists as #{existing})")
        if existing:
            print(f"{task_id}\t{existing}\t{title}\t{milestone}\tN/A", file=sys.stderr)
        return existing

    # Create the issue
    cmd = f"""gh issue create --repo {REPO} --title "{title}" --body "{body.replace('"', '\\"')}" --label "{labels}" --milestone "{milestone}" 2>&1"""
    output, code = run_cmd(cmd)

    if code == 0:
        # Extract issue number from output (format: https://github.com/...)
        issue_num = output.split('/')[-1].strip()
        print(f"  ✓ {task_id}: #{issue_num} {title}")
        print(f"{task_id}\t{issue_num}\t{title}\t{milestone}\tS", file=sys.stderr)
        return issue_num
    else:
        print(f"  ✗ Failed to create {task_id}: {output}", file=sys.stderr)
        return None

def main():
    print("=== Phase 5 GitHub Issue Bootstrap ===\n")

    # Initialize issue map
    if not ISSUE_MAP_PATH.exists():
        with open(ISSUE_MAP_PATH, 'w') as f:
            f.write("Task\tIssue\tTitle\tMilestone\tSize\n")

    # Create issues
    print("Creating 38 Phase 5 issues...")
    for i, task in enumerate(TASKS, 1):
        if i > 1 and i % 10 == 1:
            print(f"\n  (processing {i}/38)...\n")
        create_issue(
            task["id"],
            task["title"],
            task["body"],
            task["labels"],
            task["milestone"]
        )

    print("\n✓ Bootstrap script for m1 created (38 total tasks defined)")
    print("Note: Script is incomplete - only m1 issues created so far")
    print(f"Issue map: {ISSUE_MAP_PATH}")

if __name__ == "__main__":
    main()
