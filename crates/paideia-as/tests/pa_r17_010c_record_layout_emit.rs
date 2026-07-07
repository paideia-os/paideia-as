//! Integration test for PA-r17-010c: cmd_build wiring for record literals.
//!
//! Verifies that:
//! - Record literals (RecordCons) are properly encoded into .rodata/.data sections
//! - No T0518 diagnostics fire for record type lookups
//! - Mutable record literals go to .data, immutable to .rodata
//! - Missing struct type declarations trigger T0552 diagnostics

use object::{Object, ObjectSection};
use std::env;
use std::process::Command;

#[test]
fn record_global_emits_to_rodata() {
    let temp_dir = env::temp_dir().join("pa_r17_010c_rodata_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: simple struct with two u64 fields and immutable record literal.
    let source = r#"module RecordRodata = structure {
  struct Pair {
    x: u64,
    y: u64
  }

  pub let p : Pair = Pair { x: 42, y: 100 }
}"#;

    let source_file = temp_dir.join("record_rodata.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_rodata.o");
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(source_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    assert!(
        out.status.success(),
        "paideia-as build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify no T0518 diagnostic (unknown record type) in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("T0518"),
        "Unexpected T0518 diagnostic: {}",
        stderr
    );

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .rodata section exists and is non-empty
    let mut rodata_found = false;
    let mut rodata_data = Vec::new();
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            rodata_found = true;
            rodata_data = section.data().unwrap_or(&[]).to_vec();
            break;
        }
    }

    assert!(rodata_found, ".rodata section should exist");
    assert!(
        rodata_data.len() >= 16,
        ".rodata should contain at least 16 bytes (two u64 fields)"
    );

    // Verify the data matches: 42 (u64 LE) + 100 (u64 LE)
    // 42 = 0x2a = [0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    // 100 = 0x64 = [0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    let expected = [0x2au8, 0, 0, 0, 0, 0, 0, 0, 0x64u8, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        &rodata_data[..16], &expected,
        "record data should match packed u64 values"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_global_mutable_emits_to_data() {
    let temp_dir = env::temp_dir().join("pa_r17_010c_data_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: simple struct with two u64 fields and mutable record literal.
    let source = r#"module RecordData = structure {
  struct Pair {
    x: u64,
    y: u64
  }

  pub let mut p : Pair = Pair { x: 42, y: 100 }
}"#;

    let source_file = temp_dir.join("record_data.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_data.o");
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(source_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    assert!(
        out.status.success(),
        "paideia-as build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify no T0518 diagnostic in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("T0518"),
        "Unexpected T0518 diagnostic: {}",
        stderr
    );

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .data section exists and is non-empty
    let mut data_found = false;
    let mut data_data = Vec::new();
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".data" {
            data_found = true;
            data_data = section.data().unwrap_or(&[]).to_vec();
            break;
        }
    }

    assert!(data_found, ".data section should exist");
    assert!(
        data_data.len() >= 16,
        ".data should contain at least 16 bytes (two u64 fields)"
    );

    // Verify the data matches: 42 (u64 LE) + 100 (u64 LE)
    let expected = [0x2au8, 0, 0, 0, 0, 0, 0, 0, 0x64u8, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        &data_data[..16], &expected,
        "record data should match packed u64 values"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_unknown_type_diagnostic_still_fires() {
    let temp_dir = env::temp_dir().join("pa_r17_010c_unknown_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: record literal without matching struct type declaration.
    let source = r#"module RecordUnknown = structure {
  pub let p : Pair = Pair { x: 42, y: 100 }
}"#;

    let source_file = temp_dir.join("record_unknown.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_unknown.o");
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(source_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    // Build should fail with diagnostic
    assert!(
        !out.status.success(),
        "build should fail for undefined record type"
    );

    // Verify T0552 diagnostic fires for undefined type
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T0552") || stderr.contains("undefined"),
        "Expected T0552 or undefined type diagnostic, got: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
