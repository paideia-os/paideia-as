# Test restructure — 2026-07-08

Goal: reshape the paideia-as test surface so it (1) compiles faster, (2) has
less boilerplate to maintain, and (3) is organized topically rather than by
ticket number. Research-only plan — execution is a separate work item.

Author: softarch. Scope: whole workspace, all crates.

---

## 1. Current state (measured 2026-07-08 @ HEAD ff624b8)

### Integration-test binary counts (each `tests/*.rs` file = one linked binary)

| Crate                       | Integ files | Notes |
|-----------------------------|-------------|-------|
| `paideia-as`                | **89**      | build_emit_*, pa_r17_*, boot_*, paideia_os_*, plus one-offs |
| `paideia-as-encoder`        | **47**      | SDM byte-exact tests, mostly identical scaffolding |
| `paideia-as-elaborator`     | **16**      | already partially hierarchical (emit_walker_tests.rs, unsafe_walker_tests.rs) |
| `paideia-as-parser`         | 7           |       |
| `paideia-as-diagnostics`    | 6           |       |
| `paideia-as-emitter-elf`    | 5           |       |
| `paideia-as-ast`            | 2           |       |
| `paideia-as-lexer`          | 2           |       |
| `paideia-as-types`          | 1           |       |
| `paideia-as-effects`        | 1           |       |
| `paideia-as-dwarf`          | 1           |       |
| `paideia-as-emitter-pax`    | 1           |       |
| `paideia-stdlib`            | 1           |       |
| `paideia-lsp`               | 1           |       |
| `paideia-pq-sign`           | 1           |       |
| `paideia-fmt`               | 1           |       |
| `paideia-as-ir`             | 0           | all lib tests |
| **Total integration binaries** | **~183**   | plus lib-test + bin-test binaries per crate |

`target/release/deps/` currently caches ~250 uniquely-named binaries (integ +
lib + doc + bin variants). Each carries its own copy of the workspace linker
work.

### Test-function counts

- `paideia-as` integration: 248 tests across 89 files (~15.9k LoC).
- `paideia-as-encoder` integration: 603 tests across 47 files (~13.3k LoC).
- `paideia-as-elaborator` integration: 104 tests across 16 files.
- `paideia-as-elaborator` lib: 838 tests (already hierarchical under
  `src/**/tests.rs`; leave alone).
- `paideia-as-parser` lib: 335. `paideia-as-ir` lib: 421.

### Shared-boilerplate duplication (paideia-as crate)

- **58** files declare their own `fn cargo_run(args: &[&str])` verbatim.
- **58** files declare their own `fn build_emit_data(name: &str) -> PathBuf`
  pointing at `../../tests/build-emit/`.
- **61** files re-parse the emitted ELF via `object::File::parse(&*bytes)`.
- **19** files hard-code `/tmp/...` paths (race-prone under `cargo test -j`).
- **4** files use `tempfile::TempDir` (modern pattern).

No shared harness module exists today. `crates/paideia-as-test/` is unrelated
— it is a runtime discoverer of `#[test]` items inside `.pdx` sources, not a
Rust-test scaffold.

### Fixture layout

- `tests/build-emit/*.pdx` at workspace root: **78 files** (+ 3 `.expected*.txt`).
- `tests/pa_r17_1074_t0535_record_field/` at root: 3 ticket-scoped `.pdx`.
- `tests/pa_r17_003b_t0535/` at root: ticket-scoped fixtures.
- `crates/paideia-as/tests/data/*.pdx`: 12 legacy fixtures.
- `crates/paideia-as/tests/cross_file/`: 2 fixtures for the cross-file test.
- `crates/paideia-as/tests/fixtures/`: 2 items (`pa7c_link.ld`, `stub_partner.S`).
- Plus 20+ **project-corpus** dirs under `tests/` (opt-regression/,
  linearity-regression/, effects-corpus/, etc.) which are Paideia projects,
  not Rust fixtures — leave untouched.

### Diagnosis

The Rust link step dominates test-cycle time. With ~183 integration binaries
+ ~30 lib-test binaries, we pay N × per-binary link overhead every time the
workspace crate graph changes. On a typical incremental rebuild after a
touched shared dependency, this is minutes, not seconds. Consolidating a
sub-tree of `tests/*.rs` files into a single entry-point binary (with
`mod` sub-modules) removes exactly one link per collapsed file.

The `cargo_run`/`build_emit_data` duplication in `paideia-as` is a
correctness liability too: any change to build invocation (new flag, new env
var, replacing shell-out with in-process `paideia_as::run_build(...)`)
requires 58 edits.

---

## 2. Target state (directory tree per crate)

### 2a. `paideia-as`  (89 binaries -> ~4 binaries)

