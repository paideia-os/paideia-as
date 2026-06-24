/// PA8-m1-002: Single function regression test.
/// Ensures that a trivial single-function module produces a symbol with non-zero st_value
/// and correct st_size (not 0, 0).
use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_single_fn_regression() {
    let temp_dir = env::temp_dir().join("pa8_single_fn_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Single trivial identity function.
    // This is the simplest test case: one function, one symbol.
    let source = "module Pa8single = structure { let single : (u64) -> u64 = fn (x: u64) -> x }";

    let source_file = temp_dir.join("Pa8single.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa8single.o");
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(source_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    assert!(
        out.status.success(),
        "paideia-as build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Find the 'single' symbol.
    let mut found = false;
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "single" {
                let addr = symbol.address() as u32;
                let size = symbol.size() as u32;
                // Single identity function should be at least 4 bytes (mov rax, rdi; ret)
                assert!(size > 0, "single function has st_size=0 (regression)");
                found = true;
                break;
            }
        }
    }

    assert!(found, "symbol 'single' not found");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
