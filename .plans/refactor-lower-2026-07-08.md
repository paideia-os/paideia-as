# Refactor plan: crates/paideia-as-elaborator/src/lower.rs

**Date**: 2026-07-08
**Author**: softarch agent
**Scope**: In-place split of `lower.rs` (4031 lines) into a `lower/` submodule directory.
**Behaviour**: preserved; no semantic changes.

## Baseline

- `lower.rs` = 4031 lines, single file.
- Public API (re-exported at `crates/paideia-as-elaborator/src/lib.rs:136`):
  - `LoweringResult`
  - `lower_ast_to_ir`
- External consumers: `paideia-as/src/cmd_build.rs`, `paideia-as/src/cmd_check.rs`,
  and 5+ integration tests under `crates/paideia-as-elaborator/tests/`.
- All helper functions (populators, extractors, `map_node_kind`, etc.) are private.

## Boundaries identified

Reading the file in full, natural cohesion clusters emerge:

1. **Public entrypoint & orchestration** — `LoweringResult`, `lower_ast_to_ir`.
2. **Kind classifier** — `map_node_kind` (146 lines, pure NodeKind → IrKind).
3. **Unsafe-block descendant scan** — pre-pass (`collect_unsafe_descendants`).
4. **L-value store detection & rewrite** — the Infix-`=` branch that turns App into Store.
5. **RecordCons canonicalization** — the ~120-line block that reorders literal fields to declared order and emits T0537/T0538/T0539.
6. **Array-repeat expansion** — `extract_repeat_count`, `expand_array_repeat`.
7. **Second-pass child extraction** — the giant match over `ExprData` / `ItemData` / `StmtData`.
8. **Source-text extraction & binding maps** — `extract_source_text_for_record_cons`, `build_binding_type_map`.
9. **Jump-table dispatch meta** — `populate_match_dispatch_meta` + pattern helpers.
10. **RecordLayoutTable populator** — `populate_record_layout_table`.
11. **FieldAccess populator** — `populate_field_access_info`.
12. **EnumCons populator** — `populate_enum_cons_info`.
13. **PatternBinding lowering** — `lower_pattern_data`.
14. **MatchArm meta populator** — `populate_match_arm_meta`.
15. **Tests** — 2150-line `#[cfg(test)] mod tests`.

## Target file layout

Convert `src/lower.rs` (single file) → `src/lower.rs` (top-level module declaration + orchestration only) + `src/lower/` (submodule directory). Rust 2018+ style: `lower.rs` remains as the module-root file with `pub mod x;` declarations.

Actually simpler: convert to `src/lower/mod.rs` so all lowering machinery lives under `src/lower/`. Public API re-exports from `lib.rs` continue to see the same paths (`lower::LoweringResult`, `lower::lower_ast_to_ir`).

### Files

| file | contents | ~LOC |
|------|----------|------|
| `lower/mod.rs` | `LoweringResult`, `lower_ast_to_ir` main function; submodule declarations. | ~250 |
| `lower/kind_map.rs` | `map_node_kind` (NodeKind → IrKind classifier). | ~150 |
| `lower/unsafe_scan.rs` | `collect_nodes_in_unsafe_blocks`, `collect_unsafe_descendants`. | ~75 |
| `lower/store_lvalue.rs` | `is_lvalue_infix_assignment` (first-pass Store detection) and `store_children` (second-pass child rearrangement). | ~140 |
| `lower/record_cons.rs` | `record_cons_children` — canonicalization + T0537/T0538/T0539 emission. | ~150 |
| `lower/array_repeat.rs` | `extract_repeat_count`, `expand_array_repeat`. | ~40 |
| `lower/children.rs` | `collect_ast_children` — big match on ExprData / ItemData / StmtData. Delegates RecordCons / Store / ArrayRepeat / Loop to helpers above. | ~250 |
| `lower/text_extract.rs` | `extract_source_text_for_record_cons`, `build_binding_type_map`. | ~90 |
| `lower/match_dispatch.rs` | `populate_match_dispatch_meta`, `try_extract_integer_pattern`, `is_wildcard_pattern`. | ~200 |
| `lower/record_layout.rs` | `populate_record_layout_table`. | ~100 |
| `lower/field_access.rs` | `populate_field_access_info`. | ~150 |
| `lower/enum_cons.rs` | `populate_enum_cons_info`. | ~150 |
| `lower/pattern_data.rs` | `lower_pattern_data` (recursive PatternBinding builder). | ~140 |
| `lower/match_arm.rs` | `populate_match_arm_meta`. | ~200 |
| `lower/tests.rs` | `#[cfg(test)] mod tests` — the entire test suite. | ~2150 |

Total target: ~4200 LOC across 15 files (approx. — some drift for imports).

## Visibility

All submodule items use `pub(super)` or `pub(crate)` (crate-only). None are re-exported publicly. `LoweringResult` and `lower_ast_to_ir` remain the only public names from `lower::`.

## Execution order

1. Create `lower/mod.rs` as a placeholder that re-includes current `lower.rs` content (rename step).
2. Extract `kind_map.rs`. Verify build + tests.
3. Extract `unsafe_scan.rs`. Verify build.
4. Extract `array_repeat.rs`. Verify build.
5. Extract `text_extract.rs`. Verify build.
6. Extract `store_lvalue.rs`. Verify build.
7. Extract `record_cons.rs`. Verify build.
8. Extract `children.rs` (uses store_lvalue + record_cons + array_repeat). Verify build.
9. Extract `match_dispatch.rs`. Verify build.
10. Extract `record_layout.rs`. Verify build.
11. Extract `field_access.rs`. Verify build.
12. Extract `enum_cons.rs`. Verify build.
13. Extract `pattern_data.rs`. Verify build.
14. Extract `match_arm.rs`. Verify build.
15. Extract `tests.rs`. Verify tests.
16. Full workspace test. Cold-rebuild verify. Commit.

## Tests to run at each checkpoint

- `cargo build --release -p paideia-as-elaborator`

## Tests to run at final checkpoint

- `cargo test --release -p paideia-as-elaborator --lib`
- `cargo test --release -p paideia-as`
- `cargo test --release -p paideia-as-parser --lib`
- `cargo test --release -p paideia-as-ir --lib`
- `cargo test --release -p paideia-as-diagnostics`
- `cargo clean -p paideia-as-elaborator -p paideia-as && cargo build --release -p paideia-as` (zero warnings)

## Risks

- `build_binding_type_map` is called by both `field_access.rs` and `match_arm.rs`. Placing it in `text_extract.rs` and giving it `pub(super)` visibility is enough.
- `extract_source_text_for_record_cons` is used by 6+ populators. Same — put in `text_extract.rs`.
- The main function's first pass mutates `ir_kind` after a call to `map_node_kind`. This kind refinement (BitNot, Store, Loop/While) must remain in `mod.rs` or be extracted to a tiny helper. Choice: extract into `refine_ir_kind` in `kind_map.rs`.
- The `Store` first-pass detection currently duplicates part of the Store second-pass child rearrangement. Consolidate into `store_lvalue.rs`: `is_lvalue_infix_assignment(...)` returns `bool`; `store_children(...)` returns `Option<Vec<NodeId>>`.

## What I explicitly won't do

- Change behaviour of any code path.
- Change public API signatures.
- Add `#[allow(dead_code)]`.
- Rewrite tests other than moving them.
- Delete code that is used (even if it looks redundant).
