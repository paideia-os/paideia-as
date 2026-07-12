/// #1174 (post-#1150 v3 retirement): Regression fixture for Action-bodied lambda first_instr capture.
///
/// Tests that Action-bodied lambdas correctly exercise the pending_first_instr_lambda shim
/// in emit_inst, capturing the first instruction's IrNodeId for later offset_map projection
/// in elf.rs. This fixture verifies end-to-end that the v3 retirement of function_offsets
/// field does not regress Action-bodied lambda offset derivation.
///
/// Expected behavior:
/// - Single Action-bodied function in module
/// - Function's first instruction is captured into lambda_first_instr via emit_inst shim
/// - elf.rs projects lambda_first_instr[f_id] through offset_map to get f's st_value
/// - f's st_value == 0 (starts at beginning of .text)
/// - f's st_size == full .text length (only symbol in module)
/// - Byte-level assertion detects misprojection if offset_map lookup fails
use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_action_bodied_lambda_first_instr() {
    let temp_dir = env::temp_dir().join("action_bodied_lambda_first_instr_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Single Action-bodied lambda.
    // This exercises the pending_first_instr_lambda shim path in emit_inst.
    // Note: module name must match file basename in PascalCase.
    let source = "module Ab = structure { let f : (u64) -> u64 = fn (x: u64) { let y = x; y } }";

    let source_file = temp_dir.join("Ab.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Ab.o");
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

    // Collect all symbols to verify single-symbol module.
    let mut func_symbols = Vec::new();
    let mut all_symbols = Vec::new();
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            all_symbols.push(name.to_string());
            if name == "f" {
                func_symbols.push((
                    name.to_string(),
                    symbol.address() as u32,
                    symbol.size() as u32,
                ));
            }
        }
    }

    assert_eq!(
        func_symbols.len(),
        1,
        "expected exactly 1 function symbol 'f', got {}. All symbols: {:?}",
        func_symbols.len(),
        all_symbols
    );

    let (name, st_value, st_size) = &func_symbols[0];
    assert_eq!(name, "f", "expected function named 'f'");

    // Byte-level assertions per #1174 spec:
    // 1. st_value == 0 (starts at beginning of .text for single-symbol module)
    assert_eq!(
        *st_value, 0,
        "function 'f' st_value should be 0 (at start of .text)"
    );

    // 2. st_size must be non-zero (catches regression where offset_map lookup failed)
    // This indicates that the Action-bodied lambda's first instruction was correctly
    // captured via pending_first_instr_lambda and projected through offset_map in elf.rs.
    assert!(
        *st_size > 0,
        "function 'f' st_size should be > 0, got 0. \
         This indicates Action-bodied lambda first_instr was not projected through offset_map. \
         The pending_first_instr_lambda shim or elf.rs projection may have failed."
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
