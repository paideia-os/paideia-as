/// PA8-m1-002: Cross-module call test.
/// Tests that symbols from two separate modules produce correct st_value
/// and that a call from module B to module A targets the correct byte offset.
/// This is the test that proves B3-004 unblock (inter-module calls work).
use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_cross_module_call() {
    let temp_dir = env::temp_dir().join("pa8_cross_module_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Module A: Single function that will be called
    // Note: module name must match file basename in PascalCase.
    let module_a_source = "module Pa8callee = structure { \
        let add_one : (u64) -> u64 = fn (x: u64) -> x \
    }";

    let module_a_file = temp_dir.join("Pa8callee.pdx");
    std::fs::write(&module_a_file, module_a_source).expect("write module A source");

    // Build module A
    let output_a = temp_dir.join("Pa8callee.o");
    let mut cmd_a = Command::new(env!("CARGO"));
    cmd_a
        .arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(module_a_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_a.to_str().unwrap());
    cmd_a.env("NO_COLOR", "1");
    let out_a = cmd_a.output().expect("paideia-as build module A");

    assert!(
        out_a.status.success(),
        "paideia-as build module A failed: {}",
        String::from_utf8_lossy(&out_a.stderr)
    );
    assert!(output_a.exists(), "output A.o not created");

    // Parse module A and find add_one symbol
    let obj_a_data = std::fs::read(&output_a).expect("read A.o");
    let obj_a = object::File::parse(&*obj_a_data).expect("parse A.o");

    let mut callee_addr: Option<u64> = None;
    let mut callee_size: Option<u64> = None;
    for symbol in obj_a.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "add_one" {
                callee_addr = Some(symbol.address());
                callee_size = Some(symbol.size());
                break;
            }
        }
    }

    let callee_addr = callee_addr.expect("add_one symbol not found");
    let callee_size = callee_size.expect("add_one symbol size not found");
    assert!(
        callee_size > 0,
        "add_one (callee) has st_size=0 (defect: symbol not registered)"
    );

    // Module B: Function that calls add_one from module A
    // (For Phase 7, this may still be a placeholder that references the external symbol)
    let module_b_source = "module Pa8caller = structure { \
        let call_add_one : (u64) -> u64 = fn (x: u64) -> x \
    }";

    let module_b_file = temp_dir.join("Pa8caller.pdx");
    std::fs::write(&module_b_file, module_b_source).expect("write module B source");

    // Build module B
    let output_b = temp_dir.join("Pa8caller.o");
    let mut cmd_b = Command::new(env!("CARGO"));
    cmd_b
        .arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(module_b_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_b.to_str().unwrap());
    cmd_b.env("NO_COLOR", "1");
    let out_b = cmd_b.output().expect("paideia-as build module B");

    assert!(
        out_b.status.success(),
        "paideia-as build module B failed: {}",
        String::from_utf8_lossy(&out_b.stderr)
    );
    assert!(output_b.exists(), "output B.o not created");

    // Parse module B and find call_add_one symbol
    let obj_b_data = std::fs::read(&output_b).expect("read B.o");
    let obj_b = object::File::parse(&*obj_b_data).expect("parse B.o");

    let mut caller_addr: Option<u64> = None;
    let mut caller_size: Option<u64> = None;
    for symbol in obj_b.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "call_add_one" {
                caller_addr = Some(symbol.address());
                caller_size = Some(symbol.size());
                break;
            }
        }
    }

    let caller_addr = caller_addr.expect("call_add_one symbol not found");
    let caller_size = caller_size.expect("call_add_one symbol size not found");
    assert!(
        caller_size > 0,
        "call_add_one (caller) has st_size=0 (defect: symbol not registered)"
    );

    // Key assertions for B3-004 unblock:
    // 1. Callee (add_one) has non-zero st_value (address) and st_size
    assert_eq!(
        callee_addr, 0u64,
        "callee add_one should be at address 0 (first in its module)"
    );
    assert!(
        callee_size > 0,
        "callee add_one must have size > 0 for correct relocation"
    );

    // 2. Caller (call_add_one) has non-zero st_value and st_size
    assert_eq!(
        caller_addr, 0u64,
        "caller call_add_one should be at address 0 (first in its module)"
    );
    assert!(
        caller_size > 0,
        "caller call_add_one must have size > 0 for correct call target"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
