//! Issue #1198: enum-literal call arguments marshalled into parameter register (first position)
//!
//! This test verifies that a bare enum variant literal (e.g., `A` in `helper(A, 0x37)`)
//! is correctly rewritten from Var to EnumCons during lowering, and emits a MOV instruction
//! to load the variant index (0) into RDI (the first argument register).
//!
//! Expected behavior:
//! - Parser accepts enum definition and call with bare enum variant as first argument
//! - AST→IR lowering rewrites Var node to EnumCons and populates EnumConsInfo
//! - Emit generates `mov rdi, 0` (48 C7 C7 00 00 00 00) to load variant index
//! - Helper function receives variant index 0 in RDI, returns the second argument value

use std::path::PathBuf;
use std::process::Command;
use object::{Object, ObjectSection};

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
fn enum_lit_call_arg_first_pos_emits_mov_rdi() {
    let input = build_emit_data("enum_lit_call_arg_first_pos.pdx");
    let output_path = "/tmp/test_enum_lit_call_arg_first_pos.o";
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Build failed unexpectedly. stdout:\n{}\n\nstderr:\n{}",
        stdout,
        stderr
    );

    // Verify the ELF output contains the mov rdi, 0 instruction
    let file_data = std::fs::read(output_path)
        .expect("Failed to read ELF file");
    let object_file = object::File::parse(&*file_data)
        .expect("Failed to parse ELF file");

    // Find the .text section
    let text_section = object_file
        .sections()
        .find(|s| s.name().ok() == Some(".text"))
        .expect("No .text section found in ELF");

    let text_data = text_section.data().expect("Failed to read .text data");

    // Look for the sequence: 48 C7 C7 00 00 00 00 (mov rdi, 0)
    let mov_rdi_zero = [0x48u8, 0xC7, 0xC7, 0x00, 0x00, 0x00, 0x00];
    let found = text_data.windows(mov_rdi_zero.len())
        .any(|w| w == mov_rdi_zero);

    assert!(found, "Expected 'mov rdi, 0' instruction not found in .text section");
}
