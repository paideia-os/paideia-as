/// PA8-m1-002: Witness test for symbol st_value/st_size threading.
/// Tests that three function symbols with different bodies emit at distinct offsets
/// with correct contiguous sizes.
use object::{Object, ObjectSection, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_st_value() {
    let temp_dir = env::temp_dir().join("pa8_st_value_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Three function bindings with different bodies that emit.
    // Using identity, bitwise-not, and identity-again (simpler than add).
    // Note: module name must match file basename in PascalCase.
    let source = "module Pa8three = structure { let a : (u64) -> u64 = fn (x: u64) -> x ; let b : (u64) -> u64 = fn (x: u64) -> ~x ; let c : (u64) -> u64 = fn (x: u64) -> x }";

    let source_file = temp_dir.join("Pa8three.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa8Three.o");
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

    // Find the .text section size.
    let mut text_size = 0u64;
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_size = section.size();
            break;
        }
    }
    assert!(text_size > 0, ".text section must exist and have size > 0");

    // Collect symbols for a, b, c.
    let mut symbols = std::collections::HashMap::new();
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "a" || name == "b" || name == "c" {
                symbols.insert(
                    name.to_string(),
                    (symbol.address() as u32, symbol.size() as u32),
                );
            }
        }
    }

    assert_eq!(symbols.len(), 3, "expected exactly 3 symbols (a, b, c)");

    let a_addr = symbols["a"].0;
    let a_size = symbols["a"].1;
    let b_addr = symbols["b"].0;
    let b_size = symbols["b"].1;
    let c_addr = symbols["c"].0;
    let c_size = symbols["c"].1;

    // Assertion 1: Addresses in increasing order.
    assert!(
        a_addr < b_addr,
        "expected a.address() < b.address(), got {} >= {}",
        a_addr,
        b_addr
    );
    assert!(
        b_addr < c_addr,
        "expected b.address() < c.address(), got {} >= {}",
        b_addr,
        c_addr
    );

    // Assertion 2: Contiguous layout.
    assert_eq!(
        a_addr + a_size,
        b_addr,
        "a and b not contiguous: a.address={}, a.size={}, b.address={}",
        a_addr,
        a_size,
        b_addr
    );
    assert_eq!(
        b_addr + b_size,
        c_addr,
        "b and c not contiguous: b.address={}, b.size={}, c.address={}",
        b_addr,
        b_size,
        c_addr
    );

    // Assertion 3: c extends to end of .text.
    assert_eq!(
        c_addr + c_size,
        text_size as u32,
        "c doesn't extend to .text end: c.address={}, c.size={}, text_size={}",
        c_addr,
        c_size,
        text_size
    );

    // Assertion 4: None of the symbols have address == 0 && size == 0.
    assert!(
        !(a_addr == 0 && a_size == 0),
        "symbol a has st_value=0, st_size=0 (regression)"
    );
    assert!(
        !(b_addr == 0 && b_size == 0),
        "symbol b has st_value=0, st_size=0 (regression)"
    );
    assert!(
        !(c_addr == 0 && c_size == 0),
        "symbol c has st_value=0, st_size=0 (regression)"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
