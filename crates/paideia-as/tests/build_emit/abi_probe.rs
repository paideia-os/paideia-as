//! Integration tests for @abi calling convention attribute (PA19-r19-001).
//! Tests parsing and validation of @abi("ms") and @abi("sysv") directives on let bindings.
//! PA19-r19-006: Tests MS x64 callee prologue emitter and U1620 narrowing.

use crate::common::elf;
use std::fs;
use std::path::PathBuf;

/// Helper: run cargo with the paideia-as command
fn cargo_run(args: &[&str]) -> std::process::Output {
    std::process::Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-as")
        .arg("--")
        .args(args)
        .output()
        .expect("failed to run cargo")
}

#[test]
fn abi_ms_add_imm_lambda_compiles_and_uses_rcx() {
    // PA19-r19-006: MS x64 add-imm lambda: fn(x) -> x + 1 should compile and use RCX (not RDI).
    // Verify bytecode contains lea with rcx base (48 8d 41 01) and NOT lea with rdi base (48 8d 47 01).
    let tmp_file = PathBuf::from("/tmp/Test5.pdx");
    let source = r#"module Test5 = structure {
  pub let my_init : (u64) -> u64 = fn(x : u64) -> x + 1 @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_ms_add_imm_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed with narrowed U1620 (add-imm is supported)
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "MS x64 add-imm lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    // Extract my_init function bytes and verify it uses RCX (not RDI) for lea base
    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_init") {
        // Expect lea rax, [rcx+1]: 48 8d 41 01 (or similar RCX-based encoding)
        let has_lea_rcx_base = func_bytes.windows(4).any(|w| {
            (w[0] == 0x48 && w[1] == 0x8d && w[2] == 0x41) || // lea rax, [rcx+1]
            (w[0] == 0x48 && w[1] == 0x8d && w[2] == 0x49)    // lea rax, [rcx+offset]
        });
        assert!(has_lea_rcx_base, "Expected lea with RCX base in my_init bytecode");

        // Verify it does NOT use RDI base (48 8d 47 NN)
        let has_lea_rdi_base = func_bytes.windows(4).any(|w| {
            w[0] == 0x48 && w[1] == 0x8d && w[2] == 0x47
        });
        assert!(!has_lea_rdi_base, "Should NOT use RDI base (SysV) in MS x64 lambda");
    } else {
        panic!("Could not extract my_init function bytes from ELF");
    }
}

#[test]
fn abi_sysv_lambda_builds_cleanly() {
    // Test that @abi("sysv") on a lambda binding builds successfully (no U1620)
    let tmp_file = PathBuf::from("/tmp/Test2.pdx");
    let source = r#"module Test2 = structure {
  pub let my_init : (u64) -> u64 = fn(x : u64) -> x + 1 @abi("sysv")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_sysv_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "@abi(\"sysv\") lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No U1620 in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("U1620"),
        "should NOT emit U1620 for @abi(\"sysv\")"
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn abi_absent_lambda_builds_cleanly() {
    // Test regression: unannotated lambda still emits under paideia default
    let tmp_file = PathBuf::from("/tmp/Test3.pdx");
    let source = r#"module Test3 = structure {
  pub let my_init : (u64) -> u64 = fn(x : u64) -> x + 1
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_absent_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "unannotated lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No diagnostics in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("U1620") && !stderr.contains("P0286"),
        "unannotated lambda must not emit diagnostics"
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn abi_on_non_lambda_p0286() {
    // Test that @abi on a non-lambda binding emits P0286 error during build
    let tmp_file = PathBuf::from("/tmp/Test4.pdx");
    let source = r#"module Test4 = structure {
  pub let x : u64 = 42 @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_non_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should fail with P0286
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must fail
    assert_ne!(
        out.status.code(),
        Some(0),
        "@abi on non-lambda must fail to build"
    );

    // Stderr must contain P0286
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("P0286"),
        "expected P0286 in stderr, got: {}",
        stderr
    );

    // Check: message mentions function-shaped or lambda
    assert!(
        stderr.to_lowercase().contains("function") ||
        stderr.to_lowercase().contains("lambda"),
        "P0286 message should mention function or lambda: {}",
        stderr
    );
}

// PA19-r19-006: New MS x64 lambda emitter tests

#[test]
fn abi_ms_identity_lambda_compiles() {
    // PA19-r19-006: MS x64 identity lambda: fn(x) -> x should compile successfully.
    let tmp_file = PathBuf::from("/tmp/Test6.pdx");
    let source = r#"module Test6 = structure {
  pub let my_id : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_ms_identity_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed (identity is supported)
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "MS x64 identity lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    // Verify my_id function contains mov or lea instructions (typical for identity)
    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_id") {
        // Should contain either mov rax, rcx (48 89 c8) or similar
        let has_mov_or_lea = func_bytes.windows(3).any(|w| {
            (w[0] == 0x48 && w[1] == 0x89) || // mov variants
            (w[0] == 0x48 && w[1] == 0x8d)    // lea variants
        });
        assert!(has_mov_or_lea, "Expected mov or lea in my_id bytecode");
    }
}

