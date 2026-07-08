/// PA8-m1-002b: Witness test for unsafe-block lambda symbol st_value/st_size.
/// Tests that function symbols with unsafe-block bodies emit at distinct offsets
/// with correct contiguous sizes, using post-encoding offset_map truth.
use object::{Object, ObjectSection, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_unsafe_block_st_value() {
    let temp_dir = env::temp_dir().join("pa8_unsafe_block_st_value_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Two function bindings with unsafe-block bodies.
    // f1: { hlt; hlt } → 2 bytes
    // f2: { cli; sti; hlt } → 3 bytes
    // Note: module name must match file basename in PascalCase.
    let source = "module Pa8unsafeblock = structure { \
        let f1 : (u64) -> u64 !{} @{} = fn (x: u64) -> unsafe { effects: {}, capabilities: {}, justification: \"test1\", block: { hlt; hlt } } ; \
        let f2 : (u64) -> u64 !{} @{} = fn (x: u64) -> unsafe { effects: {}, capabilities: {}, justification: \"test2\", block: { cli; sti; hlt } } \
    }";

    let source_file = temp_dir.join("Pa8unsafeblock.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa8unsafeblock.o");
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

    // Collect symbols for f1, f2.
    let mut symbols = std::collections::HashMap::new();
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "f1" || name == "f2" {
                symbols.insert(
                    name.to_string(),
                    (symbol.address() as u32, symbol.size() as u32),
                );
            }
        }
    }

    assert_eq!(
        symbols.len(),
        2,
        "expected exactly 2 symbols (f1, f2), got {}",
        symbols.len()
    );

    let f1_addr = symbols["f1"].0;
    let f1_size = symbols["f1"].1;
    let f2_addr = symbols["f2"].0;
    let f2_size = symbols["f2"].1;

    // Assertion 1: Addresses should be distinct.
    assert!(
        f1_addr != f2_addr,
        "PA8-m1-002b: f1 and f2 have the same address (offset) — regression"
    );

    // Assertion 2: Addresses in increasing order.
    assert!(
        f1_addr < f2_addr,
        "expected f1.address() < f2.address(), got {} >= {}",
        f1_addr,
        f2_addr
    );

    // Assertion 3: Contiguous layout (no gaps).
    assert_eq!(
        f1_addr + f1_size,
        f2_addr,
        "f1 and f2 not contiguous: f1.address={}, f1.size={}, f2.address={}",
        f1_addr,
        f1_size,
        f2_addr
    );

    // Assertion 4: f2 extends to end of .text.
    assert_eq!(
        f2_addr + f2_size,
        text_size as u32,
        "f2 doesn't extend to .text end: f2.address={}, f2.size={}, text_size={}",
        f2_addr,
        f2_size,
        text_size
    );

    // Assertion 5: Expected sizes.
    // f1: hlt (1 byte) + hlt (1 byte) = 2 bytes
    // f2: cli (1 byte) + sti (1 byte) + hlt (1 byte) = 3 bytes
    assert_eq!(
        f1_size, 2,
        "PA8-m1-002b: f1 size should be 2 (hlt;hlt), got {}",
        f1_size
    );
    assert_eq!(
        f2_size, 3,
        "PA8-m1-002b: f2 size should be 3 (cli;sti;hlt), got {}",
        f2_size
    );

    // Assertion 6: None of the symbols have address == 0 && size == 0.
    assert!(
        !(f1_addr == 0 && f1_size == 0),
        "symbol f1 has st_value=0, st_size=0 (regression)"
    );
    assert!(
        !(f2_addr == 0 && f2_size == 0),
        "symbol f2 has st_value=0, st_size=0 (regression)"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
