# Self-hosting Phase 5 plan

**Status:** Phase 4 m13-001 catalogue.
**Scope:** Inventory of every paideia-as crate by (LoC, Rust deps, .pdx-portability), categorised into three tiers for incremental Phase 5 self-hosting bring-up.

## 0. Why this catalogue exists

Phase 4 m13 documents the work needed before paideia-as can compile itself. Self-hosting is not a Phase 4 deliverable — Phase 4 m13 lays groundwork; Phase 5 executes. This catalogue is the input to the Phase 5 plan.

The discipline: **inventory every Rust crate**, mark the LoC, Rust-dependency profile, and how tractable a `.pdx` port is. Then sequence tier-by-tier: lowest-dependency-count crates port first (Tier 1), highest-complexity crates port last (Tier 3).

## 1. Crate inventory

LoC measured via `find crates/<name> -name "*.rs" | xargs wc -l` (Rust source only, excludes target/).

| Crate                       | LoC    | Tier | .pdx-portability | Critical Rust deps                          |
|-----------------------------|--------|------|------------------|--------------------------------------------|
| paideia-as-test             |    275 |  T0  | Ports cleanly    | regex (needs stdlib regex)                  |
| paideia-as-doc              |    386 |  T0  | Ports cleanly    | (none beyond paideia-as-ast/parser)         |
| paideia-fmt                 |    270 |  T0  | Ports cleanly    | (none)                                      |
| paideia-as-dwarf            |    804 |  T2  | Ports cleanly    | (constants only; no third-party)            |
| paideia-as-effects          |   1500 |  T2  | Ports cleanly    | (none)                                      |
| paideia-as-linker           |   1710 |  T2  | Mostly ports     | (none external)                             |
| paideia-stdlib              |   1746 |  T0  | .pdx already     | n/a (the source-side stdlib itself)         |
| paideia-as-encoder          |   2202 |  T2  | Ports cleanly    | iced-x86 (test-only; can drop)              |
| paideia-as (binary)         |   2212 |  T0  | Glue + CLI       | clap, paideia-fmt, ... (mostly internal)    |
| paideia-as-emitter-pe       |   2495 |  T3  | Ports cleanly    | (none external)                             |
| paideia-as-emitter-elf      |   2511 |  T3  | Ports cleanly    | (none external)                             |
| paideia-as-diagnostics      |   3895 |  T1  | Ports cleanly    | serde, toml (config + SARIF rendering)      |
| paideia-as-types            |   3935 |  T2  | Ports cleanly    | smallvec                                    |
| paideia-lsp                 |   4208 |  T3  | DEFERRED         | tower-lsp, tokio (async runtime — Phase 6+) |
| paideia-pq-sign             |   4567 |  T3  | DEFERRED         | ed25519-dalek, ml-dsa, cryptoki, yubihsm, reqwest, blake3 — large external surface |
| paideia-as-lexer            |   4774 |  T1  | Ports cleanly    | (none)                                      |
| paideia-as-emitter-pax      |   5403 |  T3  | Ports cleanly    | (none external)                             |
| paideia-as-ast              |   5663 |  T1  | Ports cleanly    | smallvec                                    |
| paideia-as-ir               |  10363 |  T2  | Ports cleanly    | smallvec                                    |
| paideia-as-parser           |  15032 |  T1  | Ports cleanly    | (none external)                             |
| paideia-as-elaborator       |  19306 |  T2  | Ports cleanly    | (mostly internal — paideia-as-{ast,types,ir})|

**Totals**: ~93k LoC across 21 crates. ~50k LoC in Tier 1 (lexer + parser + AST), ~37k LoC in Tier 2 (types + IR + elaborator), ~6k LoC in Tier 3 (emitters + linker + dwarf).

## 2. Tier definitions

### Tier 0 — tooling + already-`.pdx` (no port needed)

- `paideia-as-test`, `paideia-as-doc`, `paideia-fmt`: tooling layered on top of compiler crates. Port after the compiler self-hosts.
- `paideia-stdlib`: already `.pdx` source.
- `paideia-as` (binary): glue + CLI dispatcher. Last to port.

### Tier 1 — frontend (port first)

- **paideia-as-lexer** (4.8k LoC): pure character-stream → token-stream. No external deps. Easiest port.
- **paideia-as-parser** (15k LoC): token-stream → AST. Recursive-descent; no external deps. Bulky but mechanical.
- **paideia-as-ast** (5.7k LoC): arena + node definitions. Only smallvec — needs stdlib equivalent.
- **paideia-as-diagnostics** (3.9k LoC): catalog + SARIF rendering. Needs serde / toml equivalents for SARIF.

Tier 1 totals ~30k LoC. After Tier 1 lands, `.pdx` source can compile to AST + diagnostics — useful for a self-hosted-lexer / self-hosted-parser smoke target.

### Tier 2 — middle layers (port after Tier 1)

- **paideia-as-types** (3.9k LoC): type interner + unifier + regions. Internal to compiler.
- **paideia-as-effects** (1.5k LoC): effect registry + capability table.
- **paideia-as-ir** (10.4k LoC): IR arena + walker + side-tables. Bulky but compositional.
- **paideia-as-elaborator** (19.3k LoC): the largest crate; type-check + populate + walkers. Last Tier-2 port.
- **paideia-as-dwarf** (0.8k LoC): vendor-section content builders. Trivial after IR ports.
- **paideia-as-encoder** (2.2k LoC): x86_64 byte emission. No external deps (drops iced-x86 test dep).
- **paideia-as-linker** (1.7k LoC): paideia-link 4-phase pipeline.

