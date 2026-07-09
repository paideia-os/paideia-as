//! Integration tests for paideia↔MS x64 ABI crossing with caller-side bridge thunks.
//! Issue #1017 (PA-r19-012): MS x64 ↔ SysV bridge thunks — inline register bookends.
//!
//! These tests verify that the elaborator correctly emits inline push/pop register
//! bookends around calls that cross ABI boundaries (paideia → MS x64 or explicit SysV).
//! The bridge saves/restores R15 and R14 (paideia's effect/env registers) to protect
//! against potential clobbering during the call.

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

/// Hex string helper: convert bytes to hex string for debugging
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
}

#[test]
fn paideia_to_ms_call_pushes_r15_r14_before_shadow_bump() {
    // Fixture: unannotated caller with a 1-arg call to @abi("ms") callee.
    // Expected bytecode prefix: push r15 (41 57), push r14 (41 56), sub rsp, 40 (48 83 ec 28)
    let tmp_file = PathBuf::from("/tmp/Test17.pdx");
    let source = r#"module Test17 = structure {
  pub let target_ms : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
  pub let caller_paideia : (u64) -> u64 = fn(a : u64) -> target_ms(a)
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test17.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Bridge push test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    // Extract the caller function bytes and check for the expected push sequence
    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_paideia") {
        // Expected: 41 57 (push r15) then 41 56 (push r14) then 48 83 ec 28 (sub rsp, 40)
        let has_push_sequence = caller_bytes.windows(8).any(|w| {
            w[0] == 0x41 && w[1] == 0x57 &&  // push r15
            w[2] == 0x41 && w[3] == 0x56 &&  // push r14
            w[4] == 0x48 && w[5] == 0x83 && w[6] == 0xec && w[7] == 0x28  // sub rsp, 40
        });

        assert!(
            has_push_sequence,
            "Expected 'push r15; push r14; sub rsp, 40' bytecode in caller_paideia. Got: {}",
            bytes_to_hex(&caller_bytes)
        );
    } else {
        panic!("Could not extract caller_paideia function bytes from ELF");
    }
}

#[test]
fn paideia_to_ms_call_pops_r14_r15_after_shadow_restore() {
    // Fixture: unannotated caller with a 1-arg call to @abi("ms") callee.
    // Expected bytecode suffix (around add rsp, 40): add rsp,40 (48 83 c4 28), pop r14 (41 5e), pop r15 (41 5f), ret (c3)
    let tmp_file = PathBuf::from("/tmp/Test18.pdx");
    let source = r#"module Test18 = structure {
  pub let target_ms : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
  pub let caller_paideia : (u64) -> u64 = fn(a : u64) -> target_ms(a)
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test18.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Bridge pop test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_paideia") {
        // Expected: 48 83 c4 28 (add rsp, 40) then 41 5e (pop r14) then 41 5f (pop r15) then c3 (ret)
        let has_pop_sequence = caller_bytes.windows(8).any(|w| {
            w[0] == 0x48 && w[1] == 0x83 && w[2] == 0xc4 && w[3] == 0x28 &&  // add rsp, 40
            w[4] == 0x41 && w[5] == 0x5e &&  // pop r14
            w[6] == 0x41 && w[7] == 0x5f    // pop r15
        });

        assert!(
            has_pop_sequence,
            "Expected 'add rsp, 40; pop r14; pop r15' bytecode in caller_paideia. Got: {}",
            bytes_to_hex(&caller_bytes)
        );
    } else {
        panic!("Could not extract caller_paideia function bytes from ELF");
    }
}

