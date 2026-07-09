//! Integration tests for @abi calling convention attribute (PA19-r19-001).
//! Tests parsing and validation of @abi("ms") and @abi("sysv") directives on let bindings.
//! PA19-r19-006: Tests MS x64 callee prologue emitter and U1620 narrowing.

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn abi_ms_add_imm_lambda_compiles_and_uses_rcx() {
    // PA19-r19-006: MS x64 add-imm lambda: fn(x) -> x + 1 must use RCX (not RDI).
    let out = run_build(build_emit("abi_ms_add_imm_probe.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    let func_bytes = elf::symbol_bytes(&bytes, "my_init")
        .expect("Could not extract my_init function bytes from ELF");

    let has_lea_rcx_base = func_bytes.windows(4).any(|w| {
        (w[0] == 0x48 && w[1] == 0x8d && w[2] == 0x41)
            || (w[0] == 0x48 && w[1] == 0x8d && w[2] == 0x49)
    });
    assert!(has_lea_rcx_base, "Expected lea with RCX base in my_init bytecode");

    let has_lea_rdi_base = func_bytes
        .windows(4)
        .any(|w| w[0] == 0x48 && w[1] == 0x8d && w[2] == 0x47);
    assert!(!has_lea_rdi_base, "Should NOT use RDI base (SysV) in MS x64 lambda");
}

#[test]
fn abi_sysv_lambda_builds_cleanly() {
    let out = run_build(build_emit("abi_sysv_probe.pdx"));
    out.assert_ok();
    assert!(!out.stderr_contains("U1620"), "should NOT emit U1620 for @abi(\"sysv\"); stderr:\n{}", out.stderr);
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn abi_absent_lambda_builds_cleanly() {
    let out = run_build(build_emit("abi_absent_probe.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("U1620") && !out.stderr_contains("P0286"),
        "unannotated lambda must not emit diagnostics; stderr:\n{}", out.stderr
    );
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn abi_on_non_lambda_p0286() {
    let out = run_build(build_emit("abi_non_lambda_probe.pdx"));
    out.assert_diag("P0286");
    let stderr_lc = out.stderr.to_lowercase();
    assert!(
        stderr_lc.contains("function") || stderr_lc.contains("lambda"),
        "P0286 message should mention function or lambda: {}", out.stderr
    );
}

#[test]
fn abi_ms_identity_lambda_compiles() {
    let out = run_build(build_emit("abi_ms_identity_probe.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_id") {
        let has_mov_or_lea = func_bytes.windows(3).any(|w| {
            (w[0] == 0x48 && w[1] == 0x89) || (w[0] == 0x48 && w[1] == 0x8d)
        });
        assert!(has_mov_or_lea, "Expected mov or lea in my_id bytecode");
    }
}

#[test]
fn abi_ms_literal_return_lambda_compiles() {
    let out = run_build(build_emit("abi_ms_literal_probe.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_lit") {
        let has_mov_imm = func_bytes.windows(2).any(|w| {
            (w[0] == 0x48 && w[1] == 0xb8) || (w[0] == 0x48 && w[1] == 0xc7)
        });
        assert!(has_mov_imm, "Expected mov with immediate in my_lit bytecode");
    }
}

#[test]
fn abi_ms_five_arg_lambda_still_emits_u1620() {
    let out = run_build(build_emit("abi_ms_five_arg_probe.pdx"));
    out.assert_diag("U1620");
    let stderr_lc = out.stderr.to_lowercase();
    assert!(
        stderr_lc.contains("parameter") || out.stderr.contains("5"),
        "U1620 message should mention parameters or count: {}", out.stderr
    );
}

#[test]
fn abi_ms_complex_body_still_emits_u1620() {
    let out = run_build(build_emit("abi_ms_complex_body_probe.pdx"));
    out.assert_diag("U1620");
    let stderr_lc = out.stderr.to_lowercase();
    assert!(
        stderr_lc.contains("body") || stderr_lc.contains("shape"),
        "U1620 message should mention body shape: {}", out.stderr
    );
}
