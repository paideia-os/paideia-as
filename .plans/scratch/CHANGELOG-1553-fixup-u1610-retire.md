# paideia-as#1553: retire unreachable fixup-pass U1610 (closes #1488)

## Summary

Chose **Option B** from the issue: retire the user-facing U1610
(`unresolved-label`) emission in the label-fixup pass and preserve only
the elaborator's own `process_stmt` U1610 guard, which is the sole
code path any `.pdx` source can reach for this diagnostic.

## Reachability argument (why Option B is safe)

`patch_label_fixups` (in `crates/paideia-as/src/cmd_build/fixup.rs`)
takes `Vec<LabelFixup>` from the encoder and, for each fixup, either
patches the rel32 displacement (label found in the resolved-labels map)
or previously emitted U1610 (label missing). To reach the missing arm
from user syntax, some `.pdx` source would need to produce an
`Operand::LabelRef` naming a label that is not registered before
fixup time. It cannot:

  * `parse_operand_from_ast` in
    `crates/paideia-as-elaborator/src/unsafe_walker/operand.rs` only
    emits `Operand::LabelRef { name, addend: 0 }` at three sites
    (lines 93, 125, 182), each gated by
    `supports_label_ref(mnemonic) && labels.contains_key(&name)`.
    Unknown identifiers in `jmp` / `jcc` / `call` operand position
    fall through to `parse_symbol_ref_from_ident`
    (`unsafe_walker/symbol_ref.rs:55`), which produces
    `Operand::SymbolRef` — resolved at link time via .rela.text, not
    at fixup time.

  * Even if the elaborator's per-block `labels` map is stale, the
    validator loop at
    `crates/paideia-as-elaborator/src/unsafe_walker/process_stmt.rs:215-232`
    re-checks `LabelRef` operands against the same map and emits U1610
    itself if the label is missing, then bails on the instruction.
    That U1610 is the canonical user-facing emission.

  * The only remaining producers of `Operand::LabelRef` are
    compiler-synthesised call/jmp/jcc lowerings in
    `emit_control_flow.rs`, `emit_int_match.rs`, `emit_enum_match/*`,
    `emit_block_body/block_body.rs`, `emit_call.rs`, `emit_int_match`,
    `stdlib_lowering/*`, etc. Those sites author both the label
    definition (`insert_label`) and its references, so a missing
    label there is a compiler bug — an ICE, not a user diagnostic.

Retiring the fixup emission therefore removes dead user-visible code.
The safety net for compiler bugs is preserved by making the fixup
`None` arm a `panic!` with a bug-report message: cleaner than a
misleading U1610 that pretends the user wrote something wrong.

## Options weighed

  * **Option A** (add a Rust unit-level driver that synthesises a
    stray `LabelRef` to keep the diagnostic reachable) — rejected as
    negative-value scaffolding. The code would only run via test
    scaffolding, so the coverage is circular. The elaborator's
    process_stmt path already carries the semantic value.

  * **Option B** (retire) — **chosen**. Cleanest, smallest diff.
    Preserves the U1610 catalog code (still emitted by the
    elaborator) and the encoder/emitter diagnostics next door
    (B1703/B1704/B1705/B1706).

  * **Option C** (relax `parse_operand_from_ast` so unknown
    identifiers become `LabelRef` first) — rejected. Would break the
    SymbolRef fallback that every intra-module call currently depends
    on, disturbing far more code than Option B saves.

## Files touched

  * `crates/paideia-as/src/cmd_build/fixup.rs` — full rewrite of the
    header + function docs; `patch_label_fixups` signature reduced to
    `(buffer, label_fixups, labels)`; `None` arm is now `panic!`.

  * `crates/paideia-as/src/cmd_build/elf.rs` (~L127-137) — removed
    `let strict_mode = true;` and the four extra args from the
    `patch_label_fixups` call.

  * `crates/paideia-as/src/cmd_build/pe.rs` (~L129-139) — same
    simplification.

  * `crates/paideia-as/src/cmd_build/diagnostics.rs` — deleted
    `unresolved_label` (U1610 builder), `node_for_fixup` reverse
    lookup, its unit test
    `node_for_fixup_finds_referring_instruction`, and the
    `paideia_as_encoder::LabelFixup` import (root + tests scope).
    Header bullets updated.

  * `crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs`
    — deleted `unresolved_label_typed_diagnostic_in_sarif`
    (Wave-28 ignored placeholder from paideia-as#1488). Module header
    updated. **Closes paideia-as#1488.**

  * `crates/paideia-as/tests/build_emit/label_patches.rs` — module
    header updated (dropped the stale "unresolved labels emit U1610"
    sentence, added Wave 29 note).

  * `Cargo.toml` — `workspace.package.version` 0.36.72 → 0.36.73.

  * `CHANGELOG.md` — new entry at top.

## Preserved (per scope)

  * `crates/paideia-as-elaborator/src/unsafe_walker/process_stmt.rs`
    U1610 emission — untouched. Sole surviving emitter.

  * `crates/paideia-as-elaborator/src/unsafe_walker/diag.rs` and its
    `U_UNKNOWN_LABEL` code — untouched.

  * `parse_operand_from_ast` and the SymbolRef fallback — untouched
    (Option C explicitly rejected).

  * All docstrings that reference U1610 as a semantic description of
    "unknown label reference" in `paideia-as-runtime`, `paideia-as-ast`,
    `paideia-as-ir` — untouched. They still correctly describe the
    elaborator's behaviour.

## Follow-ups (out of scope for this wave)

  * None. The fixup-pass retirement is self-contained. The
    elaborator's U1610 already has regression coverage in
    `paideia-as-elaborator/tests/unsafe_walker/top_level.rs` around
    lines 1433 / 1463.

## Build

Not run per softarch protocol. Main will invoke
`bash tools/build.sh` and (on clean) commit + push; on failure the
tail comes back for debugger triage.

## Version

  * workspace.package.version: **0.36.72 → 0.36.73**
  * CHANGELOG.md entry added at head.

## Issues

  * Closes paideia-as#1488 (Wave 28 B1-005 fixup fixture — no fixture
    is authorable; test deleted).
  * Resolves paideia-as#1553 (fate of the fixup-pass U1610 — Option B
    landed).
