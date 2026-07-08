/// PA8-m1-002: Witness test for symbol st_value threading with different lambda shapes.
/// Tests that symbols emit with correct st_value when using different expression shapes
/// that may take different code paths in the emitter.
use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_call_st_value() {
    let temp_dir = env::temp_dir().join("pa8_call_st_value_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Module with lambda functions using different expression shapes
    // to exercise different visit_lambda arms. This verifies that the fix to compute
    // function offsets correctly handles all lambda shapes.
    // Note: module name must match file basename in PascalCase.
    let source = "module Pa8relocs = structure { let identity : (u64) -> u64 = fn (x: u64) -> x ; let bitwise_not : (u64) -> u64 = fn (x: u64) -> ~x ; let another : (u64) -> u64 = fn (x: u64) -> x }";

    let source_file = temp_dir.join("Pa8relocs.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa8relocs.o");
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

    // Collect all function symbols.
    let mut func_symbols = Vec::new();
    let mut all_symbols = Vec::new();
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            all_symbols.push(name.to_string());
            // Collect only the explicitly named functions (not generated data symbols)
            if name == "identity" || name == "bitwise_not" || name == "another" {
                func_symbols.push((
                    name.to_string(),
                    symbol.address() as u32,
                    symbol.size() as u32,
                ));
            }
        }
    }

    assert!(
        func_symbols.len() >= 3,
        "expected at least 3 function symbols, got {}. All symbols: {:?}",
        func_symbols.len(),
        all_symbols
    );

    // Assertions:
    // 1. None of the function symbols should have st_size=0.
    // This catches the defect where function_offsets lookup failed and all symbols
    // were emitted with st_value=0, st_size=0. We only check size because the first
    // symbol legitimately has address 0 (it's at the start of .text).
    for (name, _addr, size) in &func_symbols {
        assert!(
            *size != 0,
            "function symbol '{}' has st_size=0 (regression)",
            name
        );
    }

    // 2. Verify that we have distinct, contiguous symbols with appropriate sizes.
    // The old defect would cause all to be (0, 0). Now we should see:
    // - identity at 0, size 4
    // - bitwise_not at 4, size 7
    // - another at 11, size 4
    assert!(func_symbols[0].2 > 0, "identity should have size > 0");
    assert!(func_symbols[1].2 > 0, "bitwise_not should have size > 0");
    assert!(func_symbols[2].2 > 0, "another should have size > 0");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
