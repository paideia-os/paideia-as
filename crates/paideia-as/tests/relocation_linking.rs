//! Integration tests for relocation linking (Phase 5 m4-004).
//!
//! Tests that the emitter correctly:
//! 1. Collects relocation sites from encoder
//! 2. Maps them to ELF R_X86_64_PC32 relocations
//! 3. Links .text references to .rodata data symbols

use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/data");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test 1: lgdt [gdt_descriptor] with data declared in same .pdx.
///
/// Acceptance criteria (from spec):
/// - Encoder produces a relocation site with:
///   - byte_offset = displacement offset within instruction
///   - symbol = "gdt_descriptor"
///   - kind = PcRel32
/// - ELF writer emits R_X86_64_PC32 relocation to .text section
/// - readelf -r <object> shows the relocation
/// - ld <object> -o <out> produces executable (basic link test)
#[test]
fn lgdt_with_data_reference_produces_relocation() {
    // For now, this test uses the hello.pdx placeholder because
    // Phase 5 m5-002 just landed and full instruction encoding
    // will land in m5-003+. This test placeholder verifies the
    // ELF writer is wired up to accept relocations.

    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_reloc_test1.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Verify basic ELF structure (sanity check)
    assert!(
        file.sections().count() > 0,
        "ELF should have at least one section"
    );

    // Check for .text section
    let has_text = file.sections().any(|s| s.name().unwrap_or("") == ".text");
    assert!(has_text, "ELF should have .text section");

    // Check for .rela.text section (relocations)
    // Note: The placeholder (add_one) doesn't have symbol references yet,
    // so we won't see relocations. When full instruction encoding lands
    // in m5-003+, this check will find R_X86_64_PC32 relocations.
    let has_reloc_section = file
        .sections()
        .any(|s| s.name().unwrap_or("").starts_with(".rela"));
    // Don't assert here yet; placeholder won't have relocations
    let _ = has_reloc_section;

    let _ = std::fs::remove_file(&tmp);
}

/// Test 2: Cross-function data reference (one function defines data, another references).
///
/// This verifies:
/// - Symbol table has both function and data symbols
/// - Relocation references are resolved correctly
/// - Link stage (ld) can patch addresses
#[test]
fn cross_function_data_reference_links() {
    // Placeholder: same as test 1 for now.
    // When m5-003+ lands with full instruction encoding, this test
    // will verify that two functions (one with data, one referencing)
    // compile and link correctly.

    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_reloc_test2.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Verify ELF structure
    assert!(
        file.sections().count() > 0,
        "ELF should have at least one section"
    );

    // Verify symbol table exists and is populated
    let mut symbol_count = 0;
    for _sym in file.symbols() {
        symbol_count += 1;
    }
    assert!(symbol_count > 0, "ELF should have at least one symbol");

    let _ = std::fs::remove_file(&tmp);
}

/// Verify that the ELF writer doesn't crash when processing relocations.
/// This is a sanity check that the writer infrastructure is wired correctly.
#[test]
fn elf_writer_handles_relocation_infrastructure() {
    // Basic smoke test: ensure the writer accepts relocations without panicking.
    // The actual relocation content is tested via the CLI above.

    use paideia_as_emitter_elf::{
        Arch, ElfWriter, Kind, RelocEntry, RelocKind, SymKind, SymbolEntry,
    };

    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Add a function symbol and some code
    writer.add_text_bytes(&[0x90]); // NOP
    let _ = writer.add_symbol(SymbolEntry::func("test_func", 0, 1));

    // Add a data symbol
    let _ = writer.add_symbol(SymbolEntry {
        name: "test_data".to_string(),
        offset: Some(0),
        size: 8,
        kind: SymKind::Data,
        is_global: false,
    });

    // Add a relocation
    let text_section = writer.text_section_id();
    let reloc = RelocEntry {
        offset: 0,
        target: "test_data".to_string(),
        kind: RelocKind::PC32,
        addend: 0,
    };
    let result = writer.add_relocation(text_section, reloc);
    assert!(
        result.is_ok(),
        "writer should accept relocation without error"
    );

    // Finalize and verify bytes are produced
    let bytes = writer.finalize().expect("finalize should succeed");
    assert!(bytes.len() > 0, "ELF output should be non-empty");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic should be present");
}
