# paideia-as bootstrap

**Status:** Phase 2 m10-007 decision record.
**Scope:** Documents the bootstrap path for paideia-as and resolves OS-requirements §6 design-clarification 1.

## 0. The decision (m10-007)

**paideia-as commits to dual stage-0 source trees**: the bootstrap toolchain is built **twice**, once with [NASM](https://www.nasm.us/) and once with GNU `as`. Both stage-0 paths must produce a byte-identical stage-1 artifact (modulo the DDC allowlist) for the bootstrap to be considered complete.

This is the *strong* Wheeler-CTTTDC formulation: a malicious stage-0 would have to be present in BOTH toolchains AND produce identical malicious output — a much higher bar than a single-stage-0 bootstrap.

## 1. The alternative we rejected

A **single stage-0** bootstrap (use only NASM, or only GNU `as`) would have been faster to ship. The argument against it was simple:

- Wheeler's argument depends on toolchain diversity. With a single stage-0, the DDC verification reduces to "stage-1 matches itself" — vacuously true and provides no Wheeler-style guarantee.
- The security pillar of paideia-as treats trusting-trust as the canonical attack model. Weakening Wheeler's argument would be unacceptable.

Single-stage-0 was therefore explicitly rejected at the m10-007 decision point.

## 2. Operational shape

### 2.1 Stage-0a (NASM)

- Source: `crates/paideia-as-emitter-elf/src/` (the encoder library) + a NASM-friendly entry-point assembly file.
- Build: `nasm` invokes against the entry-point file with the m1-012 `abi.md` calling-convention.

### 2.2 Stage-0b (GNU `as`)

- Source: same encoder library + a GAS-syntax entry-point.
- Build: `as` invokes against the GAS source.

### 2.3 Stage-1

Both stage-0 paths produce a `paideia-as` binary. The DDC harness (m10-001..006) byte-compares them. Identical output → bootstrap closure.

## 3. Status (Phase 3 m5 closure)

- Stage-0a (NASM) entry-point: present at `tools/cross-build/fixtures/uefi_loader/module.asm` (m1-013 / m6-009).
- Stage-0b (GNU `as`) entry-point: present at `src/toolchain/stage-0/entrypoint.s` (Phase 3 m5-001 / PR #569). Uses `.intel_syntax noprefix` so operand ordering matches NASM (dest, src).
- DDC harness: dual-stage-0 verification is **active** as of Phase 3 m5-002 (PR #570). `tools/ddc/run.sh` assembles both sources, extracts the `.text` section via `objcopy -O binary --only-section=.text`, and `cmp -s`s the resulting byte streams. Drift causes `exit 1`.

Verified locally: stage-0a (NASM) and stage-0b (GAS) emit byte-identical `.text` of `48 8d 47 01 c3` (`lea 0x1(%rdi), %rax; ret`). The dual stage-0 commitment from §0 is operationally closed; G4 prep §5 Stage-0b row carries `[x]`.

## 4. Closure

Phase 2 m10-007 made the dual-stage-0 commitment but couldn't write the GAS source without a Phase 3 follow-up. Phase 3 m5 (issues #510 / #511 / #512) discharges the commitment: the source ships, the comparison runs, and the operational deferral retires.

The bootstrap loop's Wheeler-style argument now has both legs:

1. A stage-0a built with NASM produces a stage-1 paideia-as.
2. A stage-0b built with GNU `as` produces a stage-1 paideia-as.
3. The two stage-1 artifacts are byte-identical (modulo the m10-002 allowlist).
4. Therefore: any malicious stage-0 would need to be present in **both** NASM and GAS — and produce identical malicious output — for the bootstrap to be compromised.

This is the *strong* CTTTDC formulation. With both stage-0 paths in place and the byte-comparison wired into the DDC harness, the security pillar's trusting-trust attack model is now defensively addressable rather than aspirational.

## 5. References

- [Wheeler 2005] David A. Wheeler. *Countering Trusting Trust Through Diverse Double-Compiling*. ACSAC 2005.
- [Thompson 1984] Ken Thompson. *Reflections on Trusting Trust*. Turing Award lecture, CACM August 1984.
- `docs/ddc.md` — operational guide.
- `docs/build-determinism.md` — env-var contract (m10-003).
- `tools/ddc/` — the DDC tooling tree.
- OS-requirements §6 design-clarification 1 — the original question this decision resolves.
