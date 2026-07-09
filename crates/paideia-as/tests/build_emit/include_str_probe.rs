//! Issue #1014: @include_str(...) literal emission test.
//!
//! Verifies that the `@include_str("path/to/file")` inline string literal is correctly wired
//! through cmd_build.rs's data emission loop and emitted into .rodata with the file's exact
//! byte sequence (treating the text file as UTF-8-validated bytes).
//!
//! The fixture embeds a file containing 8 bytes: "paideia\n"
//! Full expected bytes: [0x70, 0x61, 0x69, 0x64, 0x65, 0x69, 0x61, 0x0a]

use object::{Object, ObjectSymbol};

use crate::common::elf::rodata_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Issue #1014: Test that @include_str(...) literals emit correct bytes into .rodata.
///
/// The fixture (include_str_probe.pdx) contains:
///   ```pdx
///   module IncludeStrProbe = structure {
///     pub let payload : [u8; 8] = @include_str("data/include_str_probe.txt")
///   }
///   ```
///
/// This test:
/// 1. Builds the fixture to ELF64
/// 2. Locates the `payload` symbol in .rodata
/// 3. Asserts the 8-byte payload matches the file's exact bytes
#[test]
fn include_str_emits_exact_file_bytes() {
    let out = run_build(build_emit("include_str_probe.pdx"));
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

    // Expected: the exact file contents "paideia\n" (8 bytes)
    let expected_bytes: Vec<u8> = vec![0x70, 0x61, 0x69, 0x64, 0x65, 0x69, 0x61, 0x0a];

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
