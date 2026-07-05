//! Integration test for peephole optimization pass.
//!
//! This test verifies that:
//! 1. The --optimize 0 (default) produces unoptimized code with instructions that peephole would remove
//! 2. The --optimize 1 produces optimized code where those instructions are removed
//! 3. The byte-for-byte output differs when optimization is applied

use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::Command;

fn build_emit_data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test that verifies peephole optimization is wired and changes code.
///
/// This test:
/// 1. Creates a simple function that adds zero (a pattern peephole should optimize)
/// 2. Builds it with --optimize 0 (unoptimized)
/// 3. Builds it with --optimize 1 (with peephole enabled)
/// 4. Verifies the optimized .text section is smaller (instruction was removed)
#[test]
fn peephole_optimize_removes_instructions() {
    // Create a temporary test file with a pattern peephole would remove.
    // This is a simple function that adds 0 to rax (should be optimized away by peephole).
    let test_source = r#"module TestOpt = structure {
  let optimize_me : () -> () !{} @{} = fn(_: ()) -> unsafe {
    effects: {},
    capabilities: {},
    justification: "test",
    block: {
      mov rax, 0x42
      add rax, 0
      ret
    }
  }
}
"#;

    let test_dir = std::env::temp_dir().join("paideia_as_opt_test");
    let _ = std::fs::create_dir_all(&test_dir);

    let source_file = test_dir.join("test_opt.pdx");
    let out_unopt = test_dir.join("test_opt_unopt.o");
    let out_opt = test_dir.join("test_opt_opt.o");

    std::fs::write(&source_file, test_source).expect("failed to write test source");

    // Build without optimization (--optimize 0, which is the default)
    let out1 = cargo_run(&[
        "build",
        source_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-O",
        "0",
        "-o",
        out_unopt.to_str().unwrap(),
    ]);

    assert!(
        out1.status.success(),
        "build with --optimize 0 failed: {}",
        String::from_utf8_lossy(&out1.stderr)
    );

    // Build with optimization (--optimize 1)
    let out2 = cargo_run(&[
        "build",
        source_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-O",
        "1",
        "-o",
        out_opt.to_str().unwrap(),
    ]);

    assert!(
        out2.status.success(),
        "build with --optimize 1 failed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );

    // Read both ELF files
    let bytes_unopt = std::fs::read(&out_unopt).expect("unoptimized output should exist");
    let bytes_opt = std::fs::read(&out_opt).expect("optimized output should exist");

    // Parse both as ELF
    let file_unopt = object::File::parse(&bytes_unopt[..])
        .expect("unoptimized ELF should parse");
    let file_opt = object::File::parse(&bytes_opt[..])
        .expect("optimized ELF should parse");

    // Extract .text sections
    let mut text_unopt = Vec::new();
    let mut text_opt = Vec::new();

    for section in file_unopt.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_unopt = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    for section in file_opt.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_opt = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_unopt.is_empty(), "unoptimized .text should exist");
    assert!(!text_opt.is_empty(), "optimized .text should exist");

    eprintln!(
        "Unoptimized .text: {} bytes\nOptimized .text: {} bytes",
        text_unopt.len(),
        text_opt.len()
    );

    // The optimized version must be smaller (peephole removed at least one instruction)
    assert!(
        text_opt.len() < text_unopt.len(),
        "Optimized .text ({} bytes) must be smaller than unoptimized ({} bytes). \
         Peephole optimization did not fire or code is identical.",
        text_opt.len(),
        text_unopt.len()
    );
    eprintln!(
        "SUCCESS: peephole optimization reduced .text by {} bytes",
        text_unopt.len() - text_opt.len()
    );

    // Cleanup
    let _ = std::fs::remove_file(&source_file);
    let _ = std::fs::remove_file(&out_unopt);
    let _ = std::fs::remove_file(&out_opt);
}

/// Test that verifies --optimize flag doesn't break default behavior
#[test]
fn optimize_flag_defaults_to_zero() {
    let test_source = r#"module TestDefault = structure {
  let simple_fn : () -> () !{} @{} = fn(_: ()) -> unsafe {
    effects: {},
    capabilities: {},
    justification: "test",
    block: {
      mov rax, 0x42
      ret
    }
  }
}
"#;

    let test_dir = std::env::temp_dir().join("paideia_as_opt_default_test");
    let _ = std::fs::create_dir_all(&test_dir);

    let source_file = test_dir.join("test_default.pdx");
    let out_implicit = test_dir.join("test_default_implicit.o");
    let out_explicit = test_dir.join("test_default_explicit.o");

    std::fs::write(&source_file, test_source).expect("failed to write test source");

    // Build without specifying --optimize (should default to 0)
    let out1 = cargo_run(&[
        "build",
        source_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_implicit.to_str().unwrap(),
    ]);

    assert!(
        out1.status.success(),
        "build without --optimize flag failed: {}",
        String::from_utf8_lossy(&out1.stderr)
    );

    // Build with explicit --optimize 0
    let out2 = cargo_run(&[
        "build",
        source_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-O",
        "0",
        "-o",
        out_explicit.to_str().unwrap(),
    ]);

    assert!(
        out2.status.success(),
        "build with explicit --optimize 0 failed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );

    // Both should produce identical output (default = off)
    let bytes_implicit = std::fs::read(&out_implicit).expect("implicit output should exist");
    let bytes_explicit = std::fs::read(&out_explicit).expect("explicit output should exist");

    assert_eq!(
        bytes_implicit.len(),
        bytes_explicit.len(),
        "implicit and explicit --optimize 0 should produce same-size ELF"
    );

    // Cleanup
    let _ = std::fs::remove_file(&source_file);
    let _ = std::fs::remove_file(&out_implicit);
    let _ = std::fs::remove_file(&out_explicit);
}
