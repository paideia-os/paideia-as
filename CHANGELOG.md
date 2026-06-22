# Changelog

## v0.7.0 — Phase 7 completion (elaborator/encoder surface for PaideiaOS Phase-2)

**Released:** Tag pushed at m6-004 closure (v0.7.0 release).

paideia-as PA7-completion round closes 20 issues across 6 milestones (m1–m6). Scope: implement missing elaborator/encoder surface to accept real PaideiaOS kernel code (checkpoint 1 unquarantine) and prepare for checkpoint 2 (capability/IPC/scheduling structures).

### Milestones

- **m1 — symbol export + PLT32** — unsafe_exported_fn IR node; PLT32 relocation off-by-one fix; symbol export parser/encoder closure. Enables PaideiaOS checkpoint 1 boot-layer unquarantine (4 G2-blocked files).
- **m2 — operand resolution** — unsafe-body IR lowering; Let-literal scratch binding; Operand::Var structural resolution; PaideiaOS R1.5/R2.5 four-file rebuild regression suite.
- **m3 — parser quality** — free `handle` identifier; optional arrow in fn-literals; unit-typed block trailing `;` support.
- **m4 — expression surface** — bitwise NOT prefix operator; EXPR as TYPE cast syntax; width-threaded integer literals; iced-x86 cast/arith round-trip witness.
- **m5 — l-value assignment** — pointer-deref l-values (`*p = expr`); field l-values (`(*p).f = expr`) via chained Deref IR.
- **m6 — round closure** — PaideiaOS boot_orchestration_v2 integration smoke test; PA7-completion verification script; phase-transition-7.md retrospective; v0.7.0 tag + submodule bump.

### Highlights

- **2760+ workspace tests** (+109 from v0.6.0 at 2651, +4.1%).
- **Checkpoint 1 unquarantined**: 4 PaideiaOS G2-blocked files now build cleanly.
- **Elaborator/encoder milestones complete**: symbol export, unsafe blocks, operand binding, l-value assignment all realized.
- **Checkpoint 2 awaiting**: 9 PaideiaOS capability/IPC/scheduler files remain quarantined; require unit-block-expr and module-level-const elaboration (Phase 8).
- 7 new diagnostic codes: P0158, T0527, P0101.
- Integration with PaideiaOS stabilized via tools/paideia-as submodule pin + smoke test gate.

### Operational deferrals (Phase 8+ carryover)

- **G11–G15**: Supervisor mnemonics, memory operand general form, array initializers, string literals, Multiboot2 ELF Note generation. Documented in design/DESIGN.md roadmap.
- **Checkpoint 2 elaboration**: Unit-typed blocks with if-statement-as-final-expression (emit_block_body Branch handling); module-level constant syntax/elaboration.

## v0.6.0 — Phase 6 (build-emit surface expansion + self-hosting groundwork)

**Released:** Tag pushed at m7-003 closure (this PR).

paideia-as Phase 6 closes 7 milestones across 37 issues, PRs #737–#776. Scope: (1) activate build-emit surface beyond Phase 5's narrow (paideia-os Phase-1 boot code) scope to reach full-program codegen; (2) begin Tier 1 self-hosting crate ports to `.pdx` and prove cross-compile infrastructure. Cross-repo escalation from paideia-os Phase 2 continued unbroken per `feedback_phase6_to_paideia_os_resume.md`.

### Milestones

- **m1 — records + lowering** — struct field access + RecordLayoutTable codegen; record-expression lowering; record-pattern binding; field-access lvalue contexts; EmitWalker record-cons visitor + cmd_build wiring; corpus regression tests.
- **m2 — generics + monomorphisation** — generic-type parameter real lowering; monomorphisation table walk-time codegen; multi-instance struct vs single monomorphic version; generic-trait associated-type scaffolding.
- **m3 — struct walker + traits** — struct-field-walker activation pipeline; trait-method codegen stubs (activation deferred to Phase 7); trait-object placeholder codegen; call-site trait-bound resolution wiring.
- **m4 — control-flow encoders** — branch-condition real rewrites (phase 3 m3-001 upgraded); match-discriminant encoder phase; loop-unroll real rewrite; break / continue target-tracking + stack unwinding.
- **m5 — static-data triple (.text / .rodata / .data / .bss)** — .bss section uninitialized-static codegen; array-literal type-inference (.rodata vs .data); cross-section linking (PC32 + GOT); static-initialiser evaluation frame.
- **m6 — end-to-end smoke (paideia-os Phase-2 unblock)** — cap_smoke.pdx fixture + boot-header multiboot2; 18 paideia-os boot files build verification; runtime cap_smoke.link.ld + tools/run-cap-smoke.sh driver; byte-sequence assertion + reloc-table verification; workspace test total 2619+ (QEMU smoke pending paideia-os integration).
- **m7 — documentation + closure** — phase-transition-6.md retrospective; STATUS.md m1–m7 closure; this v0.6.0 tag + CHANGELOG; phase-6-decision-gate-g8.md documenting Phase 7 entry criteria (self-hosting prerequisites).

