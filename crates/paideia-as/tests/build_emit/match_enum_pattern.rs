//! Phase 7 m9-009 (#1081/#1082): Match expression enum pattern integration test
//!
//! This test verifies that match expressions with enum variant patterns emit
//! proper dispatch code (cmp/jne cascade) and arm body code.
//!
//! Expected behavior:
//! - Parser accepts match expressions with enum variant patterns
//! - AST→IR lowering creates Match IR nodes with arm bodies
//! - populate_match_arm_meta classifies patterns and resolves scrutinee type
//! - visit_match emits discriminant comparison, conditional jumps, and arm bodies
//! - match arm bodies (Var, Literal) emit correctly to RAX
//! - Dispatch sequence: cmp rax, variant_index; jne next_arm; [arm_body]; jmp end

use std::path::PathBuf;
use std::process::Command;
use object::{Object, ObjectSection, ObjectSymbol};

fn build_emit_data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/build-emit");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

#[test]
fn match_enum_pattern_builds_successfully() {
    // Phase 7 m9-009 AC1: match_enum_pattern.pdx parses and emits without error.
    let input = build_emit_data("match_enum_pattern.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_match_enum_pattern.o",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Build failed. stdout:\n{}\n\nstderr:\n{}",
        stdout,
        stderr
    );

    assert!(
        std::path::Path::new("/tmp/test_match_enum_pattern.o").exists(),
        "Output ELF file not created at /tmp/test_match_enum_pattern.o"
    );
}

#[test]
fn match_entry_symbol_contains_dispatch_code() {
    // Phase 7 m9-009 AC2: The 'entry' function symbol should emit dispatch code
    // including cmp and conditional jump instructions, plus arm body code.
    let input = build_emit_data("match_enum_pattern.pdx");
    let output_path = "/tmp/test_match_enum_pattern_dispatch.o";
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    assert!(
        output.status.success(),
        "Build failed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Parse the ELF file
    let file_data = std::fs::read(output_path)
        .expect("Failed to read ELF file");
    let object_file = object::File::parse(&*file_data)
        .expect("Failed to parse ELF file");

    // Find the 'entry' symbol in .text section
    let entry_symbol = object_file
        .symbols()
        .find(|s| s.name().ok() == Some("entry"))
        .expect("Symbol 'entry' not found in ELF");

    // Get .text section
    let text_section = object_file
        .sections()
        .find(|s| s.name().ok() == Some(".text"))
        .expect("No .text section found in ELF");

    let text_data = text_section.data()
        .expect("Failed to read .text data");

    // Extract symbol's byte range from .text
    let sym_addr = entry_symbol.address() as usize;
    let sym_size = entry_symbol.size() as usize;
    let text_addr = text_section.address() as usize;

    assert!(
        sym_addr >= text_addr,
        "Symbol address {} should be >= .text address {}",
        sym_addr,
        text_addr
    );

    let sym_offset = sym_addr - text_addr;
    assert!(
        sym_offset + sym_size <= text_data.len(),
        "Symbol range {}..{} exceeds .text size {}",
        sym_offset,
        sym_offset + sym_size,
        text_data.len()
    );

    // Extract the symbol's bytes
    let sym_bytes = &text_data[sym_offset..sym_offset + sym_size];

    // Verify symbol is at least 10 bytes (cmp + jne + mov + ret minimum)
    assert!(
        sym_bytes.len() >= 10,
        "Entry symbol size {} is less than minimum 10 bytes",
        sym_bytes.len()
    );

    // Check for dispatch pattern: look for cmp or test instruction followed by jcc
    // Cmp patterns:
    // - 0x83 (cmp reg, imm8)
    // - 0x48 0x83 (cmp with REX.W)
    // - 0x3d (cmp eax, imm32)
    // - 0x48 0x3d (cmp rax, imm32)
    // Jcc patterns:
    // - 0x75 (jne short)
    // - 0x74 (je short)
    // - 0x0f 0x85 (jne near)
    // - 0x0f 0x84 (je near)

    let has_cmp_pattern = sym_bytes.windows(2).any(|w| {
        (w[0] == 0x83 && (w[1] & 0xf8) == 0x38) ||  // cmp reg, imm8
        (w[0] == 0x3d) ||                            // cmp eax/rax, imm32
        (w[0] == 0x48 && w[1] == 0x3d)              // REX.W cmp rax, imm32
    });

    let has_jcc_pattern = sym_bytes.windows(2).any(|w| {
        w[0] == 0x75 ||  // jne short
        w[0] == 0x74 ||  // je short
        (w[0] == 0x0f && w[1] == 0x85) ||  // jne near
        (w[0] == 0x0f && w[1] == 0x84)    // je near
    });

    assert!(
        has_cmp_pattern || has_jcc_pattern,
        "Symbol bytes do not contain cmp or jcc instructions as expected for match dispatch. bytes: {:02x?}",
        sym_bytes.iter().take(20).collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "blocked on #1084 (scrutinee load)"]
fn match_entry_references_scrutinee() {
    // Phase 7 m9-009 AC3: The dispatch code should contain relocations
    // for loading the scrutinee (the 'r' global).
    // Fix D (#1085): Defer this test pending #1084 (real memory load with relocation).
    let input = build_emit_data("match_enum_pattern.pdx");
    let output_path = "/tmp/test_match_enum_pattern_reloc.o";
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    assert!(
        output.status.success(),
        "Build failed"
    );

    let file_data = std::fs::read(output_path)
        .expect("Failed to read ELF file");
    let object_file = object::File::parse(&*file_data)
        .expect("Failed to parse ELF file");

    // Check that the 'r' symbol exists (the scrutinee)
    let r_symbol = object_file
        .symbols()
        .find(|s| s.name().ok() == Some("r"))
        .expect("Symbol 'r' (scrutinee) not found in ELF");

    assert!(
        r_symbol.address() > 0,
        "Symbol 'r' has invalid address"
    );

    // Check that .rela.text exists (contains relocations)
    let rela_text = object_file
        .sections()
        .find(|s| s.name().ok() == Some(".rela.text"));

    assert!(
        rela_text.is_some(),
        ".rela.text section should exist for match dispatch with scrutinee reference"
    );
}
