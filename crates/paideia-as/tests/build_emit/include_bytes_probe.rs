//! Issue #1013: @include_bytes(...) literal emission test.
//!
//! Verifies that the `@include_bytes("path/to/file")` inline bytes literal is correctly wired
//! through cmd_build.rs's data emission loop and emitted into .rodata with the file's exact
//! byte sequence.
//!
//! The fixture embeds a file containing 8 bytes: "PDX!" + 0xDEADBEEF
//! Full expected bytes: [0x50, 0x44, 0x58, 0x21, 0xDE, 0xAD, 0xBE, 0xEF]

use object::{Object, ObjectSymbol};
use tempfile;

use crate::common::elf::rodata_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Issue #1013: Test that @include_bytes(...) literals emit correct bytes into .rodata.
///
/// The fixture (include_bytes_probe.pdx) contains:
///   ```pdx
///   module IncludeBytesProbe = structure {
///     pub let payload : [u8; 8] = @include_bytes("data/include_bytes_probe.bin")
///   }
///   ```
///
/// This test:
/// 1. Builds the fixture to ELF64
/// 2. Locates the `payload` symbol in .rodata
/// 3. Asserts the 8-byte payload matches the file's exact bytes
#[test]
fn include_bytes_emits_exact_file_bytes() {
    let out = run_build(build_emit("include_bytes_probe.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Verify ELF64 magic
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let rodata = rodata_bytes(&bytes);
    assert!(!rodata.is_empty(), ".rodata section must exist in ELF");

    // Parse the ELF to find the payload symbol
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the payload symbol and extract its bytes from .rodata
    let mut payload_bytes: Option<Vec<u8>> = None;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "payload" {
                let addr = sym.address() as usize;
                let size = sym.size() as usize;
                if size == 8 && addr + size <= rodata.len() {
                    payload_bytes = Some(rodata[addr..addr + size].to_vec());
                }
                break;
            }
        }
    }

    let actual_bytes = payload_bytes.expect("payload symbol should exist in .rodata with size 8");

    // Expected: the exact file contents "PDX!" + 0xDEADBEEF
    let expected_bytes: Vec<u8> = vec![0x50, 0x44, 0x58, 0x21, 0xDE, 0xAD, 0xBE, 0xEF];

    if actual_bytes != expected_bytes {
        eprintln!(
            "MISMATCH: payload bytes do not match expected\n\
             Expected (8 bytes): {:02X?}\n\
             Got (8 bytes):      {:02X?}",
            expected_bytes, actual_bytes
        );
        panic!("payload symbol bytes do not match file contents");
    }
}

/// Test that T0558 fires when declared size mismatches actual file size.
/// This test intentionally creates a size mismatch to verify the retroactive guard.
#[test]
fn t0558_fires_on_size_mismatch() {
    // Build a temporary fixture with wrong size annotation
    let pdx_content = r#"
module IncludeBytesProbe = structure {
  pub let payload : [u8; 7] = @include_bytes("data/include_bytes_probe.bin")
}
"#;

    use std::fs;

    // Create a temporary directory with the required structure
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let data_dir = temp_dir.path().join("data");
    fs::create_dir(&data_dir).expect("create data subdir");

    // Write the data file
    let data_file = data_dir.join("include_bytes_probe.bin");
    fs::write(&data_file, b"PDX!\xDE\xAD\xBE\xEF").expect("write data file");

    // Write the .pdx fixture
    let temp_pdx = temp_dir.path().join("test_t0558_mismatch.pdx");
    fs::write(&temp_pdx, pdx_content).expect("write temp fixture");

    let out = run_build(&temp_pdx.to_string_lossy().to_string());

    // Should fail (not success)
    assert!(!out.status.success(), "build should fail due to T0558 size mismatch");

    // Verify T0558 in stderr
    assert!(
        out.stderr.contains("T0558"),
        "stderr should contain T0558 diagnostic, got: {}",
        out.stderr
    );

    // temp_dir automatically cleaned up when dropped
}