```
crates/paideia-as/tests/
    integration.rs                 # single entry: mod build_emit; mod boot; mod diagnostics; mod cli; mod misc;
    common/
        mod.rs                     # re-exports helpers below
        harness.rs                 # run_build(fixture, opts) -> BuildOutcome
        elf.rs                     # ElfView newtype + assertions
        fixture.rs                 # ScratchDir (tempfile-backed), fixture path resolver
        expected.rs                # parse *.expected_bytes.txt / *.expected_objdump.txt
    build_emit/
        mod.rs
        smoke.rs                   # <- build_emit_smoke.rs
        control_flow.rs            # <- build_emit_control_flow_corpus.rs, back_to_back_labels.rs, label_patches.rs
        call.rs                    # <- build_emit_call_sym.rs, build_emit_pa8_call_st_value.rs,
                                   #    build_emit_pa8_cross_module_call.rs, build_emit_pa8_st_value.rs,
                                   #    build_emit_pa8_single_fn_regression.rs
        field.rs                   # <- build_emit_field_access_call.rs,
                                   #    build_emit_field_read{,_u32,_multi_field}.rs,
                                   #    build_emit_field_write_u32.rs
        record.rs                  # <- build_emit_record_reorder.rs, build_emit_enum_record_payload.rs
        enum_cons.rs               # <- build_emit_enum_cons.rs, build_emit_match_enum_pattern.rs
        rep_string.rs              # <- build_emit_rep_movsb.rs, build_emit_rep_stosq.rs
        imm.rs                     # <- build_emit_imm64_top_bit.rs, build_emit_pa10_006i_imm.rs
        pa7c_witness.rs            # <- build_emit_pa7c_{expr_surface,plt32_witness,symbol_export,unsafe_body}.rs
        pa10_006.rs                # <- build_emit_pa10_006k_ljmp.rs, _006l_inout.rs
        pa_r17.rs                  # <- build_emit_pa_r17_004_*, _005_*.rs (+ move pa_r17_009a_nested_patterns.rs,
                                   #    pa_r17_010c_record_layout_emit.rs here from top-level)
        pa8_shapes.rs              # <- build_emit_pa8_{mixed_shapes,m1_001b_lambda_params,m3_004}.rs
        misc.rs                    # small one-offs: bss_reloc.rs, cap_smoke.rs, imm64_top_bit.rs left over,
                                   #    pa10_007_data_symbol_names.rs, pa10_008_multi_fn_offsets.rs,
                                   #    pa_r12_004_pure_body.rs, pa_r13_008_mut_literal_data.rs,
                                   #    encoder_strict.rs
        unsafe_diag.rs             # <- build_emit_unsafe_call_stmt_diagnostic.rs,
                                   #    build_emit_unsafe_stmt_kinds_diagnostic.rs
    boot/
        mod.rs
        observable.rs              # <- boot_observable_smoke.rs
        orchestration.rs           # <- boot_orchestration_smoke.rs, boot_orchestration_v2.rs,
                                   #    pa8_m7_001_checkpoint2_orchestration.rs
        paideia_os.rs              # <- paideia_os_checkpoint2_m2_canary.rs, m3_829_byte_snapshot.rs,
                                   #    m4_003_unsafe_regression.rs, m5_835_lapic_ipi_snapshot.rs,
                                   #    phase1_rebuild.rs, r1_5_r2_5_rebuild.rs
        qemu_smoke.rs              # <- qemu_smoke.rs (gated by target_os = "linux")
    milestone/
        mod.rs
        pa10.rs                    # <- pa10_006y_align_{default,pml4_bss}.rs, pa10_013_local_stub_no_collision.rs
        pa14.rs                    # <- pa14_008_ring_synthesis.rs, pa14_012_driver_corpus.rs
        pa15.rs                    # <- pa15_009_jump_table.rs, pa15_011_udp_echo_canary.rs
        pa16.rs                    # <- pa16_012_cow_fs_canary.rs
        pa17.rs                    # <- pa17_003_fnptr_addr_of.rs, pa17_004_indirect_call.rs
        pa_r17.rs                  # <- pa_r17_003b_t0535.rs, _004c_call_rip_fnptr.rs,
                                   #    _010_record_fnptr_reloc.rs, _014_lea_addend_i32_guard.rs,
                                   #    _015_call_rip_rel.rs, _1074_t0535_record_field.rs, _012_string_literal.rs
    codegen/
        mod.rs
        cross_file_reloc.rs        # <- cross_file_relocation.rs, pub_cross_module_link.rs
        note_paideia.rs            # <- note_paideia_layouts.rs
        opt_peephole.rs            # <- opt_peephole_smoke.rs
        symtab.rs                  # <- symtab_underscore_prefix.rs
        relocation.rs              # <- relocation_linking.rs
        array_widths.rs            # <- array_element_widths.rs
        bss.rs                     # <- bss_emission.rs
        data_rodata.rs             # <- data_rodata.rs
        e2e_elf.rs                 # <- e2e_elf.rs
        cap_runtime.rs             # <- cap_smoke_runtime.rs
        checkpoint2.rs             # <- checkpoint2_orchestration test (moved out of the .pdx-named file)
    cli.rs                         # <- cli.rs (standalone; wide 16-test file, keep as-is under integration.rs)
    abi_pdx.rs                     # <- abi_pdx.rs (parser sanity)
    examples_corpus.rs             # <- examples_corpus.rs (corpus sweep)
    build_unsafe.rs                # <- build_unsafe.rs
```

