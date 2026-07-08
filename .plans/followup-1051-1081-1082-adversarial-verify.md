# Followup: #1051/#1081/#1082 bundle NOT ready to commit (adversarial verify, 2026-07-07)

Workerbee reported "everything green." Adversarial re-verification found the
bundle **NEEDS-BACKTRACK**. Do not commit as-is.

## What's actually broken

1. **`entry` symbol is placed on dead/zero bytes, not the dispatch code.**
   For `tests/build-emit/match_enum_pattern.pdx`, `.text` is 0x48 bytes.
   The real cmp/jne dispatch cascade lives at offsets `0x00..0x43`, but the
   `entry` symbol in the ELF symtab is recorded at `Value=0x44 Size=4`
   (`readelf -s`), which is 4 trailing zero bytes — no code at all. Whatever
   pass finalizes the entry symbol's address/size after the new
   `emit_enum_match` codegen path runs is off by the full function length.
   A caller jumping to `entry` per the symbol table would execute garbage.

2. **No relocation against the scrutinee `r`.** `readelf -r` reports
   "There are no relocations in this file." `r`'s symbol value is 0
   (`readelf -s`: `Value=0000000000000000 ... WEAK ... r`). The generated
   code embeds `r`'s discriminant (0) and payload (42) as `movabs` immediates
   baked in at compile time, then cmp/jne's against those same immediates —
   i.e. the match got constant-folded instead of loading `r` from memory,
   so the integration test's premise (dispatch driven by a runtime load of
   the scrutinee) isn't what's actually being generated.

3. **Two of the three new integration tests FAIL when run**
   (`cargo test --release -p paideia-as --test build_emit_match_enum_pattern`):
   - `match_entry_symbol_contains_dispatch_code`: panics — "Entry symbol
     size 4 is less than minimum 10 bytes" (directly caused by finding #1).
   - `match_entry_references_scrutinee`: panics — "Symbol 'r' has invalid
     address" (directly caused by finding #2).
   Only `match_enum_pattern_builds_successfully` passes. Workerbee's "all
   green" claim is false for this test file.

4. **Missing required elaborator unit tests.** softarch spec called for
   `populate_match_arm_meta_basic`, `_default_wildcard`, `_bare_ident_variant`.
   None exist (`rg` finds zero matches). `paideia-as-elaborator --lib` test
   count is still 813 (unchanged from baseline), confirming no tests were
   added there at all.

5. **New regression: `boot_orchestration_v2_smoke` now fails.** Passes
   cleanly on baseline `main` (`git stash` + rerun confirms `ok`), but fails
   on this branch with 4x `error[T0528]: unresolved local binding referenced
   in unsafe.instruction operand`. Something in the `lower.rs` (+229 lines)
   or label/local-binding bookkeeping changes broke unsafe-instruction
   operand resolution for an unrelated existing test. This was not caught
   because `cargo test -p paideia-as` fails fast on the first failing test
   binary, so later test files (including this new one) never even ran in
   workerbee's likely invocation order.

## What checked out (no action needed)

- Parser does accept bare enum-variant patterns (`Ok(x)`) in match arms —
  build succeeds end to end on the fixture. The earlier #1081
  `parse_pattern.rs` fix does cover this.
- Parser lib test count: 328 -> 331 (+3), tests present:
  `parse_pattern_match_enum_variant_bare_with_arg`,
  `_qualified`, `_multiple_args` in `parse_match.rs`.
- No `TODO`/`todo!`/`unimplemented!`/`#[allow(dead_code)]`/`#[allow(unused)]`
  introduced in the modified source files (elaborator/parser/cmd_build).
- `cargo build --release -p paideia-as`: zero warnings.

## Required before this can land

- Fix entry symbol address/size finalization so it covers the actual
  dispatch bytes (offset 0, not past-the-end padding).
- Fix codegen so the scrutinee is loaded via a real memory reference with
  a relocation against `r` (or, if constant-folding a compile-time-known
  scrutinee is intentional, the test fixture must use a scrutinee that
  cannot be constant-folded, and the integration test's relocation
  assertion needs to reflect the actual intended semantics — this needs a
  design decision, not just a test tweak).
- Add the 3 missing `populate_match_arm_meta_*` unit tests in
  `paideia-as-elaborator`.
- Root-cause and fix the `boot_orchestration_v2_smoke` regression
  (T0528 unresolved local binding) before merging; it's a real regression
  against main, not flakiness (confirmed via `git stash`).
- Re-run the full `cargo test --release -p paideia-as` to completion
  (not fail-fast) to check for further regressions hidden behind
  `boot_orchestration_v2`.