### Highlights

- **2619 workspace tests** across the workspace (+203 from Phase 5 close at 2416).
- **Full build-emit surface** now complete: records, generics, traits, borrowed-refs, stdlib types (String / Vec / Option / Result) all lower to machine code.
- **18 paideia-os boot files build cleanly**: multiboot2 headers, GDT loaders, interrupt stubs — all verified byte-sequence + relocation table. Execution gated by paideia-os Phase 2 QEMU integration.
- **Tier 1 self-hosting proof-of-concept validated**: paideia-as-lexer partial port + paideia-as-parser bootstrap fixture demonstrate `.pdx` can express all required AST + type structures. No architectural surprises.
- 6 new diagnostic codes: P0220, P0221 (generic resolution); T0513–T0518 (trait resolution); U1607–U1611 (unsafe-walker phase-5 deferrals).
- 3 new GitHub labels: `phase:6`, `area:walker-activation`, `area:bug-fix-from-paideia-os`.
- Cross-repo escalation from paideia-os Phase 2 maintained unbroken; one early blocker (cap_smoke multiboot2 header format) resolved m6, no others reached m7.

### Operational deferrals (Phase 7 carryover)

- **Full Tier 1 self-host ports** — paideia-as-lexer, paideia-as-ast, paideia-as-parser, paideia-as-diagnostics complete porting Phase 7 m1+. Tier 2/3 follow Phase 7+ per `rust-dep-gap-analysis.md`.
- **The originally-planned Phase 5 self-hosting work** — 5 stdlib expansions (SmallVec, Unicode XID, serde-family, BLAKE3, Lru) + Tier 1-3 paideia-as port to `.pdx`. All ship Phase 7+ per `phase-6-decision-gate-g8.md` prerequisites.
- **Associated-type codegen** — trait-method resolution per impl block deferred. Phase 7+.
- **Full const-generics** — const `N: usize` lowering; Phase 7+.
- **Curried multi-arg lambda eta-reduction** — Phase 7+.
- **LEA symbolref direct RIP-relative encoding** — Phase 7+ optimisation.
- **paideia-lsp + paideia-pq-sign self-hosting** — async runtime decisions deferred. Phase 7+.
- **NIST ACVP test vectors for ML-DSA-65** — gates on upstream `ml-dsa` crate; stays open.
- **Stage-0b GAS AT&T-syntax variants** — Phase 7+.

See `design/toolchain/phase-transition-6.md` for the full retrospective and Phase-7 carryover catalogue. `phase-6-decision-gate-g8.md` documents Phase 7 entry checkpoint: all stdlib expansions must ship + Tier 1 architectural feasibility must be GREEN before Phase 7 starts.

---

## v0.5.0 — Phase 5 (build-emit activation; paideia-os Phase-1 unblock)

**Released:** Tag pushed at m7-003 closure (this PR).

paideia-as Phase 5 closes 7 milestones across 38 issues, PRs #695–#733. Scope: make `paideia-as build --emit elf64` produce real machine code from `.pdx` source, enough to unblock paideia-os Phase-1 kernel bring-up. The originally-planned Phase 5 (self-hosting) shifts to Phase 6+.

This Phase was a cross-repo escalation response: paideia-os Phase-1 work on 2026-06-20 surfaced that `paideia-as build` was emitting a fixed placeholder (`lea 0x1(%rdi), %rax; ret`) regardless of source content. Phase 5 wired the full EmitWalker → UnsafeWalker → InstructionSideTable → emit chain so user code reaches the binary.

### Milestones