Approximate reduction: **89 -> 1 integration binary** (all under
`integration.rs`) if all sub-modules are declared in it; realistically we
partition into 4 or 5 top-level `tests/*.rs` entry points if we want parallel
`cargo test` execution to still shard across cores:

- `tests/integration.rs`  — build_emit + codegen + misc small (~130 tests)
- `tests/boot.rs`          — boot/ + paideia_os/ + qemu_smoke (~30 tests, some `#[cfg(target_os = "linux")]`)
- `tests/milestone.rs`     — milestone/ (~35 tests, larger link times)
- `tests/cli.rs`           — kept solo (already dense at 16 tests)

That is **4 binaries instead of 89** — an ~85 fewer per-test link.

### 2b. `paideia-as-encoder`  (47 -> 1-3 binaries)

```
crates/paideia-as-encoder/tests/
    integration.rs                 # single entry
    common/
        mod.rs
        vectors.rs                 # encode(mnemonic, operands) -> Vec<u8>, encode_reg1, ...
        roundtrip.rs               # iced-x86 decoding helpers
    arith/
        mod.rs
        add_sub.rs                 # <- lock_add_sub.rs, lock_inc_add_abs.rs, dec_r64.rs, ...
        bitwise.rs                 # <- bitwise_arith_real.rs, or_r{32,64}_imm*.rs, and family
        imul.rs                    # <- imul_real.rs
        shifts.rs                  # <- rol_ror.rs, setcc_reg8.rs (setcc lives here)
    bits/
        bt_family.rs               # <- bt_family.rs, lock_bt_family.rs
        bsf_bsr.rs                 # <- bsf_bsr_tzcnt.rs
        bswap.rs                   # <- bswap_r32.rs, bswap_r64.rs
        test.rs                    # <- test_reg.rs, test_reg_imm.rs
    mov/
        narrow.rs                  # <- mov_narrow.rs, mov_mem_narrow{,_store}.rs
        abs.rs                     # <- mov_abs32.rs, mov_mem_abs_disp32.rs
        load.rs                    # <- mov_r32_load.rs
        dr.rs                      # <- mov_dr_dispatch.rs
    memory/
        fences.rs                  # <- mem_fences.rs
        cache.rs                   # <- cache_ops.rs, prefetch_family.rs, pause.rs, cld_std.rs
        gs_relative.rs             # <- gs_relative.rs
        lea.rs                     # <- lea_mode32.rs
    control_flow/
        call.rs                    # <- call_indirect.rs
        ljmp.rs                    # <- ljmp_two_operand.rs
    lock/
        xadd.rs                    # <- lock_xadd.rs
        aor.rs                     # <- lock_and_or_xor.rs
    system/
        lgdt.rs                    # <- lgdt_abs32.rs
    audit/
        sib_rex.rs                 # <- sib_rex_audit.rs
        v15_witness.rs             # <- v15_survey_witness.rs
        reloc_offsets.rs           # <- reloc_offset_4_sites.rs
```

**47 -> 1 binary** if all under `integration.rs`.

### 2c. `paideia-as-elaborator`  (16 -> 2-3 binaries)

Two entry points already exist (`emit_walker_tests.rs`, `unsafe_walker_tests.rs`).
Fold everything else into `integration.rs`:

```
crates/paideia-as-elaborator/tests/
    integration.rs                 # mod stdlib_lowering; mod call_emit; mod control_flow; mod byte_offset; mod pa_r_series;
    common/mod.rs
    stdlib_lowering/               # <- stdlib_{mmio,pause,bytes,percpu,checksum}_lowering.rs
    call_emit/                     # <- emit_call_sysvregs.rs, emit_call_recipe_labels.rs
    control_flow/                  # <- pa_r17_012_pure_control_flow.rs, pa_r17_013_match_in_return.rs
    byte_offset/                   # <- byte_offset_tests.rs (folded under new common)
    pa_r_series/                   # <- pa_r15_009c_jump_table_fixture.rs, string_lit_emit.rs,
                                   #    imm64_bitops_expansion.rs, lower_unsafe_tests.rs
    emit_walker_tests.rs           # keep as-is (already hierarchical)
    emit_walker/                   # keep as-is
    unsafe_walker_tests.rs         # keep as-is (already hierarchical)
    unsafe_walker/                 # keep as-is
```