#[test]
fn abi_ms_literal_return_lambda_compiles() {
    // PA19-r19-006: MS x64 literal return lambda: fn() -> 42 should compile successfully.
    let tmp_file = PathBuf::from("/tmp/Test7.pdx");
    let source = r#"module Test7 = structure {
  pub let my_lit : () -> u64 = fn() -> 42 @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_ms_literal_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed (literal return is supported)
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "MS x64 literal return lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    // Verify my_lit function contains mov instruction with immediate
    if let Some(func_bytes) = elf::symbol_bytes(&bytes, "my_lit") {
        // Should contain mov rax, imm (48 b8 or 48 c7 c0)
        let has_mov_imm = func_bytes.windows(2).any(|w| {
            (w[0] == 0x48 && w[1] == 0xb8) || // movabs rax, imm64
            (w[0] == 0x48 && w[1] == 0xc7)    // mov rax, imm32
        });
        assert!(has_mov_imm, "Expected mov with immediate in my_lit bytecode");
    }
}

#[test]
fn abi_ms_five_arg_lambda_still_emits_u1620() {
    // PA19-r19-006: MS x64 5+ arg lambda should fire U1620 (5th arg requires stack).
    let tmp_file = PathBuf::from("/tmp/Test8.pdx");
    let source = r#"module Test8 = structure {
  pub let my_5arg : (u64, u64, u64, u64, u64) -> u64 = fn(a : u64, b : u64, c : u64, d : u64, e : u64) -> a @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_ms_5arg_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should fail with U1620 (5 args > 4 register args)
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must fail
    assert_ne!(
        out.status.code(),
        Some(0),
        "MS x64 5+ arg lambda must fail to build (U1620)"
    );

    // Stderr must contain U1620
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("U1620"),
        "expected U1620 in stderr for 5-arg lambda, got: {}",
        stderr
    );

    // Message should mention parameters or max 4
    assert!(
        stderr.to_lowercase().contains("parameter") || stderr.contains("5"),
        "U1620 message should mention parameters or count: {}",
        stderr
    );
}

#[test]
fn abi_ms_complex_body_still_emits_u1620() {
    // PA19-r19-006: MS x64 complex lambda body (e.g., x * x) should fire U1620.
    let tmp_file = PathBuf::from("/tmp/Test9.pdx");
    let source = r#"module Test9 = structure {
  pub let my_complex : (u64) -> u64 = fn(x : u64) -> x * x @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_ms_complex_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should fail with U1620 (* is not a supported body shape)
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must fail
    assert_ne!(
        out.status.code(),
        Some(0),
        "MS x64 complex lambda must fail to build (U1620)"
    );

    // Stderr must contain U1620
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("U1620"),
        "expected U1620 in stderr for complex lambda, got: {}",
        stderr
    );

    // Message should mention body shape
    assert!(
        stderr.to_lowercase().contains("body") || stderr.to_lowercase().contains("shape"),
        "U1620 message should mention body shape: {}",
        stderr
    );
}
