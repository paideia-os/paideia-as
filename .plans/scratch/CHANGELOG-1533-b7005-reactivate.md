# CHANGELOG scratch — Issue #1533 (PAS-DEBT-B7-005)

**Site:** `tests/opt-regression/tests/encode_tight_regression.rs`
**Debt row:** 469 — "`#[ignore]`'d pending encode-tight diagnostic wiring"
**Status:** REACTIVATED — root cause was a harness dependency gap, not a pass gap.

## Diagnosis

- `EncodeTightPass` lives in `crates/paideia-as-emitter-elf/src/opt/encode_tight.rs`.
- Its `OptPass::apply` implementation has always emitted an O1506 would-fire
  diagnostic via `OptDiagSink` (line 64), and its own unit test
  `encode_tight_pass_emits_o1506` at line 110 pins that shape.
- The regression harness at `tests/opt-regression/` only depended on
  `paideia-as-ir` + `paideia-as-diagnostics`, so the emitter-side pass symbol
  was unreachable — hence the placeholder + `#[ignore]`.
- Design doc `design/toolchain/optimization-passes.md` §2.2 declares the
  Phase-2-m9 "would-fire" disclaimer chain closed for every pass except
  `unroll`, which further confirmed the regression assertions were safe to
  land.

## Changes

1. **`tests/opt-regression/Cargo.toml`** — added
   `paideia-as-emitter-elf = { path = "../../crates/paideia-as-emitter-elf" }`
   as a dev-dependency, with a comment noting that the emitter crate depends
   on `paideia-as-ir` (no dependency cycle).

2. **`tests/opt-regression/tests/encode_tight_regression.rs`** — replaced the
   single `#[ignore]`d placeholder with two active tests modelled on
   `align_regression.rs` / `macro_fusion_regression.rs` / `unroll_regression.rs`:
   - `encode_tight_pass_registered` — asserts `pass.name() == "encode-tight"`
     (matches the m9-006 catalog entry).
   - `encode_tight_apply_emits_o1506_would_fire` — asserts one O1506 would-fire
     diagnostic is emitted per dispatch, `changed == false`, and the message
     substring exactly matches the m3-004 pinned shape
     `"O1506 (would-fire): REX/EVEX prefix tightening dispatched"`.
   - File-level doc comment names the disclaimer chain closure and the flip
     contract (if encode-tight ever grows a real IR-side rewrite that touches
     `InstructionSideTable`, the message assertion must move to the
     `"O1506 rewrote N sites"` shape used by `align` / `macro-fusion`).

3. **`design/toolchain/optimization-passes.md`** — appended a "B7-005
   reactivation" paragraph after the m3-008 note, updating harness state from
   `19 active + 1 ignored` to `20 active + 0 ignored` and recording that the
   ignore was a harness dependency gap.

## Non-scope (per issue constraints)

- No pass code touched (`crates/paideia-as-emitter-elf/src/opt/encode_tight.rs`
  is unmodified).
- No `workspace.version` bump.
- No build/test run — main validates with `cargo check --tests`.

## Recommended follow-up

- If a future PR moves encode-tight from would-fire to real IR-side rewrite
  (mirroring the m1-007..010 flip that closed macro-fusion / branch-hint /
  align / pool-constants), the new assertion here breaks intentionally and
  should be updated in lock-step to `"O1506 rewrote N sites"`.
- Consider auditing sibling harnesses (`linearity-regression`,
  `ir-payload`, etc.) for similar dev-dependency gaps that mask reachable
  diagnostic tests behind `#[ignore]`.
