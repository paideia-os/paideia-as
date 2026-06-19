# Peephole Optimization Pass Regression Corpus

Test harness for validating peephole optimization pass (O1500 diagnostics).

Per `optimization-passes.md §1.2` (reference), the peephole pass rewrites are:

1. RemoveNopMov: `mov r, r` → eliminate
2. SimplifyZeroAdd: `add r, 0` → eliminate
3. SimplifyZeroSub: `sub r, 0` → eliminate
4. StrengthReduceMul: `mul r, 2` → `shl r, 1`
5. StrengthReduceDiv: `div r, 2` → `shr r, 1` (unsigned)
6. FuseLoadStore: `mov r, [mem]; mov [mem], r` (round-trip) → eliminate
7. CollapseJumpToNext: `jmp label_next` where label_next immediately follows → eliminate
8. CombinePushPop: `push r; pop r` (no intervening) → eliminate

## Corpus Structure

- `corpus/` — Eight `.pdx` fixtures, one per rewrite kind
- `tests/runner.rs` — Validates corpus has exactly 8 fixtures

## Phase-2-m9-002 Status

Fixtures are stubs; the IR is kind-only and doesn't yet carry concrete x86_64
mnemonics. The peephole pass emits O1500 "would-fire" diagnostics. Future
iterations (m9-003+) will wire actual rewrites when the per-node instruction
payload is exposed in the IR.