**16 -> 3 binaries** (integration + emit_walker + unsafe_walker).

### 2d. `paideia-as-encoder` sibling crates

Smaller collapse candidates (each still worth a single `tests/integration.rs`):

- `paideia-as-parser` (7 -> 1). Group: `snapshots.rs` (snapshots_modules,
  example_files), `attrs.rs` (align_attr_errors, ring_attr_errors,
  inner_attr_bits), `syntax.rs` (empty_fn_args, ljmp_instruction).
- `paideia-as-diagnostics` (6 -> 1). Group by concern: `render.rs`
  (render_human, sarif, sink), `codes.rs` (opt_codes_present, proptest_codes,
  m0307_retired).
- `paideia-as-emitter-elf` (5 -> 1). Topical: `note.rs` (pvh_note),
  `dispatch.rs` (fnptr_dispatch), `encoding.rs` (encoder_facade, sdm_vectors,
  indexed_load_store_smoke).
- `paideia-as-ast` (2 -> 1). `expr_integration.rs` + `items_integration.rs`
  -> `mod expr; mod items;` under `tests/integration.rs`.
- `paideia-as-lexer` (2 -> 1). `lex_driver.rs` + `hashbang_attr.rs`.
- All single-file crates (`paideia-as-types`, `paideia-as-effects`,
  `paideia-as-dwarf`, `paideia-as-emitter-pax`, `paideia-stdlib`,
  `paideia-lsp`, `paideia-pq-sign`, `paideia-fmt`) already have one binary.
  No action.

### 2e. Lib tests

`src/**/tests.rs` submodules across all crates are unchanged. They are
already the fastest shape (single-binary lib-test per crate).

---

## 3. Consolidation table — `paideia-as` (89 -> 4 binaries)