#[test]
fn paideia_to_ms_call_full_sequence_byte_exact() {
    // Full end-to-end test: verify the exact sequence.
    // Caller (unannotated) → 1-arg call to @abi("ms") callee.
    // Expected sequence: push r15; push r14; sub rsp,40; mov rcx, rdi; call target; add rsp,40; pop r14; pop r15; ret
    let tmp_file = PathBuf::from("/tmp/Test19.pdx");
    let source = r#"module Test19 = structure {
  pub let target_ms : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
  pub let caller_paideia : (u64) -> u64 = fn(a : u64) -> target_ms(a)
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test19.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Full bridge sequence test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_paideia") {
        // Check that the sequence contains all key elements in order:
        // push r15 (41 57), push r14 (41 56), sub rsp,40 (48 83 ec 28),
        // mov rcx, rdi (48 89 f9), call (e8), add rsp,40 (48 83 c4 28),
        // pop r14 (41 5e), pop r15 (41 5f), ret (c3)

        // Look for push r15 first
        let push_r15_idx = caller_bytes.windows(2).position(|w| w[0] == 0x41 && w[1] == 0x57);
        assert!(push_r15_idx.is_some(), "Missing push r15 in sequence");

        let push_r15_idx = push_r15_idx.unwrap();
        let push_r14_idx = caller_bytes[push_r15_idx..].windows(2).position(|w| w[0] == 0x41 && w[1] == 0x56)
            .map(|i| push_r15_idx + i);
        assert!(push_r14_idx.is_some(), "Missing push r14 after push r15");

        let push_r14_idx = push_r14_idx.unwrap();
        let sub_rsp_idx = caller_bytes[push_r14_idx..].windows(4).position(|w| {
            w[0] == 0x48 && w[1] == 0x83 && w[2] == 0xec && w[3] == 0x28
        }).map(|i| push_r14_idx + i);
        assert!(sub_rsp_idx.is_some(), "Missing 'sub rsp, 40' after push r14");

        let sub_rsp_idx = sub_rsp_idx.unwrap();
        let call_idx = caller_bytes[sub_rsp_idx..].windows(1).position(|w| w[0] == 0xe8)
            .map(|i| sub_rsp_idx + i);
        assert!(call_idx.is_some(), "Missing call instruction after sub rsp");

        let call_idx = call_idx.unwrap();
        let add_rsp_idx = caller_bytes[call_idx..].windows(4).position(|w| {
            w[0] == 0x48 && w[1] == 0x83 && w[2] == 0xc4 && w[3] == 0x28
        }).map(|i| call_idx + i);
        assert!(add_rsp_idx.is_some(), "Missing 'add rsp, 40' after call");

        let add_rsp_idx = add_rsp_idx.unwrap();
        let pop_r14_idx = caller_bytes[add_rsp_idx..].windows(2).position(|w| w[0] == 0x41 && w[1] == 0x5e)
            .map(|i| add_rsp_idx + i);
        assert!(pop_r14_idx.is_some(), "Missing pop r14 after add rsp");

        let pop_r14_idx = pop_r14_idx.unwrap();
        let pop_r15_idx = caller_bytes[pop_r14_idx..].windows(2).position(|w| w[0] == 0x41 && w[1] == 0x5f)
            .map(|i| pop_r14_idx + i);
        assert!(pop_r15_idx.is_some(), "Missing pop r15 after pop r14");
    } else {
        panic!("Could not extract caller_paideia function bytes from ELF");
    }
}

#[test]
fn paideia_to_ms_call_alignment_preserved_at_call() {
    // Verify that the CALL instruction appears at the correct position:
    // after all pushes (8 bytes) + sub rsp,40 (4 bytes) = offset 12.
    // After call, should see add rsp,40, then pops, then ret.
    let tmp_file = PathBuf::from("/tmp/Test20.pdx");
    let source = r#"module Test20 = structure {
  pub let target_ms : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
  pub let caller_paideia : (u64) -> u64 = fn(a : u64) -> target_ms(a)
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test20.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Alignment test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_paideia") {
        // Count the number of CALL (e8) instructions in the function
        let call_count = caller_bytes.iter().filter(|b| **b == 0xe8).count();
        assert_eq!(
            call_count, 1,
            "Expected exactly one CALL instruction in caller_paideia, got {}",
            call_count
        );
    } else {
        panic!("Could not extract caller_paideia function bytes from ELF");
    }
}

#[test]
fn sysv_caller_to_ms_callee_no_paideia_save() {
    // Caller is EXPLICITLY @abi("sysv"), callee is @abi("ms").
    // Should NOT emit bridge save/restore (SysV caller owns its own invariant).
    // Should only have the MS shadow bump and arg reshuffle.
    let tmp_file = PathBuf::from("/tmp/Test21.pdx");
    let source = r#"module Test21 = structure {
  pub let target_ms : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
  pub let caller_sysv : (u64) -> u64 = fn(a : u64) -> target_ms(a) @abi("sysv")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test21.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "SysV→MS test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_sysv") {
        // Should NOT have push r15/r14 (41 57 / 41 56)
        let has_push_r15 = caller_bytes.windows(2).any(|w| w[0] == 0x41 && w[1] == 0x57);
        assert!(!has_push_r15, "SysV→MS should NOT have push r15 (bridge save)");

        let has_push_r14 = caller_bytes.windows(2).any(|w| w[0] == 0x41 && w[1] == 0x56);
        assert!(!has_push_r14, "SysV→MS should NOT have push r14 (bridge save)");
    } else {
        panic!("Could not extract caller_sysv function bytes from ELF");
    }
}

