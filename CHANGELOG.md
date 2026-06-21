# Changelog

## v0.4.0 — Phase 4 (substrate expansion for PaideiaOS readiness)

**Released:** Tag pushed at m14-003 closure (this PR).

paideia-as Phase 4 closes fourteen milestones across 101 enumerated issues, PRs #592–#693. PaideiaOS-aware re-ordering applied: m7 → m9 → m10 → m8 → m11 → m1 → m2 → m3 → m4 → m5 → m6 → m12 → m13 → m14.

### Milestones

- **m7 — records + enums** — `struct` types with layout (RecordLayoutTable); pattern bindings + P0199 (refutable-let); record codegen; `enum` sum types with 3 payload shapes (EnumLayoutTable); match exhaustiveness T0512; enum discriminant + match codegen; RecordCons / FieldAccess / EnumCons / EnumDiscriminant IR; corpus regression. Closes records / enums for PaideiaOS kernel data structures.
- **m9 — generics + traits** — `<T>` grammar (P0200); Type::Var with HrKind::Star / Arrow; trait declarations (P0201) + impl blocks (P0202); trait-bound resolution (T0514); coherence (T0513); monomorphisation table; associated types; derive-macro infrastructure (Eq / Hash / Debug). Closes parametric polymorphism for stdlib + PaideiaOS subsystem reuse.
- **m10 — allocator + memory model** — Allocator trait + Layout; BumpAllocator; Arena; SystemAllocator with C1401/C1402 cfg-gates; Box<T>. Q3 dual-default resolved: Arena for PaideiaOS targets, SystemAllocator for host. Closes allocation discipline for kernel-vs-host context.
- **m8 — strings + loops** — string + byte-string literals (E0010/E0011); Type::Str fat pointer; heap String; for / while / loop / break / continue keywords; Loop / Break / Continue IR + LoopMetaTable; m3-006 unroll over explicit loops. Closes the control-flow + text substrate.
- **m11 — stdlib bring-up** — Option / Result / Vec / String + Str ops / HashMap / Stdin/Stdout/Stderr (IO effect + paideia.io capability) / File + Read + Write traits / Iterator + Map/Filter adapters; 135-LoC stdlib-smoke kitchen-sink. Closes the runtime-library surface.
- **m1 — walker hookups** — Call / Match / Handle / Branch walker surfaces; PositionIndex + NameResolutionTable population; macro-fusion / branch-hint / align / pool-constants 4-pass would-fire-to-real-rewrite flip. Closes the Phase 3 m3-007 deferral chain.
- **m2 — encoder real-rewrites** — PE/COFF + DWARF + PAX emitters consume InstructionSideTable; per-emit DDC fixture. Closes Phase-2-m9 honesty-disclaimer chain.
- **m3 — runtime integrations** — real cryptoki PKCS#11 + yubihsm runtime integration; reqwest RFC 3161 TSA fetch (`verify --tsa-token`); hardware-lane activation guide. Closes Phase-3-m6 runtime-deferral.
- **m4 — borrowed references grammar** — `&T` / `&mut T` types + expressions; Type::Ref interner; substructural Affine/Linear; IR Borrow / BorrowMut / Deref + BorrowSideTable; codegen as pointers.
- **m5 — region calculus** — RegionId + RegionGraph + transitive closure; lexical region inference; lifetime-variable surface syntax; per-binding region metadata in PositionIndex; Rust-style elision rules + L2001.
- **m6 — borrow checker** — BorrowWalker (S0906/S0907, renamed from spec'd A0700/A0701), LifetimeWalker (S0908, was A0702), MutationWalker (S0909, was A0703); two-phase borrows for method receivers; NLL precise drop + LastUseAnalyzer; ExtendedBorrowDiagnostic with SARIF relatedLocations; 40-fixture corpus. Closes safe-aliasing discipline for PaideiaOS kernel code.
- **m12 — paideia-as tooling** — `paideia-as test` runner (discovery + listing; execution gates on Phase 5 runtime evaluator); `paideia-as fmt` CLI (file / stdin / --check); `paideia-as doc` HTML generator with cross-reference linking. Package manager deferred to Phase 5+.
- **m13 — self-hosting groundwork** — port-target inventory (21 crates, 3 tiers); m13-002 mini-lexer bootstrap fixture in tests/self-hosting/; Rust-dep gap analysis (10 stdlib expansions identified — SmallVec, Unicode XID, serde/serde_json/toml, BLAKE3, Lru, etc.); stage-1 + DDC fixture; Phase 5 opening conditions.
- **m14 — documentation closure** — phase-transition-4.md retrospective; STATUS.md update; this v0.4.0 tag + CHANGELOG; examples README + stdlib walkthrough refresh.

### Highlights

- **2172 workspace tests** across 29+ crates and 26+ test harnesses (+343 from Phase 3 close at 1829).
- Full borrowed-reference + region + borrow-checker stack ships — paideia-as has a Rust-equivalent safe-aliasing story for PaideiaOS subsystem code.
- Stdlib bring-up (Option / Result / Vec / String / HashMap / Iterator + IO traits) is sufficient for kernel scaffolding and self-host bring-up.
- 18 new diagnostic codes (P0196..P0202, T0511..T0514, S0906..S0909, L2001, C1401..C1402, E0010..E0011, M0900) — every code in its category's reserved range.
- PaideiaOS-mode (no PR / direct-push) workflow eliminated ~50 issues of PR-overhead while keeping the cargo-green gate.
- 20-example tutorial-ordered catalog under `examples/` rewritten mid-Phase to reflect current syntax.
- Self-hosting groundwork: inventory + gap analysis + bootstrap fixture + DDC harness in place for Phase 5 m1 kickoff.

### Operational deferrals (Phase 5 carryover)

- Walker chain activation: per-walker activation in the full elaborator IR walk (m1-005..006 / m6-001..003 walkers are unit-tested but not yet activated globally; lands incrementally as the elaborator threads them).
- CLI parser consolidation: drop the older `-> ret !{Eff}` form OR migrate to the newer `-!{Eff}->` form (lex/parser layer drift exposed by examples rewrite).
- paideia-as build end-to-end for the new surface (gates on walker activation).
- L2001 elision-rule per-fn-signature activation.
- TSA token attachment as .paideia.sig sub-record (Phase 3 m8-001 scaffolded; m4 emit-stage not threaded).
- `record` vs `struct` keyword pick + migration.
- Test execution via Phase 5 runtime evaluator (m12-001 discovers; execution gates on m13).
- 5 stdlib expansions before Tier 1 self-host port: SmallVec, Unicode XID tables, serde-equivalent, BLAKE3, Lru cache.
- paideia-lsp + paideia-pq-sign self-hosting → Phase 6+ (async runtime + crypto crate decisions deferred).
- NIST ACVP test vectors for ML-DSA-65 (#525 stays open per its AC; gates on upstream ml-dsa crate).
- Stage-0b GAS AT&T-syntax variants (current: .intel_syntax noprefix only).

See `design/toolchain/phase-transition-4.md` for the full retrospective + Phase 5 carryover catalogue.

---

## v0.3.0 — Phase 3 (substrate-deferral cleanup)

**Released:** Tag pushed at m9-004 closure (this PR).

paideia-as Phase 3 closes nine milestones across 56 enumerated issues (plus 3 cross-cutting). Issue #525 (NIST ACVP test vectors for ML-DSA-65) intentionally stays open per its own AC pending upstream `ml-dsa` crate support. PRs #475–#589.

### Milestones

- **m1 — pointer types + raw memory** (PRs #475–#487) — `*T` in the type grammar; `index_*` + `ptr_sub*` intrinsic families (40 entries); `RawMem` effect + built-in `paideia.raw_mem` capability; `IrKind::Load`/`Store` + `LoadStoreSideTable`; SIB-form encoder (`48 8b 04 cf` etc.); examples 15/16/17 retired to compiles-end-to-end status. Closes the Phase 2 §15 borrowed-references open question with the documented deferral.
- **m2 — per-node IR payload** (PRs #488–#493) — `Instruction { mnemonic, operands, encoding_hint }` schema + `InstructionSideTable` keyed by `IrNodeId` (mirrors m3-007 `HandlerSideTable` / m1-006 `LoadStoreSideTable`); `encode_instruction` dispatch entry with iced-x86 round-trip tests; elaborator populate chokepoint; opt-pass helper signatures migrated to consume `&InstructionSideTable`.
- **m3 — opt-pass real-rewrites** (PRs #494–#502) — 5 passes ship real rewrites: peephole (5/8 rules), schedule, dse, encode-tight, tailcall (structural). 4 passes ship as documented would-fire pending m4 encoder/emit-stage integration: macro-fusion, branch-hint, align, pool-constants. Per-pass regression corpus `tests/opt-regression/`.
- **m4 — elaborator-driven LSP** (PRs #503–#509) — `PositionIndex` + `NameResolutionTable` side-tables; lookup paths wired through hover, definition, references, completion, inlay-hints handlers. m8-014 latency probe reactivated. Per-walker population (the insert side) lands incrementally as the walkers grow.
- **m5 — stage-0b GAS source** (PRs #569–#571) — `src/toolchain/stage-0/entrypoint.s` (`.intel_syntax noprefix`) is 1:1 with the NASM stage-0a entry-point; `tools/ddc/run.sh` byte-compares both `.text` sections (verified locally: `48 8d 47 01 c3`). `docs/g4-prep.md` §5 Stage-0b row flips checked. Activates the dual stage-0 Wheeler-CTTTDC argument.
- **m6 — hardware HSM integration** (PRs #572–#576) — `Pkcs11Signer` (cryptoki backend), `YubiHsmSigner` (with the hybrid-fallback rule for YubiHSM2's missing PQ firmware), `HybridSigner<H, S>` composer, `HsmSigner` trait with `is_hardware()`, `Q0902 hsm-no-pq-support` diagnostic, hardware-lane test corpus (`#[ignore]`'d). Runtime crate integrations (real cryptoki / yubihsm sessions) deferred.
- **m7 — substructural + effects cleanup** (PRs #577–#582) — S0902 (linear let-shadow), S0904 (affine consumed across match arms), S0905 (ordered out of order across handler) wired with detection logic + reject corpus fixtures. Row-polymorphic scope subsumption (`check_scope_subsumption_with_row_poly`) closes the m7-004 D-row from `phase-transition-2.md` §1.
- **m8 — signature lifecycle** (PRs #583–#586) — RFC 3161 timestamping client (`paideia-pq-sign::timestamp` + CLI subcommand); JSON-lines revocation list + `verify --revocation-list --ignore-revocation`. #525 (NIST ACVP test vectors) stays open per its own AC; upstream tracking in `tests/pq-corpus/ML_DSA_ACVP_STATUS.md`.
- **m9 — documentation closure** (PRs #587–#590) — `design/toolchain/phase-transition-3.md` retrospective; STATUS.md update; examples README refresh; this v0.3.0 tag.

### Highlights

- **1829 workspace tests** across 27+ crates and 24+ test harnesses (+215 from Phase 2 close at 1614).
- 12 design-clarification deferrals resolved; 2 stay deferred; 2 scope-changed; 2 resolved-with-gating-note.
- The dual stage-0 Wheeler-CTTTDC argument has both legs operational; `tools/ddc/run.sh` byte-compares.
- Hybrid PQ signing gains lifecycle handles (timestamping + revocation) and hardware HSM backends.
- Pointer types retire the most common Phase 2 `unsafe` wrappers; example 17_strlen has zero unsafe escapes.

### Operational deferrals (Phase 4 carryover)

- **Walker-side IR insert points**: PositionIndex / NameResolutionTable / Instruction populate paths cover the m2-003 minimum (Load / Store) but not the full IR-kind tree. Per-walker inserts during linearity / effect / capability walks land at Phase 4.
- **Real cryptoki / yubihsm runtime integrations**: m6 ships the scaffolds + the `HsmSigner` trait; live-device exercise needs the runtime crates plus operator validation.
- **Macro-fusion / branch-hint / align / pool-constants real rewrites**: ship as would-fire; activation lands at the m4 encoder/emit-stage integration.
- **RFC 3161 TSA HTTP fetch**: m8-001 ships synthetic-token scaffold; real fetch needs `reqwest`.
- **GitHub Actions billing restoration**: CI workflows still disabled at the org level from Phase 2; activation pairs with billing restoration.
- **NIST ACVP test vectors for ML-DSA-65**: #525 stays open until upstream `ml-dsa` crate ships them.

### Documentation

Phase 3 ships per-milestone closure appendices:

- `design/toolchain/phase-transition-3.md` (m9-001) — the retrospective.
- `design/toolchain/pointer-types-phase3.md` (m1-013) — pointer types catalogue.
- `design/toolchain/per-node-ir-payload-phase3.md` (m2-006) — IR schema + side-table catalogue.
- `design/toolchain/optimization-passes.md` Phase-3-m3 closure section (m3-009).
- `design/toolchain/lsp-phase3.md` (m4-007) — m8 + m4 LSP architecture.
- `design/toolchain/bootstrap.md` §3-§4 Phase 3 closure (m5-003).
- `design/security/pq-trust-root.md` Phase 3 m6 + m7 + m8 sections (m6-005, m7-005, m8-004).
- `tests/pq-corpus/ML_DSA_ACVP_STATUS.md` (m8-003) — open-issue tracker.
- `docs/release-signing.md` Hardware HSM backends section (m6-004).
- `docs/g4-prep.md` §5 Stage-0b row checked (m5-002).

### Decision gate

G4 was stamped during Phase 2 m11-004 prep; G5 (the Phase 3 closure gate) follows the same framework. The Phase 4 plan will introduce G5's formal checklist.

## v0.2.0 — Phase 2 (substrate complete)

**Released:** Tag pushed at m11-006 closure.

paideia-as Phase 2 ships the full substrate for PaideiaOS subsystem migration. Eleven milestones across 130+ closed PRs (#347–#470). The toolchain went from "phase-1 ELF64 smoke" to "ready to compile a capability-system module end-to-end with deterministic build, hybrid PQ signing, vendor DWARF, LSP tooling, and the opt-pass catalog."

### Milestones

- **m1** (IR walker wiring) — `IrArena.children_table` + LinearityWalker (S0900/0901/0903/0907) + EffectRowWalker (F1100/01/02/05/06) + CapWalker (C1300) + linearity-regression / end-to-end corpora + ABI doc + cross-build smoke. PRs #347–#360.
- **m2** (typed-elaborator reflection) — `Term` handle + quote / antiquote + `splice` + `elab` builtin + macro hygiene + reflection-corpus. PRs #361–#372.
- **m3** (full algebraic effects) — row polymorphism + let-generalization + handler well-typedness + deep-handler compilation + R15 / SysV bridge + RowDiff diagnostic. PRs #374–#387.
- **m4** (PAX + paideia-link) — 96-byte PaxHeader + 64-byte SectionTable + 12 vendor section content types + BLAKE3 content hash + paideia-link 4-phase pipeline + pax-introspect tool. PRs #388–#400.
- **m5** (ML modules + functors) — Signature / Structure / Functor AST + module-kind machinery + structure / sig matching + applicative-functor cache + pack / unpack + sharing-constraint checker + `.paideia.functors` PAX section + file → module mapping. PRs #401–#413.
- **m6** (PE/COFF emitter) — PE/COFF headers + shared encoder lift + section emission + `.reloc` + UEFI imports + UEFI thunk + EFI subsystem + `--emit pe-coff` + UEFI smoke harness + cross-build fixture. PRs #414–#423.
- **m7** (PQ signing) — Ed25519 + ML-DSA-65 wrappers + hybrid signature scheme + PAX header signature integration + delegation-scope check (Q0901) + release-artifact signing + soft-HSM + verification corpus. PRs #424–#431.
- **m8** (LSP server) — tower-lsp scaffold + workspace manifest reader + textDocument sync + publishDiagnostics + parse cache + hover + definition / references + incremental engine + completion + code actions + paideia-fmt + semantic tokens + inlay hints + tree-sitter grammar + VS Code / Helix / Emacs / Neovim configs + LSP harness. PRs #432–#445.
- **m9** (optimization pass catalog) — OptPass trait + peephole + scheduling + macro fusion + DSE + REX/EVEX tightening + branch-hint / align / pool-constants + tail-call elimination + loop unrolling + catalog composition + O-code registration. PRs #446–#457.
- **m10** (DDC bring-up) — dual-build orchestrator + byte-level differ + build-determinism env contract (SOURCE_DATE_EPOCH + PDX_PATH_PREFIX_MAP) + format-gate corpus + nightly CI workflow + release-pipeline gate + operational docs. PRs #458–#465.
- **m11** (Phase 2 closure) — DWARF vendor ID + vendor section content builders + capability-system smoke + G4 prep checklist + retrospective. PRs #466–#470.

### Highlights

- ~1614 workspace tests across 26+ crates + 23+ test harnesses.
- 8 design-clarification items resolved (AS3 / AS5 / AS7 / AS8 / OS §3.2 / OS §4 N1 / OS §6 ¶1 / OS §6 ¶5). 7 deferred to Phase 3 with documented mitigation paths. 1 scope-changed.
- Dual stage-0 bootstrap commitment recorded (`design/toolchain/bootstrap.md`).
- Opt-pass catalog ships 11 passes (O1500–O1512) with callable helpers; real per-node rewrites flip on when the IR exposes per-node instruction payloads.
- LSP at parity with tower-lsp 0.20: 11 textDocument handlers wired.
- Full hybrid PQ signing: Ed25519 (RFC 8032 §7.1 KAT) + ML-DSA-65 (FIPS-204) with AND semantics; 3373-byte signature ≈ 3.4 KB.

### Operational deferrals

- **GitHub Actions billing block**: CI workflows (`ci.yml`, `cross-build.yml`, `ddc.yml`, `release.yml`) shipped but disabled at the org level. `cargo test --workspace` is the gate today. Activation pairs with billing restoration.
- **Stage-0b GNU `as` entry-point**: the dual-stage-0 commitment is documented (`bootstrap.md`); the GAS source is Phase 3 work.
- **Per-node IR instruction payloads**: required to flip the m9 opt passes from "would-fire" markers to real rewrites. Helper functions are unit-tested today.
- **Elaborator-driven LSP semantics**: m8-006..009 use lexical stand-ins. m8-008 QueryEngine is in place; per-position type queries land in Phase 3.

### Documentation

Every Phase 2 design doc has a phase-2-outcome appendix:

- `design/toolchain/calling-convention.md` (m3-011 + m6-005)
- `design/toolchain/paideia-link.md` (m4-013)
- `design/toolchain/macros-phase1.md` (m2-012)
- `design/security/pq-trust-root.md` (m7-008)
- `design/toolchain/optimization-passes.md` (m9-012)
- `design/toolchain/bootstrap.md` (m10-007)
- `design/toolchain/debug-info.md` (m11-001)
- `design/toolchain/phase-transition-2.md` (m11-005) — the retrospective.
- `docs/ddc.md` (m10-007)
- `docs/build-determinism.md` (m10-003)
- `docs/release-signing.md` (m7-005)
- `docs/g4-prep.md` (m11-004) — the G4 verification checklist.

### Decision gate G4

G4 stamping is pending the reviewer note in `docs/g4-prep.md` §6. Once stamped, paideia-as enters Phase 3 with its scope set by the m11-005 retrospective's §5 carryover list.

## v0.1.0 — Phase 1 (decision gate G2)

The phase-1 closing release. See the original `STATUS.md` for the per-deliverable PR map. Highlights:

- Lexer / parser / AST + diagnostics (PRs #29–#62).
- Substructural lattice + effect rows + handlers + macros + hygiene (PRs #122–#139).
- IR + ANF + effect rewrite (PRs #140–#141).
- ELF64 emitter + x86_64 encoder + DWARF stubs (PRs #142–#147).
- Linearity-regression corpus harness (PRs #149–#152).
- `paideia-as build --emit elf64` CLI (PR #148).