| Old `crates/paideia-as/tests/<file>.rs`               | New module path in `tests/integration.rs` tree |
|-------------------------------------------------------|------------------------------------------------|
| build_emit_smoke.rs                                   | build_emit/smoke.rs                            |
| build_emit_back_to_back_labels.rs                     | build_emit/control_flow.rs                     |
| build_emit_control_flow_corpus.rs                     | build_emit/control_flow.rs                     |
| build_emit_label_patches.rs                           | build_emit/control_flow.rs                     |
| build_emit_call_sym.rs                                | build_emit/call.rs                             |
| build_emit_pa8_call_st_value.rs                       | build_emit/call.rs                             |
| build_emit_pa8_cross_module_call.rs                   | build_emit/call.rs                             |
| build_emit_pa8_st_value.rs                            | build_emit/call.rs                             |
| build_emit_pa8_single_fn_regression.rs                | build_emit/call.rs                             |
| build_emit_pa8_unsafe_block_st_value.rs               | build_emit/call.rs                             |
| build_emit_field_access_call.rs                       | build_emit/field.rs                            |
| build_emit_field_read.rs                              | build_emit/field.rs                            |
| build_emit_field_read_u32.rs                          | build_emit/field.rs                            |
| build_emit_field_read_multi_field.rs                  | build_emit/field.rs                            |
| build_emit_field_write_u32.rs                         | build_emit/field.rs                            |
| build_emit_record_reorder.rs                          | build_emit/record.rs                           |
| build_emit_enum_record_payload.rs                     | build_emit/record.rs                           |
| build_emit_enum_cons.rs                               | build_emit/enum_cons.rs                        |
| build_emit_match_enum_pattern.rs                      | build_emit/enum_cons.rs                        |
| build_emit_rep_movsb.rs                               | build_emit/rep_string.rs                       |
| build_emit_rep_stosq.rs                               | build_emit/rep_string.rs                       |
| build_emit_imm64_top_bit.rs                           | build_emit/imm.rs                              |
| build_emit_pa10_006i_imm.rs                           | build_emit/imm.rs                              |
| build_emit_pa10_006k_ljmp.rs                          | build_emit/pa10_006.rs                         |
| build_emit_pa10_006l_inout.rs                         | build_emit/pa10_006.rs                         |
| build_emit_pa7c_expr_surface.rs                       | build_emit/pa7c_witness.rs                     |
| build_emit_pa7c_plt32_witness.rs                      | build_emit/pa7c_witness.rs                     |
| build_emit_pa7c_symbol_export.rs                      | build_emit/pa7c_witness.rs                     |
| build_emit_pa7c_unsafe_body.rs                        | build_emit/pa7c_witness.rs                     |
| build_emit_pa8_m1_001b_lambda_params.rs               | build_emit/pa8_shapes.rs                       |
| build_emit_pa8_m3_004.rs                              | build_emit/pa8_shapes.rs                       |
| build_emit_pa8_mixed_shapes.rs                        | build_emit/pa8_shapes.rs                       |
| build_emit_pa_r12_004_pure_body.rs                    | build_emit/misc.rs                             |
| build_emit_pa_r13_008_mut_literal_data.rs             | build_emit/misc.rs                             |
| build_emit_pa_r17_004_identity_multi_param.rs         | build_emit/pa_r17.rs                           |
| build_emit_pa_r17_005_flat_multi_param.rs             | build_emit/pa_r17.rs                           |
| build_emit_pa10_007_data_symbol_names.rs              | build_emit/misc.rs                             |
| build_emit_pa10_008_multi_fn_offsets.rs               | build_emit/misc.rs                             |
| build_emit_bss_reloc.rs                               | build_emit/misc.rs                             |
| build_emit_cap_smoke.rs                               | build_emit/misc.rs                             |
| build_emit_encoder_strict.rs                          | build_emit/misc.rs                             |
| build_emit_phase6_cr_moves.rs                         | build_emit/misc.rs                             |
| build_emit_unsafe_call_stmt_diagnostic.rs             | build_emit/unsafe_diag.rs                      |
| build_emit_unsafe_stmt_kinds_diagnostic.rs            | build_emit/unsafe_diag.rs                      |
| boot_observable_smoke.rs                              | boot/observable.rs                             |
| boot_orchestration_smoke.rs                           | boot/orchestration.rs                          |
| boot_orchestration_v2.rs                              | boot/orchestration.rs                          |
| pa8_m7_001_checkpoint2_orchestration.rs               | boot/orchestration.rs                          |
| paideia_os_checkpoint2_m2_canary.rs                   | boot/paideia_os.rs                             |
| paideia_os_m3_829_byte_snapshot.rs                    | boot/paideia_os.rs                             |
| paideia_os_m4_003_unsafe_regression.rs                | boot/paideia_os.rs                             |
| paideia_os_m5_835_lapic_ipi_snapshot.rs               | boot/paideia_os.rs                             |
| paideia_os_phase1_rebuild.rs                          | boot/paideia_os.rs                             |
| paideia_os_r1_5_r2_5_rebuild.rs                       | boot/paideia_os.rs                             |
| qemu_smoke.rs                                         | boot/qemu_smoke.rs                             |
| pa10_006y_align_default.rs                            | milestone/pa10.rs                              |
| pa10_006y_align_pml4_bss.rs                           | milestone/pa10.rs                              |
| pa10_013_local_stub_no_collision.rs                   | milestone/pa10.rs                              |
| pa14_008_ring_synthesis.rs                            | milestone/pa14.rs                              |
| pa14_012_driver_corpus.rs                             | milestone/pa14.rs                              |
| pa15_009_jump_table.rs                                | milestone/pa15.rs                              |
| pa15_011_udp_echo_canary.rs                           | milestone/pa15.rs                              |
| pa16_012_cow_fs_canary.rs                             | milestone/pa16.rs                              |
| pa17_003_fnptr_addr_of.rs                             | milestone/pa17.rs                              |
| pa17_004_indirect_call.rs                             | milestone/pa17.rs                              |
| pa_r12_001_string_literal.rs                          | milestone/pa_r17.rs (rename bucket "pa_r_")    |
| pa_r17_003b_t0535.rs                                  | milestone/pa_r17.rs                            |
| pa_r17_004c_call_rip_fnptr.rs                         | milestone/pa_r17.rs                            |
| pa_r17_009a_nested_patterns.rs                        | milestone/pa_r17.rs                            |
| pa_r17_010_record_fnptr_reloc.rs                      | milestone/pa_r17.rs                            |
| pa_r17_010c_record_layout_emit.rs                     | milestone/pa_r17.rs                            |
| pa_r17_014_lea_addend_i32_guard.rs                    | milestone/pa_r17.rs                            |
| pa_r17_015_call_rip_rel.rs                            | milestone/pa_r17.rs                            |
| pa_r17_1074_t0535_record_field.rs                     | milestone/pa_r17.rs                            |
| abi_pdx.rs                                            | codegen/abi_pdx.rs                             |
| array_element_widths.rs                               | codegen/array_widths.rs                        |
| bss_emission.rs                                       | codegen/bss.rs                                 |
| cap_smoke_runtime.rs                                  | codegen/cap_runtime.rs                         |
| cli.rs                                                | codegen/cli.rs (or leave as own binary — dense)|
| cross_file_relocation.rs                              | codegen/cross_file_reloc.rs                    |
| pub_cross_module_link.rs                              | codegen/cross_file_reloc.rs                    |
| data_rodata.rs                                        | codegen/data_rodata.rs                         |
| e2e_elf.rs                                            | codegen/e2e_elf.rs                             |
| examples_corpus.rs                                    | codegen/examples_corpus.rs                     |
| note_paideia_layouts.rs                               | codegen/note_paideia.rs                        |
| opt_peephole_smoke.rs                                 | codegen/opt_peephole.rs                        |
| relocation_linking.rs                                 | codegen/relocation.rs                          |
| symtab_underscore_prefix.rs                           | codegen/symtab.rs                              |
| build_unsafe.rs                                       | build_emit/unsafe_diag.rs (or codegen/)        |
| checkpoint2_orchestration.pdx (mis-placed?)           | move to `tests/fixtures/boot/` (not a Rust file) |

