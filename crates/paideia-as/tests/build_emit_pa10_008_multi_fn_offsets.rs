//! PA10-008 (m1-001): function offsets in multi-fn modules.
//!
//! Tests that when a module contains multiple functions, each function
//! gets a distinct offset in the symbol table (cumulative offset tracking).
//! Previously both functions might have had offset 0.

use object::{Object, ObjectSection, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa10_008_multi_fn_module_distinct_offsets() {
    let temp_dir = env::temp_dir().join("pa10_008_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Module with three functions of different sizes.
    // Using identity, bitwise-not, and identity-again.
    let source = "module Pa10008 = structure { let f : (u64) -> u64 = fn (x: u64) -> x ; let g : (u64) -> u64 = fn (x: u64) -> ~x ; let h : (u64) -> u64 = fn (x: u64) -> x }";

    let source_file = temp_dir.join("Pa10008.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa10008.o");
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

    // Collect symbols for f, g, h.
    let mut symbols = std::collections::HashMap::new();
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "f" || name == "g" || name == "h" {
                symbols.insert(
                    name.to_string(),
                    (symbol.address() as u32, symbol.size() as u32),
                );
            }
        }
    }

    assert_eq!(
        symbols.len(),
        3,
        "expected exactly 3 symbols (f, g, h), got {}",
        symbols.len()
    );

    let f_addr = symbols["f"].0;
    let f_size = symbols["f"].1;
    let g_addr = symbols["g"].0;
    let g_size = symbols["g"].1;
    let h_addr = symbols["h"].0;
    let h_size = symbols["h"].1;

    // Assertion 1: All addresses should be distinct (not overlapping).
    assert!(
        f_addr != g_addr,
        "PA10-008: f and g have the same address (offset) — regression"
    );
    assert!(
        g_addr != h_addr,
        "PA10-008: g and h have the same address (offset) — regression"
    );
    assert!(
        f_addr != h_addr,
        "PA10-008: f and h have the same address (offset) — regression"
    );

    // Assertion 2: Addresses in increasing order.
    assert!(
        f_addr < g_addr,
        "expected f.address() < g.address(), got {} >= {}",
        f_addr,
        g_addr
    );
    assert!(
        g_addr < h_addr,
        "expected g.address() < h.address(), got {} >= {}",
        g_addr,
        h_addr
    );

    // Assertion 3: Contiguous layout (no gaps).
    assert_eq!(
        f_addr + f_size,
        g_addr,
        "f and g not contiguous: f.address={}, f.size={}, g.address={}",
        f_addr,
        f_size,
        g_addr
    );
    assert_eq!(
        g_addr + g_size,
        h_addr,
        "g and h not contiguous: g.address={}, g.size={}, h.address={}",
        g_addr,
        g_size,
        h_addr
    );

    // Assertion 4: h extends to end of .text.
    assert_eq!(
        h_addr + h_size,
        text_size as u32,
        "h doesn't extend to .text end: h.address={}, h.size={}, text_size={}",
        h_addr,
        h_size,
        text_size
    );

    // Assertion 5: None of the symbols have address == 0 && size == 0.
    assert!(
        !(f_addr == 0 && f_size == 0),
        "symbol f has st_value=0, st_size=0 (regression)"
    );
    assert!(
        !(g_addr == 0 && g_size == 0),
        "symbol g has st_value=0, st_size=0 (regression)"
    );
    assert!(
        !(h_addr == 0 && h_size == 0),
        "symbol h has st_value=0, st_size=0 (regression)"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
