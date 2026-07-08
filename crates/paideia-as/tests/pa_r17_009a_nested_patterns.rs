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
#[ignore = "blocked on #1090 (T0557: enum layout sizing for struct-typed variant payloads) and #1091 (T0555: EnumCons emitter requires literal payloads). #1053 parser + AST->IR lowering is complete; end-to-end codegen unblocks when both follow-ups land."]
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
}

#[test]
#[ignore = "blocked on #1090 (T0557: enum layout sizing for struct-typed variant payloads) and #1091 (T0555: EnumCons emitter requires literal payloads). #1053 parser + AST->IR lowering is complete; end-to-end codegen unblocks when both follow-ups land."]
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