Encoder + elaborator + smaller crates follow the same shape — see 2b–2d.

---

## 4. Shared harness API

New module: `crates/paideia-as/tests/common/`. **This is `common/mod.rs`,
not `common.rs`** — Cargo treats top-level `tests/*.rs` as separate test
binaries, so a `common.rs` sibling would compile as an extra binary and
still emit "unused" warnings. The `mod.rs` convention avoids both.

### 4.1 `common::harness`

```rust
pub struct BuildOutcome {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub artifact: Option<PathBuf>,   // Some(_) when --emit produced a file
}

impl BuildOutcome {
    pub fn assert_ok(&self) -> &Self;                     // panics with stderr
    pub fn assert_fail_with(&self, code: &str) -> &Self;  // asserts a diagnostic code fired
    pub fn elf(&self) -> ElfView<'_>;                     // parses artifact via `object`
    pub fn text_bytes(&self) -> Vec<u8>;
    pub fn symbols(&self) -> Vec<SymbolInfo>;
    pub fn rodata_slice(&self, name: &str) -> Vec<u8>;
}

pub struct BuildOpts {
    pub emit: EmitFmt,           // Elf64 | PaxV1 | ...
    pub extra_args: Vec<&'static str>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(&'static str, &'static str)>,
}

pub fn run_build(fixture: &str) -> BuildOutcome;                        // defaults: elf64, ScratchDir
pub fn run_build_with(fixture: &str, opts: BuildOpts) -> BuildOutcome;
pub fn run_cli(args: &[&str]) -> BuildOutcome;                          // for cli.rs style tests
```

Notes:
- `run_build` uses `env!("CARGO_BIN_EXE_paideia-as")` (Cargo populates this
  for integration tests of a crate that defines `[[bin]]`) rather than
  `cargo run`. That skips the per-test recompile-check and shaves seconds off
  every test.
- Output path is a `tempfile::TempDir` owned by `BuildOutcome`, dropped at
  end of scope. This eliminates the `/tmp/test_*.o` racing.

### 4.2 `common::fixture`

```rust
pub struct ScratchDir { .. }        // owns a tempfile::TempDir
impl ScratchDir {
    pub fn new(tag: &str) -> Self;
    pub fn path(&self) -> &Path;
    pub fn artifact(&self, name: &str) -> PathBuf;   // .join(name)
}

pub fn fixture_path(name: &str) -> PathBuf;          // workspace-root tests/build-emit/<name>
pub fn crate_fixture(rel: &str) -> PathBuf;          // crates/paideia-as/tests/<rel>
```

### 4.3 `common::elf`

Thin newtype around `object::File<'a>` that returns typed views:

```rust
pub struct ElfView<'data> { file: object::File<'data> }
impl<'a> ElfView<'a> {
    pub fn magic_ok(&self) -> bool;
    pub fn text(&self) -> &'a [u8];
    pub fn section(&self, name: &str) -> Option<&'a [u8]>;
    pub fn symbol(&self, name: &str) -> Option<SymbolInfo>;
    pub fn symbols(&self) -> impl Iterator<Item = SymbolInfo> + '_;
    pub fn relocations(&self, section: &str) -> Vec<RelocInfo>;
    pub fn note(&self, name: &str, kind: u32) -> Option<&'a [u8]>;
}
```

### 4.4 `common::expected`

Parser for the `tests/build-emit/*.expected_bytes.txt` format (already
duplicated in `build_emit_smoke.rs`). Move it here once.

---

## 5. Fixture regrouping decision

**Preferred: regroup topically under `tests/fixtures/`** at workspace root:

```
tests/
    fixtures/
        build-emit/            # <- current tests/build-emit/ verbatim
        boot/                  # boot_*.pdx, uart_smoke.pdx, ahci_fis_stub.pdx, checkpoint2_orchestration.pdx
        pa_r17/                # merge tests/pa_r17_003b_t0535/ + tests/pa_r17_1074_t0535_record_field/
        capabilities/          # cap_*.pdx (currently in build-emit)
        records/               # field_*, record_*, enum_*.pdx
        ... etc
```

This is **optional** and independent of the Rust-side consolidation. The
Rust harness talks to fixtures by path, so a rename is a single string change
per fixture — trivial once the harness is in place.

