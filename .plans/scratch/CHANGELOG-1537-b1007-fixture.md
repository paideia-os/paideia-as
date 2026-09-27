# PAS-DEBT-B1-007 (paideia-as#1537): B1704 SARIF fixture

## Summary

Un-ignored `lambda_no_offset_typed_diagnostic_in_sarif` in
`crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs` and
wired it to an existing fixture that reliably triggers B1704
(`function-symbol-no-offset`) through the typed SARIF pipeline.

## Root-cause path

B1704 fires in `crates/paideia-as/src/cmd_build/elf.rs:305-314` — the
`None` branch of `function_offsets.get(&symbol.ir_node.get())` inside
the function-symbol emission loop. `function_offsets` is populated
from `emit_walker.state().lambda_first_instr()` (elf.rs:270-277), so
a lambda that reaches the emitter WITHOUT calling
`record_lambda_entry(...)` falls into that None branch.

The `(Var, Literal)` `+` fast-path in
`crates/paideia-as-elaborator/src/emit_visit_lambda.rs:1011-1015`
explicitly skips emission when the immediate is outside `-128..=127`:

```rust
Some("+") => {
    // Validate immediate range before recording entry
    if !(-128..=127).contains(&value) {
        // Out of range; skip emission (B1704 will fire for missing symbol)
    } else { ... record_lambda_entry(...) ... }
}
```

The comment already predicted this exact fixture shape. The
`Some("<<")` arm at lines 985-1009 does the same for shifts outside
`0..=63`.

## Fixture

Reused the existing
`tests/build-emit/pa8_add_imm_out_of_range.pdx`:

```
module Pa8AddImmOutOfRange = structure {
  pub let f : (u64) -> u64 = fn(x : u64) -> x + 200
}
```

`200 > 127` -> add-imm fast-path skips emission -> function symbol
`f` registered with no offset -> B1704 fires as a warning. This
fixture already backs the sibling stderr-based test in
`crates/paideia-as/tests/build_emit/pa8_add_imm_out_of_range.rs` and
is known to trigger the diagnostic (elf.rs already emits the typed
diagnostic; the missing piece was a SARIF-surface test). No new
`.pdx` file was created — a duplicate would only bind two tests to
the same brittle codepath.

## Test shape (SARIF assertions)

- Removed `#[ignore = "TODO(#1105-followup): needs lambda-no-offset fixture"]`.
- Replaced `panic!` body with the same pattern used by
  `encoder_warn_typed_diagnostic_in_sarif` (which asserts B1706 at
  warning level via `--sarif`), adapted for B1704:
  - Exit code 0 (B1704 is `Diagnostic::warning`, not error).
  - SARIF results contain a `ruleId` matching `B1704` at `level == "warning"`.

## Files changed

- `crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs`
  (lines 155-224): un-ignored test, added SARIF assertion body,
  documented fixture reuse and B1704 origin in the comment.

## Files added

None. Reused existing fixture per the "no duplicate .pdx" rationale
above.

## Not done

- Did not modify `emit_lambda` / `emit_visit_lambda` logic (out of
  scope per ticket).
- Did not run `cargo build` / `cargo test` (main-only per
  `feedback_no_background_builds.md`).
- Did not bump `workspace.version` (per ticket constraint).

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
