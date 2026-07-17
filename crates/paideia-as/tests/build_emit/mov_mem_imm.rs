//! Fix #1240: Test encoding of mov [mem], imm64 in Mode64.
//!
//! Verifies that three previously-missing encoder arms now handle:
//! 1. mov [base + disp], imm64 (MemSib{index:None, disp}, Imm64)
//! 2. mov [base + index*scale + disp], imm64 (MemSib{index:Some, scale, disp}, Imm64)
//! 3. mov [rip + sym], imm64 (MemRipRelSym, Imm64)
//!
//! Also tests edge cases:
//! - Negative sign-extension (mov [rax], -1)
//! - Overflow diagnostics (mov [rax], 0x100000000)
//! - Regression test for original #1240 issue (labels + control flow)

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Test: mov [rax], 42 (basic form with no index, no disp)
#[test]
fn mov_mem_imm_basic() {
    let out = run_build(build_emit("mov_mem_imm_basic.pdx"));
    out.assert_ok();
}

/// Test: mov [rax + 8], 42 (displacement form, fits in i8)
#[test]
fn mov_mem_imm_disp8() {
    let out = run_build(build_emit("mov_mem_imm_disp8.pdx"));
    out.assert_ok();
}

/// Test: mov [rax + 0x1000], 42 (displacement form, needs i32)
#[test]
fn mov_mem_imm_disp32() {
    let out = run_build(build_emit("mov_mem_imm_disp32.pdx"));
    out.assert_ok();
}

/// Test: mov [rax + rcx*8], 42 (SIB form with index and scale, no disp)
#[test]
fn mov_mem_imm_sib() {
    let out = run_build(build_emit("mov_mem_imm_sib.pdx"));
    out.assert_ok();
}

/// Test: mov [rax + rcx*8 + 16], 42 (SIB form with index, scale, and disp)
#[test]
fn mov_mem_imm_sib_disp() {
    let out = run_build(build_emit("mov_mem_imm_sib_disp.pdx"));
    out.assert_ok();
}

/// Test: mov [rip + hm_len], 42 (RIP-relative symbol reference)
#[test]
fn mov_mem_imm_rip_rel_sym() {
    let out = run_build(build_emit("mov_mem_imm_rip_rel_sym.pdx"));
    out.assert_ok();
}

/// Test: mov [rax], -1 (negative immediate, sign-extended to W64)
#[test]
fn mov_mem_imm_neg_sxt() {
    let out = run_build(build_emit("mov_mem_imm_neg_sxt.pdx"));
    out.assert_ok();
}

/// Test: mov [rax], 0x100000000 (immediate too large for i32 sign-ext)
/// Should fail with diagnostic referencing movabs fixup.
#[test]
fn mov_mem_imm_overflow_diag() {
    let out = run_build(build_emit("mov_mem_imm_overflow_diag.pdx"));
    // This should fail; we check that it does
    assert!(
        !out.status.success(),
        "expected build to fail for immediate too large for i32 sign-ext range"
    );
    // The encoder should produce a B1705 error with a message about movabs
    assert!(
        out.stderr_contains("B1705"),
        "diagnostic should be B1705 encoder error; got:\n{}",
        out.stderr
    );
}

/// Regression test: Full #1240 original issue (labels + control flow)
/// This compiles green on HEAD (before the fix); must remain green after.
#[test]
fn preexisting_labels_regression() {
    let out = run_build(build_emit("preexisting_labels_regression.pdx"));
    out.assert_ok();
}
