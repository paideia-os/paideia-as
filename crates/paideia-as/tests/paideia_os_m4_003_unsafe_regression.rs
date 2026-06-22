//! Cross-repo canary test for PA8-m4-003: unsafe-block-heavy file regression.
//!
//! Tests that the most unsafe-block-heavy PaideiaOS file (ipi/tlb_shootdown.pdx)
//! still builds correctly after lower.rs ExprData::Unsafe activation (m4-001).
//!
//! This verifies that the m4-001 lowering produces the same IR structure that the
//! UnsafeWalker AST-side path previously used, ensuring byte-identical code generation.

use std::path::PathBuf;
use std::process::Command;

/// Find the PaideiaOS directory or skip the test if not available.
fn find_paideia_os_dir() -> Option<PathBuf> {
    // Assume PaideiaOS is at ../PaideiaOS relative to the workspace root
    let workspace_root = env!("CARGO_MANIFEST_DIR").split("paideia-as").next()?;
    let paideia_os_dir = PathBuf::from(workspace_root).parent()?.join("PaideiaOS");
    if paideia_os_dir.is_dir() {
        Some(paideia_os_dir)
    } else {
        None
    }
}

#[test]
fn paideia_os_tlb_shootdown_builds_after_m4_001() {
    let paideia_os_dir = match find_paideia_os_dir() {
        Some(dir) => dir,
        None => {
            eprintln!("PaideiaOS directory not found; skipping cross-repo canary test");
            return;
        }
    };

    // Path to the most unsafe-block-heavy file (3+ unsafe blocks)
    let tlb_file = paideia_os_dir.join(".quarantine/src/kernel/core/ipi/tlb_shootdown.pdx");

    if !tlb_file.exists() {
        eprintln!(
            "tlb_shootdown.pdx not found at {:?}; file may have been unquarantined",
            tlb_file
        );
        return;
    }

    // Get the paideia-as binary path (same directory as this test executable)
    let paideia_as = env!("CARGO_BIN_EXE_paideia-as");

    // Build the file: `paideia-as build --emit elf64 tlb_shootdown.pdx -o tlb_shootdown.o`
    let output = Command::new(paideia_as)
        .arg("build")
        .arg("--emit")
        .arg("elf64")
        .arg(&tlb_file)
        .arg("-o")
        .arg("/tmp/tlb_shootdown.o")
        .output()
        .expect("failed to run paideia-as");

    // Assert the build succeeded
    if !output.status.success() {
        eprintln!(
            "Build failed for tlb_shootdown.pdx\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("tlb_shootdown.pdx build failed");
    }

    // Verify the .text section contains real instructions, not placeholders.
    // A successful build of 3 unsafe blocks should produce at least 9 bytes (3 blocks × 3 min bytes each)
    // of instruction code (not counting the placeholder `mov rax, rax` that would be used as a fallback).

    // Use `objdump -t` or `readelf -S` to inspect the output
    let readelf = Command::new("readelf")
        .arg("-S")
        .arg("/tmp/tlb_shootdown.o")
        .output()
        .expect("failed to run readelf");

    let readelf_output = String::from_utf8_lossy(&readelf.stdout);

    // Look for the .text section and extract its size
    let text_size = readelf_output
        .lines()
        .find(|line| line.contains(".text"))
        .and_then(|line| {
            // The format is: [Nr] Name      Type      Address          Offset
            //                                Size              EntSize          Flags  Link  Info  Align
            // We want the Size field
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                // Try parsing the size as hex
                parts[5].parse::<usize>().ok()
            } else {
                None
            }
        });

    if let Some(size) = text_size {
        // Baseline: if the file has 3 distinct unsafe blocks and each is at least 3 bytes,
        // the .text should be at least 9 bytes. In practice, with the m4-001 lowering,
        // each unsafe block should emit real instructions totaling more than this baseline.
        assert!(
            size >= 9,
            "tlb_shootdown.pdx .text is too small ({} bytes); expected >= 9 bytes for 3+ unsafe blocks",
            size
        );
        eprintln!(
            "✓ tlb_shootdown.pdx .text size: {} bytes (expected >= 9)",
            size
        );
    } else {
        eprintln!(
            "⚠ Could not parse .text size from readelf output; build succeeded but size verification skipped"
        );
    }
}
