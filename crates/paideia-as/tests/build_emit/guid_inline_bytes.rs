//! Issue #1012: @guid(...) literal emission test.
//!
//! Verifies that the `@guid("...")` inline bytes literal is correctly wired
//! through cmd_build.rs's data emission loop and emitted into .rodata with
//! the correct UEFI mixed-endian byte sequence.
//!
//! The GUID "12345678-1234-1234-1234-123456789abc" should emit as:
//!   Data1 (u32): 12345678 → LE: 78 56 34 12
//!   Data2 (u16): 1234 → LE: 34 12
//!   Data3 (u16): 1234 → LE: 34 12
//!   Data4[8] (raw bytes): 12 34 56 78 9a bc → BE (as-is): 12 34 56 78 9a bc
//!
//! Full expected bytes: [0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]

use object::{Object, ObjectSymbol};

use crate::common::elf::rodata_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Issue #1012: Test that @guid(...) literals emit correct bytes into .rodata.
///
/// The fixture (guid_probe.pdx) contains:
///   ```pdx
///   module GuidProbe = structure {
///     pub let my_guid : [u8; 16] = @guid("12345678-1234-1234-1234-123456789abc")
///   }
///   ```
///
/// This test:
/// 1. Builds the fixture to ELF64
/// 2. Locates the `my_guid` symbol in .rodata
/// 3. Asserts the 16-byte payload matches the UEFI mixed-endian encoding
#[test]
fn guid_inline_bytes_emits_correct_rodata_bytes() {
    let out = run_build(build_emit("guid_probe.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Verify ELF64 magic
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let rodata = rodata_bytes(&bytes);
    assert!(!rodata.is_empty(), ".rodata section must exist in ELF");

    // Parse the ELF to find the my_guid symbol
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the my_guid symbol and extract its bytes from .rodata
    let mut my_guid_bytes: Option<Vec<u8>> = None;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "my_guid" {
                let addr = sym.address() as usize;
                let size = sym.size() as usize;
                if size == 16 && addr + size <= rodata.len() {
                    my_guid_bytes = Some(rodata[addr..addr + size].to_vec());
                }
                break;
            }
        }
    }

    let actual_bytes = my_guid_bytes.expect("my_guid symbol should exist in .rodata with size 16");

    // UEFI mixed-endian: Data1 (u32 LE) + Data2 (u16 LE) + Data3 (u16 LE) + Data4[8] (first 2 + last 6)
    // "12345678-1234-1234-1234-123456789abc" should be:
    // Data1: 0x12345678 → LE: [0x78, 0x56, 0x34, 0x12]
    // Data2: 0x1234 → LE: [0x34, 0x12]
    // Data3: 0x1234 → LE: [0x34, 0x12]
    // Data4: "1234" + "123456789abc" → [0x12, 0x34, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
    let expected_bytes: Vec<u8> = vec![
        0x78, 0x56, 0x34, 0x12, // Data1 LE
        0x34, 0x12,             // Data2 LE
        0x34, 0x12,             // Data3 LE
        0x12, 0x34, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, // Data4
    ];

    if actual_bytes != expected_bytes {
        eprintln!(
            "MISMATCH: my_guid bytes do not match expected\n\
             Expected (16 bytes): {:02X?}\n\
             Got (16 bytes):      {:02X?}",
            expected_bytes, actual_bytes
        );
        panic!("my_guid symbol bytes do not match UEFI mixed-endian encoding");
    }
}