The project-corpus directories under `tests/` (`opt-regression/`,
`linearity-regression/`, `effects-corpus/`, `cross-build/`,
`self-hosting/`, etc.) are **untouched** — they are Paideia projects with
their own `Cargo.toml`-analogous layout.

If we defer the fixture regrouping, `common::fixture_path` still centralizes
the relative-path formula so any future move is one edit.

**Recommendation**: do the Rust-side consolidation first. Do fixture
regrouping as a follow-up ticket, only after the harness stabilizes.

---

## 6. Migration strategy

Split into one PR per crate (so review, blame, and revert stay localized).

### Order

1. **`paideia-as` (biggest win, biggest risk)** — do first while attention is
   fresh.
   1. Create `tests/common/{mod.rs,harness.rs,elf.rs,fixture.rs,expected.rs}`
      as new files. Do not touch existing tests.
   2. Add `tests/integration.rs` with `mod common;` + empty topical `mod`
      stubs (`mod build_emit { mod smoke; }` etc.). Verify it builds and
      runs (zero tests found) via `cargo test -p paideia-as --test integration`.
   3. Migrate ONE topical bucket end-to-end (`build_emit/smoke.rs`).
      - Move test bodies out of `tests/build_emit_smoke.rs` into
        `tests/build_emit/smoke.rs`, replacing local `cargo_run` with
        `common::harness::run_build`.
      - Delete `tests/build_emit_smoke.rs` only after test count matches
        (`cargo test -p paideia-as 2>&1 | grep 'running .* tests'`).
      - Commit.
   4. Repeat step 3 per bucket. `boot/` and `milestone/` in a second/third
      commit; encoder-crate work in a separate PR.
   5. When `paideia-as/tests/` only has `integration.rs`, `boot.rs`,
      `milestone.rs`, `cli.rs`, `abi_pdx.rs`, `examples_corpus.rs`,
      `build_unsafe.rs`, and the `common/` + topical dirs, the crate is
      done.
2. **`paideia-as-encoder`** — mechanical, no fixtures, low risk.
3. **`paideia-as-elaborator`** — extend the existing `emit_walker_tests.rs`
   pattern to cover the flat files.
4. **Small crates** (parser, diagnostics, emitter-elf, lexer, ast) — one PR
   each, tiny.

### Per-file mechanical rewrite

Before (each test file):
```rust
use std::path::PathBuf;
use std::process::Command;
fn build_emit_data(name: &str) -> PathBuf { .. }
fn cargo_run(args: &[&str]) -> std::process::Output { .. }
#[test]
fn foo() {
    let input = build_emit_data("foo.pdx");
    let output = cargo_run(&["build", input.to_str().unwrap(), "--emit", "elf64",
                             "-o", "/tmp/foo.o"]);
    assert!(output.status.success(), ...);
    // parse ELF, assert bytes/symbols
}
```

After:
```rust
use super::common::{fixture_path, run_build};
#[test]
fn foo() {
    let out = run_build("foo.pdx");
    out.assert_ok().elf().symbol("_start").expect("start symbol");
}
```

Each moved test loses ~30 lines of boilerplate.

### Verification gate at each step

- `cargo test --release -p paideia-as --no-run` — check every migrated bucket
  still compiles.
- `cargo test --release -p paideia-as 2>&1 | grep -c '^test '` — check test
  count is preserved (currently 248 integration tests).
- Compare stderr snapshots of any diagnostic-driven test (build_emit_unsafe_*)
  before/after; the diagnostic wording must not shift.

---

## 7. Estimated time savings

Assumptions (rough, workstation-class Linux, 8-core, mold or lld available):

- Rust link of a paideia-as integration test binary: **~1-2s** cold, ~0.5s
  warm (LTO off, opt-level=0 for tests). With release profile: 2-4s cold.
- Test *execution* is dominated by the child `cargo run` (or in-process
  `paideia-as`) invocation — the link savings are separate.

Debug-profile cycle (`cargo test -p paideia-as`), full rebuild:
- **Before**: 89 * ~1.5s = ~130s spent linking integration tests.
- **After (4 binaries)**: 4 * ~2s = ~8s. **Savings ≈ 2 minutes.**

Release-profile cycle (`cargo test --release`):
- **Before**: 89 * ~3s = ~4-5 minutes.
- **After**: 4 * ~4s = ~15s. **Savings ≈ 4 minutes.**

Cross-workspace (encoder + elaborator + paideia-as):
- Before: 89 + 47 + 16 = 152 integration binaries.
- After: 4 + 1 + 3 = 8 integration binaries.
- **Delta: 144 fewer link invocations per full test cycle.**

Incremental rebuilds after touching a workspace-shared crate: same
proportional savings.

Additional benefits (not timed):
- **Compile-time boilerplate reduction**: ~58 * ~30 lines = ~1740 lines
  of duplicated Rust deleted from `paideia-as/tests/`. Faster codegen.
