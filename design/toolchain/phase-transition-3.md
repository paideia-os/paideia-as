# Phase 3 retrospective

**Status:** Phase 3 m9-001 closure note.
**Scope:** Documents the Phase 3 → Phase 4 transition for paideia-as: what shipped, what didn't, and the disposition of every open design-clarification.

## 0. Scope summary

Phase 3 ran m1 through m9 across 56 enumerated issues (plus 3 cross-cutting), PRs #475–#587. The toolchain advanced from "Phase 2 substrate complete (G4-ready)" to "pointer-types + per-node IR + opt real-rewrites + elaborator-driven LSP + dual-stage-0 bootstrap + hardware-HSM landings".

Headline outcomes:
- **m1** (pointer types + raw memory): `*T` in the type grammar; `index_*` + `ptr_sub*` intrinsic families; `RawMem` effect + `paideia.raw_mem` capability; `IrKind::Load`/`Store` + side-table; SIB-form encoder; examples 15/16/17 dramatic shrinkage.
- **m2** (per-node IR payload): `Instruction` schema + `InstructionSideTable`; `encode_instruction` mnemonic ↔ encoder bridge; populate-path elaborator chokepoint; opt-pass helper signatures migrated.
- **m3** (opt-pass real-rewrites): 5/10 passes shipped real rewrites (peephole / schedule / dse / encode-tight / tailcall); 5/10 ship as documented would-fire with named m4 deferrals.
- **m4** (elaborator-driven LSP): PositionIndex + NameResolutionTable + per-handler ports for hover / definition / references / completion / inlay; QueryEngine.invalidate_module; m8-014 latency probe reactivated.
- **m5** (stage-0b GAS source): `src/toolchain/stage-0/entrypoint.s`; dual-stage-0 byte verification in `tools/ddc/run.sh`; G4 prep §5 Stage-0b row checked.
- **m6** (hardware HSM integration): PKCS#11 + YubiHSM2 backends; HybridSigner composer; Q0902 opt-in contract; hardware-lane test corpus.
- **m7** (substructural + effects cleanup): S0902 / S0904 / S0905 wired; row-polymorphic scope subsumption (closes the m7-004 D-row).
- **m8** (signature lifecycle): RFC 3161 timestamping client + revocation list + ACVP-vector status (deferral documented).
- **m9** (documentation closure): this retrospective + STATUS.md update + examples README refresh + v0.3.0 tag.

## 1. Design-clarification dispositions

Phase 3 retired or kept the deferrals catalogued in `phase-transition-2.md` §1. Disposition column: **R** = resolved, **D** = deferred to Phase 4+, **C** = changed scope mid-Phase 3.

| Item                                              | Source           | Disposition | Where                                |
|---------------------------------------------------|------------------|-------------|--------------------------------------|
| Per-node IR instruction payloads                  | phase-trans-2 §2 | R           | m2-001..006 (Instruction schema)     |
| m9 "would-fire" → real-rewrite flip               | phase-trans-2 §2 | R (5/10)    | m3-001..009; 5/10 ship real, 5/10 land at m4+ |
| Elaborator-driven LSP semantics                   | phase-trans-2 §2 | R (gated)   | m4-001..007 (lookup paths wired; walker-side population is wider m4) |
| Hardware HSM integration                          | phase-trans-2 §5 | R           | m6-001..005                          |
| Stage-0b GNU `as` entry-point                     | phase-trans-2 §5 | R           | m5-001..003                          |
| Row-polymorphic scope subsumption                 | phase-trans-2 §5 | R           | m7-004 / PR #581                     |
| `*T` raw pointer types                            | phase-3-plan §m1 | R           | m1-001..013                          |
| `RawMem` effect + `paideia.raw_mem` capability    | phase-3-plan §m1 | R           | m1-005                               |
| RFC 3161 timestamping                             | phase-3-plan §m8 | R (scaffold) | m8-001 — runtime fetch deferred to m8-005 follow-up |
| Revocation list format + check                    | phase-3-plan §m8 | R           | m8-002                               |
| NIST ACVP test vectors for ML-DSA                 | phase-trans-2 §5 | D           | m8-003 — task stays open per its own AC |
| Borrowed references (`&T`, `&mut T`)              | phase-3-plan §15 | D           | Region calculus + borrow-checker is multi-milestone work; out of Phase 3 scope by design |
| YubiHSM2 PQ firmware support                      | phase-3-plan §15 | C           | Resolved via hybrid-fallback rule (Ed25519 hardware + ML-DSA-65 soft); Q0902 surfaces gap |
| Profile-guided opt-pass ordering                  | phase-trans-2 §1 | C           | Catalog stays canonical (carried from phase-trans-2) |
| Signature timestamping / revocation               | phase-trans-2 §5 | R           | m8-001 / m8-002 (split into two issues) |
| Per-rewrite peephole diagnostic codes             | phase-trans-2 §5 | D           | O1501/O1502 used in m3-001; per-rewrite-code expansion deferred |
| Remainder-loop generation for `#[unroll(n)]`      | phase-trans-2 §5 | R (gated)   | m3-006 — diagnostic shipped; arena mutation pending loop-entry markers |
| Elaborator call-resolution chokepoint             | phase-3-plan §m1 | R           | m2-003 (populate path lands the chokepoint)              |

12 resolved, 2 deferred, 2 scope-changed, 2 resolved-with-gating-note.

## 2. What didn't ship (honest list)

Beyond the deferral table:

- **Per-node populate for the remaining IR kinds** — m2-003 ships Load/Store recognition; intrinsic-call detection (m1-004 lookup_intrinsic in the populate path) requires Call-node introspection that doesn't exist at the IR layer yet. Lands when the elaborator gains call-resolution chokepoint deeper than m2-003.
- **Walker-side PositionIndex / NameResolutionTable population** — m4 ships the lookup paths; the inserts inside each linearity / effect / capability walker are wider m4 activity. Per-handler lsp-harness tests are gated until population lands.
- **Macro-fusion / branch-hint / align / pool-constants real rewrites** — m3-007 ships diagnostic-only would-fire. Real EncodingHint flagging / prefix emission / `.align` insertion / constant-pool wiring lands at m4 (encoder integration + emit-stage layouts + paideia-link).
- **Real TSA HTTP fetch for RFC 3161** — m8-001 ships synthetic-token scaffold gated on `--tsa-url`. Real fetch needs `reqwest` runtime dep; follow-up PR.
- **Actual stage-0 byte-comparison in CI** — m5-002 wires the comparison in `tools/ddc/run.sh`; the workflow is disabled at the org level (GitHub Actions billing block from phase-trans-2 §1). Activation pairs with billing restoration.
- **PE/COFF / DWARF emitter parity for the new IR payload** — m2 populates the table for ELF emission; PE/COFF emit-stage hasn't been threaded through. Phase 4 work.
- **Walker for the Match-arm linearity check (m7-002 base)** — S0904 has the catalog entry + the detection function + fixtures, but the actual Match-arm walker hookup is gated on the IR's Match-node walk surface. Same shape as S0905 / S0902-shadow.

None of these blocks G5 (Phase 4's decision gate); they're documented Phase 4 entry points.

## 3. What we got right

- **The autonomous loop tempo**: completing one milestone before pausing was overridden by the user mid-Phase-3 with "do not stop between milestones." The continuous loop ran through 56 milestone issues without interruption. The trade-off: heavy reliance on the workerbee + debugger pattern; the user reviews PRs in batches post-loop rather than per-issue.
- **The side-table pattern**: every new metadata addition since m1-001 stayed in dedicated side-tables (children_table → HandlerSideTable → ModuleSideTable → LoadStoreSideTable → InstructionSideTable). IrNodeData has never breached its 48-byte budget. The pattern composes — m3 passes consume InstructionSideTable transparently.
- **Phase honesty markers**: every PR that ships scaffolded behaviour documents the gating explicitly (`Phase-3-m1-007 minimum:`, `would-fire pending m4 wiring`, `populate path TODO`). Future-us can grep for `Phase-3-m\d+-\d+` and find the gate.
- **The hybrid-fallback resolution for YubiHSM2**: rather than refusing to support YubiHSM2 because of its PQ-firmware gap, m6-002 / m6-003 shipped the hybrid composition with explicit operator opt-in (`--opt-in-hybrid-fallback`) and a dedicated diagnostic (Q0902). Pragmatic + honest.
- **The dual stage-0 byte-identity** at exactly `48 8d 47 01 c3`. The five-byte `.text` is the cleanest possible Wheeler-CTTTDC demonstration: it's small enough that the byte-identity is observable by hand.

## 4. What we'd change

- **The SARIF regen discipline**: m7-002's PR landed without regenerating the SARIF snapshot, dropping `cargo test --workspace` from 1810 to 208 because of the insta short-circuit. A fix-up PR (#579) restored green. Lesson: catalog edits should hit `INSTA_UPDATE=always cargo test -p paideia-as-diagnostics && rm -f tests/snapshots/*.snap.new` as part of every commit step.
- **The workerbee's test-count reporting**: continued to be unreliable (one PR reported "1467", another "208", another "1812" — actual was somewhere between). The `cargo test --workspace 2>&1 | awk '{sum+=$4} END {print sum}'` pipeline remains the source of truth.
- **The fix-up PR pattern for m7-002**: a SARIF regen probably should have rolled into the m7-003 PR rather than spawn its own (now-merged) #579. Cleaner: include the regen in every catalog-touching PR.
- **The PKCS#11 / YubiHSM2 deps are stubbed**: real cryptoki + yubihsm crate integration would have been better landed inside m6 rather than deferred. The hardware-lane corpus tests are #[ignore]'d because the runtime libs aren't loadable today. Activating the lanes requires the real crate integrations.
- **Walker hookups continue to be the gating bottleneck**: m1-004 (intrinsic resolver), m2-003 (per-AST-kind populate), m4-002..006 (per-walker position inserts), m7-002 (Match-arm walker) all share the same gating constraint — the IR's per-kind walk surface needs to grow. Phase 4 should bundle these as a single milestone rather than scatter them across m3-m7 honesty notes.

## 5. Phase-4 carryover

- Per-node populate for the remaining IR kinds (Call / Match / Handle / Branch).
- Walker-side population for PositionIndex + NameResolutionTable.
- Macro-fusion / branch-hint / align / pool-constants real rewrites at the encoder + emit-stage.
- Real TSA HTTP fetch (RFC 3161, reqwest integration).
- PE/COFF + DWARF emitter parity for the m2 InstructionSideTable.
- PKCS#11 / YubiHSM2 runtime crate integration.
- ACVP test vectors for ML-DSA-65 (when upstream ships).
- Borrowed references (`&T`, `&mut T`) + region calculus + borrow checker — a dedicated milestone, likely Phase 4 m1.
- Per-rewrite peephole O-code expansion (O1501/02 reserved; per-rule codes a Phase-4 task).
- Workflow re-enablement (depends on GitHub Actions billing restoration).
- Stage-0b GAS-syntax parsing variants for non-Intel-syntax operators (currently `.intel_syntax noprefix` is the only configuration).

## 6. Closure

Phase 3 hit its substrate target. paideia-as advances toward G5 stamping subject to the operational items in `docs/g4-prep.md` (Phase 3 reuses Phase 2's G4 framework; Phase 4 will introduce G5). The tag v0.3.0 (m9-004) is the release closure event for Phase 3.
