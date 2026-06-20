# G4 prep checklist

**Status:** Phase 2 m11-004 closure note.
**Scope:** Decision-gate G4 verification: paideia-as is ready to support PaideiaOS migration of its first subsystem.

## 0. What G4 asserts

G2 (phase-1 close) asserted: paideia-as is self-hosting against its supported source-language subset and produces valid ELF64 objects.

G4 asserts: paideia-as is ready to compile a PaideiaOS subsystem (the capability system is the m11-003 smoke target) including signing, deterministic build, full-fidelity debug info, and editor tooling.

The checklist below is what a reviewer should verify before stamping G4.

## 1. Substrate readiness

For each Phase 2 milestone, the deliverable is:

- [x] **m1** — IR walkers wire substructural + effect + capability checks end-to-end. 14/14 issues closed. PRs #347–#360.
- [x] **m2** — Typed-elaborator reflection (quote / antiquote / Term / splice / hygiene). 13/13. PRs #361–#372.
- [x] **m3** — Full algebraic effects + handlers + row polymorphism. 14/14. PRs #374–#387.
- [x] **m4** — PAX object format + paideia-link 4-phase linker. 13/13. PRs #388–#400.
- [x] **m5** — ML-style modules + functors + first-class modules. 13/13. PRs #401–#413.
- [x] **m6** — PE/COFF emitter + UEFI thunk + SysV bridge. 10/10. PRs #414–#423.
- [x] **m7** — PQ signing (Ed25519 + ML-DSA-65 hybrid). 8/8. PRs #424–#431.
- [x] **m8** — LSP server (tower-lsp + hover / def / refs / completion / formatting / semantic tokens). 14/14. PRs #432–#445.
- [x] **m9** — Optimization pass catalog. 12/12. PRs #446–#457.
- [x] **m10** — DDC bring-up. 8/8. PRs #458–#465.
- [x] **m11** — Phase 2 closure (in progress). PRs #466+.

## 2. Design-clarification dispositions

- [x] **AS3** (third calling convention) — resolved by m3-011 + m6-005 (handler stack + UEFI thunk variant).
- [x] **AS5** (BLAKE3 content hash) — resolved by m4-007.
- [x] **AS7** (DWARF vendor ID) — resolved by m11-001 (vendor ID `paideia`).
- [x] **AS8** (PAX object format) — resolved by m4 milestone (12 module groups).
- [x] **OS §3.2** (debug info table) — resolved by m11-002.
- [x] **OS §4 N1** (delegation scope check) — resolved by m7-004 (Q0901).
- [x] **OS §6 ¶1** (TCO ships Phase 2) — resolved by m9-008.
- [x] **OS §6 ¶5** (alternative bootstrap) — resolved by m10-007 (dual stage-0 commitment).

## 3. Test / harness landscape

- [x] **Workspace tests**: ~1614 across 26+ crates and 23+ test harnesses.
- [x] **Determinism gate corpus**: m10-004 ships 10 fixtures.
- [x] **DDC harness**: m10-001..006 wires nightly + release pipelines (advisory + hard-fail).
- [x] **Capability smoke**: m11-003 fixture exercises every substrate.
- [x] **PQ verification corpus**: m7-007 with 6 happy + 4 failure tests.
- [x] **LSP harness**: m8-014 with 4 correctness + 1 latency probe.

## 4. Documentation

All Phase 2 design docs gain a phase-2-outcome appendix:

- [x] `design/toolchain/custom-assembler.md` — upstream spec (paideia-os repo); local mirror in STATUS.md.
- [x] `design/toolchain/macros-phase1.md` — m2 closure appendix (PR #372 / m2-012).
- [x] `design/toolchain/calling-convention.md` — m3-011 §2.5 (UEFI thunk), m6-005 thunk variant.
- [x] `design/toolchain/paideia-link.md` — m4 closure (created m4-013).
- [x] `design/toolchain/optimization-passes.md` — m9 closure (created m9-012).
- [x] `design/security/pq-trust-root.md` — m7 closure (created m7-008).
- [x] `design/toolchain/bootstrap.md` — m10-007.
- [x] `design/toolchain/debug-info.md` — m11-001.

## 5. Operational items

- [ ] **CI activation** — `ci.yml` / `cross-build.yml` / `ddc.yml` / `release.yml` all disabled today behind the org's GitHub Actions billing block. Hand off to release lead for billing restoration. (Acknowledged limitation — not a G4 blocker because local `cargo test --workspace` is the gate.)
- [x] **Stage-0b (GNU as)** — landed in Phase 3 m5-001 (PR #569) at `src/toolchain/stage-0/entrypoint.s`. m5-002 wires the byte-comparison into `tools/ddc/run.sh` and the existing `.github/workflows/ddc.yml`. Verified locally: stage-0a (NASM) and stage-0b (GAS) emit byte-identical `.text` (`48 8d 47 01 c3`).
- [x] **`paideia-pq-sign hsm init`** — m7-006 ships dev soft-HSM. Production HSM integration is post-Phase-2.

## 6. G4 verdict

Strike the verdict in this section when reviewing:

> _Reviewer note (to be filled in)_: paideia-as Phase 2 deliverables verified; G4 stamped on YYYY-MM-DD.

Until then, the substrate work is complete (m1–m10 closed; m11 closes via the v0.2.0 tag at m11-006). The two operational deferrals (CI activation + stage-0b) are tracked above, both with documented mitigation paths.