- **Fewer tempfile leaks / /tmp collisions**: 19 hard-coded `/tmp/` paths
  are gone.
- **Single source of truth** for build invocation: swapping `cargo run` for
  in-process `paideia_as::run_build(...)` becomes a 1-file edit rather than
  58.

---

## 8. Risk list

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| A test relies on being its own binary for process-global state (e.g., set env, install a global panic hook, exit code). | Low | Grep for `std::env::set_var`, `std::panic::set_hook`, `std::process::exit` in `tests/*.rs` before moving. If any hit, keep that test as a solo binary. |
| Cross-file `use super::something` inside `tests/`. | None found in the grep. | Grep repeated in each migration commit. |
| `#[cfg(target_os = "linux")]` or similar gate on one test, would silently drop when merged into a file compiled unconditionally. | Medium | The `mod boot { mod qemu_smoke; }` sub-module keeps the cfg on the sub-mod, not the individual `#[test]`. Preserve verbatim. |
| Renamed test IDs break CI filters (`cargo test -- foo::bar` in scripts). | Medium | Grep the repo for `cargo test .* -- ` invocations. `tools/*.sh`, `.github/workflows/*.yml`, `scripts/*.sh`. Update filters or, preferably, keep the leaf test-function names identical. |
| `env!("CARGO_BIN_EXE_paideia-as")` returns nothing if `[[bin]]` isn't linked at test time — the crate has two bins (`paideia-as`, `pax-introspect`) so the identifier is `CARGO_BIN_EXE_paideia-as`. Confirm. | Low | Sanity-check in the harness bootstrapping step. |
| `paideia-as-elaborator` `mod emit_walker` name collides with the elaborator's own `emit_walker` re-export inside tests. | Low | Rename test-side mod to `emit_walker_it` if collision surfaces. |
| Doctests: `paideia-as` and friends may have `//!` examples that get compiled — restructure does not touch these. | None | No action. |
| Simultaneous work on #1088 (another workerbee editing test files) causes merge conflicts. | High during this window | Do the restructure PR after #1088 lands. Coordinate. |
| Fixture path drift: moving `.pdx` files without updating the harness constant breaks every test at once. | Medium | Do fixture regrouping in a separate PR from Rust-side consolidation. The Rust PR keeps every fixture reference bit-identical (via `common::fixture_path`). |
| Some ticket-scoped tests (pa_r17_1074_t0535_record_field) load fixtures from `tests/pa_r17_.../` at the workspace root — these paths need explicit handling in `fixture_path`. | Low | Add `fixture_group(dir, name)` helper. Enumerate the 2 known dirs. |
| Release-mode tests may embed absolute file paths in error messages. | Low | Grep for `env!("CARGO_MANIFEST_DIR")` in `#[should_panic]` or diagnostic snapshots. |
| `#[ignore]`-marked tests (14 across paideia-as) rely on being able to be run in isolation with `cargo test -- --ignored specific_name`. | None | Keep leaf `#[test] fn <name>()` intact. `--ignored` filter still works. |

---

## 9. Non-goals

- Not touching `src/**/tests.rs` submodule layout — those already work.
- Not migrating `paideia-as-test` runtime discoverer.
- Not migrating project-corpus dirs under `tests/` (they are Paideia
  projects, not Rust fixtures).
- Not touching CI YAML in this PR — just the tree.
- Not changing default test profile (debug vs release).
- Not introducing a new workspace test-support crate. `tests/common/mod.rs`
  is enough and avoids inter-crate cycles. If a second crate later needs the
  same harness (elaborator wants `run_build`), we promote to a shared
  `dev-dependency` crate `paideia-test-support` — that is a future move.

---

## 10. Appendix — quick reproduction

Numbers in this doc were sampled at HEAD `ff624b8`. To regenerate:

```
# Integration binaries per crate
for c in crates/*/tests; do
  echo "$c: $(find "$c" -maxdepth 1 -name '*.rs' | wc -l)"
done

# Duplication counts (paideia-as)
grep -l 'fn cargo_run' crates/paideia-as/tests/*.rs | wc -l
grep -l 'fn build_emit_data' crates/paideia-as/tests/*.rs | wc -l
grep -l 'object::File::parse' crates/paideia-as/tests/*.rs | wc -l
grep -l '/tmp/' crates/paideia-as/tests/*.rs | wc -l

# Test-function counts per crate integration
grep -c '^#\[test\]' crates/paideia-as/tests/*.rs \
  | awk -F: '{s+=$2} END{print s}'

# Cached binaries in target/
ls target/release/deps/ | grep -E '\-([0-9a-f]{16})$' \
  | sed 's/-[0-9a-f]\{16\}$//' | sort -u | wc -l
```
