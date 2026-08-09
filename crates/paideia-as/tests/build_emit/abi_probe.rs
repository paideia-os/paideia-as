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

/// v0.21-001 (#1277, closes #1011): 5-arg MS x64 identity fn now emits.
/// Fixture: `fn(a,b,c,d,e) -> a` returns arg[0], which lives in RCX under MS x64.
/// Args b/c/d are register-passed (RDX/R8/R9) but unused; arg e is stack-passed
/// (above shadow space) but unused — silently unregistered by
/// register_nested_lambda_params. The body reduces to `mov rax, rcx; ret`.
///
/// Previously (r19): U1620 rejected any @abi("ms") lambda with >4 params.
#[test]
fn abi_ms_five_arg_lambda_compiles_and_uses_rcx() {
    let out = run_build(build_emit("abi_ms_five_arg_probe.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("U1620"),
        "5-arg MS x64 identity should NOT emit U1620 (v0.21-001); stderr:\n{}",
        out.stderr
    );
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_5arg") {
        // Look for mov rax, rcx (48 89 C8) — MS x64 identity from arg[0].
        let has_ms_identity = func_bytes
            .windows(3)
            .any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xC8);
        assert!(
            has_ms_identity,
            "Expected MS x64 identity `mov rax, rcx` (48 89 C8) in my_5arg: {}",
            func_bytes.iter().take(32).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
        );
        // Must NOT contain SysV identity `mov rax, rdi` (48 89 F8).
        let has_sysv_identity = func_bytes
            .windows(3)
            .any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xF8);
        assert!(
            !has_sysv_identity,
            "5-arg MS lambda must NOT emit SysV `mov rax, rdi` (48 89 F8) in my_5arg"
        );
    } else {
        panic!("my_5arg symbol not found in ELF");
    }
}

/// v0.21-001 (#1277, closes #1011): `x * x` (multiplication) body now emits
/// under @abi("ms"). Previously U1620 rejected any non-{Path, Literal,
/// Infix(+, ident, literal)} body; now the shared BinOp lowerer
/// (emit_var_assign_expr_to_rax) resolves the parameter register through
/// local_bindings, which is ABI-populated. For MS x64, `x` lives in RCX,
/// so the load is `mov rax, rcx` (48 89 C8) not `mov rax, rdi` (48 89 F8).
#[test]
fn abi_ms_complex_body_compiles_and_uses_rcx() {
    let out = run_build(build_emit("abi_ms_complex_body_probe.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("U1620"),
        "MS x64 `x * x` body should NOT emit U1620 (v0.21-001); stderr:\n{}",
        out.stderr
    );
    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);

    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_complex") {
        // Body reads `x` (in RCX under MS) into RAX; must contain 48 89 C8.
        let has_load_from_rcx = func_bytes
            .windows(3)
            .any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xC8);
        // Must NOT contain load from RDI (48 89 F8) — that would be SysV.
        let has_load_from_rdi = func_bytes
            .windows(3)
            .any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xF8);
        assert!(
            has_load_from_rcx,
            "MS x64 complex body must load `x` from RCX (48 89 C8): {}",
            func_bytes.iter().take(32).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
        );
        assert!(
            !has_load_from_rdi,
            "MS x64 complex body must NOT load `x` from RDI (SysV): {}",
            func_bytes.iter().take(32).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
        );
    } else {
        panic!("my_complex symbol not found in ELF");
    }
}
