//! Issue #1053 (#PA-r17-009a): Nested pattern parser + AST→IR lowering integration test
//!
//! This test verifies that match expressions with nested patterns (record patterns
//! inside enum variant patterns) parse, lower, and emit correctly.
//!
//! Expected behavior:
//! - Parser accepts nested patterns like Ok(Point { x, y })
//! - AST→IR lowering creates Match IR nodes with nested PatternBinding trees
//! - populate_match_arm_meta extracts nested pattern_binding for complex patterns
//! - Code emission handles nested pattern extraction correctly

use std::path::PathBuf;
use std::process::Command;
use object::{Object, ObjectSymbol};

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
fn match_nested_pattern_builds_successfully() {
    // PA-r17-009a AC1: match_nested_pattern.pdx parses and emits without error.
    let input = build_emit_data("match_nested_pattern.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_match_nested_pattern.o",
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
        std::path::Path::new("/tmp/test_match_nested_pattern.o").exists(),
        "Output ELF file not created at /tmp/test_match_nested_pattern.o"
    );

    // Disassemble and verify the entry function contains expected instructions
    let objdump_output = Command::new("objdump")
        .arg("-d")
        .arg("/tmp/test_match_nested_pattern.o")
        .output()
        .expect("Failed to run objdump");

    let disasm = String::from_utf8_lossy(&objdump_output.stdout);

    // AC1a: Verify field loads from the Point payload are present
    // The x field should be loaded from offset 0x8 of the payload
    assert!(
        disasm.contains("mov    0x8(%rdi)"),
        "Field load for x (offset 0x8) not found in disassembly"
    );

    // The y field should be loaded from offset 0xc of the payload
    assert!(
        disasm.contains("mov    0xc(%rdi)"),
        "Field load for y (offset 0xc) not found in disassembly"
    );

    // AC1b: Verify the addition operation
    assert!(
        disasm.contains("add    %rdx,%rax"),
        "Add instruction (add %rdx,%rax) not found in disassembly"
    );

    // AC1c: Verify the Err arm moves 0 to rax
    assert!(
        disasm.contains("mov    $0x0,%rax"),
        "Err arm instruction (mov $0x0,%rax) not found in disassembly"
    );

    // AC1d: Verify cmp+jne dispatch for discriminant matching
    assert!(
        disasm.contains("cmp    $0x0,%rax"),
        "Discriminant comparison (cmp $0x0,%rax) not found in disassembly"
    );
    assert!(
        disasm.contains("jne"),
        "Conditional jump (jne) for arm dispatch not found in disassembly"
    );

    // #1084: Verify scrutinee load via RIP-relative reference
    // Scrutinee loads should use RIP-relative addressing, not constant-folded movabs
    assert!(
        disasm.contains("mov    0x0(%rip),%rax"),
        "Scrutinee should be loaded via RIP-relative addressing (mov 0x0(%rip),%rax)"
    );

    // Verify no constant-folded movabs pattern (Defect B is fixed)
    // The old double-emit would create: 48 b8 <imm64> followed by 48 ba <imm64>
    let lines: Vec<&str> = disasm.lines().collect();
    for window in lines.windows(2) {
        let has_movabs_rax = window[0].contains("movabs") && window[0].contains("%rax");
        let has_movabs_rdx = window[1].contains("movabs") && window[1].contains("%rdx");
        assert!(
            !(has_movabs_rax && has_movabs_rdx),
            "Disassembly should not contain constant-folded movabs rax followed by movabs rdx pattern"
        );
    }
}

#[test]
fn match_nested_pattern_entry_symbol_exists() {
    // PA-r17-009a AC2: The 'entry' function symbol should exist and have non-zero size.
    let input = build_emit_data("match_nested_pattern.pdx");
    let output_path = "/tmp/test_match_nested_pattern_entry.o";
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

    // Find the 'entry' symbol
    let entry_symbol = object_file
        .symbols()
        .find(|s: &_| s.name().ok() == Some("entry"))
        .expect("Symbol 'entry' not found in ELF");

    let sym_size = entry_symbol.size();
    assert!(
        sym_size > 0,
        "Entry symbol size should be > 0, got {}",
        sym_size
    );
}
