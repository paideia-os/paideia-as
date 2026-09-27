# PAS-DEBT-B1-006 (paideia-as#1489): B1703 SARIF fixture

## Summary

Un-ignored `symbol_layout_invalid_typed_diagnostic_in_sarif` in
`crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs` and
authored a new fixture `tests/build-emit/duplicate_symbol.pdx` that
reliably triggers B1703 (`symbol-layout-invalid`) through the typed
SARIF pipeline.

## Root-cause path (why the fixture fires B1703)

B1703 fires in `crates/paideia-as/src/cmd_build/elf.rs:412-419` — the
`SymbolLayoutInvalid` arm of the match on `writer.finalize()`'s error:

```rust
writer.finalize().map_err(|err| match err {
    EmitterError::SymbolLayoutInvalid { message } => {
        use super::diagnostics::symbol_layout_invalid;
        let diag = symbol_layout_invalid(&message);
        let _ = sink.emit(diag);
        BuildError::Failed
    }
})
```

`SymbolLayoutInvalid` originates in
`crates/paideia-as-emitter-elf/src/writer.rs:616-628` inside
`validate_symbol_layout`, which iterates `self.symbol_names_added`
through a `HashSet` and returns the error on the first duplicate name.

The chosen driver: `@fingerprint("<tag>")`. The elaborator's
`populate_fingerprints` pass
(`crates/paideia-as-elaborator/src/fingerprint_emit.rs:80-103`) walks
every `(let_node_id, "<tag>")` on `AstArena::item_fingerprint` and
pushes one `.rodata` `DataEntry` per attribute under the synthetic
symbol `format!("fp_{}", tag)`. The parser stamps entries keyed by
Let `NodeId` (`crates/paideia-as-parser/src/parse_item/let_item/mod.rs:403-405`),
so two `pub let` bindings with the SAME tag survive parsing (tag is
not part of the key) and produce two `DataEntry`s with the identical
`symbol_name`.

Then `cmd_build/elf.rs:242-253` walks `arena.fingerprints()` and
calls `writer.add_symbol(SymbolEntry { name: entry.symbol_name.clone(), ... })`
for each. `add_symbol` (writer.rs:329-388) does not dedup at add
time — it inserts into `self.symbols` (the second overwrites the
first) and appends the name to `self.symbol_names_added`. `finalize`
then sees the duplicate and fails, sink receives B1703,
`finish_build_error` writes SARIF and exits 2.

## Fixture

New file: `tests/build-emit/duplicate_symbol.pdx`.

```
module DuplicateSymbol = structure {
  pub let marker_a : u64 = 0 @fingerprint("dup_tag")
  pub let marker_b : u64 = 1 @fingerprint("dup_tag")
}
```

- Two different binding names (`marker_a`, `marker_b`) — the parser
  will not reject the module for name collision.
- Identical fingerprint tag `"dup_tag"` (ASCII, alphanumeric +
  underscore — safe under the parser's ASCII validation).
- Yields exactly one duplicate pair (`fp_dup_tag`); the finalize
  error message is deterministic.

## Test shape (SARIF assertions)

- Removed `#[ignore = "TODO(#1105-followup): needs duplicate-symbol fixture"]`.
- Replaced `panic!` body with the same pattern used by
  `encoder_failure_typed_diagnostic_in_sarif` (which asserts B1705 as
  error via `--sarif`), adapted for B1703:
  - Exit code 2 (B1703 is `Diagnostic::error` and reaches
    `finish_build_error`).
  - SARIF results non-empty and contain a `ruleId` matching `B1703`.
- SARIF path parked at `/tmp/test_symbol_layout_invalid_diagnostic.sarif.json`,
  ELF at `/tmp/test_symbol_layout_invalid.o` — mirrors the sibling
  test's pattern.

## Files changed

- `crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs`
  (test `symbol_layout_invalid_typed_diagnostic_in_sarif`,
  approximately lines 147-201 after edit): un-ignored, replaced
  panic body with SARIF-check pattern, documented the fixture's
  fire path in the comment.

## Files added

- `tests/build-emit/duplicate_symbol.pdx` (new fixture, comment
  explains the full parse->elaborate->emit->finalize path that
  culminates in B1703).

## Diagnostic asserted

`B1703` (symbol-layout-invalid), severity `error`, exit code 2.

## Not done

- Did not modify `validate_symbol_layout`, `add_symbol`, or the
  fingerprint/data-symbol emission path (out of scope per ticket).
- Did not run `cargo build` / `cargo test` (main-only per
  `feedback_no_background_builds.md`).
- Did not bump `workspace.version` (per ticket constraint).

## Risk / follow-ups

- The fixture relies on the elaborator's fingerprint pass allowing
  duplicate tags. If a future pass rejects duplicate `@fingerprint`
  tags upstream (a defensible cleanup), this fixture stops firing
  B1703 and would need to migrate to a different duplicate-symbol
  driver (e.g., two data entries with a forced identical
  `symbol_name`, or a function symbol that collides with a data
  symbol name). The comment inside the fixture documents the shape
  so a future maintainer can adjust.

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