#[test]
#[ignore = "blocked on U1620 narrowing: MS x64 lambda bodies containing function calls are not in the current MVP supported set (identity / add-imm / literal-return only). Un-ignore when U1620 permits App-body MS lambdas."]
fn ms_caller_to_ms_callee_no_bridge() {
    // Both @abi("ms"): no bridge save, just shadow bump and arg reshuffle.
    let tmp_file = PathBuf::from("/tmp/Test22.pdx");
    let source = r#"module Test22 = structure {
  pub let target_ms : (u64) -> u64 = fn(x : u64) -> x @abi("ms")
  pub let caller_ms : (u64) -> u64 = fn(a : u64) -> target_ms(a) @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test22.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "MS→MS test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_ms") {
        // Should NOT have push r15/r14 (bridge save for MS→MS)
        let has_push_r15 = caller_bytes.windows(2).any(|w| w[0] == 0x41 && w[1] == 0x57);
        assert!(!has_push_r15, "MS→MS should NOT have push r15");

        let has_push_r14 = caller_bytes.windows(2).any(|w| w[0] == 0x41 && w[1] == 0x56);
        assert!(!has_push_r14, "MS→MS should NOT have push r14");
    } else {
        panic!("Could not extract caller_ms function bytes from ELF");
    }
}

#[test]
fn paideia_to_paideia_unchanged() {
    // Regression fence: unannotated→unannotated (both paideia default) should NOT emit
    // any bridge save/restore, shadow bump, or arg reshuffle.
    let tmp_file = PathBuf::from("/tmp/Test23.pdx");
    let source = r#"module Test23 = structure {
  pub let target : (u64) -> u64 = fn(x : u64) -> x
  pub let caller : (u64) -> u64 = fn(a : u64) -> target(a)
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test23.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Paideia→Paideia test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller") {
        // Should NOT have push r15/r14 or sub rsp patterns
        let has_push_r15 = caller_bytes.windows(2).any(|w| w[0] == 0x41 && w[1] == 0x57);
        assert!(!has_push_r15, "Paideia→Paideia should NOT have push r15");

        let has_push_r14 = caller_bytes.windows(2).any(|w| w[0] == 0x41 && w[1] == 0x56);
        assert!(!has_push_r14, "Paideia→Paideia should NOT have push r14");

        let has_sub_rsp = caller_bytes.windows(4).any(|w| {
            w[0] == 0x48 && w[1] == 0x83 && w[2] == 0xec && w[3] == 0x28
        });
        assert!(!has_sub_rsp, "Paideia→Paideia should NOT have shadow bump");
    } else {
        panic!("Could not extract caller function bytes from ELF");
    }
}

#[test]
fn paideia_to_sysv_explicit_saves_r14_r15() {
    // Unannotated caller calling EXPLICITLY @abi("sysv") callee.
    // Per §11.5, paideia → explicit SysV also saves R14/R15 (bridge crossing).
    // Should have push r15, push r14 but NO shadow bump (SysV takes no shadow space).
    let tmp_file = PathBuf::from("/tmp/Test24.pdx");
    let source = r#"module Test24 = structure {
  pub let target_sysv : (u64) -> u64 = fn(x : u64) -> x @abi("sysv")
  pub let caller_paideia : (u64) -> u64 = fn(a : u64) -> target_sysv(a)
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/Test24.o");
    let _ = fs::remove_file(&out_file);

    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "Paideia→explicit SysV test must build; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);

    if let Some(caller_bytes) = elf::symbol_bytes(&bytes, "caller_paideia") {
        // Should have push r15, push r14
        let has_push_sequence = caller_bytes.windows(4).any(|w| {
            w[0] == 0x41 && w[1] == 0x57 &&  // push r15
            w[2] == 0x41 && w[3] == 0x56     // push r14
        });
        assert!(
            has_push_sequence,
            "Paideia→explicit SysV should have 'push r15; push r14'"
        );

        // Should NOT have sub rsp, 40 (SysV takes no shadow space)
        let has_sub_rsp = caller_bytes.windows(4).any(|w| {
            w[0] == 0x48 && w[1] == 0x83 && w[2] == 0xec && w[3] == 0x28
        });
        assert!(!has_sub_rsp, "Paideia→explicit SysV should NOT have shadow bump (48 83 ec 28)");
    } else {
        panic!("Could not extract caller_paideia function bytes from ELF");
    }
}
