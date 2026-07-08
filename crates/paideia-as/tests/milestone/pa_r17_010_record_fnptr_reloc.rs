//! Integration test for #988 v2: record literals with fnptr relocation support.
//!
//! Verifies that:
//! - Record literals with fnptr fields emit relocations correctly
//! - Multiple fnptr fields in a record emit multiple relocations at correct offsets
//! - Mixed literal and fnptr fields are packed correctly
//! - Mutable record literals with fnptrs go to .data with relocations
//! - Non-function borrow targets (data symbols) trigger T0536 diagnostics
//! - Unresolved fnptr symbols still generate relocations (for linker to handle)

use object::{Object, ObjectSection};
use std::env;
use std::process::Command;

#[test]
fn record_single_fnptr_field_emits_reloc() {
    let temp_dir = env::temp_dir().join("pa_r17_010_single_fnptr_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: struct with one fnptr field and immutable record literal
    // Note: Paideia-as doesn't support fnptr types in struct fields, so we use u64
    // and verify that the codegen correctly handles Borrow values in record fields
    let source = r#"module RecordSingleFnptr = structure {
  pub let read_impl : () -> u64 = fn () -> 0

  struct VTable {
    read: u64
  }

  pub let vt : VTable = VTable { read: &read_impl }
}"#;

    let source_file = temp_dir.join("record_single_fnptr.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_fnptr_single.o");
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

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .rodata section exists with 8 zero bytes (placeholder for fnptr)
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
        rodata_data.len() >= 8,
        ".rodata should contain at least 8 bytes (one fnptr field)"
    );

    // Verify the data is 8 zero bytes
    let expected = [0u8; 8];
    assert_eq!(
        &rodata_data[..8], &expected,
        "record fnptr field should be 8 zero bytes (placeholder)"
    );

    // Verify at least 1 relocation in .rodata
    let mut reloc_count = 0;
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            for _ in section.relocations() {
                reloc_count += 1;
            }
            break;
        }
    }
    assert!(
        reloc_count >= 1,
        "should have at least 1 relocation in .rodata, got {}",
        reloc_count
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_two_fnptr_fields_emits_two_relocs() {
    let temp_dir = env::temp_dir().join("pa_r17_010_two_fnptr_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: struct with two fnptr fields
    let source = r#"module RecordTwoFnptr = structure {
  pub let read_impl : () -> u64 = fn () -> 0
  pub let write_impl : () -> u64 = fn () -> 0

  struct VTable {
    read: u64,
    write: u64
  }

  pub let vt : VTable = VTable { read: &read_impl, write: &write_impl }
}"#;

    let source_file = temp_dir.join("record_two_fnptr.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_fnptr_two.o");
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

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .rodata section with 16 bytes
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
        ".rodata should contain at least 16 bytes (two fnptr fields)"
    );

    // Verify the data is 16 zero bytes
    let expected = [0u8; 16];
    assert_eq!(
        &rodata_data[..16], &expected,
        "record with two fnptr fields should be 16 zero bytes"
    );

    // Verify 2 relocations at offsets 0 and 8
    let mut reloc_count = 0;
    let mut reloc_offsets = Vec::new();
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            for (offset, _reloc) in section.relocations() {
                reloc_count += 1;
                reloc_offsets.push(offset);
            }
            break;
        }
    }
    assert_eq!(
        reloc_count, 2,
        "should have exactly 2 relocations in .rodata, got {}",
        reloc_count
    );

    // Offsets should be 0 and 8
    reloc_offsets.sort();
    assert_eq!(reloc_offsets, vec![0, 8], "relocations should be at offsets 0 and 8");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_mixed_fnptr_and_literal() {
    let temp_dir = env::temp_dir().join("pa_r17_010_mixed_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: struct with fnptr and literal (u64) fields
    let source = r#"module RecordMixed = structure {
  pub let read_impl : () -> u64 = fn () -> 0

  struct Config {
    read: u64,
    cap: u64
  }

  pub let cfg : Config = Config { read: &read_impl, cap: 42 }
}"#;

    let source_file = temp_dir.join("record_mixed.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_mixed.o");
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

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .rodata section with 16 bytes (8 for fnptr + 8 for u64)
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
        ".rodata should contain at least 16 bytes"
    );

    // First 8 bytes should be 0 (fnptr placeholder), last 8 bytes should be 42 (u64 LE)
    // 42 = 0x2a = [0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    let expected_first = [0u8; 8];
    let expected_last = [0x2au8, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(&rodata_data[..8], &expected_first, "first field (fnptr) should be zeros");
    assert_eq!(
        &rodata_data[8..16], &expected_last,
        "second field (u64 42) should be 0x2a"
    );

    // Verify 1 relocation at offset 0
    let mut reloc_count = 0;
    let mut reloc_offsets = Vec::new();
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            for (offset, _reloc) in section.relocations() {
                reloc_count += 1;
                reloc_offsets.push(offset);
            }
            break;
        }
    }
    assert_eq!(
        reloc_count, 1,
        "should have exactly 1 relocation in .rodata, got {}",
        reloc_count
    );
    assert_eq!(reloc_offsets[0], 0, "relocation should be at offset 0");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_mutable_fnptr_emits_to_data() {
    let temp_dir = env::temp_dir().join("pa_r17_010_mutable_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: struct with fnptr field and mutable record literal
    let source = r#"module RecordMutableFnptr = structure {
  pub let read_impl : () -> u64 = fn () -> 0

  struct VTable {
    read: u64
  }

  pub let mut vt : VTable = VTable { read: &read_impl }
}"#;

    let source_file = temp_dir.join("record_mutable_fnptr.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_mutable_fnptr.o");
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

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .data section exists (not .rodata) with 8 zero bytes
    let mut data_found = false;
    let mut data_data = Vec::new();
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".data" {
            data_found = true;
            data_data = section.data().unwrap_or(&[]).to_vec();
            break;
        }
    }

    assert!(data_found, ".data section should exist for mutable record");
    assert!(
        data_data.len() >= 8,
        ".data should contain at least 8 bytes (one fnptr field)"
    );

    // Verify the data is 8 zero bytes
    let expected = [0u8; 8];
    assert_eq!(
        &data_data[..8], &expected,
        "mutable record fnptr field should be 8 zero bytes (placeholder)"
    );

    // Verify 1 relocation in .data
    let mut reloc_count = 0;
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".data" {
            for _ in section.relocations() {
                reloc_count += 1;
            }
            break;
        }
    }
    assert!(
        reloc_count >= 1,
        "should have at least 1 relocation in .data, got {}",
        reloc_count
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_non_fn_borrow_target_emits_t0536() {
    let temp_dir = env::temp_dir().join("pa_r17_010_non_fn_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: struct with fnptr field but targeting a data symbol (should fail)
    let source = r#"module RecordDataAddr = structure {
  pub let data : u64 = 42

  struct BadVTable {
    read: &data
  }

  pub let vt : BadVTable = BadVTable { read: &data }
}"#;

    let source_file = temp_dir.join("record_data_addr.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_data_addr.o");
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

    // Build should fail with T0536 diagnostic
    assert!(
        !out.status.success(),
        "build should fail for non-function borrow target"
    );

    // Verify T0536 diagnostic fires for data symbol address-of
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T0536"),
        "Expected T0536 diagnostic for data symbol address-of, got: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn record_unresolved_fn_symbol_emits_reloc() {
    let temp_dir = env::temp_dir().join("pa_r17_010_unresolved_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: struct with fnptr to unresolved (external) function
    let source = r#"module RecordUnresolvedFn = structure {
  struct VTable {
    read: u64
  }

  pub let vt : VTable = VTable { read: &extern_read_impl }
}"#;

    let source_file = temp_dir.join("record_unresolved_fn.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("record_unresolved_fn.o");
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
        "paideia-as build should succeed for unresolved fn symbol (linker will handle): {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Verify .rodata section with 8 zero bytes
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
        rodata_data.len() >= 8,
        ".rodata should contain at least 8 bytes"
    );

    // Verify the data is 8 zero bytes
    let expected = [0u8; 8];
    assert_eq!(
        &rodata_data[..8], &expected,
        "record with unresolved fnptr should be 8 zero bytes"
    );

    // Verify 1 relocation (to undefined symbol)
    let mut reloc_count = 0;
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            for _ in section.relocations() {
                reloc_count += 1;
            }
            break;
        }
    }
    assert!(
        reloc_count >= 1,
        "should have at least 1 relocation to undefined symbol, got {}",
        reloc_count
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
