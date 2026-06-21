//! Integration tests for cross-file relocation support (Phase 5 m5-004).
//!
//! Tests that undefined symbols are properly created during ELF emission,
//! allowing two object files to link together when one calls a function
//! defined in another.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cross_file")
}

/// Compile a PDX file to an ELF object file using paideia-as.
fn compile_pdx_to_elf(pdx_file: &str, output_file: &str) -> Result<(), String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cargo_target_dir =
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());

    // Find the paideia-as binary
    let paideia_as = std::path::PathBuf::from(&cargo_target_dir)
        .join("debug")
        .join("paideia-as");

    if !paideia_as.exists() {
        return Err(format!(
            "paideia-as binary not found at {:?}. Build it first with: cargo build -p paideia-as",
            paideia_as
        ));
    }

    let fixture_path = fixture_dir().join(pdx_file);
    let output_path = PathBuf::from(output_file);

    let output = Command::new(&paideia_as)
        .arg("build")
        .arg(&fixture_path)
        .arg("-o")
        .arg(&output_path)
        .output()
        .map_err(|e| format!("Failed to run paideia-as: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "paideia-as compilation failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        ));
    }

    Ok(())
}

/// Use readelf to inspect symbols in an ELF object file.
fn readelf_symbols(elf_file: &str) -> Result<String, String> {
    let output = Command::new("readelf")
        .arg("-s")
        .arg(elf_file)
        .output()
        .map_err(|e| format!("Failed to run readelf: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("readelf failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Link two ELF object files using ld.
fn link_elf_files(obj_file1: &str, obj_file2: &str, output_file: &str) -> Result<(), String> {
    let output = Command::new("ld")
        .arg(obj_file1)
        .arg(obj_file2)
        .arg("-o")
        .arg(output_file)
        .output()
        .map_err(|e| format!("Failed to run ld: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ld linking failed: {}", stderr));
    }

    Ok(())
}

/// Use objdump to disassemble a section of a binary.
fn objdump_disassemble(binary_file: &str, section: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("objdump");
    cmd.arg("-d");
    if let Some(s) = section {
        cmd.arg("-j");
        cmd.arg(s);
    }
    cmd.arg(binary_file);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run objdump: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("objdump failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[test]
#[ignore] // Requires paideia-as to be built and readelf to be available
fn test_undefined_symbol_in_single_object_file() {
    // Test 1: Compile a single PDX that calls an undefined function.
    // Verify that readelf shows the undefined symbol with type NOTYPE and SHN_UNDEF.

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let caller_obj = temp_path.join("caller.o");

    // Compile caller.pdx
    compile_pdx_to_elf("caller.pdx", caller_obj.to_str().unwrap())
        .expect("Failed to compile caller.pdx");

    // Verify the object file was created
    assert!(
        caller_obj.exists(),
        "Object file should have been created at {:?}",
        caller_obj
    );

    // Use readelf to inspect symbols
    let symbols = readelf_symbols(caller_obj.to_str().unwrap()).expect("Failed to run readelf");

    // Check that gdt_load appears as an undefined symbol (UND)
    assert!(
        symbols.contains("gdt_load"),
        "Symbol table should contain 'gdt_load': {}",
        symbols
    );
    assert!(
        symbols.contains("UND"),
        "Symbol table should show 'gdt_load' as undefined (UND): {}",
        symbols
    );
}

#[test]
#[ignore] // Requires paideia-as to be built, ld, and objdump to be available
fn test_cross_file_linking_resolves_call() {
    // Test 2: Compile two PDX files (caller.pdx and callee.pdx).
    // Link them with ld.
    // Verify that objdump shows the call's displacement is resolved.

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let caller_obj = temp_path.join("caller.o");
    let callee_obj = temp_path.join("callee.o");
    let linked_binary = temp_path.join("linked");

    // Compile both PDX files
    compile_pdx_to_elf("caller.pdx", caller_obj.to_str().unwrap())
        .expect("Failed to compile caller.pdx");
    compile_pdx_to_elf("callee.pdx", callee_obj.to_str().unwrap())
        .expect("Failed to compile callee.pdx");

    // Verify both object files were created
    assert!(caller_obj.exists(), "caller.o should have been created");
    assert!(callee_obj.exists(), "callee.o should have been created");

    // Link the two object files
    link_elf_files(
        caller_obj.to_str().unwrap(),
        callee_obj.to_str().unwrap(),
        linked_binary.to_str().unwrap(),
    )
    .expect("Failed to link object files with ld");

    // Verify the linked binary was created
    assert!(
        linked_binary.exists(),
        "Linked binary should have been created"
    );

    // Use objdump to disassemble and check the call instruction
    let disassembly = objdump_disassemble(linked_binary.to_str().unwrap(), Some(".text"))
        .expect("Failed to run objdump");

    // The disassembly should contain call instructions
    assert!(
        disassembly.contains("call"),
        "Disassembly should contain call instructions: {}",
        disassembly
    );

    // Note: The exact displacement value depends on the layout of sections,
    // but it should be a concrete value, not a relocation placeholder.
}
