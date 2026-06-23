//! PA10-007 (m1-001): let-binding data symbol names.
//!
//! Tests that `let name : Type = value` creates an ELF symbol named `name`
//! (not `data_<id>`), so cross-module references via relocations can find them.
//!
//! Integration test: two-file fixture where file A has `let target : u64 = 42`
//! and file B references `target` from file A.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/build-emit/pa10_007")
}

/// Compile a PDX file to an ELF object file.
fn compile_pdx_to_elf(pdx_file: &str, output_file: &str) -> Result<(), String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cargo_target_dir =
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());

    let paideia_as = std::path::PathBuf::from(&cargo_target_dir)
        .join("debug")
        .join("paideia-as");

    if !paideia_as.exists() {
        return Err(format!(
            "paideia-as binary not found at {:?}",
            paideia_as
        ));
    }

    let fixture_path = fixture_dir().join(pdx_file);
    let output_path = PathBuf::from(output_file);

    let output = Command::new(&paideia_as)
        .arg("build")
        .arg(&fixture_path)
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(&output_path)
        .output()
        .map_err(|e| format!("Failed to run paideia-as: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "paideia-as compilation failed:\nstderr: {}",
            stderr
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

#[test]
#[ignore] // Requires paideia-as, readelf, and ld
fn test_data_symbol_uses_binding_name() {
    // Verify that a data symbol created from `let target : u64 = 42`
    // has the symbol name `target` (not `data_<id>`).

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let obj_file = temp_path.join("data_obj.o");

    // Compile the file with data binding
    compile_pdx_to_elf("data_provider.pdx", obj_file.to_str().unwrap())
        .expect("Failed to compile data_provider.pdx");

    // Verify the object file was created
    assert!(
        obj_file.exists(),
        "Object file should have been created"
    );

    // Use readelf to inspect symbols
    let symbols = readelf_symbols(obj_file.to_str().unwrap())
        .expect("Failed to run readelf");

    // Check that the symbol `target` exists (not `data_<N>`)
    assert!(
        symbols.contains("target"),
        "Symbol table should contain 'target': {}",
        symbols
    );

    // The symbol should NOT be named generically as "data_<id>"
    // (Although a fallback name like data_2 should not appear if the binding name is used)
    // We primarily check that 'target' exists as a symbol.
}

#[test]
#[ignore] // Requires paideia-as and readelf
fn test_cross_file_data_relocation_resolves() {
    // Test that two files can link when one file defines a data symbol
    // and another references it.

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let provider_obj = temp_path.join("provider.o");
    let consumer_obj = temp_path.join("consumer.o");
    let linked = temp_path.join("linked.elf");

    // Compile both files
    compile_pdx_to_elf("data_provider.pdx", provider_obj.to_str().unwrap())
        .expect("Failed to compile data_provider.pdx");
    compile_pdx_to_elf("data_consumer.pdx", consumer_obj.to_str().unwrap())
        .expect("Failed to compile data_consumer.pdx");

    // Verify both object files were created
    assert!(provider_obj.exists(), "Provider object file should exist");
    assert!(consumer_obj.exists(), "Consumer object file should exist");

    // Link the two object files
    link_elf_files(
        provider_obj.to_str().unwrap(),
        consumer_obj.to_str().unwrap(),
        linked.to_str().unwrap(),
    )
    .expect("Linking should succeed when data symbol names are correct");

    // Verify the linked file was created
    assert!(
        linked.exists(),
        "Linked ELF file should have been created"
    );
}
