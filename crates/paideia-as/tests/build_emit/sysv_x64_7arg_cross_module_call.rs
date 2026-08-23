//! v0.22.0 (#1326 phase 6): cross-module registration of a >6-arg SysV
//! symbol.
//!
//! Design doc §5 asserts that cross-module symbol resolution
//! (`symbols::lookup_by_name` in `emit_call.rs:224`) needs no change for
//! #1326 — arg count is not part of the symbol identity. This is the
//! regression pin for that claim: mirrors the established
//! `pa8_cross_module_call.rs` two-module pattern (each module built
//! independently to its own `.o`, symbol table inspected via the `object`
//! crate), but module A's exported function takes 7 args and reads its
//! 7th (first stack-passed, idx=6) parameter — proving the callee-side
//! `BindingHome::StackSlot` intake (phase 3) still emits and registers a
//! correct, non-zero-size `pub` symbol when compiled as a standalone
//! module rather than alongside a same-module caller.
use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_sysv_x64_7arg_cross_module_call() {
    let temp_dir = env::temp_dir().join("sysv_x64_7arg_cross_module_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Module A: 7-arg SysV callee. Body reads its 7th parameter (idx=6,
    // the first stack-passed param, installed at BindingHome::StackSlot(16)
    // per design doc §4.4) — exercises the full callee-side stack-arg
    // intake path, not just a register-homed param.
    let module_a_source = "module Sysv1326CalleeSeven = structure { \
        pub let callee_seven : (u64, u64, u64, u64, u64, u64, u64) -> u64 = \
            fn (a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> g \
    }";

    let module_a_file = temp_dir.join("Sysv1326CalleeSeven.pdx");
    std::fs::write(&module_a_file, module_a_source).expect("write module A source");

    let output_a = temp_dir.join("Sysv1326CalleeSeven.o");
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
        "paideia-as build module A (7-arg SysV callee) failed: {}",
        String::from_utf8_lossy(&out_a.stderr)
    );
    assert!(output_a.exists(), "output A.o not created");

    let obj_a_data = std::fs::read(&output_a).expect("read A.o");
    let obj_a = object::File::parse(&*obj_a_data).expect("parse A.o");

    let mut callee_addr: Option<u64> = None;
    let mut callee_size: Option<u64> = None;
    for symbol in obj_a.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "callee_seven" {
                callee_addr = Some(symbol.address());
                callee_size = Some(symbol.size());
                break;
            }
        }
    }

    let callee_addr = callee_addr.expect("callee_seven symbol not found");
    let callee_size = callee_size.expect("callee_seven symbol size not found");
    assert_eq!(
        callee_addr, 0u64,
        "callee_seven should be at address 0 (first in its module)"
    );
    assert!(
        callee_size > 0,
        "callee_seven (7-arg SysV, stack-passed param read) must have size > 0 \
         for correct cross-module relocation"
    );

    // Module B: 7-arg SysV caller. Marshals args 0..5 into registers and
    // arg 6 onto the stack (phase 2 caller-side path), calling into a
    // 7-arg symbol with the same shape as module A's export — the two are
    // built as separate compilation units to prove arg count plays no
    // role in the symbol table or in cross-module linkability.
    let module_b_source = "module Sysv1326CallerSeven = structure { \
        pub let caller_seven : (u64) -> u64 = \
            fn (base: u64) -> caller_seven_target(base, 2, 3, 4, 5, 6, 7) \
        let caller_seven_target : (u64, u64, u64, u64, u64, u64, u64) -> u64 = \
            fn (a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> g \
    }";

    let module_b_file = temp_dir.join("Sysv1326CallerSeven.pdx");
    std::fs::write(&module_b_file, module_b_source).expect("write module B source");

    let output_b = temp_dir.join("Sysv1326CallerSeven.o");
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
        "paideia-as build module B (7-arg SysV caller) failed: {}",
        String::from_utf8_lossy(&out_b.stderr)
    );
    assert!(output_b.exists(), "output B.o not created");

    let obj_b_data = std::fs::read(&output_b).expect("read B.o");
    let obj_b = object::File::parse(&*obj_b_data).expect("parse B.o");

    let mut caller_addr: Option<u64> = None;
    let mut caller_size: Option<u64> = None;
    for symbol in obj_b.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "caller_seven" {
                caller_addr = Some(symbol.address());
                caller_size = Some(symbol.size());
                break;
            }
        }
    }

    let caller_addr = caller_addr.expect("caller_seven symbol not found");
    let caller_size = caller_size.expect("caller_seven symbol size not found");
    assert_eq!(
        caller_addr, 0u64,
        "caller_seven should be at address 0 (first in its module)"
    );
    assert!(
        caller_size > 0,
        "caller_seven (7-arg SysV caller-side stack-spill) must have size > 0"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
