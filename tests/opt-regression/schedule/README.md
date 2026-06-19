# Instruction Scheduling Optimization Pass Regression Corpus

Test harness for validating instruction scheduling optimization pass (O1503 diagnostics).

Per `optimization-passes.md §2` (reference), the instruction scheduling pass reorders
independent instructions within a basic block to hide latency. Key rules:

1. Loads can move EARLIER (toward the start) to hide latency.
2. Instructions can move past non-barrier independent ones.
3. Reordering stops at any barrier (AtomicLocked, Branch).
4. LOCK-prefixed atomic operations act as memory barriers.

## Corpus Structure

- `corpus/` — Four `.pdx` fixtures covering scheduling scenarios
- `tests/runner.rs` — Validates corpus has exactly 4 fixtures

## Phase-2-m9-003 Status

Fixtures are stubs; the IR is kind-only and doesn't yet carry concrete x86_64
mnemonics. The scheduling pass emits O1503 "would-fire" diagnostics. Future
iterations will wire actual reordering when the per-node instruction payload
is exposed in the IR.