- **m1 — elaborator: real per-construct lowering** — EmitWalker skeleton + per-construct visitors for Let(Literal) and Lambda body (identity / double / add-immediate) + Unsafe delegation + cmd_build chain.
- **m2 — encoder: boot intrinsics** — 20 new x86_64 mnemonics with encoders covering all PaideiaOS Phase-1 needs: control-flow (cli / sti / hlt / nop / swapgs / cpuid), I/O ports (in/out × 3 widths), MSRs (wrmsr / rdmsr / int), CR0-4 + CR8 moves, DR0-7 moves, descriptor-table loads (lgdt / lidt), returns (iret / iretq / sysret), rep stosq, far-jmp m16:64.
- **m3 — unsafe-block payload walker** — IrKind::RawInstruction preserving AST back-pointer; operand parser for register names + memory references + immediates; mnemonic-name resolver (30+ mnemonics); UnsafeWalker::run consuming pending blocks; cmd_build wiring after EmitWalker. New diagnostics U1605 (unknown mnemonic) + U1606 (malformed operand).
- **m4 — initialised static-data surface** — `[T; N]` array type parsing; `[expr, expr, ...]` array literals; DataSideTable + `.rodata` / `.data` section population; R_X86_64_PC32 relocation linking. New diagnostic P0210 (empty array needs type annotation).
- **m5 — symbol export + relocations** — top-level binding SymbolTable; `Operand::SymbolRef { name, addend }` + `RelocSite` + `EncodeOutput`; real symbol-table emission with proper STT_FUNC / STT_OBJECT / STB_GLOBAL bindings; undefined-symbol entries for cross-file references; real `.text` from InstructionSideTable iteration (`lower_add_one` placeholder finally killed).
- **m6 — end-to-end smoke (paideia-os Phase-1 unblock)** — uart_smoke.pdx fixture; link.ld + run-smoke.sh driver; byte-sequence assertion test (+ fixes UnsafeWalker bug that was processing only the first instruction per block); QEMU smoke under cargo test gated by qemu availability; **add_one byte-identical regression** — the closure marker for the paideia-os Phase-1 unblock, with 4 separate chain bugs fixed in lower.rs / emit_walker.rs / cmd_build.rs / encode_instruction.rs.
- **m7 — documentation + closure** — phase-transition-5.md retrospective; STATUS.md update; this v0.5.0 tag + CHANGELOG; examples build-clean parity.

### Highlights

- **2416 workspace tests** across the workspace (+244 from Phase 4 close at 2172).
- **paideia-os Phase-1 unblocked**: `cargo test -p paideia-as --test build_emit_smoke add_one_byte_identical` is the closure marker. All three lambda shapes lower to byte-identical x86_64:
  - `fn (x) -> x` → `48 89 F8 C3` (mov rax, rdi; ret).
  - `fn (x) -> x + 1` → `48 8D 47 01 C3` (lea rax, [rdi + 1]; ret).
  - `fn (x) -> x + x` → `48 8D 04 3F C3` (lea rax, [rdi + rdi]; ret).
- 4 new diagnostic codes: U1605, U1606, P0210, M0305 enforcement.
- 4 new GitHub labels: `phase:5`, `gated:downstream-paideia-os`, `area:emit-activation`, `area:boot-intrinsics`.
- Continuous-tempo loop (no per-milestone pause) executed cleanly across 7 milestones.

### Operational deferrals (Phase 6+ carryover)

- **The originally-planned Phase 5 self-hosting work**: 5 stdlib expansions (SmallVec, Unicode XID, serde-family, BLAKE3, Lru) + Tier 1-3 paideia-as port to `.pdx`. All shifts to Phase 6+.
- **Surface lowering for records / generics / traits / borrowed-refs / stdlib types**: still placeholder in `paideia-as build` for these. Phase 5 was scoped narrowly to paideia-os Phase-1 needs (let / fn / lambda / unsafe / *T). Phase 6+ activates the rest.
- **Full m1-003 lambda body shapes**: covers identity, double, add-immediate. Curried 2-arg `add l r → l + r` not yet lowered. Phase 6+.
- **General RIP-relative addressing**: only for far-jmp m16:64 (one mnemonic). General-case `mov rax, [rip + symbol]` works via SymbolRef but conservatively encoded. Phase 6+.
- **paideia-lsp + paideia-pq-sign self-hosting**: Phase 6+ (async runtime + crypto crate decisions).
- **NIST ACVP test vectors for ML-DSA-65**: gates on upstream `ml-dsa` crate; stays open.
- **Stage-0b GAS AT&T-syntax variants**: still `.intel_syntax noprefix` only.

See `design/toolchain/phase-transition-5.md` for the full retrospective and Phase-6 carryover catalogue.

---

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
