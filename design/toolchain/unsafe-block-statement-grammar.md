# Unsafe Block Statement Grammar

**Issue**: #1077, #1088  
**Phase**: R17 (Phase 7 m4)  
**Status**: Call-expression statements routed; Let/While/Loop/Return deferred

## Decision

Unsafe blocks accept the **full statement language** grammatically. The parser and lowering already support all statement kinds as children of Unsafe blocks. However, not all statement kinds are yet routable at IR emit time.

## Mechanism

1. **Parser and lowering** (crates/paideia-as-parser, crates/paideia-as-elaborator):
   - Parser accepts any statement inside an unsafe block (as per the surface grammar).
   - Lowering lowers each statement to IR children under the Unsafe node.

2. **Unsafe block handling** (crates/paideia-as-elaborator):
   - UnsafeWalker processes `StmtLabel` and `StmtInstruction` statements only.
   - Other statement kinds (StmtExpr, StmtLet, StmtLoop, StmtWhile, StmtReturn) are queued as pending actions.
   - After UnsafeWalker emits raw instructions, EmitWalker's `emit_pending_unsafe_bodies` processes pending actions via the standard IR emit pipeline.

3. **Unsupported statement kinds** fire U1614 diagnostics as a fallback, rather than silently dropping code.

## Routing Status

| Statement Kind | Current | Routed | Next Issue |
|---|---|---|---|
| `StmtInstruction` (asm mnemonic) | ✓ | UnsafeWalker | — |
| `StmtLabel` (label declaration) | ✓ | UnsafeWalker | — |
| `StmtExpr` (call expression) | ✓ | IR emit pipeline | #1088 |
| `StmtLet` (binding) | ✗ | — | #1089 |
| `StmtLoop` (loop) | ✗ | — | #1090 |
| `StmtWhile` (while) | ✗ | — | #1091 |
| `StmtReturn` (return) | ✗ | — | #1092 |

## Rationale

Keeping unsafe blocks orthogonal to the surface language maintains semantic consistency: the language accepts a statement anywhere in the source, and unsafe blocks should accept it too — they just emit different bytecode paths. Artificially restricting unsafe blocks to asm-only would create a "second-class statement" category, leading to user confusion and future conflicts as more statement kinds become routable.

## Implementation Notes

- U1614 diagnostics remain as a safety net for truly unroutable IR kinds (e.g., if a lowering bug produces an unexpected node type).
- The two-phase approach (UnsafeWalker → EmitWalker) keeps instruction emission isolated from action emission, preserving the RISC-like simplicity of UnsafeWalker.
