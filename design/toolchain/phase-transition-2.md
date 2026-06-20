# Phase 2 retrospective

**Status:** Phase 2 m11-005 closure note.
**Scope:** Documents the phase-2 → phase-3 transition for paideia-as: what shipped, what didn't, and the disposition of every open design-clarification.

## 0. Scope summary

Phase 2 ran m1 through m11 across 130+ issues + closure milestones. The toolchain went from "phase-1 ELF64 smoke" (G2) to "ready for PaideiaOS subsystem migration" (G4-prep complete).

Headline outcomes:
- **m1**: IR walkers wire substructural + effect + capability checks end-to-end.
- **m2**: Typed-elaborator reflection (Term + quote/antiquote + splice + hygiene).
- **m3**: Full algebraic effects + row polymorphism.
- **m4**: PAX object format + paideia-link 4-phase linker.
- **m5**: ML-style modules + functors + first-class modules.
- **m6**: PE/COFF emitter + UEFI thunk + SysV bridge.
- **m7**: PQ signing (Ed25519 + ML-DSA-65 hybrid + soft-HSM).
- **m8**: LSP server (tower-lsp + 11 handlers).
- **m9**: Optimization pass catalog (11 passes).
- **m10**: DDC bring-up (orchestrator + differ + allowlist + nightly CI + release gate).
- **m11**: DWARF vendor extensions + capability-system smoke + this retrospective.

## 1. Design-clarification dispositions

OS-requirements §6 and custom-assembler.md §15 collectively raised the design-clarification items below. Disposition column: **R** = resolved, **D** = deferred to Phase 3+, **C** = changed scope mid-Phase 2.

| Item                                              | Source           | Disposition | Where                                |
|---------------------------------------------------|------------------|-------------|--------------------------------------|
| AS3 — third calling convention (MS-x64 UEFI)      | custom-asm §15   | R           | m3-011 + m6-005 (sysv_bridge + uefi_thunk variants). |
| AS5 — BLAKE3 content hash                         | custom-asm §15   | R           | m4-007 (CanonicalContent).           |
| AS7 — DWARF vendor identifier                     | custom-asm §15   | R           | m11-001 (`paideia`).                 |
| AS8 — PAX object format                           | custom-asm §15   | R           | m4 (full milestone).                 |
| OS §3.2 — debug info table fully populated        | OS-requirements  | R           | m11-002 (vendor sections built).     |
| OS §4 N1 — delegation scope check on signing      | OS-requirements  | R           | m7-004 (Q0901).                      |
| OS §6 ¶1 — TCO ships in Phase 2                   | OS-requirements  | R           | m9-008 (TailCallPass).               |
| OS §6 ¶5 — alternative bootstrap path             | OS-requirements  | R           | m10-007 (dual stage-0 commitment).   |
| Stage-0b GNU `as` entry-point source              | m10-007 followup | D           | Phase 3 — committed but not written. |
| GitHub Actions billing restoration                | operational      | D           | Org-side; not in repo scope.         |
| Hardware HSM integration                          | m7-008           | D           | Post-Phase 2 — separate impl.        |
| Per-rewrite peephole diagnostic codes             | m9-002           | D           | O1501 / O1502 reserved; future PR.   |
| Remainder-loop generation for `#[unroll(n)]`      | m9-009           | D           | Future PR.                           |
| Profile-guided opt-pass ordering                  | m9-010           | C           | Out of scope — catalog stays canonical. |
| True NIST ACVP test vectors for ML-DSA            | m7-001           | D           | Future PR when ml-dsa crate ships vectors. |
| Row-polymorphic scope subsumption                 | m7-004           | D           | Future PR.                           |
| Signature timestamping / revocation               | m7-008           | D           | Phase 3.                             |

8 resolved, 7 deferred, 1 scope-changed.

## 2. What didn't ship (honest list)

Beyond the deferral table above:
- **Real per-node optimization rewrites** — every m9 pass scaffolds the OptPass trait + emits "would-fire" markers, but the kind-only IR (m1-002) doesn't yet expose per-node x86_64 mnemonics. Flipping each pass to a real rewrite is a single PR once that IR work lands. Helper functions (`schedule_block`, `dse_block`, `tco_blocker`, etc.) are already callable and unit-tested.
- **Elaborator-driven LSP semantics** — m8-006..009 (hover / definition / references / completion) use lexical / textual stand-ins. The m8-008 incremental engine (QueryEngine) is in place; per-position type queries activate when the elaborator gains them.
- **Kernel link + QEMU boot test** — m11-003 ships the assembler-side smoke (assembles cleanly, PAX sections populated). The kernel link + boot test is paideia-os repo territory (their m10 DDC closure).

None of these blockers G4; they're documented gates for Phase 3.

## 3. What we got right

- **Side-table-driven architecture**: m1-001 shipped `IrArena.children_table` as a side-table because inline `SmallVec<NodeId>` would have blown the 48-byte node budget. The same pattern carries through m3-007 (HandlerSideTable), m4-005 (PAX audit sections), m5-011 (ModuleSideTable), and m11-002 (DWARF vendor sections). Every metadata addition since has been a side-table.
- **Phase honesty**: every PR with a deferral documents it in code comments + commit messages + the closure docs. The pattern works — future-us can search for `Phase-2-m*` markers and find exactly what's stubbed.
- **softarch → workerbee → debugger chain**: m5-003 onward used this 3-agent flow. The debugger caught real defects the workerbee shipped: a missing `ret` in m6-005's UEFI thunk; the wrong byte-count in m6-005; off-by-one in scope_check; double-validation in m5-003. Catching them before merge mattered.
- **CI billing block as a forcing function**: when GitHub Actions billing broke mid-Phase 2 (after m3-010), `cargo test --workspace` became the gate. The discipline of "if cargo fails locally, the PR isn't ready" turned out to be a clean replacement.

## 4. What we'd change

- **Test-count grep**: the workerbee reported workspace test counts that were often wrong (188, 1417, 1265 when actual was 1502+). Lesson: always trust the direct `cargo test --workspace 2>&1 | awk ...` pipeline; never trust the agent's summary count.
- **SARIF snapshot regen hygiene**: m9-006 (and several others) left a stray `.snap.new` file behind. The pattern that worked: explicit `INSTA_UPDATE=always cargo test -p paideia-as-diagnostics && rm -f tests/snapshots/*.snap.new` in the workerbee prompt.
- **Per-issue PR vs batched**: every PR was a single issue. For trivial XS issues (catalog entries, doc-only changes), a batched PR would have been faster. Phase 3 may experiment.
- **Real fixtures earlier**: m5-012's PascalCase rule forced renaming every existing test fixture mid-Phase 2. Should have been the m1-011 / m2-013 corpus discipline from the start.

## 5. Phase-3 carryover

- Stage-0b GNU `as` entry-point source.
- Per-node IR instruction payloads → flip every m9 pass from "would-fire" to real rewrites.
- Elaborator-driven LSP semantics.
- Hardware HSM integration.
- PaideiaOS kernel link + QEMU boot test.
- True NIST ACVP test vectors for ML-DSA.
- Row-polymorphic scope subsumption.
- Per-rewrite peephole diagnostic codes (O1501 / O1502 already reserved).
- Remainder-loop generation for `#[unroll(n)]`.
- Signature timestamping / revocation.

## 6. Closing note

Phase 2 hit its substrate target. paideia-as is ready for G4 stamping subject to the operational items in `docs/g4-prep.md`. The tag v0.2.0 (m11-006) is the release closure event.