Tier 2 totals ~40k LoC. After Tier 2 lands, the compiler can elaborate and emit IR + bytes — useful for a self-hosted-IR + self-hosted-encoder smoke target.

### Tier 3 — backends + deferred

- **paideia-as-emitter-elf** (2.5k LoC): ELF64 file builder.
- **paideia-as-emitter-pax** (5.4k LoC): PAX object format.
- **paideia-as-emitter-pe** (2.5k LoC): PE/COFF emitter.

These three port cleanly but only matter after Tier 2 is done. Tier 3 totals ~10k LoC.

**Explicitly DEFERRED beyond Phase 5:**

- **paideia-lsp** (4.2k LoC): tower-lsp + tokio async runtime. Async/await isn't shipped in paideia-as today (Phase 5+ design). Self-hosted LSP is Phase 6+.
- **paideia-pq-sign** (4.6k LoC): wraps ed25519-dalek, ml-dsa, cryptoki, yubihsm, reqwest, blake3. The external surface is large; self-hosting requires either rewriting all of these in `.pdx` (huge) or providing an FFI bridge. Phase 6+ decision.

## 3. Sequencing for Phase 5

Linear bring-up:

1. **Phase 5 m1** — self-host paideia-as-lexer. Smoke: lex a small `.pdx` file with the self-hosted lexer; bytes match Rust-lexed reference.
2. **Phase 5 m2** — self-host paideia-as-ast + paideia-as-parser. Smoke: parse to AST; AST structurally matches reference.
3. **Phase 5 m3** — self-host paideia-as-types + paideia-as-effects + paideia-as-diagnostics. Smoke: type-check trivial programs.
4. **Phase 5 m4** — self-host paideia-as-ir (the bulky one). Smoke: lower a function to IR; structural match.
5. **Phase 5 m5** — self-host paideia-as-elaborator (the largest one). Smoke: full elaborate path for the m11-003 capability-system module.
6. **Phase 5 m6** — self-host paideia-as-encoder + paideia-as-linker + Tier-3 emitters. Smoke: end-to-end self-compile of paideia-as itself.

Phase 5 closure: stage-2-paideia-as (self-compiled) emits byte-identical output to stage-1-paideia-as (Rust-compiled) for the m11-003 fixture. This is the strong Wheeler-CTTTDC test at the self-hosting layer.

## 4. Per-crate gap notes

Open questions for each tier as Phase 5 begins:

**Tier 1 gaps**:
- `regex` crate for paideia-as-test: rewrite as `.pdx` or call out as host-only.
- `serde` / `toml` for paideia-as-diagnostics SARIF emit: m11 stdlib expansion may cover; otherwise hand-roll.
- `smallvec`: trivial port (~200 LoC).

**Tier 2 gaps**:
- `iced-x86` (encoder dev-dep): drop; tests can use Rust-lexer reference for byte verification.
- IR arena + side-table allocation: depends on m10 ambient allocator wiring.

**Tier 3 gaps**:
- None significant for emitters (no external crates).

## 5. Phase 6+ deferrals

After Phase 5 self-hosting closes, the remaining "self-hosted" gaps:

- **LSP**: async runtime + JSON-RPC + concurrent file watching. Requires async/await + threading primitives + collections beyond m11.
- **PQ signing**: requires `.pdx` ports of Ed25519, ML-DSA-65, BLAKE3, cryptoki, yubihsm, reqwest, ed25519-dalek. Most of these are crypto primitives that benefit from constant-time implementations — significant work.
- **Test runner execution**: `paideia-as test` discovers; execution needs a `.pdx`-resident runtime evaluator. m13-002 ships the bootstrap fixture; full execution gates on Phase 5 + a runtime.

These are explicit non-goals for Phase 5; their work lives in Phase 6+ design docs.

## 6. Tooling for the bring-up

The bring-up benefits from:

- **DDC harness** (m10 / Phase 2 + m5 / Phase 3): byte-comparison infrastructure already exists; can extend to "stage-1 vs stage-2 paideia-as binary diff."
- **paideia-as test**: discovers `#[test]` annotations in `.pdx`; m13-002 provides the bootstrap fixture for the runtime evaluator.
- **paideia-as doc**: generates documentation from the ported `.pdx` source.

The Phase 5 m1 stage activates the bootstrap-fixture + DDC pair so each tier port is verified incrementally.

## 7. Open questions for Phase 5 kickoff

- **Stdlib coverage gap**: does m11 stdlib cover all primitives paideia-as crates need? Specifically: `Vec` + `HashMap` + `String` + `Option` + `Result` cover most internal use; smallvec / regex / serde need additions.
- **Allocator strategy**: does the self-hosted compiler use Arena (PaideiaOS default) or SystemAllocator (host default) when running on a host? The dual-default lands here.
- **FFI for deferred crates**: does paideia-pq-sign get a `.pdx` FFI shim wrapping Rust calls, or does it port crypto primitives? Phase 6+.
- **Bootstrap chain length**: stage-0a NASM + stage-0b GAS + stage-1 Rust + stage-2 self → 4 stages. Wheeler-CTTTDC requires byte-identical stage-2 outputs across stage-0a vs. stage-0b paths.

## 8. Forward links

- **m13-002**: bootstrap-fixture `.pdx` mini-compiler.
- **m13-003**: Rust-dependency gap analysis.
- **m13-004**: stage-1 hash + DDC fixture.
- **m13-005**: Phase 5 opening conditions.
- **Phase 5 plan**: m1–m6 sequencing per §3.
- **Phase 6 deferrals**: LSP + PQ signing + test execution per §5.
