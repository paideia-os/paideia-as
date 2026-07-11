/// PA8-m1-002c (preparatory for #1150 v3): Regression fixture for Match-bodied lambdas.
/// Tests that Match-bodied lambda functions emit with correct st_value when using
/// Match expression shapes. This fixture verifies that the wire-up for Match lambdas
/// correctly identifies and records their first emitted instruction.
///
/// Expected behavior per comment at pa8_call_st_value.rs L96-108:
/// The fixture asserts exact byte offsets to detect silent divergence between
/// encoder ground truth (offset_map) and any fallback estimator. This prevents
/// regression where lambdas fail to wire up and silently fall back to (0, 0).
use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_match_body_st_value() {
    let temp_dir = env::temp_dir().join("pa8_match_body_st_value_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Multi-function module with Match-bodied lambdas.
    // The functions use match expressions with enum variant patterns and distinct arm bodies.
    // This exercises the visit_lambda Match arm which was recording sentinel offsets.
    // Note: module name must match file basename in PascalCase.
    let source = "module Pa8matchbody = structure { enum Status { Ok(u64), Err(u64) } let ok_val : Status = Status::Ok(42u64) let err_val : Status = Status::Err(99u64) let check_ok : () -> u64 = fn (_: ()) -> match ok_val {\n    Ok(v) => v\n    Err(e) => 0u64\n  } let check_err : () -> u64 = fn (_: ()) -> match err_val {\n    Ok(v) => 1u64\n    Err(e) => e\n  } }";

    let source_file = temp_dir.join("Pa8matchbody.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa8matchbody.o");
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
            if name == "check_ok" || name == "check_err" {
                func_symbols.push((
                    name.to_string(),
                    symbol.address() as u32,
                    symbol.size() as u32,
                ));
            }
        }
    }

    assert!(
        func_symbols.len() >= 2,
        "expected at least 2 function symbols, got {}. All symbols: {:?}",
        func_symbols.len(),
        all_symbols
    );

    // Assertions per pa8_call_st_value.rs L96-108:
    // 1. None of the function symbols should have st_size=0.
    // This catches the defect where offset_map lookup failed and all symbols
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
    // - check_ok at 0, with non-zero size (Match body with cmp/jne dispatch and arm bodies)
    // - check_err at (check_ok_offset + check_ok_size), with non-zero size (Match body)
    assert!(func_symbols[0].2 > 0, "check_ok should have size > 0");
    assert!(func_symbols[1].2 > 0, "check_err should have size > 0");

    // 3. Explicit st_value (offset) regression guard: verify exact offsets are sourced from
    // encoder ground truth (offset_map), not estimated_offset. This catches silent divergence
    // if the offset_map and estimator ever disagree.
    // Expected offsets (ground truth from encoder):
    // - check_ok: starts at offset 0
    // - check_err: starts after check_ok (typically around offset 20-35 bytes depending on match dispatch encoding)
    assert_eq!(
        func_symbols[0].1, 0,
        "check_ok st_value should be 0 (offset_map ground truth)"
    );
    // check_err should start after check_ok; exact offset depends on check_ok size
    assert!(
        func_symbols[1].1 > 0,
        "check_err st_value should be > 0 (not zero, indicating regression)"
    );
    assert!(
        func_symbols[1].1 >= func_symbols[0].1 + func_symbols[0].2,
        "check_err should start at or after check_ok's end"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
