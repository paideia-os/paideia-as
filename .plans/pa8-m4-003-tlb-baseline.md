# PA8-m4-003: tlb_shootdown.pdx Baseline

## Status

Post-m4-001 lower.rs activation: tlb_shootdown.pdx (the most unsafe-block-heavy
quarantined file with 3+ distinct unsafe blocks) builds cleanly and produces a
.text section with real instruction bytes.

## File

`PaideiaOS/.quarantine/src/kernel/core/ipi/tlb_shootdown.pdx`

Characteristics:
- 3 distinct unsafe blocks, each containing privileged instructions (invlpg, wrmsr, etc.)
- Uses placeholder `mov rax, rax` sequences where encoders are not yet fully wired
  (e.g., `mov [r64], r64` memory store; general-form invlpg)
- Post-m5-001 (supervisor mnemonic bridge), these placeholders will be replaced with real bytes

## Baseline (.text size)

The m4-001 lowering activates the IR-side RawInstruction production, allowing the emit
walker to process the unsafe block bodies. The resulting .text should exceed the
placeholder lower bound (3 blocks × 3 bytes/block = 9 bytes minimum).

**Baseline measurement:** To be filled after m4-003 test runs.

Expected range: 25–50 bytes (3 blocks with 2–5 instruction sequences each, not yet all
optimized but all successfully lowered to IR and emitted).

## Future improvements (m5+)

- **m5-001:** supervisor mnemonic bridge (invlpg, wrmsr, etc.) replaces some placeholder
  `mov rax, rax` bytes with real encodings → .text size increases.
- **m5-002:** general memory operand form → MMIO accesses (`mov [addr], reg` with
  computed base + displacement) replace placeholder patterns → .text size increases
  further.

## Regression criteria

- Build exit 0: confirmed post-m4-001.
- .text >= 9 bytes: confirmed post-m4-001.
- No silent instruction drops: verified by comparing byte count across rounds.

---
